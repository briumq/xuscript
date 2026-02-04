use crate::Value;

use crate::Runtime;

mod bool;
mod common;
mod dict;
mod enum_;
mod file;
mod float;
mod int;
mod list;
mod option;
mod str;
mod tuple;

use common::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MethodKind {
    ListPush,
    ListPop,
    ListReverse,
    ListJoin,
    ListInsert,
    ListSort,
    ListReduce,
    ListFind,
    ListFindIndex,
    ListFindOr,
    ListFirst,
    ListGet,
    ListFilter,
    ListMap,
    DictMerge,
    DictInsert,
    DictInsertInt,
    DictGet,
    DictGetInt,
    DictHas,
    DictKeys,
    DictValues,
    DictItems,
    GetOrDefault,
    FileRead,
    FileClose,
    StrFormat,
    StrSplit,
    StrToInt,
    StrToFloat,
    StrTryToInt,
    StrTryToFloat,
    StrReplace,
    StrTrim,
    StrTrimStart,
    StrTrimEnd,
    StrToUpper,
    StrToLower,
    StrStartsWith,
    StrEndsWith,
    StrFind,
    StrSubstr,
    StrMatch,
    StrGet,
    ToString,
    Abs,
    IntToBase,
    IntIsEven,
    IntIsOdd,
    FloatRound,
    FloatFloor,
    FloatCeil,
    BoolNot,
    OptOr,
    OptOrElse,
    OptMap,
    OptThen,
    OptEach,
    OptFilter,
    OptHas,
    OptGet,
    ResMapErr,
    EnumName,
    EnumTypeName,
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
            "push" => Self::ListPush,
            "pop" => Self::ListPop,
            "reverse" => Self::ListReverse,
            "join" => Self::ListJoin,
            "insert" => Self::ListInsert,
            "sort" => Self::ListSort,
            "reduce" => Self::ListReduce,
            "find" => Self::ListFind,
            "find_index" => Self::ListFindIndex,
            "find_or" => Self::ListFindOr,
            "first" => Self::ListFirst,
            "length" => Self::Len,
            "contains" => Self::Contains,
            "clear" => Self::Clear,
            "remove" => Self::Remove,
            "merge" => Self::DictMerge,
            "insert_int" => Self::DictInsertInt,
            "get" => Self::DictGet,
            "get_int" => Self::DictGetInt,
            "get_or_default" => Self::GetOrDefault,
            "keys" => Self::DictKeys,
            "values" => Self::DictValues,
            "items" => Self::DictItems,
            "read" => Self::FileRead,
            "close" => Self::FileClose,
            "format" => Self::StrFormat,
            "split" => Self::StrSplit,
            "to_int" => Self::StrToInt,
            "to_float" => Self::StrToFloat,
            "try_to_int" => Self::StrTryToInt,
            "try_to_float" => Self::StrTryToFloat,
            "replace" => Self::StrReplace,
            "trim" => Self::StrTrim,
            "trim_start" => Self::StrTrimStart,
            "trim_end" => Self::StrTrimEnd,
            "to_upper" => Self::StrToUpper,
            "to_lower" => Self::StrToLower,
            "starts_with" => Self::StrStartsWith,
            "ends_with" => Self::StrEndsWith,
            "substr" => Self::StrSubstr,
            "match" => Self::StrMatch,
            "to_string" => Self::ToString,
            "abs" => Self::Abs,
            "to_base" => Self::IntToBase,
            "is_even" => Self::IntIsEven,
            "is_odd" => Self::IntIsOdd,
            "round" => Self::FloatRound,
            "floor" => Self::FloatFloor,
            "ceil" => Self::FloatCeil,
            "not" => Self::BoolNot,
            "or" => Self::OptOr,
            "or_else" => Self::OptOrElse,
            "map" => Self::OptMap,
            "then" => Self::OptThen,
            "each" => Self::OptEach,
            "filter" => Self::OptFilter,
            "has" => Self::OptHas,
            "map_err" => Self::ResMapErr,
            "name" => Self::EnumName,
            "type_name" => Self::EnumTypeName,
            _ => Self::Unknown,
        }
    }
}

pub(super) fn dispatch_builtin_method(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let tag = recv.get_tag();

    match tag {
        crate::core::value::TAG_LIST => {
            let kind = match kind {
                MethodKind::DictGet | MethodKind::DictGetInt => MethodKind::ListGet,
                MethodKind::OptFilter => MethodKind::ListFilter,
                MethodKind::OptMap => MethodKind::ListMap,
                _ => kind,
            };
            list::dispatch(rt, recv, kind, args, method)
        }
        crate::core::value::TAG_DICT => {
            let kind = match kind {
                MethodKind::ListInsert => MethodKind::DictInsert,
                MethodKind::OptHas => MethodKind::DictHas,
                _ => kind,
            };
            dict::dispatch(rt, recv, kind, args, method)
        }
        crate::core::value::TAG_FILE => file::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_STR => {
            let kind = match kind {
                MethodKind::ListFind => MethodKind::StrFind,
                MethodKind::DictGet | MethodKind::DictGetInt => MethodKind::StrGet,
                _ => kind,
            };
            str::dispatch(rt, recv, kind, args, method)
        }
        crate::core::value::TAG_ENUM => {
            let kind = if kind == MethodKind::DictGet { MethodKind::OptGet } else { kind };
            enum_::dispatch(rt, recv, kind, args, method)
        }
        crate::core::value::TAG_OPTION => {
            let kind = if kind == MethodKind::DictGet { MethodKind::OptGet } else { kind };
            option::dispatch(rt, recv, kind, args, method)
        }
        crate::core::value::TAG_TUPLE => tuple::dispatch(rt, recv, kind, args, method),
        _ => dispatch_primitive_methods(rt, recv, kind, args, method),
    }
}

fn dispatch_primitive_methods(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    if recv.is_int() {
        return int::dispatch(rt, recv, kind, args, method);
    }
    if recv.is_f64() {
        return float::dispatch(rt, recv, kind, args, method);
    }
    if recv.is_bool() {
        return bool::dispatch(rt, recv, kind, args, method);
    }
    Err(err(
        rt,
        xu_syntax::DiagnosticKind::UnsupportedReceiver(recv.type_name().to_string()),
    ))
}
