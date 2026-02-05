//! Helper functions for VM operations.
//!
//! This module provides common utility functions to reduce code duplication
//! across the ops modules, particularly for stack operations and error handling.

use crate::core::heap::ManagedObject;
use crate::core::Value;
use crate::vm::exception::throw_value;
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Pop a single value from the stack.
#[inline(always)]
pub(crate) fn pop_stack(stack: &mut Vec<Value>) -> Result<Value, String> {
    stack.pop().ok_or_else(|| "Stack underflow".to_string())
}

/// Pop two values from the stack (returns (first_popped, second_popped) = (b, a) for a op b).
#[inline(always)]
pub(crate) fn pop2_stack(stack: &mut Vec<Value>) -> Result<(Value, Value), String> {
    let b = pop_stack(stack)?;
    let a = pop_stack(stack)?;
    Ok((a, b))
}

/// Get a mutable reference to the last element on the stack.
#[inline(always)]
pub(crate) fn peek_last_mut(stack: &mut Vec<Value>) -> Result<&mut Value, String> {
    stack.last_mut().ok_or_else(|| "Stack underflow".to_string())
}

/// Get a reference to the last element on the stack.
#[inline(always)]
pub(crate) fn peek_last(stack: &[Value]) -> Result<&Value, String> {
    stack.last().ok_or_else(|| "Stack underflow".to_string())
}

/// Create an error value from a string and attempt to throw it.
/// Returns Some(Flow) if the exception propagates, None if it was caught.
#[inline(always)]
pub(crate) fn try_throw_error(
    rt: &mut Runtime,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    stack: &mut Vec<Value>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    err: String,
) -> Option<Flow> {
    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(err.into())));
    throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val)
}
