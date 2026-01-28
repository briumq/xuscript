use crate::Value;

use super::Runtime;

mod dict;
mod enum_;
mod file;
mod list;
mod str;

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
            "length" => Self::Len,
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
    Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedReceiver(
        recv.type_name().to_string(),
    )))
}
