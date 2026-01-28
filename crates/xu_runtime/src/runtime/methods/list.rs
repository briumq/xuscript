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
    match kind {
        MethodKind::ListAdd => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::List(list) = rt.heap.get_mut(id) {
                list.push(args[0].clone());
            }
            Ok(Value::NULL)
        }
        MethodKind::Remove => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let i = to_i64(&args[0])?;
            if i < 0 {
                return Err(rt.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
            }
            if let crate::gc::ManagedObject::List(list) = rt.heap.get_mut(id) {
                let ui = i as usize;
                if ui >= list.len() {
                    return Err(rt.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                Ok(list.remove(ui))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        }
        MethodKind::Contains => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::List(list) = rt.heap.get(id) {
                let mut found = false;
                for v in list.iter() {
                    if rt.values_equal(v, &args[0]) {
                        found = true;
                        break;
                    }
                }
                Ok(Value::from_bool(found))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        }
        MethodKind::ListPop => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::List(list) = rt.heap.get_mut(id) {
                Ok(list.pop().unwrap_or(Value::NULL))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
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
            if let crate::gc::ManagedObject::List(list) = rt.heap.get_mut(id) {
                list.clear();
            }
            Ok(Value::NULL)
        }
        MethodKind::ListReverse => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::List(list) = rt.heap.get_mut(id) {
                list.reverse();
            }
            Ok(Value::NULL)
        }
        MethodKind::ListJoin => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let sep = if args[0].get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
                    s.as_str().to_string()
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::JoinParamRequired));
                }
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::JoinParamRequired));
            };
            if let crate::gc::ManagedObject::List(list) = rt.heap.get(id) {
                let strs: Vec<String> = list
                    .iter()
                    .map(|item| value_to_string(item, &rt.heap))
                    .collect();
                Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(
                    strs.join(&sep).into(),
                ))))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        }
        MethodKind::Len => {
            if !args.is_empty() {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 0,
                    expected_max: 0,
                    actual: args.len(),
                }));
            }
            if let crate::gc::ManagedObject::List(list) = rt.heap.get(id) {
                Ok(Value::from_i64(list.len() as i64))
            } else {
                Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnknownListMethod(
            method.to_string(),
        ))),
    }
}
