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
mod str;

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
    DictMerge,
    DictInsert,
    DictInsertInt,
    DictGet,
    DictGetInt,
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
    StrReplaceAll,
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
    IntToString,
    IntAbs,
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
    OptNone,
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
            "push" | "add" => Self::ListPush,
            "pop" => Self::ListPop,
            "reverse" => Self::ListReverse,
            "join" => Self::ListJoin,
            "insert" => Self::ListInsert,
            "sort" => Self::ListSort,
            "reduce" => Self::ListReduce,
            "find" => Self::ListFind,
            "length" => Self::Len,
            "contains" => Self::Contains,
            "clear" => Self::Clear,
            "remove" => Self::Remove,
            "merge" => Self::DictMerge,
            "dict_insert" => Self::DictInsert,
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
            "replace_all" => Self::StrReplaceAll,
            "trim" => Self::StrTrim,
            "trim_start" => Self::StrTrimStart,
            "trim_end" => Self::StrTrimEnd,
            "to_upper" => Self::StrToUpper,
            "to_lower" => Self::StrToLower,
            "starts_with" => Self::StrStartsWith,
            "ends_with" => Self::StrEndsWith,
            "str_find" => Self::StrFind,
            "substr" => Self::StrSubstr,
            "match" => Self::StrMatch,
            "to_string" => Self::IntToString, // 也用于 float 和 bool
            "abs" => Self::IntAbs,            // 也用于 float
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
            "none" => Self::OptNone,
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
    
    // 根据接收者类型分发到不同的处理函数
    match tag {
        crate::core::value::TAG_LIST => list::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_DICT => dict::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_FILE => file::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_STR => str::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_ENUM => enum_::dispatch(rt, recv, kind, args, method),
        crate::core::value::TAG_OPTION => dispatch_option_some(rt, recv, kind, args, method),
        _ => dispatch_primitive_methods(rt, recv, kind, args, method),
    }
}

fn dispatch_primitive_methods(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    // 处理整数方法
    if recv.is_int() {
        return int::dispatch(rt, recv, kind, args, method);
    }
    
    // 处理浮点数方法
    if recv.is_f64() {
        return float::dispatch(rt, recv, kind, args, method);
    }
    
    // 处理布尔值方法
    if recv.is_bool() {
        return bool::dispatch(rt, recv, kind, args, method);
    }
    
    Err(err(
        rt,
        xu_syntax::DiagnosticKind::UnsupportedReceiver(recv.type_name().to_string()),
    ))
}

fn dispatch_option_some(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let inner = expect_option_some(rt, recv)?;

    match kind {
        MethodKind::OptHas => Ok(Value::from_bool(true)),
        MethodKind::OptNone => Ok(Value::from_bool(false)),
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
            Ok(Value::VOID)
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
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "Option".to_string(),
            },
        )),
    }
}
