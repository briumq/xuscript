use std::hash::{BuildHasher, Hash, Hasher};
use std::rc::Rc;

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
                rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            }
            Ok(Value::UNIT)
        }
        MethodKind::DictInsert | MethodKind::ListInsert => {
            validate_arity(rt, method, args.len(), 2, 2)?;

            let value = args[1].clone();

            // Fast path for small integer keys - use elements array
            if args[0].is_int() {
                let i = args[0].as_i64();
                if i >= 0 && i < crate::core::value::ELEMENTS_MAX {
                    let idx = i as usize;
                    let me = expect_dict_mut(rt, recv)?;
                    // Ensure elements array is large enough
                    if me.elements.len() <= idx {
                        me.elements.resize(idx + 1, crate::Value::UNIT);
                    }
                    // Check if this is a new key (was VOID before)
                    let was_unit = me.elements[idx].get_tag() == crate::core::value::TAG_UNIT;
                    me.elements[idx] = value;
                    if was_unit {
                        me.ver += 1;
                        rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
                    }
                    return Ok(Value::UNIT);
                }
            }

            // Fast path for string keys - avoid creating DictKey for existing keys
            if args[0].get_tag() == crate::core::value::TAG_STR {
                // Get key string pointer and compute hash first (avoid cloning)
                let (key_ptr, key_len, hash) = {
                    let key_str = expect_str(rt, args[0])?;
                    let me = expect_dict(rt, recv)?;
                    let hash = {
                        let mut h = me.map.hasher().build_hasher();
                        h.write_u8(0); // String discriminant
                        key_str.as_bytes().hash(&mut h);
                        h.finish()
                    };
                    (key_str.as_str().as_ptr(), key_str.len(), hash)
                };

                let me = expect_dict_mut(rt, recv)?;
                // SAFETY: key_ptr/key_len are valid, we haven't modified the heap
                let key_str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };

                match me.map.raw_entry_mut().from_hash(hash, |k| k.is_str() && k.as_str() == key_str) {
                    RawEntryMut::Occupied(mut o) => {
                        // 值更新 - 不增加版本号
                        *o.get_mut() = value;
                    }
                    RawEntryMut::Vacant(vac) => {
                        // 新 key - 需要创建 DictKey
                        let key = DictKey::from_str(key_str);
                        vac.insert(key, value);
                        me.ver += 1;
                        rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
                    }
                }
                return Ok(Value::UNIT);
            }

            // Slow path for large integer keys
            let key = get_dict_key_from_value(rt, &args[0])?;

            let me = expect_dict_mut(rt, recv)?;
            let mut hasher = me.map.hasher().build_hasher();
            key.hash(&mut hasher);
            let h = hasher.finish();

            match me.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                RawEntryMut::Occupied(mut o) => {
                    // 值更新 - 不增加版本号
                    *o.get_mut() = value;
                }
                RawEntryMut::Vacant(vac) => {
                    // 新 key - 增加版本号
                    vac.insert(key, value);
                    me.ver += 1;
                    rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
                }
            }
            Ok(Value::UNIT)
        }
        MethodKind::DictInsertInt => {
            validate_arity(rt, method, args.len(), 2, 2)?;

            let i = to_i64(&args[0])?;
            let value = args[1].clone();

            // Fast path for small integer keys - use elements array
            if i >= 0 && i < crate::core::value::ELEMENTS_MAX {
                let idx = i as usize;
                let me = expect_dict_mut(rt, recv)?;
                // Ensure elements array is large enough
                if me.elements.len() <= idx {
                    me.elements.resize(idx + 1, crate::Value::UNIT);
                }
                // Check if this is a new key (was VOID before)
                let was_unit = me.elements[idx].get_tag() == crate::core::value::TAG_UNIT;
                me.elements[idx] = value;
                if was_unit {
                    me.ver += 1;
                    rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
                }
                return Ok(Value::UNIT);
            }

            // Slow path for large integer keys
            let me = expect_dict_mut(rt, recv)?;
            let h = Runtime::hash_dict_key_int(me.map.hasher(), i);
            let key = DictKey::Int(i);

            match me.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                RawEntryMut::Occupied(mut o) => {
                    // 值更新 - 不增加版本号
                    *o.get_mut() = value;
                }
                RawEntryMut::Vacant(vac) => {
                    // 新 key - 增加版本号
                    vac.insert(key, value);
                    me.ver += 1;
                    rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
                }
            }
            Ok(Value::UNIT)
        }
        MethodKind::DictGet => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "key")?;

            let id = recv.as_obj_id().0;

            // Get key string reference without cloning
            let key_str = expect_str(rt, args[0])?.as_str();

            // Check cache first (before computing hash)
            if let Some(c) = rt.caches.dict_cache_last.as_ref() {
                if c.id == id {
                    let me = expect_dict(rt, recv)?;
                    if c.ver == me.ver && c.key.as_str() == key_str {
                        return Ok(rt.option_some(c.value.clone()));
                    }
                }
            }

            let (v, cur_ver) = {
                let me = expect_dict(rt, recv)?;
                let cur_ver = me.ver;
                let key_hash = Runtime::hash_bytes(me.map.hasher(), key_str.as_bytes());
                let v = Runtime::dict_get_by_str_with_hash(me, key_str, key_hash);
                (v, cur_ver)
            };

            rt.caches.dict_version_last = Some((id, cur_ver));

            let Some(v) = v else {
                return Ok(rt.option_none());
            };

            // Only clone the key when we need to cache it
            let key_text = expect_str(rt, args[0])?.clone();
            rt.caches.dict_cache_last = Some(DictCacheLast {
                id,
                ver: cur_ver,
                key: key_text,
                value: v.clone(),
            });
            Ok(rt.option_some(v))
        }
        MethodKind::DictGetInt => {
            validate_arity(rt, method, args.len(), 1, 1)?;

            let i = to_i64(&args[0])?;
            let id = recv.as_obj_id().0;

            // Check cache first
            if let Some(c) = rt.caches.dict_cache_int_last.as_ref() {
                if c.id == id && c.key == i {
                    let me = expect_dict(rt, recv)?;
                    if c.ver == me.ver {
                        return Ok(rt.option_some(c.value.clone()));
                    }
                }
            }

            // Fast path for small integer keys - use elements array
            let (v, cur_ver) = {
                let me = expect_dict(rt, recv)?;
                let cur_ver = me.ver;
                let v = if i >= 0 && i < crate::core::value::ELEMENTS_MAX {
                    let idx = i as usize;
                    if idx < me.elements.len() {
                        let elem = me.elements[idx];
                        if elem.get_tag() != crate::core::value::TAG_UNIT {
                            Some(elem)
                        } else {
                            None
                        }
                    } else {
                        // Fall back to map lookup
                        me.map.get(&DictKey::Int(i)).cloned()
                    }
                } else {
                    // Large integer key - use map
                    me.map.get(&DictKey::Int(i)).cloned()
                };
                (v, cur_ver)
            };

            rt.caches.dict_version_last = Some((id, cur_ver));

            let Some(v) = v else {
                return Ok(rt.option_none());
            };

            rt.caches.dict_cache_int_last = Some(DictCacheIntLast {
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

            let key_str = expect_str(rt, args[0])?.as_str();
            let me = expect_dict(rt, recv)?;

            if let Some(sid) = me.shape {
                if let crate::core::heap::ManagedObject::Shape(shape) = rt.heap.get(sid) {
                    if let Some(&off) = shape.prop_map.get(key_str) {
                        let ok = me
                            .prop_values
                            .get(off)
                            .is_some_and(|v| v.get_tag() != crate::core::value::TAG_UNIT);
                        return Ok(Value::from_bool(ok));
                    }
                }
            }

            // Use raw_entry to avoid creating DictKey
            let hash = {
                let mut h = me.map.hasher().build_hasher();
                h.write_u8(0);
                key_str.as_bytes().hash(&mut h);
                h.finish()
            };
            let found = me
                .map
                .raw_entry()
                .from_hash(hash, |k| k.is_str() && k.as_str() == key_str)
                .is_some();
            Ok(Value::from_bool(found))
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
                    .from_hash(hash, |k| k.is_str() && k.as_str() == key_ptr)
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
                rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));
            }
            Ok(Value::from_bool(had_key))
        }
        MethodKind::Clear => {
            validate_arity(rt, method, args.len(), 0, 0)?;

            let me = expect_dict_mut(rt, recv)?;
            me.map.clear();
            me.elements.clear();
            me.prop_values.clear();
            me.ver += 1;
            rt.caches.dict_version_last = Some((recv.as_obj_id().0, me.ver));

            Ok(Value::UNIT)
        }
        MethodKind::DictKeys => {
            validate_arity(rt, method, args.len(), 0, 0)?;

            enum KeyData {
                Str(Rc<String>),
                Int(i64),
            }
            let mut keys: Vec<KeyData> = Vec::new();

            {
                let me = expect_dict(rt, recv)?;
                keys.reserve(me.map.len() + me.elements.len());

                // Include keys from elements array
                for (i, ev) in me.elements.iter().enumerate() {
                    if ev.get_tag() != crate::core::value::TAG_UNIT {
                        keys.push(KeyData::Int(i as i64));
                    }
                }

                for k in me.map.keys() {
                match k {
                    DictKey::StrInline { .. } | DictKey::Str { .. } => {
                        keys.push(KeyData::Str(Rc::new(k.as_str().to_string())));
                    }
                    DictKey::Int(i) => {
                        keys.push(KeyData::Int(*i));
                    }
                }
            }
            }

            let mut result = Vec::with_capacity(keys.len());
            for key in keys {
                match key {
                    KeyData::Str(s) => result.push(create_str_value(rt, &s)),
                    KeyData::Int(i) => result.push(Value::from_i64(i)),
                }
            }

            Ok(create_list_value(rt, result))
        }
        MethodKind::DictValues => {
            validate_arity(rt, method, args.len(), 0, 0)?;

            let me = expect_dict(rt, recv)?;
            let mut values: Vec<_> = Vec::with_capacity(me.map.len() + me.elements.len());

            // Include values from elements array
            for ev in &me.elements {
                if ev.get_tag() != crate::core::value::TAG_UNIT {
                    values.push(*ev);
                }
            }

            values.extend(me.map.values().cloned());
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

            enum ItemKey {
                Str(Rc<String>),
                Int(i64),
            }
            let mut items_data: Vec<(ItemKey, Value)> = Vec::new();

            {
                let me = expect_dict(rt, recv)?;
                items_data.reserve(me.map.len() + me.elements.len());

                // Include items from elements array
                for (i, ev) in me.elements.iter().enumerate() {
                    if ev.get_tag() != crate::core::value::TAG_UNIT {
                        items_data.push((ItemKey::Int(i as i64), *ev));
                    }
                }

                for (k, v) in me.map.iter() {
                match k {
                    DictKey::StrInline { .. } | DictKey::Str { .. } => {
                        items_data.push((ItemKey::Str(Rc::new(k.as_str().to_string())), v.clone()));
                    }
                    DictKey::Int(i) => {
                        items_data.push((ItemKey::Int(*i), v.clone()));
                    }
                }
            }
            }

            let mut items = Vec::with_capacity(items_data.len());
            for (key, v) in items_data {
                let key_val = match key {
                    ItemKey::Str(s) => create_str_value(rt, &s),
                    ItemKey::Int(i) => Value::from_i64(i),
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
                if ev.get_tag() != crate::core::value::TAG_UNIT {
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
