//! Math operations for the VM.
//!
//! This module contains arithmetic operations:
//! - Sub: Subtraction
//! - Mul: Multiplication
//! - Div: Division
//! - Mod: Modulo
//! - And: Logical AND
//! - Or: Logical OR

use crate::core::heap::ManagedObject;
use crate::core::value::ValueExt;
use crate::core::Value;
use crate::vm::exception::throw_value;
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};
use xu_ir::BinaryOp;

/// Execute Op::Sub - subtraction
#[inline(always)]
pub(crate) fn op_sub(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    match a.bin_op(BinaryOp::Sub, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// Execute Op::Mul - multiplication
#[inline(always)]
pub(crate) fn op_mul(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    match a.bin_op(BinaryOp::Mul, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// Execute Op::Div - division
#[inline(always)]
pub(crate) fn op_div(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    match a.bin_op(BinaryOp::Div, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// Execute Op::Mod - modulo
#[inline(always)]
pub(crate) fn op_mod(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    // Fast path for integers
    if a.is_int() && b.is_int() {
        let bv = b.as_i64();
        if bv != 0 {
            *a = Value::from_i64(a.as_i64() % bv);
            return Ok(None);
        } else {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str("Division by zero".into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    match a.bin_op(BinaryOp::Mod, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// Execute Op::And - logical AND
#[inline(always)]
pub(crate) fn op_and(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    match a.bin_op(BinaryOp::And, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    Ok(None)
}

/// Execute Op::Or - logical OR
#[inline(always)]
pub(crate) fn op_or(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    match a.bin_op(BinaryOp::Or, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
            return Ok(None);
        }
    }
    Ok(None)
}
