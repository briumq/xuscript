//! Name mangling utilities for method and static function names.

pub const METHOD_PREFIX: &str = "__method__";
pub const STATIC_PREFIX: &str = "__static__";

/// Generate a mangled method name: `__method__{type_name}__{method_name}`
#[inline]
pub fn method_name(type_name: &str, method: &str) -> String {
    format!("{}{}__{}", METHOD_PREFIX, type_name, method)
}

/// Generate a mangled static method name: `__static__{type_name}__{method_name}`
#[inline]
pub fn static_name(type_name: &str, method: &str) -> String {
    format!("{}{}__{}", STATIC_PREFIX, type_name, method)
}
