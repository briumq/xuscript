use crate::Value;

use super::{MethodKind, Runtime};
use crate::runtime::util::{to_i64, value_to_string};

pub(super) fn dispatch(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    args: &[Value],
    method: &str,
) -> Result<Value, String> {
    let id = recv.as_obj_id();
    let s = if let crate::gc::ManagedObject::Str(s) = rt.heap.get(id) {
        s.clone()
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())));
    };
    match kind {
        MethodKind::DictGet | MethodKind::DictGetInt => {
            // str.get(i) - safe access returning Option
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let i = to_i64(&args[0])?;
            if i < 0 {
                return Ok(rt.option_none());
            }
            let idx = i as usize;
            let str_ref = s.as_str();

            // Fast path for ASCII strings - direct byte indexing
            if s.is_ascii() {
                if idx >= str_ref.len() {
                    return Ok(rt.option_none());
                }
                let ch = &str_ref[idx..idx + 1];
                let str_val = Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                    crate::Text::from_str(ch),
                )));
                return Ok(rt.option_some(str_val));
            }

            // Slow path for non-ASCII strings
            let total = str_ref.chars().count();
            if idx >= total {
                Ok(rt.option_none())
            } else {
                let ch: String = str_ref.chars().skip(idx).take(1).collect();
                let str_val = Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                    crate::Text::from_string(ch),
                )));
                Ok(rt.option_some(str_val))
            }
        }
        MethodKind::StrFormat => {
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
            let dict_id = args[0].as_obj_id();

            let mut out = s.clone();
            if let crate::gc::ManagedObject::Dict(db) = rt.heap.get(dict_id) {
                for (k, v) in db.map.iter() {
                    let key = match k {
                        crate::value::DictKey::Str { data, .. } => crate::Text::from_str(data),
                        crate::value::DictKey::Int(i) => crate::value::i64_to_text_fast(*i),
                    };
                    let needle = format!("{{{}}}", key);
                    let repl = value_to_string(v, &rt.heap);
                    out = out.as_str().replace(&needle, &repl).into();
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            }
            Ok(Value::str(
                rt.heap.alloc(crate::gc::ManagedObject::Str(out)),
            ))
        }
        MethodKind::StrSplit => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let sep = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(x) = rt.heap.get(args[0].as_obj_id()) {
                    x.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::SplitParamRequired));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::SplitParamRequired));
            };
            let items = s
                .as_str()
                .split(&sep)
                .map(|p| {
                    Value::str(
                        rt.heap
                            .alloc(crate::gc::ManagedObject::Str(p.to_string().into())),
                    )
                })
                .collect::<Vec<_>>();
            Ok(Value::list(
                rt.heap.alloc(crate::gc::ManagedObject::List(items)),
            ))
        }
        MethodKind::StrToInt => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            let v =
                s.as_str().trim().parse::<i64>().map_err(|e| {
                    rt.error(xu_syntax::DiagnosticKind::ParseIntError(e.to_string()))
                })?;
            Ok(Value::from_i64(v))
        }
        MethodKind::StrToFloat => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            let v =
                s.as_str().trim().parse::<f64>().map_err(|e| {
                    rt.error(xu_syntax::DiagnosticKind::ParseFloatError(e.to_string()))
                })?;
            Ok(Value::from_f64(v))
        }
        MethodKind::StrToUpper => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                s.as_str().to_uppercase().into(),
            ))))
        }
        MethodKind::StrToLower => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                s.as_str().to_lowercase().into(),
            ))))
        }
        MethodKind::Contains => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let sub = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(x) = rt.heap.get(args[0].as_obj_id()) {
                    x.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                        expected: "string".to_string(),
                        actual: args[0].type_name().to_string(),
                    }));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "string".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            };
            Ok(Value::from_bool(s.as_str().contains(&sub)))
        }
        MethodKind::StrStartsWith => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let sub = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(x) = rt.heap.get(args[0].as_obj_id()) {
                    x.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                        expected: "string".to_string(),
                        actual: args[0].type_name().to_string(),
                    }));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "string".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            };
            Ok(Value::from_bool(s.as_str().starts_with(&sub)))
        }
        MethodKind::StrEndsWith => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let sub = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(x) = rt.heap.get(args[0].as_obj_id()) {
                    x.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                        expected: "string".to_string(),
                        actual: args[0].type_name().to_string(),
                    }));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: "string".to_string(),
                    actual: args[0].type_name().to_string(),
                }));
            };
            Ok(Value::from_bool(s.as_str().ends_with(&sub)))
        }
        MethodKind::StrTrim => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                s.as_str().trim().to_string().into(),
            ))))
        }
        MethodKind::StrReplace => {
            if args.len() != 2 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 2,
                    expected_max: 2,
                    actual: args.len(),
                }));
            }
            let from = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(x) = rt.heap.get(args[0].as_obj_id()) {
                    x.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::ReplaceParamRequired));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::ReplaceParamRequired));
            };
            let to = if args[1].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(x) = rt.heap.get(args[1].as_obj_id()) {
                    x.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::ReplaceParamRequired));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::ReplaceParamRequired));
            };
            Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                s.as_str().replace(&from, &to).into(),
            ))))
        }
        MethodKind::Len => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            Ok(Value::from_i64(s.char_count() as i64))
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnknownStrMethod(
            method.to_string(),
        ))),
    }
}
