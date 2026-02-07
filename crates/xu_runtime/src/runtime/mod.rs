//! Runtime module - the core execution engine.
//!
//! This module contains the Runtime struct and all its associated functionality,
//! organized into submodules for better maintainability.

mod config;
mod cache;
mod binary;
mod gc;
pub mod type_check;
mod type_system;
mod cache_manager;
mod object_pools;
mod locals;
mod precompile;
mod method_call;

// Re-export all public types
pub use config::{ExecResult, Flow, RuntimeConfig};
pub use cache::{ICSlot, MethodICSlot};
pub(crate) use cache::{DictCacheLast, DictCacheIntLast, DictInsertCacheLast};

// Re-export Text for use in submodules
pub use crate::core::text::Text;

// The main Runtime implementation is in core.rs
mod core;
pub use self::core::Runtime;
