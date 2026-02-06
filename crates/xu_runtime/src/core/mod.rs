//! Core runtime infrastructure.
//!
//! This module contains the fundamental types and systems for the runtime:
//! - `Value` - The runtime value representation
//! - `Heap` and GC - Garbage collection and memory management
//! - `Env` and `Scope` - Environment and scope management
//! - `Text` - Optimized string type
//! - `LocalSlots` - Local variable slot allocation

pub mod heap;
pub mod text;
pub mod value;
pub mod env;
pub(crate) mod slot_allocator;

#[cfg(feature = "generational-gc")]
pub mod generational_heap;

pub use value::*;
pub use heap::ObjectId;
pub use text::Text;
pub use env::{Env, Scope};

// Type alias for the active heap implementation
// This allows code to use `ActiveHeap` instead of `Heap` or `GenerationalHeap`
#[cfg(not(feature = "generational-gc"))]
pub type ActiveHeap = heap::Heap;

#[cfg(feature = "generational-gc")]
pub type ActiveHeap = generational_heap::GenerationalHeap;
