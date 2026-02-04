use crate::Value;

use super::{MethodKind, Runtime};
use super::common::*;

pub(super) fn dispatch(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let id = recv.as_obj_id();
    let tuple = if let crate::core::heap::ManagedObject::Tuple(t) = rt.heap.get(id) {
        t
    } else {
        return Err(err(rt, xu_syntax::DiagnosticKind::Raw("Not a tuple".into())));
    };

    match kind {
        MethodKind::Len => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(Value::from_i64(tuple.len() as i64))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "tuple".to_string(),
            },
        )),
    }
}
