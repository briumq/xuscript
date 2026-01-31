use crate::Value;

pub fn to_f64(v: &Value) -> Result<f64, String> {
    if v.is_int() {
        Ok(v.as_i64() as f64)
    } else if v.is_f64() {
        Ok(v.as_f64())
    } else {
        Err(format!("Expected number, got {}", v.type_name()))
    }
}

pub fn to_f64_pair(a: &Value, b: &Value) -> Result<(f64, f64), String> {
    Ok((to_f64(a)?, to_f64(b)?))
}
