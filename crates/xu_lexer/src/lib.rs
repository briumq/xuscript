//! xu_lexer: lexer crate.
//!
//! Normalizes source text, tokenizes it, and collects diagnostics.
//! Entry points: `Lexer::new(input).lex()` and `normalize_source`.
mod keywords;
mod lexer;
mod normalize;

pub use lexer::{LexResult, Lexer};
pub use normalize::normalize_source;
