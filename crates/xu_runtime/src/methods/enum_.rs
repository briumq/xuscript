use crate::core::heap::ManagedObject;
use crate::Value;

use super::MethodKind;
use super::Runtime;

fn enum_new(rt: &mut Runtime, ty: &str, variant: &str, payload: Vec<Value>) -> Value {
    Value::enum_obj(rt.heap.alloc(crate::core::heap::ManagedObject::Enum(Box::new((
        ty.to_string().into(),
        variant.to_string().into(),
        payload.into_boxed_slice(),
    )))))
}

pub(super) fn dispatch(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    args: &[Value],
    method: &str,
) -> Result<Value, String> {
    if recv.get_tag() != crate::core::value::TAG_ENUM {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedReceiver(
            recv.type_name().to_string(),
        )));
    }
    let (ty, variant, payload) = rt.enum_parts_cloned(recv)?;

    // 通用枚举方法（所有枚举都支持）
    match kind {
        MethodKind::EnumName => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            return Ok(Value::str(rt.heap.alloc(ManagedObject::Str(variant))));
        }
        MethodKind::EnumTypeName => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            return Ok(Value::str(rt.heap.alloc(ManagedObject::Str(ty))));
        }
        MethodKind::ToString => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            return Ok(Value::str(rt.heap.alloc(ManagedObject::Str(format!("{}#{}", ty, variant).into()))));
        }
        _ => {}
    }

    // Option/Result 特有方法
    let is_option = ty.as_str() == "Option";
    let is_result = ty.as_str() == "Result";
    if !is_option && !is_result {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: method.to_string(),
            ty: ty.as_str().to_string(),
        }));
    }

    match kind {
        MethodKind::OptHas => {
            if !is_option || !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }));
            }
            Ok(Value::from_bool(variant.as_str() == "some"))
        }
        MethodKind::OptOr => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            if is_option {
                if variant.as_str() == "some" {
                    payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Option#some missing value".into(),
                        ))
                    })
                } else if variant.as_str() == "none" {
                    Ok(args[0])
                } else {
                    Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: method.to_string(),
                        ty: ty.as_str().to_string(),
                    }))
                }
            } else {
                if variant.as_str() == "ok" {
                    payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Result#ok missing value".into(),
                        ))
                    })
                } else if variant.as_str() == "err" {
                    Ok(args[0])
                } else {
                    Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: method.to_string(),
                        ty: ty.as_str().to_string(),
                    }))
                }
            }
        }
        MethodKind::OptOrElse => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let f = args[0];
            if is_option {
                if variant.as_str() == "some" {
                    return payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Option#some missing value".into(),
                        ))
                    });
                }
                if variant.as_str() == "none" {
                    return rt.call_function(f, &[]);
                }
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            } else {
                if variant.as_str() == "ok" {
                    return payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Result#ok missing value".into(),
                        ))
                    });
                }
                if variant.as_str() == "err" {
                    return rt.call_function(f, &[]);
                }
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            }
        }
        MethodKind::OptMap => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let f = args[0];
            if is_option {
                if variant.as_str() == "none" {
                    return Ok(enum_new(rt, "Option", "none", Vec::new()));
                }
                if variant.as_str() == "some" {
                    let v = payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Option#some missing value".into(),
                        ))
                    })?;
                    let mapped = rt.call_function(f, &[v])?;
                    return Ok(enum_new(rt, "Option", "some", vec![mapped]));
                }
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            } else {
                if variant.as_str() == "err" {
                    return Ok(recv);
                }
                if variant.as_str() == "ok" {
                    let v = payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Result#ok missing value".into(),
                        ))
                    })?;
                    let mapped = rt.call_function(f, &[v])?;
                    return Ok(enum_new(rt, "Result", "ok", vec![mapped]));
                }
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            }
        }
        MethodKind::ResMapErr => {
            if !is_result {
                return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }));
            }
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let f = args[0];
            if variant.as_str() == "ok" {
                return Ok(recv);
            }
            if variant.as_str() == "err" {
                let e = payload.get(0).cloned().ok_or_else(|| {
                    rt.error(xu_syntax::DiagnosticKind::Raw(
                        "Result#err missing value".into(),
                    ))
                })?;
                let mapped = rt.call_function(f, &[e])?;
                return Ok(enum_new(rt, "Result", "err", vec![mapped]));
            }
            Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: ty.as_str().to_string(),
            }))
        }
        MethodKind::OptThen => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let f = args[0];
            if is_option {
                if variant.as_str() == "none" {
                    return Ok(enum_new(rt, "Option", "none", Vec::new()));
                }
                if variant.as_str() == "some" {
                    let v = payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Option#some missing value".into(),
                        ))
                    })?;
                    return rt.call_function(f, &[v]);
                }
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            } else {
                if variant.as_str() == "err" {
                    return Ok(recv);
                }
                if variant.as_str() == "ok" {
                    let v = payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Result#ok missing value".into(),
                        ))
                    })?;
                    return rt.call_function(f, &[v]);
                }
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            }
        }
        MethodKind::OptEach => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let f = args[0];
            if is_option {
                if variant.as_str() == "some" {
                    let v = payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Option#some missing value".into(),
                        ))
                    })?;
                    let _ = rt.call_function(f, &[v])?;
                } else if variant.as_str() != "none" {
                    return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: method.to_string(),
                        ty: ty.as_str().to_string(),
                    }));
                }
                Ok(Value::UNIT)
            } else {
                if variant.as_str() == "ok" {
                    let v = payload.get(0).cloned().ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::Raw(
                            "Result#ok missing value".into(),
                        ))
                    })?;
                    let _ = rt.call_function(f, &[v])?;
                    Ok(Value::UNIT)
                } else if variant.as_str() == "err" {
                    Ok(Value::UNIT)
                } else {
                    Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: method.to_string(),
                        ty: ty.as_str().to_string(),
                    }))
                }
            }
        }
        MethodKind::OptFilter => {
            if !is_option {
                return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }));
            }
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let pred = args[0];
            if variant.as_str() == "none" {
                return Ok(enum_new(rt, "Option", "none", Vec::new()));
            }
            if variant.as_str() == "some" {
                let v = payload.get(0).cloned().ok_or_else(|| {
                    rt.error(xu_syntax::DiagnosticKind::Raw(
                        "Option#some missing value".into(),
                    ))
                })?;
                let keep = rt.call_function(pred, &[v])?;
                if keep.is_bool() && keep.as_bool() {
                    return Ok(recv);
                }
                return Ok(enum_new(rt, "Option", "none", Vec::new()));
            }
            Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: ty.as_str().to_string(),
            }))
        }
        MethodKind::OptGet | MethodKind::DictGet => {
            // get() for Option - OptGet is mapped from "get" method name
            if !is_option {
                return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }));
            }
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if variant.as_str() == "some" {
                payload.get(0).cloned().ok_or_else(|| {
                    rt.error(xu_syntax::DiagnosticKind::Raw(
                        "Option#some missing value".into(),
                    ))
                })
            } else if variant.as_str() == "none" {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw(
                    format!("Called {}() on None value", method).into(),
                )))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                    method: method.to_string(),
                    ty: ty.as_str().to_string(),
                }))
            }
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: method.to_string(),
            ty: ty.as_str().to_string(),
        })),
    }
}
