//! Bytecode Virtual Machine.
//!
//! This module contains the bytecode interpreter and related operations.

mod dispatch;
mod exception;
mod fast;
pub(crate) mod ops;

pub use dispatch::VM;
pub(crate) use dispatch::{run_bytecode, Handler, Pending, IterState};
pub(crate) use fast::{run_bytecode_fast, run_bytecode_fast_params_only};
