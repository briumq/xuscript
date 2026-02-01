//! Core types for XuScript runtime.
//!
//! This crate contains the fundamental types that are independent of the runtime:
//! - `Text` - Optimized string type with small string optimization
//! - `Value` - NaN-boxed runtime value representation
//! - `ObjectId` - Handle to heap-allocated objects
//! - `Capabilities` - System capability traits (Clock, FileSystem, RNG)

pub mod text;
pub mod capabilities;
pub mod gc;
pub mod value;

pub use text::Text;
pub use capabilities::{Capabilities, Clock, FileStat, FileSystem, RngAlgorithm};
pub use gc::ObjectId;
pub use value::Value;
