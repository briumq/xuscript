use crate::Value;

use super::{MethodKind, Runtime};
use crate::value::DictKey;

pub(super) fn dispatch(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    args: &[Value],
    method: &str,
) -> Result<Value, String> {
    let id = recv.as_obj_id();
    match kind {
        MethodKind::Contains => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let key = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
                    DictKey::Str(s.clone())
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())));
                }
            } else if args[0].is_int() {
                DictKey::Int(args[0].as_i64())
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired));
            };
            if let crate::gc::ManagedObject::Set(s) = rt.heap.get(id) {
                Ok(Value::from_bool(s.map.contains_key(&key)))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a set".into())))
            }
        }
        MethodKind::Remove => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let key = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
                    DictKey::Str(s.clone())
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())));
                }
            } else if args[0].is_int() {
                DictKey::Int(args[0].as_i64())
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired));
            };
            if let crate::gc::ManagedObject::Set(s) = rt.heap.get_mut(id) {
                Ok(Value::from_bool(s.map.remove(&key).is_some()))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a set".into())))
            }
        }
        MethodKind::Clear => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::Set(s) = rt.heap.get_mut(id) {
                s.map.clear();
            }
            Ok(Value::NULL)
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: method.to_string(),
            ty: recv.type_name().to_string(),
        })),
    }
}

