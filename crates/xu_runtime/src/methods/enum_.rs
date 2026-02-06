use crate::core::heap::ManagedObject;
use crate::Text;
use crate::Value;

use super::common::validate_arity;
use super::MethodKind;
use super::Runtime;

fn enum_new(rt: &mut Runtime, ty: &str, variant: &str, payload: Vec<Value>) -> Value {
    Value::enum_obj(rt.alloc(ManagedObject::Enum(Box::new((
        ty.to_string().into(),
        variant.to_string().into(),
        payload.into_boxed_slice(),
    )))))
}

fn unsupported_method(rt: &Runtime, method: &str, ty: &Text) -> String {
    rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
        method: method.to_string(),
        ty: ty.as_str().to_string(),
    })
}

fn get_payload(rt: &Runtime, payload: &[Value], ty: &str, variant: &str) -> Result<Value, String> {
    payload.first().copied().ok_or_else(|| {
        rt.error(xu_syntax::DiagnosticKind::Raw(
            format!("{ty}#{variant} missing value"),
        ))
    })
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
    let var = variant.as_str();

    // 通用枚举方法
    match kind {
        MethodKind::EnumName => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            return Ok(Value::str(rt.alloc(ManagedObject::Str(variant))));
        }
        MethodKind::EnumTypeName => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            return Ok(Value::str(rt.alloc(ManagedObject::Str(ty))));
        }
        MethodKind::ToString => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            return Ok(Value::str(rt.alloc(ManagedObject::Str(
                format!("{}#{}", ty, variant).into(),
            ))));
        }
        _ => {}
    }

    // Option/Result 特有方法
    let is_option = ty.as_str() == "Option";
    let is_result = ty.as_str() == "Result";
    if !is_option && !is_result {
        return Err(unsupported_method(rt, method, &ty));
    }

    // 定义成功/失败变体名
    let (success_var, fail_var) = if is_option {
        ("some", "none")
    } else {
        ("ok", "err")
    };
    let is_success = var == success_var;
    let is_fail = var == fail_var;

    match kind {
        MethodKind::Has => {
            if !is_option {
                return Err(unsupported_method(rt, method, &ty));
            }
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(Value::from_bool(is_success))
        }
        MethodKind::Or => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            if is_success {
                get_payload(rt, &payload, ty.as_str(), var)
            } else if is_fail {
                Ok(args[0])
            } else {
                Err(unsupported_method(rt, method, &ty))
            }
        }
        MethodKind::OrElse => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            if is_success {
                get_payload(rt, &payload, ty.as_str(), var)
            } else if is_fail {
                rt.call_function(args[0], &[])
            } else {
                Err(unsupported_method(rt, method, &ty))
            }
        }
        MethodKind::Map => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            if is_fail {
                return Ok(if is_option {
                    enum_new(rt, "Option", "none", Vec::new())
                } else {
                    recv
                });
            }
            if is_success {
                let v = get_payload(rt, &payload, ty.as_str(), var)?;
                let mapped = rt.call_function(args[0], &[v])?;
                return Ok(enum_new(
                    rt,
                    ty.as_str(),
                    success_var,
                    vec![mapped],
                ));
            }
            Err(unsupported_method(rt, method, &ty))
        }
        MethodKind::MapErr => {
            if !is_result {
                return Err(unsupported_method(rt, method, &ty));
            }
            validate_arity(rt, method, args.len(), 1, 1)?;
            if var == "ok" {
                return Ok(recv);
            }
            if var == "err" {
                let e = get_payload(rt, &payload, "Result", "err")?;
                let mapped = rt.call_function(args[0], &[e])?;
                return Ok(enum_new(rt, "Result", "err", vec![mapped]));
            }
            Err(unsupported_method(rt, method, &ty))
        }
        MethodKind::Then => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            if is_fail {
                return Ok(if is_option {
                    enum_new(rt, "Option", "none", Vec::new())
                } else {
                    recv
                });
            }
            if is_success {
                let v = get_payload(rt, &payload, ty.as_str(), var)?;
                return rt.call_function(args[0], &[v]);
            }
            Err(unsupported_method(rt, method, &ty))
        }
        MethodKind::Each => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            if is_success {
                let v = get_payload(rt, &payload, ty.as_str(), var)?;
                let _ = rt.call_function(args[0], &[v])?;
            } else if !is_fail {
                return Err(unsupported_method(rt, method, &ty));
            }
            Ok(Value::UNIT)
        }
        MethodKind::Filter => {
            if !is_option {
                return Err(unsupported_method(rt, method, &ty));
            }
            validate_arity(rt, method, args.len(), 1, 1)?;
            if var == "none" {
                return Ok(enum_new(rt, "Option", "none", Vec::new()));
            }
            if var == "some" {
                let v = get_payload(rt, &payload, "Option", "some")?;
                let keep = rt.call_function(args[0], &[v])?;
                if keep.is_bool() && keep.as_bool() {
                    return Ok(recv);
                }
                return Ok(enum_new(rt, "Option", "none", Vec::new()));
            }
            Err(unsupported_method(rt, method, &ty))
        }
        MethodKind::Get => {
            if !is_option {
                return Err(unsupported_method(rt, method, &ty));
            }
            validate_arity(rt, method, args.len(), 0, 0)?;
            if var == "some" {
                get_payload(rt, &payload, "Option", "some")
            } else if var == "none" {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw(
                    format!("Called {method}() on None value"),
                )))
            } else {
                Err(unsupported_method(rt, method, &ty))
            }
        }
        _ => Err(unsupported_method(rt, method, &ty)),
    }
}
