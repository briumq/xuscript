use std::hash::{BuildHasher, Hash, Hasher};

use hashbrown::hash_map::RawEntryMut;

use crate::Value;
use crate::runtime::util::to_i64;
use crate::value::DictKey;

use super::super::{DictCacheIntLast, DictCacheLast};
use super::{MethodKind, Runtime};

pub(super) fn dispatch(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    args: &[Value],
    method: &str,
) -> Result<Value, String> {
    let id = recv.as_obj_id();
    match kind {
        MethodKind::DictMerge => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            if args[0].get_tag() != crate::value::TAG_DICT {
                return Err(rt.error(xu_syntax::DiagnosticKind::FormatDictRequired));
            }
            let other_id = args[0].as_obj_id();

            let mut changed = false;
            let mut entries: Vec<(crate::value::DictKey, Value)> = Vec::new();
            if let crate::gc::ManagedObject::Dict(d) = rt.heap.get(other_id) {
                entries.reserve(d.map.len());
                for (k, v) in d.map.iter() {
                    entries.push((k.clone(), v.clone()));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };

            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get_mut(id) {
                me.map.reserve(entries.len());
                for (k, v) in entries.into_iter() {
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
                    rt.dict_version_last = Some((id.0, me.ver));
                }
            }
            Ok(Value::VOID)
        }
        MethodKind::DictInsert | MethodKind::ListInsert => {
            if args.len() != 2 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 2,
                    expected_max: 2,
                    actual: args.len(),
                }));
            }
            let k_val = &args[0];
            let value = args[1].clone();
            let key = if k_val.get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(k_val.as_obj_id()) {
                    crate::value::DictKey::from_text(s)
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::InsertKeyRequired));
                }
            } else if k_val.is_int() {
                crate::value::DictKey::Int(k_val.as_i64())
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::InsertKeyRequired));
            };

            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get_mut(id) {
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
                    rt.dict_version_last = Some((id.0, me.ver));
                }
            }
            Ok(Value::VOID)
        }
        MethodKind::DictInsertInt => {
            if args.len() != 2 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 2,
                    expected_max: 2,
                    actual: args.len(),
                }));
            }
            let i = to_i64(&args[0])?;
            let value = args[1].clone();
            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get_mut(id) {
                let h = Runtime::hash_dict_key_int(me.map.hasher(), i);
                let mut changed = false;
                let key = crate::value::DictKey::Int(i);
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
                    rt.dict_version_last = Some((id.0, me.ver));
                }
            }
            Ok(Value::VOID)
        }
        MethodKind::DictGet => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let k_val = &args[0];
            let key = if k_val.get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(k_val.as_obj_id()) {
                    s.clone()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::GetKeyRequired));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::GetKeyRequired));
            };

            let (cur_ver, key_hash) = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                (me.ver, Runtime::hash_bytes(me.map.hasher(), key.as_bytes()))
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };

            rt.dict_version_last = Some((id.0, cur_ver));

            if let Some(c) = rt.dict_cache_last.as_ref() {
                if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash && c.key == key {
                    return Ok(rt.option_some(c.value.clone()));
                }
            }

            let v = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                Runtime::dict_get_by_str_with_hash(me, &key, key_hash)
            } else {
                None
            };
            let Some(v) = v else {
                return Ok(rt.option_none());
            };

            rt.dict_cache_last = Some(DictCacheLast {
                id: id.0,
                key_hash,
                ver: cur_ver,
                key,
                value: v.clone(),
            });
            Ok(rt.option_some(v))
        }
        MethodKind::DictGetInt => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let i = to_i64(&args[0])?;
            let cur_ver = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                me.ver
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            rt.dict_version_last = Some((id.0, cur_ver));

            if let Some(c) = rt.dict_cache_int_last.as_ref() {
                if c.id == id.0 && c.ver == cur_ver && c.key == i {
                    return Ok(rt.option_some(c.value.clone()));
                }
            }
            let v = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                me.map.get(&crate::value::DictKey::Int(i)).cloned()
            } else {
                None
            };
            let Some(v) = v else {
                return Ok(rt.option_none());
            };

            rt.dict_cache_int_last = Some(DictCacheIntLast {
                id: id.0,
                key: i,
                ver: cur_ver,
                value: v.clone(),
            });
            Ok(rt.option_some(v))
        }
        MethodKind::OptHas => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            if args[0].get_tag() != crate::value::TAG_STR {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "text".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            }
            let key = if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
                s.clone()
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a text".into())));
            };
            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                if let Some(sid) = me.shape {
                    if let crate::gc::ManagedObject::Shape(shape) = rt.heap.get(sid) {
                        if let Some(&off) = shape.prop_map.get(key.as_str()) {
                            let ok = me
                                .prop_values
                                .get(off)
                                .is_some_and(|v| v.get_tag() != crate::value::TAG_VOID);
                            return Ok(Value::from_bool(ok));
                        }
                    }
                }
                Ok(Value::from_bool(me.map.contains_key(&DictKey::from_text(&key))))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
            }
        }
        MethodKind::Contains => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            // Optimize: use pointer comparison to avoid cloning the key
            if args[0].get_tag() == crate::value::TAG_STR {
                let key_ptr = if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
                    s.as_str()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a text".into())));
                };
                if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                    // Use raw_entry to avoid cloning the key
                    let hash = {
                        let mut h = me.map.hasher().build_hasher();
                        h.write_u8(0);
                        key_ptr.as_bytes().hash(&mut h);
                        h.finish()
                    };
                    let found = me.map.raw_entry().from_hash(hash, |k| {
                        match k {
                            DictKey::Str { data, .. } => data.as_str() == key_ptr,
                            _ => false,
                        }
                    }).is_some();
                    Ok(Value::from_bool(found))
                } else {
                    Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
                }
            } else if args[0].is_int() {
                let key = DictKey::Int(args[0].as_i64());
                if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                    Ok(Value::from_bool(me.map.contains_key(&key)))
                } else {
                    Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "text or int".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            }
        }
        MethodKind::Remove => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let key = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
                    DictKey::from_text(s)
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a text".into())));
                }
            } else if args[0].is_int() {
                DictKey::Int(args[0].as_i64())
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "text or int".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            };

            let removed = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get_mut(id) {
                me.map.remove(&key)
            } else {
                None
            };

            let had_key = removed.is_some();
            if had_key {
                if let crate::gc::ManagedObject::Dict(me) = rt.heap.get_mut(id) {
                    me.ver += 1;
                    rt.dict_version_last = Some((id.0, me.ver));
                }
            }
            Ok(Value::from_bool(had_key))
        }
        MethodKind::Clear => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get_mut(id) {
                me.map.clear();
                me.ver += 1;
                rt.dict_version_last = Some((id.0, me.ver));
            }
            Ok(Value::VOID)
        }
        MethodKind::DictKeys => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            let keys_raw: Vec<_> = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                me.map.keys().cloned().collect()
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            let mut keys = Vec::with_capacity(keys_raw.len());
            for k in keys_raw {
                match k {
                    crate::value::DictKey::Str { data, .. } => {
                        keys.push(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(crate::Text::from_str(&data)))))
                    }
                    crate::value::DictKey::Int(i) => keys.push(Value::from_i64(i)),
                }
            }
            Ok(Value::list(
                rt.heap.alloc(crate::gc::ManagedObject::List(keys)),
            ))
        }
        MethodKind::DictValues => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                let values: Vec<_> = me.map.values().cloned().collect();
                Ok(Value::list(
                    rt.heap.alloc(crate::gc::ManagedObject::List(values)),
                ))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
            }
        }
        MethodKind::GetOrDefault => {
            if args.len() != 2 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 2,
                    expected_max: 2,
                    actual: args.len(),
                }));
            }
            let k_val = &args[0];
            let default = args[1].clone();
            let key = if k_val.get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(k_val.as_obj_id()) {
                    DictKey::from_text(s)
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a text".into())));
                }
            } else if k_val.is_int() {
                DictKey::Int(k_val.as_i64())
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "text or int".to_string(),
                    actual: k_val.type_name().to_string(),
                }));
            };

            let value = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                me.map.get(&key).cloned().unwrap_or(default)
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            Ok(value)
        }
        MethodKind::DictItems => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            let dict_entries: Vec<(crate::value::DictKey, Value)> = if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                me.map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            let mut items = Vec::with_capacity(dict_entries.len());
            for (k, v) in dict_entries {
                let key_val = match k {
                    crate::value::DictKey::Str { data, .. } => {
                        Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(crate::Text::from_str(&data))))
                    }
                    crate::value::DictKey::Int(i) => Value::from_i64(i),
                };
                let entry = rt.heap.alloc(crate::gc::ManagedObject::List(vec![key_val, v]));
                items.push(Value::list(entry));
            }
            Ok(Value::list(
                rt.heap.alloc(crate::gc::ManagedObject::List(items)),
            ))
        }
        MethodKind::Len => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::Dict(me) = rt.heap.get(id) {
                let mut n = me.map.len();
                n += me.prop_values.len();
                for ev in &me.elements {
                    if ev.get_tag() != crate::value::TAG_VOID {
                        n += 1;
                    }
                }
                Ok(Value::from_i64(n as i64))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
            }
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnknownDictMethod(
            method.to_string(),
        ))),
    }
}
