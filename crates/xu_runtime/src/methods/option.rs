use crate::Value;

use super::{MethodKind, Runtime};
use super::common::*;

/// 处理 TAG_OPTION (Option#some) 的方法调用
pub(super) fn dispatch(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let inner = expect_option_some(rt, recv)?;

    match kind {
        MethodKind::OptHas => Ok(Value::from_bool(true)),
        MethodKind::OptOr => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            Ok(inner)
        }
        MethodKind::OptOrElse => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            Ok(inner)
        }
        MethodKind::OptMap => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            let f = args[0];
            let mapped = rt.call_function(f, &[inner])?;
            Ok(rt.option_some(mapped))
        }
        MethodKind::OptThen => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            let f = args[0];
            rt.call_function(f, &[inner])
        }
        MethodKind::OptEach => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            let f = args[0];
            let _ = rt.call_function(f, &[inner])?;
            Ok(Value::UNIT)
        }
        MethodKind::OptFilter => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            let pred = args[0];
            let keep = rt.call_function(pred, &[inner])?;
            if keep.is_bool() && keep.as_bool() {
                Ok(recv)
            } else {
                Ok(rt.option_none())
            }
        }
        MethodKind::OptGet | MethodKind::DictGet => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(inner)
        }
        MethodKind::ToString => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let inner_str = crate::util::value_to_string(&inner, &rt.heap);
            let s = format!("Option#some({})", inner_str);
            Ok(create_str_value(rt, &s))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "Option".to_string(),
            },
        )),
    }
}
