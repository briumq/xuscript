use crate::Value;

use super::Runtime;

mod common;
mod dict;
mod enum_;
mod file;
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
        crate::value::TAG_LIST => list::dispatch(rt, recv, kind, args, method),
        crate::value::TAG_DICT => dict::dispatch(rt, recv, kind, args, method),
        crate::value::TAG_FILE => file::dispatch(rt, recv, kind, args, method),
        crate::value::TAG_STR => str::dispatch(rt, recv, kind, args, method),
        crate::value::TAG_ENUM => enum_::dispatch(rt, recv, kind, args, method),
        crate::value::TAG_OPTION => dispatch_option_some(rt, recv, kind, args, method),
        _ => dispatch_primitive_methods(rt, recv, kind, args, method),
    }
}

fn dispatch_primitive_methods(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    // 处理整数方法
    if recv.is_int() {
        return dispatch_int_methods(rt, recv, kind, args, method);
    }
    
    // 处理浮点数方法
    if recv.is_f64() {
        return dispatch_float_methods(rt, recv, kind, args, method);
    }
    
    // 处理布尔值方法
    if recv.is_bool() {
        return dispatch_bool_methods(rt, recv, kind, args, method);
    }
    
    Err(err(
        rt,
        xu_syntax::DiagnosticKind::UnsupportedReceiver(recv.type_name().to_string()),
    ))
}

fn dispatch_int_methods(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let i = recv.as_i64();
    
    match kind {
        MethodKind::IntToString => {
            let s = i.to_string();
            Ok(create_str_value(rt, &s))
        }
        MethodKind::IntAbs => {
            let abs = i.abs();
            Ok(Value::from_i64(abs))
        }
        MethodKind::IntToBase => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let base = args[0].as_i64();
            if base < 2 || base > 36 {
                return Err(err(
                    rt,
                    xu_syntax::DiagnosticKind::Raw("Base must be between 2 and 36".into()),
                ));
            }
            
            let s = if i == 0 {
                "0".to_string()
            } else {
                let mut result = String::new();
                let mut n = i.abs();
                let digits = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
                while n > 0 {
                    let digit = (n % base) as usize;
                    result.push(digits.chars().nth(digit).unwrap());
                    n /= base;
                }
                if i < 0 {
                    result.push('-');
                }
                result.chars().rev().collect()
            };
            Ok(create_str_value(rt, &s))
        }
        MethodKind::IntIsEven => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(Value::from_bool(i % 2 == 0))
        }
        MethodKind::IntIsOdd => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            Ok(Value::from_bool(i % 2 != 0))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "int".to_string(),
            },
        )),
    }
}

fn dispatch_float_methods(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let f = recv.as_f64();
    
    match kind {
        MethodKind::IntToString => {
            let s = f.to_string();
            Ok(create_str_value(rt, &s))
        }
        MethodKind::IntAbs => {
            let abs = f.abs();
            Ok(Value::from_f64(abs))
        }
        MethodKind::StrToInt => {
            let i = f as i64;
            Ok(Value::from_i64(i))
        }
        MethodKind::FloatRound => {
            validate_arity(rt, method, args.len(), 0, 1)?;
            
            let rounded = if args.len() == 1 {
                let digits = args[0].as_i64();
                let factor = 10.0_f64.powi(digits as i32);
                (f * factor).round() / factor
            } else {
                f.round()
            };
            Ok(Value::from_f64(rounded))
        }
        MethodKind::FloatFloor => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let floor = f.floor();
            Ok(Value::from_f64(floor))
        }
        MethodKind::FloatCeil => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let ceil = f.ceil();
            Ok(Value::from_f64(ceil))
        }
        _ => Err(err(
            rt,
            xu_syntax::DiagnosticKind::UnsupportedMethod {
                method: method.to_string(),
                ty: "float".to_string(),
            },
        )),
    }
}

fn dispatch_bool_methods(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    let b = recv.as_bool();
    
    match kind {
        MethodKind::IntToString => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = b.to_string();
            Ok(create_str_value(rt, &s))
        }
        MethodKind::BoolNot => {
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
