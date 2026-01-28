//!
//!
pub mod gc;
pub mod runtime;
mod text;
mod value;

pub use runtime::builtins_registry::{BuiltinProvider, BuiltinRegistry, StdBuiltinProvider};
pub use runtime::{Bytecode, ExecResult, Op, Runtime, VM};
pub use text::Text;
pub use value::Value;
