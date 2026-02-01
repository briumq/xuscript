//! Runtime configuration and result types.

use crate::core::Value;

/// Result of executing a program or module.
#[derive(Debug)]
pub struct ExecResult {
    pub value: Option<Value>,
    pub output: String,
}

/// Runtime configuration options.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeConfig {
    pub strict_vars: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self { strict_vars: true }
    }
}

/// Control flow result from statement execution.
pub enum Flow {
    None,
    Return(Value),
    Break,
    Continue,
    Throw(Value),
}
