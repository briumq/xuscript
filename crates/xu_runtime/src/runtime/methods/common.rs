#![allow(dead_code)]

use crate::Value;
use super::Runtime;

pub fn validate_arity(
    rt: &Runtime,
    _method: &str,
    args_len: usize,
    min: usize,
    max: usize,
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

pub fn expect_tag(rt: &Runtime, v: &Value, tag: u64, expected: &str) -> Result<(), String> {
    if v.get_tag() != tag {
        return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: expected.to_string(),
            actual: v.type_name().to_string(),
        }));
    }
    Ok(())
}

pub fn err(rt: &Runtime, kind: xu_syntax::DiagnosticKind) -> String {
    rt.error(kind)
}
