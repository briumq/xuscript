//! Xu language driver for parsing, analysis, and compilation.

#![allow(clippy::type_complexity)]
#![allow(clippy::only_used_in_recursion)]
#![allow(clippy::question_mark)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_lifetimes)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::empty_docs)]

mod analyzer;
mod bytecode_compiler;
mod frontend;
mod analyzer_util;

pub use frontend::{Driver, LexedFile, ParsedFile, Timings};
