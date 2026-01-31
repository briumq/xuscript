//! 
//! 
pub mod gc;
pub mod text;
mod value;
mod runtime;

// Runtime modules
mod appendable;
mod builtins;
pub mod builtins_registry;
mod capabilities;
mod diag;
mod env;
mod executor;
mod stmt_exec;
mod args_eval;
mod ir;
mod ir_throw;
mod pattern;
mod methods;
mod module_loader;
mod modules;
mod op_dispatch;
mod slot_allocator;
mod util;

// Re-exports
pub use appendable::Appendable;
pub use builtins_registry::{BuiltinProvider, BuiltinRegistry, StdBuiltinProvider};
pub use capabilities::{Clock, FileStat, FileSystem, RngAlgorithm};
pub use env::{Env, Scope};
pub use ir::VM;
pub use text::Text;
pub use value::Value;
pub use xu_ir::{Bytecode, Op};

// Runtime structs and enums
pub use runtime::ExecResult;
pub use runtime::Runtime;
pub use runtime::ICSlot;
pub use runtime::MethodICSlot;
pub use runtime::RuntimeConfig;
pub use runtime::Flow;
