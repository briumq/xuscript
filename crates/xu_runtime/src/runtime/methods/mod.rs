use crate::Value;

use super::Runtime;

mod dict;
mod enum_;
mod file;
mod list;
mod str;
mod common;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MethodKind {
    ListAdd,
    ListPop,
    ListReverse,
    ListJoin,
    DictMerge,
    DictInsert,
    DictInsertInt,
    DictGet,
    DictGetInt,
    DictKeys,
    DictValues,
    FileRead,
    FileClose,
    StrFormat,
    StrSplit,
    StrToInt,
    StrToFloat,
    StrReplace,
    StrTrim,
    StrToUpper,
    StrToLower,
    StrStartsWith,
    StrEndsWith,
    OptOr,
    OptOrElse,
    OptMap,
    OptThen,
    OptEach,
    OptFilter,
    OptHas,
    OptNone,
    ResMapErr,
    Len,
    Contains,
    Clear,
    Remove,
    Unknown,
}

impl Default for MethodKind {
    fn default() -> Self {
        Self::Unknown
    }
}

impl MethodKind {
    pub(crate) fn from_str(s: &str) -> Self {
        match s {
            "add" => Self::ListAdd,
            "pop" => Self::ListPop,
            "reverse" => Self::ListReverse,
            "join" => Self::ListJoin,
            "len" | "length" => Self::Len,
            "contains" => Self::Contains,
            "clear" => Self::Clear,
            "remove" => Self::Remove,
            "merge" => Self::DictMerge,
            "insert" => Self::DictInsert,
            "insert_int" => Self::DictInsertInt,
            "get" => Self::DictGet,
            "get_int" => Self::DictGetInt,
            "keys" => Self::DictKeys,
            "values" => Self::DictValues,
            "read" => Self::FileRead,
            "close" => Self::FileClose,
            "format" => Self::StrFormat,
            "split" => Self::StrSplit,
            "to_int" => Self::StrToInt,
            "to_float" => Self::StrToFloat,
            "replace" => Self::StrReplace,
            "trim" => Self::StrTrim,
            "to_upper" => Self::StrToUpper,
            "to_lower" => Self::StrToLower,
            "starts" | "starts_with" => Self::StrStartsWith,
            "ends" | "ends_with" => Self::StrEndsWith,
            "or" => Self::OptOr,
            "or_else" => Self::OptOrElse,
            "map" => Self::OptMap,
            "then" => Self::OptThen,
            "each" => Self::OptEach,
            "filter" => Self::OptFilter,
            "has" => Self::OptHas,
            "none" => Self::OptNone,
            "map_err" => Self::ResMapErr,
            _ => Self::Unknown,
        }
    }
}

pub(super) fn dispatch_builtin_method(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    args: &[Value],
    method: &str,
) -> Result<Value, String> {
    let tag = recv.get_tag();
    if tag == crate::value::TAG_LIST {
        return list::dispatch(rt, recv, kind, args, method);
    }
    if tag == crate::value::TAG_DICT {
        return dict::dispatch(rt, recv, kind, args, method);
    }
    if tag == crate::value::TAG_FILE {
        return file::dispatch(rt, recv, kind, args, method);
    }
    if tag == crate::value::TAG_STR {
        return str::dispatch(rt, recv, kind, args, method);
    }
    if tag == crate::value::TAG_ENUM {
        return enum_::dispatch(rt, recv, kind, args, method);
    }
    if tag == crate::value::TAG_OPTION {
        // Handle optimized Option#some variant
        return dispatch_option_some(rt, recv, kind, args, method);
    }
    Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedReceiver(
        recv.type_name().to_string(),
    )))
}

fn dispatch_option_some(
    rt: &mut Runtime,
    recv: Value,
    kind: MethodKind,
    args: &[Value],
    method: &str,
) -> Result<Value, String> {
    let id = recv.as_obj_id();
    let inner = if let crate::gc::ManagedObject::OptionSome(v) = rt.heap.get(id) {
        *v
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Invalid OptionSome".into())));
    };

    match kind {
        MethodKind::OptHas => Ok(Value::from_bool(true)),
        MethodKind::OptNone => Ok(Value::from_bool(false)),
        MethodKind::OptOr => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            Ok(inner)
        }
        MethodKind::OptOrElse => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            Ok(inner)
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
            let mapped = rt.call_function(f, &[inner])?;
            Ok(rt.option_some(mapped))
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
            rt.call_function(f, &[inner])
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
            let _ = rt.call_function(f, &[inner])?;
            Ok(Value::VOID)
        }
        MethodKind::OptFilter => {
            if args.len() != 1 {
                return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                    expected_min: 1,
                    expected_max: 1,
                    actual: args.len(),
                }));
            }
            let pred = args[0];
            let keep = rt.call_function(pred, &[inner])?;
            if keep.is_bool() && keep.as_bool() {
                Ok(recv)
            } else {
                Ok(rt.option_none())
            }
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: method.to_string(),
            ty: "Option".to_string(),
        })),
    }
}
