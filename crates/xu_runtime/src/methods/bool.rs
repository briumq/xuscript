use super::common::*;
use super::Runtime;
use crate::Value;

pub(crate) fn dispatch(rt: &mut Runtime, recv: Value, kind: super::MethodKind, args: &[Value], method: &str) -> Result<Value, String> {
    let b = recv.as_bool();

    match kind {
        super::MethodKind::ToString => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = b.to_string();
            Ok(create_str_value(rt, &s))
        }
        super::MethodKind::BoolNot => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let not_b = !b;
            Ok(Value::from_bool(not_b))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "bool".to_string(),
            },
        )),
    }
}
