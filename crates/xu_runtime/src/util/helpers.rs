use std::collections::HashSet;

use crate::Value;
use crate::core::heap::Heap;
use crate::core::value::{DictKey, i64_to_string_fast};

pub(crate) fn value_to_string(v: &Value, heap: &Heap) -> String {
    let mut visited = HashSet::new();
    value_to_string_impl(v, heap, &mut visited)
}

fn value_to_string_impl(v: &Value, heap: &Heap, visited: &mut HashSet<usize>) -> String {
    if v.is_unit() {
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
            crate::core::value::TAG_STR => {
                if let crate::core::heap::ManagedObject::Str(s) = heap.get(id) {
                    s.to_string()
                } else {
                    "".to_string()
                }
            }
            crate::core::value::TAG_LIST => {
                if visited.contains(&id.0) {
                    return "[...]".to_string();
                }
                visited.insert(id.0);
                if let crate::core::heap::ManagedObject::List(items) = heap.get(id) {
                    let strs: Vec<_> = items
                        .iter()
                        .map(|item| value_to_string_impl(item, heap, visited))
                        .collect();
                    format!("[{}]", strs.join(","))
                } else {
                    "[]".into()
                }
            }
            crate::core::value::TAG_TUPLE => {
                if visited.contains(&id.0) {
                    return "(...)".to_string();
                }
                visited.insert(id.0);
                if let crate::core::heap::ManagedObject::Tuple(items) = heap.get(id) {
                    let strs: Vec<_> = items
                        .iter()
                        .map(|item| value_to_string_impl(item, heap, visited))
                        .collect();
                    format!("({})", strs.join(","))
                } else {
                    "()".into()
                }
            }
            crate::core::value::TAG_DICT => {
                if visited.contains(&id.0) {
                    return "{...}".to_string();
                }
                visited.insert(id.0);
                if let crate::core::heap::ManagedObject::Dict(items) = heap.get(id) {
                    let strs: Vec<_> = items
                        .map
                        .iter()
                        .map(|(k, v)| {
                            let ks = match k {
                                DictKey::StrInline { .. } | DictKey::Str { .. } => k.as_str().to_string(),
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
            crate::core::value::TAG_MODULE => "module".to_string(),
            crate::core::value::TAG_STRUCT => {
                if visited.contains(&id.0) {
                    return "{...}".to_string();
                }
                visited.insert(id.0);
                if let crate::core::heap::ManagedObject::Struct(s) = heap.get(id) {
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
            crate::core::value::TAG_ENUM => {
                if let crate::core::heap::ManagedObject::Enum(e) = heap.get(id) {
                    let (ty, variant, _) = e.as_ref();
                    format!("{}#{}", ty, variant)
                } else {
                    "enum".to_string()
                }
            }
            crate::core::value::TAG_FUNC => "function".to_string(),
            crate::core::value::TAG_FILE => {
                if let crate::core::heap::ManagedObject::File(h) = heap.get(id) {
                    format!("file({})", h.path)
                } else {
                    "file".into()
                }
            }
            crate::core::value::TAG_RANGE => {
                if let crate::core::heap::ManagedObject::Range(start, end, inclusive) = heap.get(id) {
                    if *inclusive {
                        format!("[{start}..={end}]")
                    } else {
                        format!("[{start}..{end}]")
                    }
                } else {
                    "range".into()
                }
            }
            crate::core::value::TAG_BUILDER => {
                if let crate::core::heap::ManagedObject::Builder(s) = heap.get(id) {
                    s.clone()
                } else {
                    "".into()
                }
            }
            crate::core::value::TAG_OPTION => {
                if let crate::core::heap::ManagedObject::OptionSome(inner) = heap.get(id) {
                    value_to_string_impl(inner, heap, visited)
                } else {
                    "Option#some(?)".into()
                }
            }
            _ => "unknown".to_string(),
        }
    }
}

pub(crate) fn type_matches(ty: &str, v: &Value, heap: &Heap) -> bool {
    match ty {
        "any" => true,
        "int" => v.is_int(),
        "float" => v.is_f64() || v.is_int(),
        "string" => v.get_tag() == crate::core::value::TAG_STR,
        "bool" | "?" => v.is_bool(),
        "list" => v.get_tag() == crate::core::value::TAG_LIST,
        "dict" => v.get_tag() == crate::core::value::TAG_DICT,
        "tuple" => v.get_tag() == crate::core::value::TAG_TUPLE,
        "module" => v.get_tag() == crate::core::value::TAG_MODULE,
        "range" => v.get_tag() == crate::core::value::TAG_RANGE,
        "file" => v.get_tag() == crate::core::value::TAG_FILE,
        "unit" => v.is_unit(),
        _ => {
            let tag = v.get_tag();
            // Option type: Option#none is an enum, Option#some has TAG_OPTION
            if ty == "Option" || ty.starts_with("Option[") {
                tag == crate::core::value::TAG_OPTION || {
                    // Option#none is represented as an enum
                    if tag == crate::core::value::TAG_ENUM {
                        if let crate::core::heap::ManagedObject::Enum(e) = heap.get(v.as_obj_id()) {
                            let (ety, _, _) = e.as_ref();
                            ety.as_str() == "Option"
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            } else if tag == crate::core::value::TAG_STRUCT {
                if let crate::core::heap::ManagedObject::Struct(s) = heap.get(v.as_obj_id()) {
                    s.ty == ty
                } else {
                    false
                }
            } else if tag == crate::core::value::TAG_ENUM {
                if let crate::core::heap::ManagedObject::Enum(e) = heap.get(v.as_obj_id()) {
                    let (ety, _, _) = e.as_ref();
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

pub(crate) fn to_i64(v: &Value) -> Result<i64, String> {
    if v.is_int() {
        Ok(v.as_i64())
    } else if v.is_f64() {
        Ok(v.as_f64() as i64)
    } else {
        Err(format!("[E0003] Expected number, got {}", v.type_name()))
    }
}


