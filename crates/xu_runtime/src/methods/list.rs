use crate::Value;

use super::{MethodKind, Runtime};
use super::common::*;
use crate::util::{to_i64, value_to_string};

pub(super) fn dispatch(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    match kind {
        MethodKind::DictGet | MethodKind::DictGetInt => {
            // list.get(i) - safe access returning Option
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let i = to_i64(&args[0])?;
            let list = expect_list(rt, recv)?;
            
            match safe_get_from_list(rt, list, i) {
                Some(value) => Ok(rt.option_some(value)),
                None => Ok(rt.option_none()),
            }
        }
        MethodKind::ListPush => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let list = expect_list_mut(rt, recv)?;
            list.push(args[0].clone());
            Ok(Value::VOID)
        }
        MethodKind::Remove => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let i = to_i64(&args[0])?;
            let list = expect_list_mut(rt, recv)?;
            
            if i < 0 || (i as usize) >= list.len() {
                return Err(rt.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
            }
            let index = i as usize;
            
            Ok(list.remove(index))
        }
        MethodKind::Contains => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let list = expect_list(rt, recv)?;
            let mut found = false;
            for v in list.iter() {
                if rt.values_equal(v, &args[0]) {
                    found = true;
                    break;
                }
            }
            Ok(Value::from_bool(found))
        }
        MethodKind::ListPop => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let list = expect_list_mut(rt, recv)?;
            Ok(list.pop().unwrap_or(Value::VOID))
        }
        MethodKind::Clear => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let list = expect_list_mut(rt, recv)?;
            list.clear();
            Ok(Value::VOID)
        }
        MethodKind::ListReverse => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let list = expect_list_mut(rt, recv)?;
            list.reverse();
            Ok(Value::VOID)
        }
        MethodKind::ListJoin => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "separator")?;
            
            let sep = get_str_from_value(rt, &args[0])?;
            let list = expect_list(rt, recv)?;
            
            let strs: Vec<String> = list
                .iter()
                .map(|item| value_to_string(item, &rt.heap))
                .collect();
            
            Ok(create_str_value(rt, &strs.join(&sep)))
        }
        MethodKind::Len => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let list = expect_list(rt, recv)?;
            Ok(Value::from_i64(list.len() as i64))
        }
        MethodKind::OptFilter => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let f = args[0];
            let list = expect_list(rt, recv)?;
            let items = list.to_vec();
            
            let mut out: Vec<Value> = Vec::with_capacity(items.len());
            for item in items {
                let keep = rt.call_function(f, &[item])?;
                if !keep.is_bool() {
                    return Err(err(rt, xu_syntax::DiagnosticKind::InvalidConditionType(
                        keep.type_name().to_string(),
                    )));
                }
                if keep.as_bool() {
                    out.push(item);
                }
            }
            
            Ok(create_list_value(rt, out))
        }
        MethodKind::OptMap => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let f = args[0];
            let list = expect_list(rt, recv)?;
            let items = list.to_vec();
            
            let mut out: Vec<Value> = Vec::with_capacity(items.len());
            for item in items {
                out.push(rt.call_function(f, &[item])?);
            }
            
            Ok(create_list_value(rt, out))
        }
        MethodKind::ListInsert => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            
            let i = to_i64(&args[0])?;
            let value = args[1].clone();
            let list = expect_list_mut(rt, recv)?;
            
            if i < 0 || (i as usize) > list.len() {
                return Err(rt.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
            }
            let index = i as usize;
            list.insert(index, value);
            Ok(Value::VOID)
        }
        MethodKind::ListSort => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let list = expect_list_mut(rt, recv)?;
            list.sort_by(|a, b| {
                if a.is_int() && b.is_int() {
                    a.as_i64().cmp(&b.as_i64())
                } else if a.is_f64() && b.is_f64() {
                    a.as_f64()
                        .partial_cmp(&b.as_f64())
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    std::cmp::Ordering::Equal
                }
            });
            Ok(Value::VOID)
        }
        MethodKind::ListReduce => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            
            let f = args[0];
            let mut acc = args[1].clone();
            let list = expect_list(rt, recv)?;
            let items = list.to_vec();
            
            for item in items {
                acc = rt.call_function(f, &[acc, item])?;
            }
            Ok(acc)
        }
        MethodKind::ListFind => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let f = args[0];
            let list = expect_list(rt, recv)?;
            let items = list.to_vec();
            
            for item in items {
                let found = rt.call_function(f, &[item])?;
                if !found.is_bool() {
                    return Err(err(rt, xu_syntax::DiagnosticKind::InvalidConditionType(
                        found.type_name().to_string(),
                    )));
                }
                if found.as_bool() {
                    return Ok(rt.option_some(item));
                }
            }
            Ok(rt.option_none())
        }
        MethodKind::ListFindIndex => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let f = args[0];
            let list = expect_list(rt, recv)?;
            let items = list.to_vec();
            
            for (index, item) in items.iter().enumerate() {
                let found = rt.call_function(f, &[*item])?;
                if !found.is_bool() {
                    return Err(err(rt, xu_syntax::DiagnosticKind::InvalidConditionType(
                        found.type_name().to_string(),
                    )));
                }
                if found.as_bool() {
                    return Ok(rt.option_some(Value::from_i64(index as i64)));
                }
            }
            Ok(rt.option_none())
        }
        _ => Err(rt.error(xu_syntax::DiagnosticKind::UnknownListMethod(
            method.to_string(),
        ))),

    }
}
