#![allow(dead_code)]

use crate::Runtime;
use crate::Value;

/// 验证参数数量是否在指定范围内
pub fn validate_arity(
    rt: &Runtime, _method: &str, args_len: usize, min: usize, max: usize,
) -> Result<(), String> {
    if args_len < min || args_len > max {
        return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
            expected_min: min,
            expected_max: max,
            actual: args_len,
        }));
    }
    Ok(())
}

/// 验证值的类型标签是否符合预期
pub fn expect_tag(rt: &Runtime, v: &Value, tag: u64, expected: &str) -> Result<(), String> {
    if v.get_tag() != tag {
        return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: expected.to_string(),
            actual: v.type_name().to_string(),
        }));
    }
    Ok(())
}

/// 生成错误信息
pub fn err(rt: &Runtime, kind: xu_syntax::DiagnosticKind) -> String {
    rt.error(kind)
}

/// 验证值是否为列表类型
pub fn expect_list(rt: &Runtime, value: Value) -> Result<&Vec<Value>, String> {
    let id = value.as_obj_id();
    let obj = rt.heap.get(id);
    if let crate::core::gc::ManagedObject::List(list) = obj {
        Ok(list)
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
    }
}

/// 验证值是否为可变列表类型
pub fn expect_list_mut(rt: &mut Runtime, value: Value) -> Result<&mut Vec<Value>, String> {
    // 先使用不可变引用检查类型
    {
        let id = value.as_obj_id();
        let obj = rt.heap.get(id);
        if !matches!(obj, crate::core::gc::ManagedObject::List(_)) {
            return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())));
        }
    }
    
    // 然后获取可变引用
    let id = value.as_obj_id();
    let obj = rt.heap.get_mut(id);
    // 由于前面已经检查过类型，这里可以安全地使用unwrap
    match obj {
        crate::core::gc::ManagedObject::List(list) => Ok(list),
        _ => unreachable!(),
    }
}

/// 验证值是否为字典类型
pub fn expect_dict(rt: &Runtime, value: Value) -> Result<&crate::core::value::Dict, String> {
    let id = value.as_obj_id();
    let obj = rt.heap.get(id);
    if let crate::core::gc::ManagedObject::Dict(dict) = obj {
        Ok(dict)
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
    }
}

/// 验证值是否为可变字典类型
pub fn expect_dict_mut(rt: &mut Runtime, value: Value) -> Result<&mut crate::core::value::Dict, String> {
    // 先使用不可变引用检查类型
    {
        let id = value.as_obj_id();
        let obj = rt.heap.get(id);
        if !matches!(obj, crate::core::gc::ManagedObject::Dict(_)) {
            return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
        }
    }
    
    // 然后获取可变引用
    let id = value.as_obj_id();
    let obj = rt.heap.get_mut(id);
    // 由于前面已经检查过类型，这里可以安全地使用unwrap
    match obj {
        crate::core::gc::ManagedObject::Dict(dict) => Ok(dict),
        _ => unreachable!(),
    }
}

/// 验证值是否为字符串类型
pub fn expect_str(rt: &Runtime, value: Value) -> Result<&crate::Text, String> {
    let id = value.as_obj_id();
    let obj = rt.heap.get(id);
    if let crate::core::gc::ManagedObject::Str(s) = obj {
        Ok(s)
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())))
    }
}

/// 验证值是否为OptionSome类型
pub fn expect_option_some(rt: &Runtime, value: Value) -> Result<Value, String> {
    let id = value.as_obj_id();
    let obj = rt.heap.get(id);
    if let crate::core::gc::ManagedObject::OptionSome(v) = obj {
        Ok(*v)
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::Raw("Invalid OptionSome".into())))
    }
}

