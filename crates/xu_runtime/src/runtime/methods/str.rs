use crate::Value;

use super::{MethodKind, Runtime};
use super::common::*;
use crate::runtime::util::{to_i64, value_to_string};
use regex::Regex;

pub(super) fn dispatch(
    rt: &mut Runtime, recv: Value, kind: MethodKind, args: &[Value], method: &str,
) -> Result<Value, String> {
    match kind {
        MethodKind::DictGet | MethodKind::DictGetInt => {
            // str.get(i) - safe access returning Option
            validate_arity(rt, method, args.len(), 1, 1)?;
            
            let i = to_i64(&args[0])?;
            let result = {
                let s = expect_str(rt, recv)?;
                
                if i < 0 {
                    None
                } else {
                    let idx = i as usize;
                    let str_ref = s.as_str();

                    if s.is_ascii() {
                        if idx >= str_ref.len() {
                            None
                        } else {
                            Some(str_ref[idx..idx + 1].to_string())
                        }
                    } else {
                        let total = str_ref.chars().count();
                        if idx >= total {
                            None
                        } else {
                            Some(str_ref.chars().skip(idx).take(1).collect())
                        }
                    }
                }
            };
            
            match result {
                Some(ch) => {
                    let char_val = create_str_value(rt, &ch);
                    Ok(rt.option_some(char_val))
                }
                None => Ok(rt.option_none()),
            }
        }
        MethodKind::StrFormat => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_dict_param(rt, &args[0], "format dict")?;

            let s = expect_str(rt, recv)?;
            let mut out = s.clone();
            
            {
                let dict = expect_dict(rt, args[0])?;
                
                for (k, v) in dict.map.iter() {
                    let key = match k {
                        crate::value::DictKey::Str { data, .. } => crate::Text::from_str(&data),
                        crate::value::DictKey::Int(i) => crate::value::i64_to_text_fast(*i),
                    };
                    let needle = format!("{{{}}}", key);
                    let repl = value_to_string(v, &rt.heap);
                    out = out.as_str().replace(&needle, &repl).into();
                }
            }
            
            Ok(create_str_value(rt, &out.as_str()))
        }
        MethodKind::StrSplit => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "separator")?;
            
            let sep = get_str_from_value(rt, &args[0])?;
            let s = expect_str(rt, recv)?;
            
            // 先收集所有分割后的字符串
            let parts: Vec<String> = s
                .as_str()
                .split(&sep)
                .map(|p| p.to_string())
                .collect();
            
            // 然后创建Value对象
            let mut items = Vec::with_capacity(parts.len());
            for p_str in parts {
                items.push(create_str_value(rt, &p_str));
            }
            
            Ok(create_list_value(rt, items))
        }
        MethodKind::StrToInt => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let s = expect_str(rt, recv)?;
            let v =
                s.as_str().trim().parse::<i64>().map_err(|e| {
                    err(rt, xu_syntax::DiagnosticKind::ParseIntError(e.to_string()))
                })?;
            Ok(Value::from_i64(v))
        }
        MethodKind::StrToFloat => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let s = expect_str(rt, recv)?;
            let v = s.as_str().trim().parse::<f64>().map_err(|e| {
                err(
                    rt,
                    xu_syntax::DiagnosticKind::ParseFloatError(e.to_string()),
                )
            })?;
            Ok(Value::from_f64(v))
        }
        MethodKind::StrTryToInt => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let s = expect_str(rt, recv)?;
            match s.as_str().trim().parse::<i64>() {
                Ok(v) => Ok(rt.option_some(Value::from_i64(v))),
                Err(_) => Ok(rt.option_none()),
            }
        }
        MethodKind::StrTryToFloat => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            
            let s = expect_str(rt, recv)?;
            match s.as_str().trim().parse::<f64>() {
                Ok(v) => Ok(rt.option_some(Value::from_f64(v))),
                Err(_) => Ok(rt.option_none()),
            }
        }
        MethodKind::StrToUpper => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().to_uppercase();
            Ok(create_str_value(rt, &result))
        }
        MethodKind::StrToLower => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().to_lowercase();
            Ok(create_str_value(rt, &result))
        }
        MethodKind::Contains => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "substring")?;
            
            let sub = get_str_from_value(rt, &args[0])?;
            let s = expect_str(rt, recv)?;
            Ok(Value::from_bool(s.as_str().contains(&sub)))
        }
        MethodKind::StrStartsWith => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "prefix")?;
            
            let sub = get_str_from_value(rt, &args[0])?;
            let s = expect_str(rt, recv)?;
            Ok(Value::from_bool(s.as_str().starts_with(&sub)))
        }
        MethodKind::StrEndsWith => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "suffix")?;
            
            let sub = get_str_from_value(rt, &args[0])?;
            let s = expect_str(rt, recv)?;
            Ok(Value::from_bool(s.as_str().ends_with(&sub)))
        }
        MethodKind::StrTrim => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().trim().to_string();
            Ok(create_str_value(rt, &result))
        }
        MethodKind::StrReplace => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            validate_str_param(rt, &args[0], "from")?;
            validate_str_param(rt, &args[1], "to")?;
            
            let from = get_str_from_value(rt, &args[0])?;
            let to = get_str_from_value(rt, &args[1])?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().replace(&from, &to);
            Ok(create_str_value(rt, &result))
        }
        MethodKind::StrReplaceAll => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            validate_str_param(rt, &args[0], "from")?;
            validate_str_param(rt, &args[1], "to")?;
            
            let from = get_str_from_value(rt, &args[0])?;
            let to = get_str_from_value(rt, &args[1])?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().replace(&from, &to);
            Ok(create_str_value(rt, &result))
        }
        MethodKind::StrTrimStart => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().trim_start().to_string();
            Ok(create_str_value(rt, &result))
        }
        MethodKind::StrTrimEnd => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = expect_str(rt, recv)?;
            let result = s.as_str().trim_end().to_string();
            Ok(create_str_value(rt, &result))
        }
        MethodKind::StrFind | MethodKind::ListFind => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "substring")?;
            
            let sub = get_str_from_value(rt, &args[0])?;
            let s = expect_str(rt, recv)?;
            
            match s.as_str().find(&sub) {
                Some(idx) => Ok(rt.option_some(Value::from_i64(idx as i64))),
                None => Ok(rt.option_none()),
            }
        }
        MethodKind::StrSubstr => {
            validate_arity(rt, method, args.len(), 2, 2)?;
            
            let start = to_i64(&args[0])?;
            let length = to_i64(&args[1])?;

            if start < 0 || length < 0 {
                return Ok(create_str_value(rt, ""));
            }

            let s = expect_str(rt, recv)?;
            let start_idx = start as usize;
            let length_idx = length as usize;
            let str_ref = s.as_str();

            let result = if s.is_ascii() {
                if start_idx >= str_ref.len() {
                    "".to_string()
                } else {
                    let end_idx = std::cmp::min(start_idx + length_idx, str_ref.len());
                    str_ref[start_idx..end_idx].to_string()
                }
            } else {
                let chars: Vec<char> = str_ref.chars().collect();
                if start_idx >= chars.len() {
                    "".to_string()
                } else {
                    let end_idx = std::cmp::min(start_idx + length_idx, chars.len());
                    chars[start_idx..end_idx].iter().collect()
                }
            };
            
            Ok(create_str_value(rt, &result))
        }
        MethodKind::Len => {
            validate_arity(rt, method, args.len(), 0, 0)?;
            let s = expect_str(rt, recv)?;
            Ok(Value::from_i64(s.char_count() as i64))
        }
        MethodKind::StrMatch => {
            validate_arity(rt, method, args.len(), 1, 1)?;
            validate_str_param(rt, &args[0], "pattern")?;
            
            let pattern = get_str_from_value(rt, &args[0])?;
            let s = expect_str(rt, recv)?;

            let regex = Regex::new(&pattern).map_err(|e| {
                err(rt, xu_syntax::DiagnosticKind::Raw(format!(
                    "Invalid regex: {}",
                    e
                )))
            })?;

            let captures = regex.captures(s.as_str());
            match captures {
                Some(caps) => {
                    // 先收集所有匹配的字符串
                    let mut matched_parts = Vec::new();
                    for cap in caps.iter() {
                        matched_parts.push(cap.map(|m| m.as_str().to_string()));
                    }
                    
                    // 然后创建Value对象
                    let mut groups = Vec::with_capacity(matched_parts.len());
                    for cap in matched_parts {
                        match cap {
                            Some(matched_str) => {
                                groups.push(create_str_value(rt, &matched_str));
                            }
                            None => {
                                groups.push(create_str_value(rt, ""));
                            }
                        }
                    }
                    
                    Ok(create_list_value(rt, groups))
                }
                None => Ok(create_list_value(rt, Vec::new())),
            }
        }
        _ => Err(err(rt, xu_syntax::DiagnosticKind::UnknownStrMethod(
            method.to_string(),
        ))),
    }
}
