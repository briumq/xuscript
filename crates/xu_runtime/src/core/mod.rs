//! Core runtime infrastructure.
//!
//! This module contains the fundamental types and systems for the runtime:
//! - `Value` - The runtime value representation
//! - `Heap` and GC - Garbage collection and memory management
//! - `Env` and `Scope` - Environment and scope management
//! - `Text` - Optimized string type
//! - `LocalSlots` - Local variable slot allocation

pub mod gc;
pub mod text;
pub mod value;
pub mod env;
pub(crate) mod slot_allocator;

pub use value::*;
pub use env::{Env, Scope};
