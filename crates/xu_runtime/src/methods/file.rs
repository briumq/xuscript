use crate::Value;

use super::{MethodKind, Runtime};

pub(super) fn dispatch(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    _args: &[Value],
    method: &str,
) -> Result<Value, String> {
    let id = recv.as_obj_id();
    match kind {
        MethodKind::FileRead => {
            let (open, path) = if let crate::core::heap::ManagedObject::File(h) = rt.heap.get(id) {
                (h.open, h.path.clone())
            } else {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a file".into())));
            };
            if !open {
                return Err(rt.error(xu_syntax::DiagnosticKind::FileClosed));
            }
            let content = rt.fs_read_to_string(&path)?;
            Ok(Value::str(rt.heap.alloc(crate::core::heap::ManagedObject::Str(
                content.trim_end_matches(['\n', '\r']).to_string().into(),
            ))))
        }
        MethodKind::FileClose => {
            if let crate::core::heap::ManagedObject::File(h) = rt.heap.get_mut(id) {
                h.open = false;
            }
            Ok(Value::UNIT)
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnknownFileMethod(
            method.to_string(),
        ))),
    }
}
