//! AST-based executor.
//!
//! This module contains the tree-walking interpreter that executes AST nodes directly.

mod access;
mod call;
pub(crate) mod closure;
mod expr;
mod stmt;
