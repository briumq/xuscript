//! Comparison operations for the VM.
//!
//! This module contains operations for:
//! - Eq: Equality comparison
//! - Ne: Inequality comparison
//! - Gt: Greater than comparison
//! - Lt: Less than comparison
//! - Ge: Greater than or equal comparison
//! - Le: Less than or equal comparison

use crate::core::heap::ManagedObject;
use crate::core::value::{ValueExt, TAG_STR};
use crate::core::Value;
use crate::vm::exception::throw_value;
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Execute Op::Eq - equality comparison
#[inline(always)]
pub(crate) fn op_eq(rt: &Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack
        .last_mut()
        .ok_or_else(|| "Stack underflow".to_string())?;
    *a = Value::from_bool(rt.values_equal(a, &b));
    Ok(())
}

/// Execute Op::Ne - inequality comparison
#[inline(always)]
pub(crate) fn op_ne(rt: &Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack
        .last_mut()
        .ok_or_else(|| "Stack underflow".to_string())?;
    *a = Value::from_bool(!rt.values_equal(a, &b));
    Ok(())
}

/// Execute Op::Gt - greater than comparison
#[inline(always)]
pub(crate) fn op_gt(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack
        .last_mut()
        .ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.get_tag() == TAG_STR {
        let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        *a = Value::from_bool(sa > sb);
    } else {
        match a.bin_op(xu_ir::BinaryOp::Gt, b) {
            Ok(r) => *a = r,
            Err(e) => {
                let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                if let Some(flow) = throw_value(
                    rt, ip, handlers, stack, iters, pending, thrown, err_val,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Execute Op::Lt - less than comparison
#[inline(always)]
pub(crate) fn op_lt(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack
        .last_mut()
        .ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.get_tag() == TAG_STR {
        let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        *a = Value::from_bool(sa < sb);
    } else {
        match a.bin_op(xu_ir::BinaryOp::Lt, b) {
            Ok(r) => *a = r,
            Err(e) => {
                let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                if let Some(flow) = throw_value(
                    rt, ip, handlers, stack, iters, pending, thrown, err_val,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Execute Op::Ge - greater than or equal comparison
#[inline(always)]
pub(crate) fn op_ge(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack
        .last_mut()
        .ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.get_tag() == TAG_STR {
        let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        *a = Value::from_bool(sa >= sb);
    } else {
        match a.bin_op(xu_ir::BinaryOp::Ge, b) {
            Ok(r) => *a = r,
            Err(e) => {
                let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                if let Some(flow) = throw_value(
                    rt, ip, handlers, stack, iters, pending, thrown, err_val,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Execute Op::Le - less than or equal comparison
#[inline(always)]
pub(crate) fn op_le(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack
        .last_mut()
        .ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.get_tag() == TAG_STR {
        let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
            s.as_str()
        } else {
            ""
        };
        *a = Value::from_bool(sa <= sb);
    } else {
        match a.bin_op(xu_ir::BinaryOp::Le, b) {
            Ok(r) => *a = r,
            Err(e) => {
                let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                if let Some(flow) = throw_value(
                    rt, ip, handlers, stack, iters, pending, thrown, err_val,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}
