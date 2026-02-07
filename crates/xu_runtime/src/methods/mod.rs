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
    // 通用方法（多个类型共享同一方法名）
    Get,      // list.get(i), dict.get(k), str.get(i), option.get()
    GetInt,   // list.get(i), dict.get_int(i), str.get(i)
    Insert,   // list.insert(i, v), dict.insert(k, v)
    Find,     // list.find(pred), str.find(substr)
    Filter,   // list.filter(pred), option.filter(pred)
    Map,      // list.map(f), option.map(f)
    Has,      // dict.has(k), option.has()
    Contains, // list.contains(v), str.contains(s)
    Len,      // list.length(), str.length(), dict.length(), tuple.length()
    Clear,    // list.clear(), dict.clear()
    Remove,   // list.remove(i), dict.remove(k)
    ToString, // int.to_string(), float.to_string(), bool.to_string(), option.to_string()
    Abs,      // int.abs(), float.abs()

    // List 专用方法
    ListPush,
    ListPop,
    ListReverse,
    ListJoin,
    ListSort,
    ListReduce,
    ListFindIndex,
    ListFindOr,
    ListFirst,

    // Dict 专用方法
    DictMerge,
    DictInsertInt,
    DictKeys,
    DictValues,
    DictItems,
    GetOrDefault,

    // File 专用方法
    FileRead,
    FileClose,

    // String 专用方法
    StrFormat,
    StrSplit,
    StrSplitLazy,
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
    StrSubstr,
    StrMatch,

    // Int 专用方法
    IntToBase,
    IntIsEven,
    IntIsOdd,

    // Float 专用方法
    FloatRound,
    FloatFloor,
    FloatCeil,

    // Bool 专用方法
    BoolNot,

    // Option/Result 专用方法
    Or,
    OrElse,
    Then,
    Each,
    MapErr,

    // Enum 专用方法
    EnumName,
    EnumTypeName,

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
            // 通用方法
            "get" => Self::Get,
            "get_int" => Self::GetInt,
            "insert" => Self::Insert,
            "find" => Self::Find,
            "filter" => Self::Filter,
            "map" => Self::Map,
            "has" => Self::Has,
            "contains" => Self::Contains,
            "length" => Self::Len,
            "clear" => Self::Clear,
            "remove" => Self::Remove,
            "to_string" => Self::ToString,
            "abs" => Self::Abs,

            // List 专用
            "push" => Self::ListPush,
            "pop" => Self::ListPop,
            "reverse" => Self::ListReverse,
            "join" => Self::ListJoin,
            "sort" => Self::ListSort,
            "reduce" => Self::ListReduce,
            "find_index" => Self::ListFindIndex,
            "find_or" => Self::ListFindOr,
            "first" => Self::ListFirst,

            // Dict 专用
            "merge" => Self::DictMerge,
            "insert_int" => Self::DictInsertInt,
            "keys" => Self::DictKeys,
            "values" => Self::DictValues,
            "items" => Self::DictItems,
            "get_or_default" => Self::GetOrDefault,

            // File 专用
            "read" => Self::FileRead,
            "close" => Self::FileClose,

            // String 专用
            "format" => Self::StrFormat,
            "split" => Self::StrSplit,
            "split_lazy" => Self::StrSplitLazy,
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

            // Int 专用
            "to_base" => Self::IntToBase,
            "is_even" => Self::IntIsEven,
            "is_odd" => Self::IntIsOdd,

            // Float 专用
            "round" => Self::FloatRound,
            "floor" => Self::FloatFloor,
            "ceil" => Self::FloatCeil,

            // Bool 专用
            "not" => Self::BoolNot,

            // Option/Result 专用
            "or" => Self::Or,
            "or_else" => Self::OrElse,
            "then" => Self::Then,
            "each" => Self::Each,
            "map_err" => Self::MapErr,

            // Enum 专用
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

    // 根据接收者类型分发到不同的处理函数
    match tag {
        crate::core::value::TAG_LIST => list::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_DICT => dict::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_FILE => file::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_STR => str::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_ENUM => enum_::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_OPTION => option::dispatch(rt, recv, kind, args, method),
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
