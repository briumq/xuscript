use super::super::Runtime;
use crate::Value;
use crate::core::value::i64_to_text_fast;

pub fn builtin_parse_int(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("parse_int expects 1 argument".into());
    }
    let v = &args[0];
    if v.get_tag() == crate::core::value::TAG_STR {
        if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(v.as_obj_id()) {
            let ss = s.trim();
            if let Ok(v) = ss.parse::<i64>() {
                Ok(Value::from_i64(v))
            } else if let Ok(fv) = ss.parse::<f64>() {
                Ok(Value::from_i64(fv as i64))
            } else {
                Err("parse_int expects numeric text".into())
            }
        } else {
            Err("parse_int expects text".into())
        }
    } else if v.is_int() {
        Ok(Value::from_i64(v.as_i64()))
    } else if v.is_f64() {
        Ok(Value::from_i64(v.as_f64() as i64))
    } else {
        Err("parse_int expects text or number".into())
    }
}

pub fn builtin_parse_float(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("parse_float expects 1 argument".into());
    }
    let v = &args[0];
    if v.get_tag() == crate::core::value::TAG_STR {
        if let crate::core::heap::ManagedObject::Str(s) = rt.heap.get(v.as_obj_id()) {
            let ss = s.trim();
            let v = ss
                .parse::<f64>()
                .map_err(|_| "parse_float expects numeric text".to_string())?;
            Ok(Value::from_f64(v))
        } else {
            Err("parse_float expects text".into())
        }
    } else if v.is_int() {
        Ok(Value::from_f64(v.as_i64() as f64))
    } else if v.is_f64() {
        Ok(Value::from_f64(v.as_f64()))
    } else {
        Err("parse_float expects text or number".into())
    }
}

pub fn builtin_to_text(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("to_text expects 1 argument".into());
    }
    let v = &args[0];
    // Fast path for small integers - use cached strings
    if v.is_int() {
        let i = v.as_i64();
        if let Some(cached) = rt.get_small_int_string(i) {
            return Ok(cached);
        }
        // Fall through to normal path for large integers
        let s = i64_to_text_fast(i);
        return Ok(Value::str(rt.heap.alloc(crate::core::heap::ManagedObject::Str(s))));
    }
    let s = if v.get_tag() == crate::core::value::TAG_STR {
        if let crate::core::heap::ManagedObject::Str(x) = rt.heap.get(v.as_obj_id()) {
            x.clone()
        } else {
            "text".into()
        }
    } else if v.is_unit() {
        "()".into()
    } else if v.is_bool() {
        if v.as_bool() {
            "true".into()
        } else {
            "false".into()
        }
    } else if v.is_f64() {
        let f = v.as_f64();
        if f.fract() == 0.0 {
            i64_to_text_fast(f as i64)
        } else {
            f.to_string().into()
        }
    } else {
        super::super::util::value_to_string(v, &rt.heap).into()
    };
    Ok(Value::str(rt.heap.alloc(crate::core::heap::ManagedObject::Str(s))))
}
