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

use crate::core::heap::ManagedObject;
use crate::core::Value;
use crate::vm::exception::throw_value;
use crate::vm::ops::helpers::{exec_binary_op, peek_last_mut, pop_stack, try_throw_error};
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
    let b = pop_stack(stack)?;
    let a = peek_last_mut(stack)?;
    if a.is_int() && b.is_int() {
        let res = a.as_i64().wrapping_add(b.as_i64());
        *a = Value::from_i64(res);
        return Ok(None);
    }
    match add_with_heap(rt, *a, b) {
        Ok(r) => *a = r,
        Err(e) => {
            if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, e) {
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
    exec_binary_op(rt, stack, ip, handlers, iters, pending, thrown, BinaryOp::Sub)
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
    exec_binary_op(rt, stack, ip, handlers, iters, pending, thrown, BinaryOp::Mul)
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
    exec_binary_op(rt, stack, ip, handlers, iters, pending, thrown, BinaryOp::Div)
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
    let b = pop_stack(stack)?;
    let a = peek_last_mut(stack)?;
    // Fast path for integers
    if a.is_int() && b.is_int() {
        let bv = b.as_i64();
        if bv != 0 {
            *a = Value::from_i64(a.as_i64() % bv);
            return Ok(None);
        }
        // Division by zero - fall through to general path which will error
    }
    // Put b back and use general binary op
    stack.push(b);
    exec_binary_op(rt, stack, ip, handlers, iters, pending, thrown, BinaryOp::Mod)
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
    exec_binary_op(rt, stack, ip, handlers, iters, pending, thrown, BinaryOp::And)
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
    exec_binary_op(rt, stack, ip, handlers, iters, pending, thrown, BinaryOp::Or)
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
    let v = pop_stack(stack)?;
    if v.is_bool() {
        stack.push(Value::from_bool(!v.as_bool()));
        return Ok(None);
    }
    let err_msg = rt.error(xu_syntax::DiagnosticKind::InvalidUnaryOperand {
        op: '!',
        expected: "?".to_string(),
    });
    let err_val = Value::str(rt.alloc(ManagedObject::Str(err_msg.into())));
    if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
        return Ok(Some(flow));
    }
    Ok(None)
}

