//! Xu language runtime.

#![allow(clippy::collapsible_if)]
#![allow(clippy::collapsible_else_if)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::new_without_default)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::map_clone)]
#![allow(clippy::unused_enumerate_index)]
#![allow(clippy::unwrap_or_default)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::question_mark)]
#![allow(clippy::manual_range_contains)]
#![allow(clippy::useless_format)]
#![allow(clippy::manual_hash_one)]
#![allow(clippy::get_first)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::borrowed_box)]
#![allow(clippy::iter_cloned_collect)]
#![allow(clippy::unnecessary_cast)]
#![allow(clippy::option_map_or_none)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::len_zero)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::useless_conversion)]
#![allow(clippy::unnecessary_map_or)]

// Reorganized module structure
pub mod core;
pub mod vm;
pub mod errors;
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
