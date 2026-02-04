//! Collection operations for the VM.
//!
//! This module contains operations for:
//! - ListNew: Create a new list
//! - TupleNew: Create a new tuple
//! - DictNew: Create a new dictionary
//! - ListAppend: Append items to a list
//! - MakeRange: Create a range

use smallvec::SmallVec;

use crate::core::heap::ManagedObject;
use crate::core::value::{DictKey, TAG_LIST, TAG_STR};
use crate::core::Value;
use crate::errors::messages::NOT_A_STRING;
use crate::util::to_i64;
use crate::Runtime;

/// Execute Op::ListNew - create a new list
#[inline(always)]
pub(crate) fn op_list_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    // Try to get a pooled list for small lists (≤8 elements)
    let mut items = if let Some(pooled) = rt.pools.get_small_list(n) {
        pooled
    } else {
        Vec::with_capacity(n)
    };
    for _ in 0..n {
        items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
    }
    items.reverse();
    let id = rt.heap.alloc(ManagedObject::List(items));
    stack.push(Value::list(id));
    Ok(())
}

/// Execute Op::TupleNew - create a new tuple
#[inline(always)]
pub(crate) fn op_tuple_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<bool, String> {
    if n == 0 {
        stack.push(Value::VOID);
        return Ok(true); // Signal to continue (skip ip increment)
    }
    // Try to get a pooled list for small tuples (≤8 elements)
    let mut items = if let Some(pooled) = rt.pools.get_small_list(n) {
        pooled
    } else {
        Vec::with_capacity(n)
    };
    for _ in 0..n {
        items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
    }
    items.reverse();
    let id = rt.heap.alloc(ManagedObject::Tuple(items));
    stack.push(Value::tuple(id));
    Ok(false)
}

/// Execute Op::DictNew - create a new dictionary
#[inline(always)]
pub(crate) fn op_dict_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    let mut map = crate::core::value::dict_with_capacity(n);
    for _ in 0..n {
        let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
        let k = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
        let key = if k.get_tag() == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(k.as_obj_id()) {
                DictKey::from_text(s)
            } else {
                return Err(NOT_A_STRING.into());
            }
        } else if k.is_int() {
            DictKey::Int(k.as_i64())
        } else {
            return Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired));
        };
        map.map.insert(key, v);
    }
    let id = rt.heap.alloc(ManagedObject::Dict(map));
    stack.push(Value::dict(id));
    Ok(())
}

/// Execute Op::ListAppend - append items to a list
#[inline(always)]
pub(crate) fn op_list_append(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    n: usize,
) -> Result<(), String> {
    let mut items: SmallVec<[Value; 8]> = SmallVec::with_capacity(n);
    for _ in 0..n {
        items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
    }
    items.reverse();
    let recv = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;

    if recv.get_tag() != TAG_LIST {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "add".to_string(),
            ty: recv.type_name().to_string(),
        }));
    }

    let id = recv.as_obj_id();
    if let ManagedObject::List(vs) = rt.heap.get_mut(id) {
        vs.reserve(items.len());
        for v in items {
            vs.push(v);
        }
    }
    stack.push(recv);
    Ok(())
}

/// Execute Op::MakeRange - create a range object
#[inline(always)]
pub(crate) fn op_make_range(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    inclusive: bool,
) -> Result<(), String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let start = to_i64(&a)?;
    let end = to_i64(&b)?;
    let id = rt.heap.alloc(ManagedObject::Range(start, end, inclusive));
    stack.push(Value::range(id));
    Ok(())
}
