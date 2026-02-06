//! VM stack types and helper functions.
//!
//! This module contains the core types used by the bytecode interpreter:
//! - `IterState`: Iterator state for foreach loops
//! - `Handler`: Exception handler state
//! - `Pending`: Pending operations (for exception handling)

use crate::core::heap::{ManagedObject, ObjectId};
use crate::core::Value;
use crate::core::value::ValueExt;
use crate::errors::messages::NOT_A_STRING;
use crate::util::Appendable;
use crate::Runtime;
use xu_ir::Op;

use crate::core::value::TAG_STR;

/// Iterator state for foreach loops in the VM.
pub(crate) enum IterState {
    /// Iterating over a list
    List {
        id: ObjectId,
        idx: usize,
        len: usize,
    },
    /// Iterating over a range
    Range {
        cur: i64,
        end: i64,
        step: i64,
        inclusive: bool,
    },
    /// Iterating over dict keys only
    Dict {
        keys: Vec<Value>,
        idx: usize,
    },
    /// Iterating over dict key-value pairs (for `for (k, v) in dict` syntax)
    DictKV {
        items: Vec<Value>,
        idx: usize,
    },
}

/// Exception handler state.
pub(crate) struct Handler {
    pub(crate) catch_ip: Option<usize>,
    pub(crate) finally_ip: Option<usize>,
    pub(crate) stack_len: usize,
    pub(crate) iter_len: usize,
    pub(crate) env_depth: usize,
}

/// Pending operations for exception handling.
pub(crate) enum Pending {
    Throw(#[allow(dead_code)] Value),
}

/// Add two values, handling string concatenation with heap allocation.
#[inline(always)]
pub(crate) fn add_with_heap(rt: &mut Runtime, a: Value, b: Value) -> Result<Value, String> {
    let at = a.get_tag();
    let bt = b.get_tag();

    // Fast path: string + int (common pattern like "k" + i)
    if at == TAG_STR && b.is_int() {
        let result = {
            let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s
            } else {
                return Err(NOT_A_STRING.into());
            };
            crate::core::text::Text::concat_str_int(sa, b.as_i64())
        };
        return Ok(Value::str(rt.alloc(ManagedObject::Str(result))));
    }

    // Fast path: int + string
    if a.is_int() && bt == TAG_STR {
        let result = {
            let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                s
            } else {
                return Err(NOT_A_STRING.into());
            };
            crate::core::text::Text::concat_int_str(a.as_i64(), sb)
        };
        return Ok(Value::str(rt.alloc(ManagedObject::Str(result))));
    }

    if at == TAG_STR && bt == TAG_STR {
        // Fast path: both are strings - use Text::concat2
        let result = {
            let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s
            } else {
                return Err(NOT_A_STRING.into());
            };
            let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                s
            } else {
                return Err(NOT_A_STRING.into());
            };
            crate::core::text::Text::concat2(sa, sb)
        };
        Ok(Value::str(rt.alloc(ManagedObject::Str(result))))
    } else if at == TAG_STR || bt == TAG_STR {
        // Slow path: one is string, one is not
        // Pre-calculate lengths to avoid reallocations
        let a_len = if at == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s.len()
            } else {
                return Err(NOT_A_STRING.into());
            }
        } else {
            20 // estimate for non-string
        };
        let b_len = if bt == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                s.len()
            } else {
                return Err(NOT_A_STRING.into());
            }
        } else {
            20 // estimate for non-string
        };

        // Pre-allocate with exact capacity
        let mut result = String::with_capacity(a_len + b_len);

        // Append a
        if at == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                result.push_str(s.as_str());
            }
        } else {
            result.append_value(&a, &rt.heap);
        }

        // Append b
        if bt == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                result.push_str(s.as_str());
            }
        } else {
            result.append_value(&b, &rt.heap);
        }

        Ok(Value::str(rt.alloc(ManagedObject::Str(result.into()))))
    } else {
        a.bin_op(xu_ir::BinaryOp::Add, b)
    }
}

/// Generate a stack underflow error message.
#[inline(always)]
pub(crate) fn stack_underflow(ip: usize, op: &Op) -> String {
    format!("Stack underflow at ip={ip} op={op:?}")
}
