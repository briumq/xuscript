//! Garbage collection infrastructure - base types.

/// Handle to a heap-allocated object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub usize);
