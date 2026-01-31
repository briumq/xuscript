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
            "push" => Self::ListPush,
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
            "keys" => Self::DictKeys,
            "values" => Self::DictValues,
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
            "to_string" => Self::IntToString,  // 也用于 float
            "abs" => Self::IntAbs,  // 也用于 float
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
    // 处理整数的 to_string() 方法
    if recv.is_int() && kind == MethodKind::IntToString {
        let s = recv.as_i64().to_string();
        return Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(crate::Text::from_string(s)))));
    }
    // 处理浮点数的 to_string() 方法
    if recv.is_f64() && kind == MethodKind::IntToString {
        let s = recv.as_f64().to_string();
        return Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(crate::Text::from_string(s)))));
    }
    // 处理浮点数的 to_int() 方法
    if recv.is_f64() && kind == MethodKind::StrToInt {
        let i = recv.as_f64() as i64;
        return Ok(Value::from_i64(i));
    }
    
    // 处理整数的 abs() 方法
    if recv.is_int() && kind == MethodKind::IntAbs {
        let i = recv.as_i64().abs();
        return Ok(Value::from_i64(i));
    }
    
    // 处理浮点数的 abs() 方法
    if recv.is_f64() && kind == MethodKind::IntAbs {
        let f = recv.as_f64().abs();
        return Ok(Value::from_f64(f));
    }
    
    // 处理整数的 to_base() 方法
    if recv.is_int() && kind == MethodKind::IntToBase {
        if args.len() != 1 {
            return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
                expected_min: 1,
                expected_max: 1,
                actual: args.len(),
            }));
        }
        let base = args[0].as_i64();
        if base < 2 || base > 36 {
            return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Base must be between 2 and 36".into())));
        }
        let num = recv.as_i64();
        let s = if num == 0 {
            "0".to_string()
        } else {
            let mut result = String::new();
            let mut n = num.abs();
            let digits = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
            while n > 0 {
                let digit = (n % base) as usize;
                result.push(digits.chars().nth(digit).unwrap());
                n /= base;
            }
            if num < 0 {
                result.push('-');
            }
            result.chars().rev().collect()
        };
        return Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(crate::Text::from_string(s)))));
    }
    
    // 处理整数的 is_even() 方法
    if recv.is_int() && kind == MethodKind::IntIsEven {
        let i = recv.as_i64();
        return Ok(Value::from_bool(i % 2 == 0));
    }
    
    // 处理整数的 is_odd() 方法
    if recv.is_int() && kind == MethodKind::IntIsOdd {
        let i = recv.as_i64();
        return Ok(Value::from_bool(i % 2 != 0));
    }
    
    // 处理浮点数的 round() 方法
    if recv.is_f64() && kind == MethodKind::FloatRound {
        let f = recv.as_f64();
        let rounded = if args.len() == 1 {
            let digits = args[0].as_i64();
            let factor = 10.0_f64.powi(digits as i32);
            (f * factor).round() / factor
        } else {
            f.round()
        };
        return Ok(Value::from_f64(rounded));
    }
    
    // 处理浮点数的 floor() 方法
    if recv.is_f64() && kind == MethodKind::FloatFloor {
        let f = recv.as_f64().floor();
        return Ok(Value::from_f64(f));
    }
    
    // 处理浮点数的 ceil() 方法
    if recv.is_f64() && kind == MethodKind::FloatCeil {
        let f = recv.as_f64().ceil();
        return Ok(Value::from_f64(f));
    }
    
    // 处理布尔值的 to_string() 方法
    if recv.is_bool() && kind == MethodKind::IntToString {
        let s = recv.as_bool().to_string();
        return Ok(Value::str(rt.heap.alloc(crate::gc::ManagedObject::Str(crate::Text::from_string(s)))));
    }
    
    // 处理布尔值的 not() 方法
    if recv.is_bool() && kind == MethodKind::BoolNot {
        let b = !recv.as_bool();
        return Ok(Value::from_bool(b));
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
