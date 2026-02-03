//! Math operations for the VM.
//!
//! This module contains arithmetic operations:
//! - Add: Addition
//! - Sub: Subtraction
//! - Mul: Multiplication
//! - Div: Division
//! - Mod: Modulo
//! - And: Logical AND
//! - Or: Logical OR
//! - Not: Logical NOT
//! - Inc: Increment

use crate::core::heap::ManagedObject;
use crate::core::value::ValueExt;
use crate::core::Value;
use crate::vm::exception::throw_value;
use crate::vm::stack::{add_with_heap, Handler, IterState, Pending};
use crate::{Flow, Runtime};
use xu_ir::BinaryOp;

/// Execute Op::Add - addition (with string concatenation support)
#[inline(always)]
pub(crate) fn op_add(
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
    if a.is_int() && b.is_int() {
        let res = a.as_i64().wrapping_add(b.as_i64());
        *a = Value::from_i64(res);
        return Ok(None);
    }
    match add_with_heap(rt, *a, b) {
        Ok(r) => *a = r,
        Err(e) => {
            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
            if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
                return Ok(Some(flow));
            }
        }
    }
    Ok(None)
}

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

/// Execute Op::Not - logical NOT
#[inline(always)]
pub(crate) fn op_not(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if v.is_bool() {
        stack.push(Value::from_bool(!v.as_bool()));
        return Ok(None);
    }
    let err_msg = rt.error(xu_syntax::DiagnosticKind::InvalidUnaryOperand {
        op: '!',
        expected: "?".to_string(),
    });
    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(err_msg.into())));
    if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
        return Ok(Some(flow));
    }
    Ok(None)
}

/// Execute Op::Inc - increment top of stack
#[inline(always)]
pub(crate) fn op_inc(rt: &Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let a = stack.last_mut().ok_or_else(|| "Stack underflow".to_string())?;
    if a.is_int() {
        let v = a.as_i64().saturating_add(1);
        *a = Value::from_i64(v);
    } else if a.is_f64() {
        let v = a.as_f64() + 1.0;
        *a = Value::from_f64(v);
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::InvalidUnaryOperand {
            op: '+',
            expected: "number".to_string(),
        }));
    }
    Ok(())
}