/// 验证索引是否有效
pub fn validate_index(rt: &Runtime, index: i64, length: usize) -> Result<usize, String> {
    if index < 0 || (index as usize) >= length {
        return Err(rt.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
    }
    Ok(index as usize)
}

/// 验证插入索引是否有效（允许等于长度）
pub fn validate_insert_index(rt: &Runtime, index: i64, length: usize) -> Result<usize, String> {
    if index < 0 || (index as usize) > length {
        return Err(rt.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
    }
    Ok(index as usize)
}

/// 验证索引是否有效（接受可变Runtime引用）
pub fn validate_index_mut(rt: &mut Runtime, index: i64, length: usize) -> Result<usize, String> {
    // 创建一个不可变引用的副本用于错误处理
    let rt_ref: &Runtime = &*rt;
    validate_index(rt_ref, index, length)
}

/// 验证插入索引是否有效（接受可变Runtime引用）
pub fn validate_insert_index_mut(rt: &mut Runtime, index: i64, length: usize) -> Result<usize, String> {
    // 创建一个不可变引用的副本用于错误处理
    let rt_ref: &Runtime = &*rt;
    validate_insert_index(rt_ref, index, length)
}

/// 从Value中获取字符串的辅助函数
pub fn get_str_from_value(rt: &Runtime, value: &Value) -> Result<String, String> {
    if value.get_tag() == crate::core::value::TAG_STR {
        let s = expect_str(rt, *value)?;
        Ok(s.as_str().to_string())
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: "string".to_string(),
            actual: value.type_name().to_string(),
        }))
    }
}

/// 从Value中获取字典键的辅助函数
pub fn get_dict_key_from_value(rt: &Runtime, value: &Value) -> Result<crate::core::value::DictKey, String> {
    if value.get_tag() == crate::core::value::TAG_STR {
        let s = expect_str(rt, *value)?;
        Ok(crate::core::value::DictKey::from_text(s))
    } else if value.is_int() {
        Ok(crate::core::value::DictKey::Int(value.as_i64()))
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: "text or int".to_string(),
            actual: value.type_name().to_string(),
        }))
    }
}

/// 创建字符串Value的辅助函数
pub fn create_str_value(rt: &mut Runtime, s: &str) -> Value {
    Value::str(rt.heap.alloc(crate::core::gc::ManagedObject::Str(s.into())))
}

/// 创建列表Value的辅助函数
pub fn create_list_value(rt: &mut Runtime, items: Vec<Value>) -> Value {
    Value::list(rt.heap.alloc(crate::core::gc::ManagedObject::List(items)))
}

/// 验证参数是否为字符串类型
pub fn validate_str_param(rt: &Runtime, param: &Value, _param_name: &str) -> Result<(), String> {
    if param.get_tag() != crate::core::value::TAG_STR {
        return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: "string".to_string(),
            actual: param.type_name().to_string(),
        }));
    }
    Ok(())
}

/// 验证参数是否为字典类型
pub fn validate_dict_param(rt: &Runtime, param: &Value, _param_name: &str) -> Result<(), String> {
    if param.get_tag() != crate::core::value::TAG_DICT {
        return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: "dict".to_string(),
            actual: param.type_name().to_string(),
        }));
    }
    Ok(())
}

/// 安全地从列表中获取元素
pub fn safe_get_from_list(_rt: &Runtime, list: &Vec<Value>, index: i64) -> Option<Value> {
    if index < 0 || (index as usize) >= list.len() {
        None
    } else {
        Some(list[index as usize])
    }
}

/// 安全地从字符串中获取字符
pub fn safe_get_from_str(rt: &mut Runtime, s: &crate::Text, index: i64) -> Option<Value> {
    if index < 0 {
        return None;
    }
    let idx = index as usize;
    let str_ref = s.as_str();

    if s.is_ascii() {
        if idx >= str_ref.len() {
            return None;
        }
        let ch = &str_ref[idx..idx + 1];
        Some(create_str_value(rt, ch))
    } else {
        let total = str_ref.chars().count();
        if idx >= total {
            None
        } else {
            let ch: String = str_ref.chars().skip(idx).take(1).collect();
            Some(create_str_value(rt, &ch))
        }
    }
}
