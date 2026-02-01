use super::super::Runtime;
use crate::Value;

pub fn builtin_contains(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("contains expects 2 arguments".into());
    }
    let hay = &args[0];
    let needle = &args[1];
    if hay.get_tag() != crate::core::value::TAG_STR || needle.get_tag() != crate::core::value::TAG_STR {
        return Err("contains expects (text, text)".into());
    }
    let hay_id = hay.as_obj_id();
    let needle_id = needle.as_obj_id();

    // Get references without cloning
    let result = if let (
        crate::core::gc::ManagedObject::Str(hs),
        crate::core::gc::ManagedObject::Str(ns),
    ) = (rt.heap.get(hay_id), rt.heap.get(needle_id)) {
        hs.as_str().contains(ns.as_str())
    } else {
        return Err("contains expects text".into());
    };
    Ok(Value::from_bool(result))
}

pub fn builtin_starts_with(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("starts_with expects 2 arguments".into());
    }
    let hay = &args[0];
    let prefix = &args[1];
    if hay.get_tag() != crate::core::value::TAG_STR || prefix.get_tag() != crate::core::value::TAG_STR {
        return Err("starts_with expects (text, text)".into());
    }
    let hay_id = hay.as_obj_id();
    let prefix_id = prefix.as_obj_id();

    // Get references without cloning
    let result = if let (
        crate::core::gc::ManagedObject::Str(hs),
        crate::core::gc::ManagedObject::Str(ps),
    ) = (rt.heap.get(hay_id), rt.heap.get(prefix_id)) {
        hs.as_str().starts_with(ps.as_str())
    } else {
        return Err("starts_with expects text".into());
    };
    Ok(Value::from_bool(result))
}

pub fn builtin_ends_with(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("ends_with expects 2 arguments".into());
    }
    let hay = &args[0];
    let suffix = &args[1];
    if hay.get_tag() != crate::core::value::TAG_STR || suffix.get_tag() != crate::core::value::TAG_STR {
        return Err("ends_with expects (text, text)".into());
    }
    let hay_id = hay.as_obj_id();
    let suffix_id = suffix.as_obj_id();

    // Get references without cloning
    let result = if let (
        crate::core::gc::ManagedObject::Str(hs),
        crate::core::gc::ManagedObject::Str(ss),
    ) = (rt.heap.get(hay_id), rt.heap.get(suffix_id)) {
        hs.as_str().ends_with(ss.as_str())
    } else {
        return Err("ends_with expects text".into());
    };
    Ok(Value::from_bool(result))
}

/// Create a set (dict with unit values) from a list
pub fn builtin_set_from_list(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("__set_from_list expects 1 argument".into());
    }
    let list = &args[0];
    if list.get_tag() != crate::core::value::TAG_LIST {
        return Err("__set_from_list expects list".into());
    }
    let items = if let crate::core::gc::ManagedObject::List(items) = rt.heap.get(list.as_obj_id()) {
        items.clone()
    } else {
        return Err("__set_from_list expects list".into());
    };

    let mut dict = crate::core::value::dict_with_capacity(items.len());
    for item in items {
        let key = if item.get_tag() == crate::core::value::TAG_STR {
            if let crate::core::gc::ManagedObject::Str(s) = rt.heap.get(item.as_obj_id()) {
                crate::core::value::DictKey::from_text(s)
            } else {
                return Err("Invalid set item".into());
            }
        } else if item.is_int() {
            crate::core::value::DictKey::Int(item.as_i64())
        } else {
            return Err("Set items must be int or string".into());
        };
        dict.map.insert(key, Value::VOID);
    }

    Ok(Value::dict(rt.heap.alloc(crate::core::gc::ManagedObject::Dict(dict))))
}
