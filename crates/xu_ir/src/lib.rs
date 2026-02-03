//!
//!
//!
mod ast;
mod bytecode;
mod executable;
mod frontend;
mod hash;
mod program;

pub use ast::*;
pub use bytecode::*;
pub use executable::*;
pub use frontend::*;
pub use hash::*;
pub use program::*;

/// Infer a module alias from an import path.
/// Extracts the filename without extension from the path.
pub fn infer_module_alias(path: &str) -> String {
    let mut last = path;
    if let Some((_, tail)) = path.rsplit_once('/') {
        last = tail;
    } else if let Some((_, tail)) = path.rsplit_once('\\') {
        last = tail;
    }
    let last = last.trim_end_matches('/');
    let last = last.trim_end_matches('\\');
    last.strip_suffix(".xu").unwrap_or(last).to_string()
}
