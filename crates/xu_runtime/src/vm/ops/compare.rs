//! Comparison operations for the VM.
//!
//! This module contains operations for:
//! - Eq: Equality comparison
//! - Ne: Inequality comparison
//! - Gt/Lt/Ge/Le: Ordered comparisons (unified implementation)

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

/// Comparison kind for ordered comparisons
#[derive(Clone, Copy)]
pub(crate) enum CmpKind {
    Gt,
    Lt,
    Ge,
    Le,
}

impl CmpKind {
    #[inline(always)]
    fn compare_str(self, a: &str, b: &str) -> bool {
        match self {
            CmpKind::Gt => a > b,
            CmpKind::Lt => a < b,
            CmpKind::Ge => a >= b,
            CmpKind::Le => a <= b,
        }
    }

    #[inline(always)]
    fn binary_op(self) -> xu_ir::BinaryOp {
        match self {
            CmpKind::Gt => xu_ir::BinaryOp::Gt,
            CmpKind::Lt => xu_ir::BinaryOp::Lt,
            CmpKind::Ge => xu_ir::BinaryOp::Ge,
            CmpKind::Le => xu_ir::BinaryOp::Le,
        }
    }
}

/// Unified ordered comparison (Gt/Lt/Ge/Le)
#[inline(always)]
pub(crate) fn op_cmp(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    kind: CmpKind,
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
        *a = Value::from_bool(kind.compare_str(sa, sb));
    } else {
        match a.bin_op(kind.binary_op(), b) {
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

// Convenience wrappers for dispatch.rs compatibility
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
    op_cmp(rt, stack, ip, handlers, iters, pending, thrown, CmpKind::Gt)
}

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
    op_cmp(rt, stack, ip, handlers, iters, pending, thrown, CmpKind::Lt)
}

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
    op_cmp(rt, stack, ip, handlers, iters, pending, thrown, CmpKind::Ge)
}

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
    op_cmp(rt, stack, ip, handlers, iters, pending, thrown, CmpKind::Le)
}
