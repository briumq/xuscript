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
pub use xu_ir::*;
