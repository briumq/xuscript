//! Xu language parser.

#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]

mod expr;
mod interp;
pub mod mangling;
mod parser;
mod stmt;
mod types;

pub use parser::{ParseResult, Parser};

// Re-export all xu_ir types for internal use and public API
// Note: This is intentional as xu_parser is the primary interface for AST types
pub use xu_ir::*;
