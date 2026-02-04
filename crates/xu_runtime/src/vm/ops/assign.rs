//! Compound assignment operations for the VM.
//!
//! This module contains operations for:
//! - AddAssignName: Add-assign to a named variable
//! - AddAssignLocal: Add-assign to a local variable by index

use xu_ir::Bytecode;

use crate::core::heap::ManagedObject;
use crate::core::value::ValueExt;
use crate::core::Value;
use crate::vm::exception::throw_value;
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Throw undefined identifier error
#[inline(always)]
fn throw_undefined(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    name: &str,
) -> Result<Option<Flow>, String> {
    let err_val = Value::str(
        rt.heap.alloc(ManagedObject::Str(
            rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(name.to_string()))
                .into(),
        )),
    );
    if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
        return Ok(Some(flow));
    }
    Ok(None)
}

/// Perform add-assign operation and handle errors
#[inline(always)]
fn do_add_assign(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    cur: &mut Value,
    rhs: Value,
) -> Result<Option<Flow>, String> {
    if let Err(e) = cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap) {
        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
        if let Some(flow) = throw_value(rt, ip, handlers, stack, iters, pending, thrown, err_val) {
            return Ok(Some(flow));
        }
        return Ok(None);
    }
    Ok(None)
}

/// Execute Op::AddAssignName - add-assign to a named variable
#[inline(always)]
pub(crate) fn op_add_assign_name(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    idx: u32,
) -> Result<Option<Flow>, String> {
    let rhs = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let name = rt.get_const_str(idx, &bc.constants);
    let mut handled = false;

    if rt.locals.is_active() {
        // Try compiled locals first
        let local_idx = rt.current_func.as_ref().and_then(|func_name| {
            rt.compiled_locals_idx.get(func_name).and_then(|idxmap| idxmap.get(name).copied())
        });

        if let Some(local_idx) = local_idx {
            let Some(cur) = rt.get_local_by_index(local_idx) else {
                return throw_undefined(rt, stack, ip, handlers, iters, pending, thrown, name);
            };
            let mut cur = cur;
            if let Some(flow) = do_add_assign(rt, stack, ip, handlers, iters, pending, thrown, &mut cur, rhs)? {
                return Ok(Some(flow));
            }
            rt.set_local_by_index(local_idx, cur);
            handled = true;
        }

        if !handled {
            if let Some(cur) = rt.get_local(name) {
                let mut cur = cur;
                if let Some(flow) = do_add_assign(rt, stack, ip, handlers, iters, pending, thrown, &mut cur, rhs)? {
                    return Ok(Some(flow));
                }
                let _ = rt.set_local(name, cur);
                handled = true;
            }
        }
        if !handled {
            let Some(cur) = rt.env.get_cached(name) else {
                return throw_undefined(rt, stack, ip, handlers, iters, pending, thrown, name);
            };
            let mut cur = cur;
            if let Some(flow) = do_add_assign(rt, stack, ip, handlers, iters, pending, thrown, &mut cur, rhs)? {
                return Ok(Some(flow));
            }
            let assigned = rt.env.assign(name, cur);
            if !assigned {
                rt.env.define(name.to_string(), cur);
            }
        }
    } else {
        let Some(cur) = rt.env.get_cached(name) else {
            return throw_undefined(rt, stack, ip, handlers, iters, pending, thrown, name);
        };
        let mut cur = cur;
        if let Some(flow) = do_add_assign(rt, stack, ip, handlers, iters, pending, thrown, &mut cur, rhs)? {
            return Ok(Some(flow));
        }
        let assigned = rt.env.assign(name, cur);
        if !assigned {
            rt.env.define(name.to_string(), cur);
        }
    }
    Ok(None)
}

/// Execute Op::AddAssignLocal - add-assign to a local variable by index
#[inline(always)]
pub(crate) fn op_add_assign_local(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    idx: usize,
) -> Result<Option<Flow>, String> {
    let rhs = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let Some(mut cur) = rt.get_local_by_index(idx) else {
        return Err(format!("Undefined local variable index: {}", idx));
    };
    if cur.is_int() && rhs.is_int() {
        cur = Value::from_i64(cur.as_i64().wrapping_add(rhs.as_i64()));
    } else if let Some(flow) = do_add_assign(rt, stack, ip, handlers, iters, pending, thrown, &mut cur, rhs)? {
        return Ok(Some(flow));
    }
    rt.set_local_by_index(idx, cur);
    Ok(None)
}

/// Execute Op::IncLocal - increment a local variable by index
#[inline(always)]
pub(crate) fn op_inc_local(
    rt: &mut Runtime,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    stack: &mut Vec<Value>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    idx: usize,
) -> Result<Option<Flow>, String> {
    let Some(cur) = rt.get_local_by_index(idx) else {
        return Err(format!("Undefined local variable index: {}", idx));
    };
    if cur.is_int() {
        rt.set_local_by_index(idx, Value::from_i64(cur.as_i64().wrapping_add(1)));
    } else {
        let mut cur = cur;
        if let Some(flow) = do_add_assign(rt, stack, ip, handlers, iters, pending, thrown, &mut cur, Value::from_i64(1))? {
            return Ok(Some(flow));
        }
        rt.set_local_by_index(idx, cur);
    }
    Ok(None)
}
