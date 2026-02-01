//!
//!

// Reorganized module structure
pub mod core;
pub mod vm;
mod ast_exec;
mod modules;
mod util;

// Remaining modules at root level
mod runtime;
mod builtins;
pub mod builtins_registry;
mod methods;

// Re-exports from core/
pub use core::heap;
pub use core::text;
pub use core::text::Text;
pub use core::value::Value;
pub use core::value::ValueExt;
pub use core::env::{Env, Scope};

// Re-exports from vm/
pub use vm::VM;

// Re-exports from modules/
pub use modules::{ImportStamp, ModuleLoader, StdModuleLoader};

// Re-exports from util/
pub use util::Appendable;
pub use util::{Capabilities, Clock, FileStat, FileSystem, RngAlgorithm};

// Re-exports from other modules
pub use builtins_registry::{BuiltinProvider, BuiltinRegistry, StdBuiltinProvider};
pub use xu_ir::{Bytecode, Op};

// Runtime structs and enums
pub use runtime::ExecResult;
pub use runtime::Runtime;
pub use runtime::ICSlot;
pub use runtime::MethodICSlot;
pub use runtime::RuntimeConfig;
pub use runtime::Flow;
