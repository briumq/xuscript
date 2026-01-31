use xu_ir::Pattern;

use super::Runtime;
use crate::Value;

pub(super) fn match_pattern(
    rt: &mut Runtime,
    pat: &Pattern,
    v: &Value,
) -> Option<Vec<(String, Value)>> {
    match pat {
        Pattern::Wildcard => Some(Vec::new()),
        Pattern::Bind(name) => Some(vec![(name.clone(), v.clone())]),
        Pattern::Tuple(items) => {
            if v.get_tag() != crate::value::TAG_TUPLE {
                return None;
            }
            let elems: Vec<Value> =
                if let crate::gc::ManagedObject::Tuple(xs) = rt.heap.get(v.as_obj_id()) {
                    if xs.len() != items.len() {
                        return None;
                    }
                    xs.iter().cloned().collect()
                } else {
                    return None;
                };
            let mut out: Vec<(String, Value)> = Vec::new();
            for (p, val) in items.iter().zip(elems.iter()) {
                let bindings = match_pattern(rt, p, val)?;
                out.extend(bindings);
            }
            Some(out)
        }
        Pattern::Int(i) => {
            if v.is_int() && v.as_i64() == *i {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::Float(f) => {
            if v.is_f64() && v.as_f64() == *f {
                Some(Vec::new())
            } else if v.is_int() && (v.as_i64() as f64) == *f {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::Str(s) => {
            if v.get_tag() != crate::value::TAG_STR {
                return None;
            }
            if let crate::gc::ManagedObject::Str(x) = rt.heap.get(v.as_obj_id()) {
                if x.as_str() == s.as_str() {
                    Some(Vec::new())
                } else {
                    None
                }
            } else {
                None
            }
        }
        Pattern::Bool(b) => {
            if v.is_bool() && v.as_bool() == *b {
                Some(Vec::new())
            } else {
                None
            }
        }
        Pattern::EnumVariant { ty, variant, args } => {
            // Handle optimized OptionSome variant
            if v.get_tag() == crate::value::TAG_OPTION {
                if ty == "Option" && variant == "some" && args.len() == 1 {
                    if let crate::gc::ManagedObject::OptionSome(inner) = rt.heap.get(v.as_obj_id()) {
                        let inner_val = *inner;
                        return match_pattern(rt, &args[0], &inner_val);
                    }
                }
                return None;
            }

            if v.get_tag() != crate::value::TAG_ENUM {
                return None;
            }
            let payload_vals: Vec<Value> =
                if let crate::gc::ManagedObject::Enum(e) = rt.heap.get(v.as_obj_id())
                {
                    let (ety, ev, payload) = e.as_ref();
                    if ety.as_str() != ty.as_str() || ev.as_str() != variant.as_str() {
                        return None;
                    }
                    if payload.len() != args.len() {
                        return None;
                    }
                    payload.iter().cloned().collect()
                } else {
                    return None;
                };

            let mut out: Vec<(String, Value)> = Vec::new();
            for (p, val) in args.iter().zip(payload_vals.iter()) {
                let bindings = match_pattern(rt, p, val)?;
                out.extend(bindings);
            }
            Some(out)
        }
    }
}
