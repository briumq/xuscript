use super::common::to_f64; use super::common::to_f64_pair;
use super::super::Runtime;
use crate::Value;

pub fn builtin_abs(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("abs expects 1 argument".into());
    }
    let v = &args[0];
    if v.is_int() {
        Ok(Value::from_i64(v.as_i64().abs()))
    } else if v.is_f64() {
        Ok(Value::from_f64(v.as_f64().abs()))
    } else {
        Err("abs expects number".into())
    }
}

pub fn builtin_max(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("max expects 2 arguments".into());
    }
    if args[0].is_int() && args[1].is_int() {
        Ok(Value::from_i64(args[0].as_i64().max(args[1].as_i64())))
    } else {
        let (a, b) = to_f64_pair(&args[0], &args[1])?;
        Ok(Value::from_f64(a.max(b)))
    }
}

pub fn builtin_min(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("min expects 2 arguments".into());
    }
    if args[0].is_int() && args[1].is_int() {
        Ok(Value::from_i64(args[0].as_i64().min(args[1].as_i64())))
    } else {
        let (a, b) = to_f64_pair(&args[0], &args[1])?;
        Ok(Value::from_f64(a.min(b)))
    }
}

pub fn builtin_sin(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("sin expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.sin()))
}

pub fn builtin_cos(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("cos expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.cos()))
}

pub fn builtin_tan(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("tan expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.tan()))
}

pub fn builtin_sqrt(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("sqrt expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.sqrt()))
}

pub fn builtin_log(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("log expects 1 argument".into());
    }
    let v = to_f64(&args[0])?;
    Ok(Value::from_f64(v.ln()))
}

pub fn builtin_pow(_rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("pow expects 2 arguments".into());
    }
    let (base, exp) = to_f64_pair(&args[0], &args[1])?;
    Ok(Value::from_f64(base.powf(exp)))
}
