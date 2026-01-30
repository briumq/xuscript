use std::collections::HashSet;

use crate::Value;
use crate::gc::Heap;
use crate::value::{DictKey, i64_to_string_fast};

pub(super) fn value_to_string(v: &Value, heap: &Heap) -> String {
    let mut visited = HashSet::new();
    value_to_string_impl(v, heap, &mut visited)
}

fn value_to_string_impl(v: &Value, heap: &Heap, visited: &mut HashSet<usize>) -> String {
    if v.is_void() {
        "()".to_string()
    } else if v.is_bool() {
        if v.as_bool() {
            "true".to_string()
        } else {
            "false".to_string()
        }
    } else if v.is_int() {
        i64_to_string_fast(v.as_i64())
    } else if v.is_f64() {
        let f = v.as_f64();
        if f.fract() == 0.0 {
            i64_to_string_fast(f as i64)
        } else {
            f.to_string()
        }
    } else {
        let tag = v.get_tag();
        let id = v.as_obj_id();
        match tag {
            crate::value::TAG_STR => {
                if let crate::gc::ManagedObject::Str(s) = heap.get(id) {
                    s.to_string()
                } else {
                    "".to_string()
                }
            }
            crate::value::TAG_LIST => {
                if visited.contains(&id.0) {
                    return "[...]".to_string();
                }
                visited.insert(id.0);
                if let crate::gc::ManagedObject::List(items) = heap.get(id) {
                    let strs: Vec<_> = items
                        .iter()
                        .map(|item| value_to_string_impl(item, heap, visited))
                        .collect();
                    format!("[{}]", strs.join(","))
                } else {
                    "[]".into()
                }
            }
            crate::value::TAG_TUPLE => {
                if visited.contains(&id.0) {
                    return "(...)".to_string();
                }
                visited.insert(id.0);
                if let crate::gc::ManagedObject::Tuple(items) = heap.get(id) {
                    let strs: Vec<_> = items
                        .iter()
                        .map(|item| value_to_string_impl(item, heap, visited))
                        .collect();
                    format!("({})", strs.join(","))
                } else {
                    "()".into()
                }
            }
            crate::value::TAG_DICT => {
                if visited.contains(&id.0) {
                    return "{...}".to_string();
                }
                visited.insert(id.0);
                if let crate::gc::ManagedObject::Dict(items) = heap.get(id) {
                    let strs: Vec<_> = items
                        .map
                        .iter()
                        .map(|(k, v)| {
                            let ks = match k {
                                DictKey::Str(s) => s.to_string(),
                                DictKey::Int(i) => i.to_string(),
                            };
                            format!("\"{}\":{}", ks, value_to_string_impl(v, heap, visited))
                        })
                        .collect();
                    format!("{{{}}}", strs.join(","))
                } else {
                    "{}".into()
                }
            }
            crate::value::TAG_MODULE => "module".to_string(),
            crate::value::TAG_STRUCT => {
                if visited.contains(&id.0) {
                    return "{...}".to_string();
                }
                visited.insert(id.0);
                if let crate::gc::ManagedObject::Struct(s) = heap.get(id) {
                    let mut strs = Vec::with_capacity(s.fields.len());
                    for i in 0..s.fields.len() {
                        let k = &s.field_names[i];
                        let v = &s.fields[i];
                        strs.push(format!("{}:{}", k, value_to_string_impl(v, heap, visited)));
                    }
                    format!("{}{{{}}}", s.ty, strs.join(","))
                } else {
                    "struct".into()
                }
            }
            crate::value::TAG_ENUM => {
                if let crate::gc::ManagedObject::Enum(ty, variant, _) = heap.get(id) {
                    format!("{}#{}", ty, variant)
                } else {
                    "enum".to_string()
                }
            }
            crate::value::TAG_FUNC => "function".to_string(),
            crate::value::TAG_FILE => {
                if let crate::gc::ManagedObject::File(h) = heap.get(id) {
                    format!("file({})", h.path)
                } else {
                    "file".into()
                }
            }
            crate::value::TAG_RANGE => {
                if let crate::gc::ManagedObject::Range(start, end, inclusive) = heap.get(id) {
                    if *inclusive {
                        format!("[{start}..={end}]")
                    } else {
                        format!("[{start}..{end}]")
                    }
                } else {
                    "range".into()
                }
            }
            crate::value::TAG_BUILDER => {
                if let crate::gc::ManagedObject::Builder(s) = heap.get(id) {
                    s.clone()
                } else {
                    "".into()
                }
            }
            crate::value::TAG_OPTION => {
                if let crate::gc::ManagedObject::OptionSome(inner) = heap.get(id) {
                    value_to_string_impl(inner, heap, visited)
                } else {
                    "Option#some(?)".into()
                }
            }
            _ => "unknown".to_string(),
        }
    }
}

pub(super) fn type_matches(ty: &str, v: &Value, heap: &Heap) -> bool {
    match ty {
        "any" => true,
        "int" => v.is_int(),
        "float" => v.is_f64() || v.is_int(),
        "string" => v.get_tag() == crate::value::TAG_STR,
        "bool" | "?" => v.is_bool(),
        "list" => v.get_tag() == crate::value::TAG_LIST,
        "dict" => v.get_tag() == crate::value::TAG_DICT,
        "tuple" => v.get_tag() == crate::value::TAG_TUPLE,
        "module" => v.get_tag() == crate::value::TAG_MODULE,
        "range" => v.get_tag() == crate::value::TAG_RANGE,
        "file" => v.get_tag() == crate::value::TAG_FILE,
        "void" => v.is_void(),
        _ => {
            let tag = v.get_tag();
            if tag == crate::value::TAG_STRUCT {
                if let crate::gc::ManagedObject::Struct(s) = heap.get(v.as_obj_id()) {
                    s.ty == ty
                } else {
                    false
                }
            } else if tag == crate::value::TAG_ENUM {
                if let crate::gc::ManagedObject::Enum(ety, _, _) = heap.get(v.as_obj_id()) {
                    ety.as_str() == ty
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
}

pub(super) fn to_i64(v: &Value) -> Result<i64, String> {
    if v.is_int() {
        Ok(v.as_i64())
    } else if v.is_f64() {
        Ok(v.as_f64() as i64)
    } else {
        Err(format!("[E0003] Expected number, got {}", v.type_name()))
    }
}

pub(super) fn to_f64_pair(a: &Value, b: &Value) -> Result<(f64, f64), String> {
    let x = if a.is_int() {
        a.as_i64() as f64
    } else if a.is_f64() {
        a.as_f64()
    } else {
        return Err(format!("Expected number, got {}", a.type_name()));
    };
    let y = if b.is_int() {
        b.as_i64() as f64
    } else if b.is_f64() {
        b.as_f64()
    } else {
        return Err(format!("[E0003] Expected number, got {}", b.type_name()));
    };
    Ok((x, y))
}
