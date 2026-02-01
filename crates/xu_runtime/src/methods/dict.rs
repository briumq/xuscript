use std::hash::{BuildHasher, Hash, Hasher};

use hashbrown::hash_map::RawEntryMut;

use crate::Value;
use crate::util::to_i64;
use crate::core::value::DictKey;

use super::super::runtime::{DictCacheIntLast, DictCacheLast};
use super::{MethodKind, Runtime};
use super::common::*;

pub(super) fn dispatch(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    match kind {
        MethodKind::DictMerge => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_dict_param(rt, &args[0], "other")?;
            
            let other_dict = expect_dict(rt, args[0])?;
            let entries: Vec<(DictKey, Value)> = other_dict
                .map
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            let me = expect_dict_mut(rt, recv)?;
            let mut changed = false;
            me.map.reserve(entries.len());
            
            for (k, v) in entries {
                let mut hasher = me.map.hasher().build_hasher();
                k.hash(&mut hasher);
                let h = hasher.finish();
                match me.map.raw_entry_mut().from_hash(h, |kk| kk == &k) {
                    RawEntryMut::Occupied(mut o) => {
                        *o.get_mut() = v;
                    }
                    RawEntryMut::Vacant(vac) => {
                        vac.insert(k, v);
                        changed = true;
                    }
                }
            }
            
            if changed {
                me.ver += 1;
                rt.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            }
            Ok(Value::VOID)
        }
        MethodKind::DictInsert | MethodKind::ListInsert => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            
            let key = get_dict_key_from_value(rt, &args[0])?;
            let value = args[1].clone();

            let me = expect_dict_mut(rt, recv)?;
            let mut hasher = me.map.hasher().build_hasher();
            key.hash(&mut hasher);
            let h = hasher.finish();
            let mut changed = false;
            
            match me.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                RawEntryMut::Occupied(mut o) => {
                    let prev = o.get().clone();
                    *o.get_mut() = value.clone();
                    if prev != value {
                        changed = true;
                    }
                }
                RawEntryMut::Vacant(vac) => {
                    vac.insert(key, value);
                    changed = true;
                }
            }
            
            if changed {
                me.ver += 1;
                rt.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            }
            Ok(Value::VOID)
        }
        MethodKind::DictInsertInt => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            
            let i = to_i64(&args[0])?;
            let value = args[1].clone();
            let me = expect_dict_mut(rt, recv)?;
            
            let h = Runtime::hash_dict_key_int(me.map.hasher(), i);
            let mut changed = false;
            let key = DictKey::Int(i);
            
            match me.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                RawEntryMut::Occupied(mut o) => {
                    let prev = o.get().clone();
                    *o.get_mut() = value.clone();
                    if prev != value {
                        changed = true;
                    }
                }
                RawEntryMut::Vacant(vac) => {
                    vac.insert(key, value.clone());
                    changed = true;
                }
            }
            
            if changed {
                me.ver += 1;
                rt.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            }
            Ok(Value::VOID)
        }
        MethodKind::DictGet => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "key")?;
            
            let key = expect_str(rt, args[0])?.clone();
            let id = recv.as_obj_id().0;
            
            let (v, cur_ver, key_hash) = {
                let me = expect_dict(rt, recv)?;
                let cur_ver = me.ver;
                let key_hash = Runtime::hash_bytes(me.map.hasher(), key.as_bytes());
                let v = Runtime::dict_get_by_str_with_hash(me, &key, key_hash);
                (v, cur_ver, key_hash)
            };
            
            rt.dict_version_last = Some((id, cur_ver));

            if let Some(c) = rt.dict_cache_last.as_ref() {
                if c.id == id && c.ver == cur_ver && c.key_hash == key_hash && c.key == key {
                    return Ok(rt.option_some(c.value.clone()));
                }
            }

            let Some(v) = v else {
                return Ok(rt.option_none());
            };

            rt.dict_cache_last = Some(DictCacheLast {
                id,
                key_hash,
                ver: cur_ver,
                key,
                value: v.clone(),
            });
            Ok(rt.option_some(v))
        }
        MethodKind::DictGetInt => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let i = to_i64(&args[0])?;
            let id = recv.as_obj_id().0;
            
            let (v, cur_ver) = {
                let me = expect_dict(rt, recv)?;
                let cur_ver = me.ver;
                let v = me.map.get(&DictKey::Int(i)).cloned();
                (v, cur_ver)
            };
            
            rt.dict_version_last = Some((id, cur_ver));

            if let Some(c) = rt.dict_cache_int_last.as_ref() {
                if c.id == id && c.ver == cur_ver && c.key == i {
                    return Ok(rt.option_some(c.value.clone()));
                }
            }
            
            let Some(v) = v else {
                return Ok(rt.option_none());
            };

            rt.dict_cache_int_last = Some(DictCacheIntLast {
                id,
                key: i,
                ver: cur_ver,
                value: v.clone(),
            });
            Ok(rt.option_some(v))
        }
        MethodKind::OptHas => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "key")?;
            
            let key = expect_str(rt, args[0])?.clone();
            let me = expect_dict(rt, recv)?;
            
            if let Some(sid) = me.shape {
                if let crate::core::gc::ManagedObject::Shape(shape) = rt.heap.get(sid) {
                    if let Some(&off) = shape.prop_map.get(key.as_str()) {
                        let ok = me
                            .prop_values
                            .get(off)
                            .is_some_and(|v| v.get_tag() != crate::core::value::TAG_VOID);
                        return Ok(Value::from_bool(ok));
                    }
                }
            }
            
            Ok(Value::from_bool(
                me.map.contains_key(&DictKey::from_text(&key)),
            ))
        }
        MethodKind::Contains => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            // Optimize: use pointer comparison to avoid cloning the key
            if args[0].get_tag() == crate::core::value::TAG_STR {
                let key_ptr = expect_str(rt, args[0])?.as_str();
                let me = expect_dict(rt, recv)?;
                
                // Use raw_entry to avoid cloning the key
                let hash = {
                    let mut h = me.map.hasher().build_hasher();
                    h.write_u8(0);
                    key_ptr.as_bytes().hash(&mut h);
                    h.finish()
                };
                
                let found = me
                    .map
                    .raw_entry()
                    .from_hash(hash, |k| match k {
                        DictKey::Str { data, .. } => data.as_str() == key_ptr,
                        _ => false,
                    })
                    .is_some();
                Ok(Value::from_bool(found))
            } else if args[0].is_int() {
                let key = DictKey::Int(args[0].as_i64());
                let me = expect_dict(rt, recv)?;
                Ok(Value::from_bool(me.map.contains_key(&key)))
            } else {
                return Err(err(rt, xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "text or int".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            }
        }
        MethodKind::Remove => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let key = get_dict_key_from_value(rt, &args[0])?;
            let me = expect_dict_mut(rt, recv)?;
            
            let removed = me.map.remove(&key);
            let had_key = removed.is_some();
            
            if had_key {
                me.ver += 1;
                rt.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            }
            Ok(Value::from_bool(had_key))
        }
        MethodKind::Clear => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let me = expect_dict_mut(rt, recv)?;
            me.map.clear();
            me.ver += 1;
            rt.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            
            Ok(Value::VOID)
        }
        MethodKind::DictKeys => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let mut keys = Vec::new();
            
            {
                let me = expect_dict(rt, recv)?;
                keys.reserve(me.map.len());
                
                for k in me.map.keys() {
                match k {
                    DictKey::Str { data, .. } => {
                        keys.push((true, data.clone()));
                    }
                    DictKey::Int(i) => {
                        keys.push((false, i.to_string().into()));
                    }
                }
            }
            }
            
            let mut result = Vec::with_capacity(keys.len());
            for (is_str, data) in keys {
                if is_str {
                    result.push(create_str_value(rt, &data));
                } else {
                    result.push(Value::from_i64(data.parse().unwrap()));
                }
            }
            
            Ok(create_list_value(rt, result))
        }
        MethodKind::DictValues => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let me = expect_dict(rt, recv)?;
            let values: Vec<_> = me.map.values().cloned().collect();
            Ok(create_list_value(rt, values))
        }
        MethodKind::GetOrDefault => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            
            let key = get_dict_key_from_value(rt, &args[0])?;
            let default = args[1].clone();
            let me = expect_dict(rt, recv)?;
            
            let value = me.map.get(&key).cloned().unwrap_or(default);
            Ok(value)
        }
        MethodKind::DictItems => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let mut items_data = Vec::new();
            
            {
                let me = expect_dict(rt, recv)?;
                items_data.reserve(me.map.len());
                
                for (k, v) in me.map.iter() {
                match k {
                    DictKey::Str { data, .. } => {
                        items_data.push((true, data.clone(), v.clone()));
                    }
                    DictKey::Int(i) => {
                        items_data.push((false, i.to_string().into(), v.clone()));
                    }
                }
            }
            }
            
            let mut items = Vec::with_capacity(items_data.len());
            for (is_str, data, v) in items_data {
                let key_val = if is_str {
                    create_str_value(rt, &data)
                } else {
                    Value::from_i64(data.parse().unwrap())
                };
                let entry = create_list_value(rt, vec![key_val, v]);
                items.push(entry);
            }
            
            Ok(create_list_value(rt, items))
        }
        MethodKind::Len => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let me = expect_dict(rt, recv)?;
            let mut n = me.map.len();
            n += me.prop_values.len();
            for ev in &me.elements {
                if ev.get_tag() != crate::core::value::TAG_VOID {
                    n += 1;
                }
            }
            Ok(Value::from_i64(n as i64))
        }
        _ => Err(err(rt, xu_syntax::DiagnosticKind::UnknownDictMethod(
            method.to_string(),
        ))),
    }
}
