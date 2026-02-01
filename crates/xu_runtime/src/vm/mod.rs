//! Bytecode Virtual Machine.
//!
//! This module contains the bytecode interpreter and related operations.

mod dispatch;
mod exception;
mod fast;
pub(crate) mod ops;
pub(crate) mod stack;

pub use dispatch::VM;
pub(crate) use dispatch::run_bytecode;
pub(crate) use stack::{Handler, IterState, Pending};
pub(crate) use fast::{run_bytecode_fast, run_bytecode_fast_params_only};
