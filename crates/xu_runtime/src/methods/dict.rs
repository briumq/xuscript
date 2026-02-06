use std::rc::Rc;

use hashbrown::hash_map::RawEntryMut;

use crate::Value;
use crate::util::to_i64;
use crate::core::value::DictKey;

use super::super::runtime::{DictCacheIntLast, DictCacheLast};
use super::{MethodKind, Runtime};
use super::common::*;

/// 字典键的临时表示
enum TempKey {
    Str(Rc<String>),
    Int(i64),
}

impl TempKey {
    fn into_value(self, rt: &mut Runtime) -> Value {
        match self {
            TempKey::Str(s) => create_str_value(rt, &s),
            TempKey::Int(i) => Value::from_i64(i),
        }
    }
}

pub(super) fn dispatch(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    match kind {
        MethodKind::DictMerge => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_dict_param(rt, &args[0], "other")?;

            let entries: Vec<_> = {
                let other = expect_dict(rt, args[0])?;
                other.map.iter().map(|(k, v)| (k.clone(), *v)).collect()
            };

            let id = recv.as_obj_id().0;
            let me = expect_dict_mut(rt, recv)?;
            let mut changed = false;
            me.map.reserve(entries.len());

            for (k, v) in entries {
                let h = me.map.hasher().hash_one(&k);
                match me.map.raw_entry_mut().from_hash(h, |kk| kk == &k) {
                    RawEntryMut::Occupied(mut o) => { *o.get_mut() = v; }
                    RawEntryMut::Vacant(vac) => { vac.insert(k, v); changed = true; }
                }
            }

            if changed {
                me.ver += 1;
                rt.caches.dict_version_last = Some((id, me.ver));
            }
            Ok(Value::UNIT)
        }
        MethodKind::Insert => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            let value = args[1];
            let id = recv.as_obj_id().0;

            // 小整数键快速路径
            if args[0].is_int() {
                let i = args[0].as_i64();
                if i >= 0 && i < crate::core::value::ELEMENTS_MAX {
                    let idx = i as usize;
                    let me = expect_dict_mut(rt, recv)?;
                    if me.elements.len() <= idx {
                        me.elements.resize(idx + 1, Value::UNIT);
                    }
                    let was_unit = me.elements[idx].get_tag() == crate::core::value::TAG_UNIT;
                    me.elements[idx] = value;
                    if was_unit {
                        me.ver += 1;
                        rt.caches.dict_version_last = Some((id, me.ver));
                    }
                    return Ok(Value::UNIT);
                }
            }

            // 字符串键快速路径
            if args[0].get_tag() == crate::core::value::TAG_STR {
                let key_id = args[0].as_obj_id();
                let dict_key_hash = {
                    let key_str = expect_str(rt, args[0])?;
                    DictKey::hash_str(key_str.as_str())
                };

                // Compute HashMap hash from DictKey hash
                let hash = {
                    let me = expect_dict(rt, recv)?;
                    use std::hash::{BuildHasher, Hasher};
                    let mut h = me.map.hasher().build_hasher();
                    h.write_u8(0); // String discriminant
                    h.write_u64(dict_key_hash);
                    h.finish()
                };

                let me = expect_dict_mut(rt, recv)?;

                match me.map.raw_entry_mut().from_hash(hash, |k| {
                    if let DictKey::StrRef { hash: h, obj_id } = k {
                        *h == dict_key_hash && (*obj_id == key_id.0 || *h == dict_key_hash)
                    } else {
                        false
                    }
                }) {
                    RawEntryMut::Occupied(mut o) => { *o.get_mut() = value; }
                    RawEntryMut::Vacant(vac) => {
                        // Use ObjectId directly - no string copy!
                        vac.insert(DictKey::from_str_obj(key_id, dict_key_hash), value);
                        me.ver += 1;
                        rt.caches.dict_version_last = Some((id, me.ver));
                    }
                }
                return Ok(Value::UNIT);
            }

            // 慢速路径
            let key = get_dict_key_from_value(rt, &args[0])?;
            let me = expect_dict_mut(rt, recv)?;
            let h = me.map.hasher().hash_one(&key);

            match me.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                RawEntryMut::Occupied(mut o) => { *o.get_mut() = value; }
                RawEntryMut::Vacant(vac) => {
                    vac.insert(key, value);
                    me.ver += 1;
                    rt.caches.dict_version_last = Some((id, me.ver));
                }
            }
            Ok(Value::UNIT)
        }
        MethodKind::DictInsertInt => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            let i = to_i64(&args[0])?;
            let value = args[1];
            let id = recv.as_obj_id().0;

            // 小整数键快速路径
            if i >= 0 && i < crate::core::value::ELEMENTS_MAX {
                let idx = i as usize;
                let me = expect_dict_mut(rt, recv)?;
                if me.elements.len() <= idx {
                    me.elements.resize(idx + 1, Value::UNIT);
                }
                let was_unit = me.elements[idx].get_tag() == crate::core::value::TAG_UNIT;
                me.elements[idx] = value;
                if was_unit {
                    me.ver += 1;
                    rt.caches.dict_version_last = Some((id, me.ver));
                }
                return Ok(Value::UNIT);
            }

            // 大整数键
            let me = expect_dict_mut(rt, recv)?;
            let h = Runtime::hash_dict_key_int(me.map.hasher(), i);
            let key = DictKey::Int(i);

            match me.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                RawEntryMut::Occupied(mut o) => { *o.get_mut() = value; }
                RawEntryMut::Vacant(vac) => {
                    vac.insert(key, value);
                    me.ver += 1;
                    rt.caches.dict_version_last = Some((id, me.ver));
                }
            }
            Ok(Value::UNIT)
        }
        MethodKind::Get => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "key")?;

            let id = recv.as_obj_id().0;
            let key_str = expect_str(rt, args[0])?.as_str();

            // 检查缓存
            if let Some(c) = rt.caches.dict_cache_last.as_ref() {
                if c.id == id {
                    let me = expect_dict(rt, recv)?;
                    if c.ver == me.ver && c.key.as_str() == key_str {
                        return Ok(rt.option_some(c.value));
                    }
                }
            }

            let (v, cur_ver) = {
                let me = expect_dict(rt, recv)?;
                let hash = Runtime::hash_bytes(me.map.hasher(), key_str.as_bytes());
                (Runtime::dict_get_by_str_with_hash(me, key_str, hash), me.ver)
            };

            rt.caches.dict_version_last = Some((id, cur_ver));

            let Some(v) = v else { return Ok(rt.option_none()); };

            let key_text = expect_str(rt, args[0])?.clone();
            rt.caches.dict_cache_last = Some(DictCacheLast { id, ver: cur_ver, key: key_text, value: v });
            Ok(rt.option_some(v))
        }
        MethodKind::GetInt => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            let i = to_i64(&args[0])?;
            let id = recv.as_obj_id().0;

            // 检查缓存
            if let Some(c) = rt.caches.dict_cache_int_last.as_ref() {
                if c.id == id && c.key == i {
                    let me = expect_dict(rt, recv)?;
                    if c.ver == me.ver {
                        return Ok(rt.option_some(c.value));
                    }
                }
            }

            let (v, cur_ver) = {
                let me = expect_dict(rt, recv)?;
                let v = if i >= 0 && i < crate::core::value::ELEMENTS_MAX {
                    let idx = i as usize;
                    if idx < me.elements.len() && me.elements[idx].get_tag() != crate::core::value::TAG_UNIT {
                        Some(me.elements[idx])
                    } else {
                        me.map.get(&DictKey::Int(i)).copied()
                    }
                } else {
                    me.map.get(&DictKey::Int(i)).copied()
                };
                (v, me.ver)
            };

            rt.caches.dict_version_last = Some((id, cur_ver));

            let Some(v) = v else { return Ok(rt.option_none()); };

            rt.caches.dict_cache_int_last = Some(DictCacheIntLast { id, key: i, ver: cur_ver, value: v });
            Ok(rt.option_some(v))
        }
        MethodKind::Has | MethodKind::Contains => {
            validate_arity(rt, method, args.len(), 1, 1)?;

            if kind == MethodKind::Has {
                validate_str_param(rt, &args[0], "key")?;
            }

            if args[0].get_tag() == crate::core::value::TAG_STR {
                let key_id = args[0].as_obj_id();
                let (key_str_ptr, key_str_len, dict_key_hash) = {
                    let key_str = expect_str(rt, args[0])?;
                    (key_str.as_str().as_ptr(), key_str.len(), DictKey::hash_str(key_str.as_str()))
                };
                let me = expect_dict(rt, recv)?;

                // 检查 shape
                if let Some(sid) = me.shape {
                    if let crate::core::heap::ManagedObject::Shape(shape) = rt.heap.get(sid) {
                        let key_str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_str_ptr, key_str_len)) };
                        if let Some(&off) = shape.prop_map.get(key_str) {
                            let ok = me.prop_values.get(off).is_some_and(|v| v.get_tag() != crate::core::value::TAG_UNIT);
                            return Ok(Value::from_bool(ok));
                        }
                    }
                }

                // Compute HashMap hash from DictKey hash
                use std::hash::{BuildHasher, Hasher};
                let mut h = me.map.hasher().build_hasher();
                h.write_u8(0);
                h.write_u64(dict_key_hash);
                let hash = h.finish();
                let found = me.map.raw_entry().from_hash(hash, |k| {
                    if let DictKey::StrRef { hash: kh, obj_id } = k {
                        *kh == dict_key_hash && (*obj_id == key_id.0 || *kh == dict_key_hash)
                    } else {
                        false
                    }
                }).is_some();
                Ok(Value::from_bool(found))
            } else if args[0].is_int() {
                let me = expect_dict(rt, recv)?;
                Ok(Value::from_bool(me.map.contains_key(&DictKey::Int(args[0].as_i64()))))
            } else {
                Err(err(rt, xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "text or int".to_string(),
                    actual: args[0].type_name().to_string(),
                }))
            }
        }
        MethodKind::Remove => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            let key = get_dict_key_from_value(rt, &args[0])?;
            let id = recv.as_obj_id().0;
            let me = expect_dict_mut(rt, recv)?;

            if me.map.remove(&key).is_some() {
                me.ver += 1;
                rt.caches.dict_version_last = Some((id, me.ver));
                Ok(Value::from_bool(true))
            } else {
                Ok(Value::from_bool(false))
            }
        }
        MethodKind::Clear => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let id = recv.as_obj_id().0;
            let me = expect_dict_mut(rt, recv)?;
            me.map.clear();
            me.elements.clear();
            me.prop_values.clear();
            me.ver += 1;
            rt.caches.dict_version_last = Some((id, me.ver));
            Ok(Value::UNIT)
        }
        MethodKind::DictKeys => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let keys = collect_dict_keys(rt, recv)?;
            let result: Vec<_> = keys.into_iter().map(|k| k.into_value(rt)).collect();
            Ok(create_list_value(rt, result))
        }
        MethodKind::DictValues => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let me = expect_dict(rt, recv)?;
            let mut values = Vec::with_capacity(me.map.len() + me.elements.len());
            for ev in &me.elements {
                if ev.get_tag() != crate::core::value::TAG_UNIT {
                    values.push(*ev);
                }
            }
            values.extend(me.map.values().copied());
            Ok(create_list_value(rt, values))
        }
        MethodKind::GetOrDefault => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            let key = get_dict_key_from_value(rt, &args[0])?;
            let me = expect_dict(rt, recv)?;
            Ok(me.map.get(&key).copied().unwrap_or(args[1]))
        }
        MethodKind::DictItems => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let items_data = collect_dict_items(rt, recv)?;
            let items: Vec<_> = items_data.into_iter().map(|(k, v)| {
                let key_val = k.into_value(rt);
                create_list_value(rt, vec![key_val, v])
            }).collect();
            Ok(create_list_value(rt, items))
        }
        MethodKind::Len => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let me = expect_dict(rt, recv)?;
            let mut n = me.map.len() + me.prop_values.len();
            n += me.elements.iter().filter(|ev| ev.get_tag() != crate::core::value::TAG_UNIT).count();
            Ok(Value::from_i64(n as i64))
        }
        _ => Err(err(rt, xu_syntax::DiagnosticKind::UnknownDictMethod(method.to_string()))),
    }
}

