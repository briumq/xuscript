//! Module loading and import system.

mod loader;
mod import;

pub use loader::{ImportStamp, ModuleLoader, StdModuleLoader};
pub(crate) use import::{import_path, infer_module_alias, ImportParseCacheEntry};