fn collect_dict_keys(rt: &Runtime, recv: Value) -> Result<Vec<TempKey>, String> {
    let me = expect_dict(rt, recv)?;
    let mut keys = Vec::with_capacity(me.map.len() + me.elements.len());

    for (i, ev) in me.elements.iter().enumerate() {
        if ev.get_tag() != crate::core::value::TAG_UNIT {
            keys.push(TempKey::Int(i as i64));
        }
    }

    for k in me.map.keys() {
        match k {
            DictKey::StrRef { obj_id, .. } => {
                // Get string from heap
                if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(crate::core::heap::ObjectId(*obj_id)) {
                    keys.push(TempKey::Str(Rc::new(s.as_str().to_string())));
                }
            }
            DictKey::Int(i) => keys.push(TempKey::Int(*i)),
        }
    }
    Ok(keys)
}

fn collect_dict_items(rt: &Runtime, recv: Value) -> Result<Vec<(TempKey, Value)>, String> {
    let me = expect_dict(rt, recv)?;
    let mut items = Vec::with_capacity(me.map.len() + me.elements.len());

    for (i, ev) in me.elements.iter().enumerate() {
        if ev.get_tag() != crate::core::value::TAG_UNIT {
            items.push((TempKey::Int(i as i64), *ev));
        }
    }

    for (k, v) in me.map.iter() {
        match k {
            DictKey::StrRef { obj_id, .. } => {
                // Get string from heap
                if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(crate::core::heap::ObjectId(*obj_id)) {
                    items.push((TempKey::Str(Rc::new(s.as_str().to_string())), *v));
                }
            }
            DictKey::Int(i) => items.push((TempKey::Int(*i), *v)),
        }
    }
    Ok(items)
}
