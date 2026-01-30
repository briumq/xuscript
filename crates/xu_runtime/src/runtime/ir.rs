use crate::value::TAG_BUILDER;

use smallvec::SmallVec;
use xu_ir::{Bytecode, Op};

use super::appendable::Appendable;
use crate::Text;
use crate::Value;
use crate::gc::ManagedObject;
use crate::value::{DictKey, Function, TAG_DICT, TAG_LIST, TAG_RANGE, TAG_STR};

use super::util::{to_i64, type_matches, value_to_string};
use super::{Flow, Runtime};
use crate::runtime::ir_throw::throw_value;

mod dict_ops;
mod fast;

pub(super) use fast::{run_bytecode_fast, run_bytecode_fast_params_only};

pub(super) enum IterState {
    List {
        id: crate::gc::ObjectId,
        idx: usize,
        len: usize,
    },
    Range {
        cur: i64,
        end: i64,
        step: i64,
        inclusive: bool,
    },
    Dict {
        keys: Vec<Value>,
        idx: usize,
    },
}

pub(super) struct Handler {
    pub(super) catch_ip: Option<usize>,
    pub(super) finally_ip: Option<usize>,
    pub(super) stack_len: usize,
    pub(super) iter_len: usize,
    pub(super) env_depth: usize,
}

#[allow(dead_code)]
pub(super) enum Pending {
    Throw(Value),
}

#[inline(always)]
fn add_with_heap(rt: &mut Runtime, a: Value, b: Value) -> Result<Value, String> {
    let at = a.get_tag();
    let bt = b.get_tag();
    if at == TAG_STR || bt == TAG_STR {
        // Pre-calculate lengths to avoid reallocations
        let a_len = if at == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s.len()
            } else {
                return Err("Not a string".into());
            }
        } else {
            20 // estimate for non-string
        };
        let b_len = if bt == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                s.len()
            } else {
                return Err("Not a string".into());
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

        Ok(Value::str(rt.heap.alloc(ManagedObject::Str(result.into()))))
    } else {
        a.bin_op(xu_ir::BinaryOp::Add, b)
    }
}

#[inline(always)]
fn stack_underflow(ip: usize, op: &Op) -> String {
    format!("Stack underflow at ip={ip} op={op:?}")
}
pub(super) fn run_bytecode(rt: &mut Runtime, bc: &Bytecode) -> Result<Flow, String> {
    let mut stack = rt
        .vm_stack_pool
        .pop()
        .unwrap_or_else(|| Vec::with_capacity(32));
    stack.clear();
    let mut iters = rt
        .vm_iters_pool
        .pop()
        .unwrap_or_else(|| Vec::with_capacity(4));
    iters.clear();
    let mut handlers = rt
        .vm_handlers_pool
        .pop()
        .unwrap_or_else(|| Vec::with_capacity(4));
    handlers.clear();

    // Register this VM stack for GC protection
    let stack_ptr: *const Vec<Value> = &stack;
    rt.active_vm_stacks.push(stack_ptr);

    struct VmScratchReturn {
        rt: *mut Runtime,
        stack: *mut Vec<Value>,
        iters: *mut Vec<IterState>,
        handlers: *mut Vec<Handler>,
    }

    impl Drop for VmScratchReturn {
        fn drop(&mut self) {
            unsafe {
                // Unregister VM stack from GC protection
                (*self.rt).active_vm_stacks.pop();
                (*self.rt)
                    .vm_stack_pool
                    .push(std::mem::take(&mut *self.stack));
                (*self.rt)
                    .vm_iters_pool
                    .push(std::mem::take(&mut *self.iters));
                (*self.rt)
                    .vm_handlers_pool
                    .push(std::mem::take(&mut *self.handlers));
            }
        }
    }

    let _scratch_return = VmScratchReturn {
        rt: rt as *mut Runtime,
        stack: &mut stack,
        iters: &mut iters,
        handlers: &mut handlers,
    };
    let mut pending: Option<Pending> = None;
    let mut thrown: Option<Value> = None;

    let mut ip: usize = 0;
    let ops = &bc.ops;
    let ops_len = ops.len();

    while ip < ops_len {
        let op = unsafe { ops.get_unchecked(ip) };
        rt.stmt_count = rt.stmt_count.wrapping_add(1);
        if rt.stmt_count & 127 == 0 {
            rt.maybe_gc_with_roots(&stack);
        }
        match op {
            Op::ConstInt(i) => stack.push(Value::from_i64(*i)),
            Op::ConstFloat(f) => stack.push(Value::from_f64(*f)),
            Op::Const(idx) => {
                let c = rt.get_constant(*idx, &bc.constants);
                match c {
                    xu_ir::Constant::Str(s) => {
                        let bc_ptr = bc as *const Bytecode as usize;
                        stack.push(rt.get_string_const(bc_ptr, *idx, s));
                    }
                    xu_ir::Constant::Int(i) => stack.push(Value::from_i64(*i)),
                    xu_ir::Constant::Float(f) => stack.push(Value::from_f64(*f)),
                    _ => return Err("Unexpected constant type in VM loop".into()),
                }
            }
            Op::ConstBool(b) => stack.push(Value::from_bool(*b)),
            Op::ConstNull => stack.push(Value::VOID),
            Op::Pop => {
                let _ = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
            }
            Op::LoadLocal(idx) => {
                let Some(val) = rt.get_local_by_index(*idx) else {
                    return Err(format!("Undefined local variable index: {}", idx));
                };
                stack.push(val);
            }
            Op::StoreLocal(idx) => {
                let val = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                if !rt.set_local_by_index(*idx, val) {
                    // Define up to idx if not exist
                    while rt.get_local_by_index(*idx).is_none() {
                        rt.define_local(format!("_tmp_{}", idx), Value::VOID);
                    }
                    rt.set_local_by_index(*idx, val);
                }
            }
            Op::Use(path_idx, alias_idx) => {
                let path = rt.get_const_str(*path_idx, &bc.constants);
                let alias = rt.get_const_str(*alias_idx, &bc.constants);
                match super::modules::import_path(rt, path) {
                    Ok(module_obj) => {
                        rt.env.define(alias.to_string(), module_obj);
                    }
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        ip += 1;
                        continue;
                    }
                }
            }
            Op::Add => {
                let b = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let a = stack.last_mut().ok_or_else(|| stack_underflow(ip, op))?;
                if a.is_int() && b.is_int() {
                    let res = a.as_i64().wrapping_add(b.as_i64());
                    *a = Value::from_i64(res);
                } else {
                    match add_with_heap(rt, *a, b) {
                        Ok(r) => *a = r,
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    ip += 1;
                    continue;
                }
            }
            Op::AddAssignName(idx) => {
                let rhs = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let name = rt.get_const_str(*idx, &bc.constants);
                let mut handled = false;

                if rt.locals.is_active() {
                    if let Some(func_name) = &rt.current_func {
                        if let Some(idxmap) = rt.compiled_locals_idx.get(func_name) {
                            if let Some(idx) = idxmap.get(name) {
                                let Some(cur) = rt.get_local_by_index(*idx) else {
                                    let err_val = Value::str(
                                        rt.heap.alloc(ManagedObject::Str(
                                            rt.error(
                                                xu_syntax::DiagnosticKind::UndefinedIdentifier(
                                                    name.to_string(),
                                                ),
                                            )
                                            .into(),
                                        )),
                                    );
                                    if let Some(flow) = throw_value(
                                        rt,
                                        &mut ip,
                                        &mut handlers,
                                        &mut stack,
                                        &mut iters,
                                        &mut pending,
                                        &mut thrown,
                                        err_val,
                                    ) {
                                        return Ok(flow);
                                    }
                                    continue;
                                };
                                let mut cur = cur;
                                if let Err(e) =
                                    cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap)
                                {
                                    let err_val =
                                        Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                                    if let Some(flow) = throw_value(
                                        rt,
                                        &mut ip,
                                        &mut handlers,
                                        &mut stack,
                                        &mut iters,
                                        &mut pending,
                                        &mut thrown,
                                        err_val,
                                    ) {
                                        return Ok(flow);
                                    }
                                    continue;
                                }
                                rt.set_local_by_index(*idx, cur);
                                handled = true;
                            }
                        }
                    }
                    if !handled {
                        if let Some(cur) = rt.get_local(name) {
                            let mut cur = cur;
                            if let Err(e) =
                                cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap)
                            {
                                let err_val =
                                    Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                                if let Some(flow) = throw_value(
                                    rt,
                                    &mut ip,
                                    &mut handlers,
                                    &mut stack,
                                    &mut iters,
                                    &mut pending,
                                    &mut thrown,
                                    err_val,
                                ) {
                                    return Ok(flow);
                                }
                                continue;
                            }
                            let _ = rt.set_local(name, cur);
                            handled = true;
                        }
                    }
                    if handled {
                        // Fall through
                    } else {
                        let Some(cur) = rt.env.get_cached(name) else {
                            let err_val = Value::str(
                                rt.heap.alloc(ManagedObject::Str(
                                    rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(
                                        name.to_string(),
                                    ))
                                    .into(),
                                )),
                            );
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        };
                        let mut cur = cur;
                        if let Err(e) = cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap) {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                        let assigned = rt.env.assign(name, cur);
                        if !assigned {
                            rt.env.define(name.to_string(), cur);
                        }
                    }
                } else {
                    let Some(cur) = rt.env.get_cached(name) else {
                        let err_val = Value::str(
                            rt.heap.alloc(ManagedObject::Str(
                                rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(
                                    name.to_string(),
                                ))
                                .into(),
                            )),
                        );
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    };
                    let mut cur = cur;
                    if let Err(e) = cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap) {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                    let assigned = rt.env.assign(name, cur);
                    if !assigned {
                        rt.env.define(name.to_string(), cur);
                    }
                }
            }
            Op::AddAssignLocal(idx) => {
                let rhs = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let Some(mut cur) = rt.get_local_by_index(*idx) else {
                    return Err(format!("Undefined local variable index: {}", idx));
                };
                if cur.is_int() && rhs.is_int() {
                    cur = Value::from_i64(cur.as_i64().wrapping_add(rhs.as_i64()));
                } else {
                    if let Err(e) = cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap) {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
                rt.set_local_by_index(*idx, cur);
            }
            Op::IncLocal(idx) => {
                let Some(cur) = rt.get_local_by_index(*idx) else {
                    return Err(format!("Undefined local variable index: {}", idx));
                };
                if cur.is_int() {
                    rt.set_local_by_index(*idx, Value::from_i64(cur.as_i64().wrapping_add(1)));
                } else {
                    let mut cur = cur;
                    if let Err(e) =
                        cur.bin_op_assign(xu_ir::BinaryOp::Add, Value::from_i64(1), &mut rt.heap)
                    {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                    rt.set_local_by_index(*idx, cur);
                }
            }
            Op::Sub => {
                let b = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let a = stack.last_mut().ok_or_else(|| stack_underflow(ip, op))?;
                match a.bin_op(xu_ir::BinaryOp::Sub, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Mul => {
                let b = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let a = stack.last_mut().ok_or_else(|| stack_underflow(ip, op))?;
                match a.bin_op(xu_ir::BinaryOp::Mul, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Div => {
                let b = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let a = stack.last_mut().ok_or_else(|| stack_underflow(ip, op))?;
                match a.bin_op(xu_ir::BinaryOp::Div, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Mod => {
                let b = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let a = stack.last_mut().ok_or_else(|| stack_underflow(ip, op))?;
                // Fast path for integers
                if a.is_int() && b.is_int() {
                    let bv = b.as_i64();
                    if bv != 0 {
                        *a = Value::from_i64(a.as_i64() % bv);
                    } else {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str("Division by zero".into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                } else {
                    match a.bin_op(xu_ir::BinaryOp::Mod, b) {
                        Ok(r) => *a = r,
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::StrAppend => {
                let b = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                let a = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                if a.get_tag() == TAG_STR {
                    let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        return Err("Not a string".into());
                    };
                    sa.append_value(&b, &rt.heap);
                    stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
                } else {
                    match add_with_heap(rt, a, b) {
                        Ok(r) => stack.push(r),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::StrAppendNull => {
                let a = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                if a.get_tag() == TAG_STR {
                    let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        return Err("Not a string".into());
                    };
                    sa.append_null();
                    stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
                } else {
                    match add_with_heap(rt, a, Value::VOID) {
                        Ok(r) => stack.push(r),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::StrAppendBool => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if a.get_tag() == TAG_STR && b.is_bool() {
                    let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        return Err("Not a string".into());
                    };
                    sa.append_bool(b.as_bool());
                    stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
                } else {
                    match add_with_heap(rt, a, b) {
                        Ok(r) => stack.push(r),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::StrAppendInt => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if a.get_tag() == TAG_STR && b.is_int() {
                    let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        return Err("Not a string".into());
                    };
                    sa.append_i64(b.as_i64());
                    stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
                } else {
                    match add_with_heap(rt, a, b) {
                        Ok(r) => stack.push(r),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::StrAppendFloat => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if a.get_tag() == TAG_STR && b.is_f64() {
                    let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        return Err("Not a string".into());
                    };
                    sa.append_f64(b.as_f64());
                    stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
                } else {
                    match add_with_heap(rt, a, b) {
                        Ok(r) => stack.push(r),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::StrAppendStr => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if a.get_tag() == TAG_STR && b.get_tag() == TAG_STR {
                    let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        return Err("Not a string".into());
                    };
                    if let ManagedObject::Str(sb) = rt.heap.get(b.as_obj_id()) {
                        sa.append_str(sb.as_str());
                    }
                    stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
                } else {
                    match add_with_heap(rt, a, b) {
                        Ok(r) => stack.push(r),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::Eq => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                *a = Value::from_bool(rt.values_equal(a, &b));
            }
            Op::Ne => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                *a = Value::from_bool(!rt.values_equal(a, &b));
            }
            Op::And => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                match a.bin_op(xu_ir::BinaryOp::And, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Or => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                match a.bin_op(xu_ir::BinaryOp::Or, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Gt => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                match a.bin_op(xu_ir::BinaryOp::Gt, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Lt => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                match a.bin_op(xu_ir::BinaryOp::Lt, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Ge => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                match a.bin_op(xu_ir::BinaryOp::Ge, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Le => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
                match a.bin_op(xu_ir::BinaryOp::Le, b) {
                    Ok(r) => *a = r,
                    Err(e) => {
                        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(
                            rt,
                            &mut ip,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &mut thrown,
                            err_val,
                        ) {
                            return Ok(flow);
                        }
                        continue;
                    }
                }
            }
            Op::Not => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if v.is_bool() {
                    stack.push(Value::from_bool(!v.as_bool()));
                } else {
                    let err_msg = rt.error(xu_syntax::DiagnosticKind::InvalidUnaryOperand {
                        op: '!',
                        expected: "?".to_string(),
                    });
                    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(err_msg.into())));
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
            }
            Op::Jump(to) => {
                ip = *to;
                continue;
            }
            Op::JumpIfFalse(to) => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if v.is_bool() {
                    if !v.as_bool() {
                        ip = *to;
                        continue;
                    }
                } else {
                    let msg = rt.error(xu_syntax::DiagnosticKind::InvalidConditionType(
                        v.type_name().to_string(),
                    ));
                    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(msg.into())));
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
            }
            Op::LoadName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                let v = if rt.locals.is_active() {
                    if let Some(v) = rt.get_local(name) {
                        v
                    } else {
                        rt.env.get_cached(name).ok_or_else(|| {
                            rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(
                                name.to_string(),
                            ))
                        })?
                    }
                } else {
                    rt.env.get_cached(name).ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(
                            name.to_string(),
                        ))
                    })?
                };
                stack.push(v);
            }
            Op::StoreName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                let v = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                if rt.locals.is_active() {
                    if !rt.set_local(name, v) {
                        rt.define_local(name.to_string(), v);
                    }
                } else {
                    if !rt.env.assign(name, v) {
                        rt.env.define(name.to_string(), v);
                    }
                }
            }
            Op::AssertType(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                let v = stack.last().ok_or_else(|| "Stack underflow".to_string())?;
                if !type_matches(name, v, &rt.heap) {
                    let msg = rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                        expected: name.to_string(),
                        actual: v.type_name().to_string(),
                    });
                    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(msg.into())));
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
            }
            Op::DefineStruct(idx) => {
                let c = rt.get_constant(*idx, &bc.constants);
                if let xu_ir::Constant::Struct(def) = c {
                    let layout: std::rc::Rc<[String]> =
                        def.fields.iter().map(|f| f.name.clone()).collect();
                    rt.struct_layouts.insert(def.name.clone(), layout);
                    rt.structs.insert(def.name.clone(), def.clone());
                }
            }
            Op::DefineEnum(idx) => {
                let c = rt.get_constant(*idx, &bc.constants);
                if let xu_ir::Constant::Enum(def) = c {
                    rt.enums.insert(def.name.clone(), def.variants.to_vec());
                }
            }
            Op::StructInit(t_idx, n_idx) => {
                let ty = rt.get_const_str(*t_idx, &bc.constants).to_string();
                let fields = rt.get_const_names(*n_idx, &bc.constants).to_vec();
                let layout = if let Some(l) = rt.struct_layouts.get(&ty).cloned() {
                    l
                } else {
                    let err_val = Value::str(
                        rt.heap.alloc(ManagedObject::Str(
                            rt.error(xu_syntax::DiagnosticKind::UnknownStruct(ty.clone()))
                                .into(),
                        )),
                    );
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                };

                let mut values = vec![Value::VOID; layout.len()];

                // Apply default values from struct definition
                if let Some(def) = rt.structs.get(&ty).cloned() {
                    for (i, field) in def.fields.iter().enumerate() {
                        if let Some(ref default_expr) = field.default {
                            if i < values.len() {
                                match rt.eval_expr(default_expr) {
                                    Ok(v) => values[i] = v,
                                    Err(e) => {
                                        let err_val = Value::str(
                                            rt.heap.alloc(ManagedObject::Str(e.into())),
                                        );
                                        if let Some(flow) = throw_value(
                                            rt,
                                            &mut ip,
                                            &mut handlers,
                                            &mut stack,
                                            &mut iters,
                                            &mut pending,
                                            &mut thrown,
                                            err_val,
                                        ) {
                                            return Ok(flow);
                                        }
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                }

                // Override with explicitly provided field values
                for k in fields.iter().rev() {
                    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                    if let Some(pos) = layout.iter().position(|f| f == k) {
                        values[pos] = v;
                    }
                }

                let id = rt
                    .heap
                    .alloc(ManagedObject::Struct(Box::new(crate::value::StructInstance {
                        ty: ty.clone(),
                        ty_hash: xu_ir::stable_hash64(&ty),
                        fields: values.into_boxed_slice(),
                        field_names: layout.clone(),
                    })));
                stack.push(Value::struct_obj(id));
            }
            Op::EnumCtor(t_idx, v_idx) => {
                let ty = rt.get_const_str(*t_idx, &bc.constants);
                let variant = rt.get_const_str(*v_idx, &bc.constants);
                let v = rt.enum_new_checked(ty, variant, Box::new([]))?;
                stack.push(v);
            }
            Op::EnumCtorN(t_idx, v_idx, argc) => {
                let ty = rt.get_const_str(*t_idx, &bc.constants);
                let variant = rt.get_const_str(*v_idx, &bc.constants);
                let mut payload: Vec<Value> = Vec::with_capacity(*argc);
                for _ in 0..*argc {
                    payload.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
                }
                payload.reverse();
                let v = rt.enum_new_checked(ty, variant, payload.into_boxed_slice())?;
                stack.push(v);
            }
            Op::MakeFunction(f_idx) => {
                let c = rt.get_constant(*f_idx, &bc.constants);
                if let xu_ir::Constant::Func(func_bc) = c {
                    let def = &func_bc.def;
                    let bytecode = &func_bc.bytecode;
                    let locals_count = func_bc.locals_count;

                    if rt.locals.is_active() {
                        let bindings = rt.locals.current_bindings();
                        if !bindings.is_empty() {
                            let env = &mut rt.env;
                            for (name, value) in bindings {
                                let assigned = env.assign(&name, value);
                                if !assigned {
                                    env.define(name, value);
                                }
                            }
                        }
                    }
                    if let Some(bindings) = rt.current_param_bindings.as_ref() {
                        if !bindings.is_empty() {
                            let mut captured: Vec<(String, Value)> =
                                Vec::with_capacity(bindings.len());
                            for (name, idx) in bindings {
                                if let Some(value) = rt.get_local_by_index(*idx) {
                                    captured.push((name.clone(), value));
                                }
                            }
                            let env = &mut rt.env;
                            for (name, value) in captured {
                                let assigned = env.assign(&name, value);
                                if !assigned {
                                    env.define(name, value);
                                }
                            }
                        }
                    }
                    let needs_env_frame = bytecode
                        .ops
                        .iter()
                        .any(|op| matches!(op, Op::MakeFunction(_)));
                    let fun = crate::value::BytecodeFunction {
                        def: def.clone(),
                        bytecode: std::rc::Rc::new((**bytecode).clone()),
                        env: rt.env.freeze(),
                        needs_env_frame,
                        locals_count,
                        type_sig_ic: std::cell::Cell::new(None),
                    };
                    let id = rt.heap.alloc(ManagedObject::Function(Function::Bytecode(
                        std::rc::Rc::new(fun),
                    )));
                    stack.push(Value::function(id));
                }
            }
            Op::Call(n) => {
                let n = *n;
                if stack.len() < n + 1 {
                    return Err(stack_underflow(ip, op));
                }
                let args_start = stack.len() - n;
                let callee = stack[args_start - 1];

                // Fast path for bytecode functions
                let mut fast_res = None;
                if callee.get_tag() == crate::value::TAG_FUNC {
                    let func_id = callee.as_obj_id();
                    if let ManagedObject::Function(crate::value::Function::Bytecode(f)) = rt.heap.get(func_id) {
                        let f = f.clone();
                        if !f.needs_env_frame && f.def.params.len() == n && f.def.params.iter().all(|p| p.default.is_none()) {
                            let args = &stack[args_start..];
                            if let Some(res) = run_bytecode_fast_params_only(rt, &f.bytecode, &f.def.params, args) {
                                fast_res = Some(res);
                            }
                        }
                    }
                }

                if let Some(res) = fast_res {
                    stack.truncate(args_start - 1);
                    match res {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                } else {
                    let args: SmallVec<[Value; 8]> = stack.drain(args_start..).collect();
                    let callee = stack.pop().expect("Stack underflow in Call (callee)");
                    match rt.call_function(callee, &args) {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::CallMethod(m_idx, method_hash, n, slot_idx) => {
                let n = *n;
                if stack.len() < n + 1 {
                    return Err(stack_underflow(ip, op));
                }
                let args_start = stack.len() - n;
                let recv = stack[args_start - 1];
                let tag = recv.get_tag();

                // Fast path for dict.get with string key - inline the entire operation
                if tag == TAG_DICT && n == 1 {
                    static GET_HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
                    let get_hash = *GET_HASH.get_or_init(|| xu_ir::stable_hash64("get"));
                    if *method_hash == get_hash {
                        let key_val = stack[args_start];
                        if key_val.get_tag() == TAG_STR {
                            let dict_id = recv.as_obj_id();
                            let key_id = key_val.as_obj_id();

                            // Get key pointer/len without cloning
                            let (key_ptr, key_len) = if let ManagedObject::Str(s) = rt.heap.get(key_id) {
                                (s.as_str().as_ptr(), s.as_str().len())
                            } else {
                                ("".as_ptr(), 0)
                            };
                            // SAFETY: key_ptr is valid during this operation
                            let key_bytes = unsafe { std::slice::from_raw_parts(key_ptr, key_len) };

                            // Check IC cache first - fast path when same dict, same key, same version
                            let mut cached_option = None;
                            if let Some(idx) = slot_idx {
                                if *idx < rt.ic_slots.len() {
                                    let c = &rt.ic_slots[*idx];
                                    if c.id == dict_id.0 && c.key_len as usize == key_bytes.len() && key_bytes.len() <= 16 {
                                        // Fast compare short keys
                                        if &c.key_short[..key_bytes.len()] == key_bytes {
                                            if let ManagedObject::Dict(me) = rt.heap.get(dict_id) {
                                                if c.ver == me.ver {
                                                    cached_option = Some(c.option_some_cached);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(opt) = cached_option {
                                stack.truncate(args_start - 1);
                                stack.push(opt);
                                ip += 1;
                                continue;
                            }

                            // SAFETY: key_ptr still valid
                            let key_str = unsafe { std::str::from_utf8_unchecked(key_bytes) };
                            let result = if let ManagedObject::Dict(me) = rt.heap.get(dict_id) {
                                let cur_ver = me.ver;
                                let key_hash = Runtime::hash_bytes(me.map.hasher(), key_bytes);
                                if let Some(v) = Runtime::dict_get_by_str_with_hash(me, key_str, key_hash) {
                                    // Create Option::some and cache it
                                    let opt = rt.option_some(v);
                                    if let Some(idx) = slot_idx {
                                        while rt.ic_slots.len() <= *idx {
                                            rt.ic_slots.push(super::ICSlot::default());
                                        }
                                        let mut key_short = [0u8; 16];
                                        let klen = key_bytes.len().min(16);
                                        key_short[..klen].copy_from_slice(&key_bytes[..klen]);
                                        rt.ic_slots[*idx] = super::ICSlot {
                                            id: dict_id.0,
                                            key_hash,
                                            key_id: key_id.0,
                                            key_short,
                                            key_len: klen as u8,
                                            ver: cur_ver,
                                            value: v,
                                            option_some_cached: opt,
                                            ..Default::default()
                                        };
                                    }
                                    Some(opt)
                                } else {
                                    Some(rt.option_none())
                                }
                            } else {
                                None
                            };

                            if let Some(result) = result {
                                stack.truncate(args_start - 1);
                                stack.push(result);
                                ip += 1;
                                continue;
                            }
                        }
                    }

                    // Fast path for dict.insert with string key
                    static INSERT_HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
                    let insert_hash = *INSERT_HASH.get_or_init(|| xu_ir::stable_hash64("insert"));
                    if *method_hash == insert_hash && n == 2 {
                        let key_val = stack[args_start];
                        let value = stack[args_start + 1];
                        if key_val.get_tag() == TAG_STR {
                            let dict_id = recv.as_obj_id();
                            let key_id = key_val.as_obj_id();

                            // Get key pointer/len without cloning
                            let (key_ptr, key_len) = if let ManagedObject::Str(s) = rt.heap.get(key_id) {
                                (s.as_str().as_ptr(), s.as_str().len())
                            } else {
                                ("".as_ptr(), 0)
                            };

                            // IC optimization for insert
                            let mut cached_hash = None;
                            if let Some(idx) = slot_idx {
                                if *idx < rt.ic_slots.len() {
                                    let c = &rt.ic_slots[*idx];
                                    if c.id == dict_id.0 && c.key_id == key_id.0 {
                                        // Cache hit: same dict and same key object (e.g. constant string)
                                        cached_hash = Some(c.key_hash);
                                    }
                                }
                            }

                            if let ManagedObject::Dict(me) = rt.heap.get_mut(dict_id) {
                                // SAFETY: key_ptr is valid during this operation
                                let key_str = unsafe {
                                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len))
                                };

                                let key_hash = if let Some(h) = cached_hash {
                                    h
                                } else {
                                    let h = Runtime::hash_bytes(me.map.hasher(), key_str.as_bytes());
                                    // Update IC cache
                                    if let Some(idx) = slot_idx {
                                        while rt.ic_slots.len() <= *idx {
                                            rt.ic_slots.push(super::ICSlot::default());
                                        }
                                        rt.ic_slots[*idx] = super::ICSlot {
                                            id: dict_id.0,
                                            key_hash: h,
                                            key_id: key_id.0,
                                            // We don't need short key for insert as we rely on object identity
                                            key_len: 0,
                                            ver: 0, // Not used for hash caching
                                            value: Value::VOID,
                                            ..Default::default()
                                        };
                                    }
                                    h
                                };

                                use hashbrown::hash_map::RawEntryMut;
                                match me.map.raw_entry_mut().from_hash(key_hash, |kk| {
                                    match kk {
                                        DictKey::Str { data, .. } => data.as_str() == key_str,
                                        _ => false,
                                    }
                                }) {
                                    RawEntryMut::Occupied(mut o) => {
                                        *o.get_mut() = value;
                                    }
                                    RawEntryMut::Vacant(vac) => {
                                        // Only allocate key when inserting new entry
                                        let key = DictKey::from_str(key_str);
                                        vac.insert(key, value);
                                    }
                                }
                                me.ver += 1;
                            }
                            stack.truncate(args_start - 1);
                            stack.push(Value::VOID);
                            ip += 1;
                            continue;
                        }
                    }
                }

                // IC check (Hot path for bytecode methods)
                let mut fast_res = None;
                if let Some(idx) = slot_idx {
                    if *idx < rt.ic_method_slots.len() {
                        let slot = &rt.ic_method_slots[*idx];
                        if slot.tag == tag && slot.method_hash == *method_hash {
                            if tag == crate::value::TAG_STRUCT {
                                let id = recv.as_obj_id();
                                if let ManagedObject::Struct(s) = rt.heap.get(id) {
                                    if slot.struct_ty_hash == s.ty_hash {
                                        if let Some(f) = slot.cached_bytecode.clone() {
                                            if !f.needs_env_frame && f.def.params.len() == n + 1 {
                                                let all_args = &stack[args_start - 1..];
                                                if let Some(res) = run_bytecode_fast_params_only(
                                                    rt,
                                                    &f.bytecode,
                                                    &f.def.params,
                                                    all_args,
                                                ) {
                                                    fast_res = Some(res);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(res) = fast_res {
                    stack.truncate(args_start - 1);
                    match res {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                        }
                    }
                } else {
                    let method = rt.get_const_str(*m_idx, &bc.constants);
                    let res = rt.call_method_with_ic_raw(
                        recv,
                        method,
                        *method_hash,
                        &stack[args_start..],
                        slot_idx.clone(),
                    );
                    stack.truncate(args_start - 1);
                    match res {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::Inc => {
                let a = stack
                    .last_mut()
                    .ok_or_else(|| "Stack underflow".to_string())?;
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
            }
            Op::MakeRange(inclusive) => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let start = to_i64(&a)?;
                let end = to_i64(&b)?;
                let id = rt.heap.alloc(ManagedObject::Range(start, end, *inclusive));
                stack.push(Value::range(id));
            }
            Op::GetMember(idx, slot_idx) => {
                let obj = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let tag = obj.get_tag();
                let field = rt.get_const_str(*idx, &bc.constants);
                if tag == crate::value::TAG_STRUCT {
                    let id = obj.as_obj_id();
                    if let ManagedObject::Struct(s) = rt.heap.get(id) {
                        let field_hash = xu_ir::stable_hash64(field);
                        // IC check
                        let mut val = None;
                        if let Some(idx_slot) = slot_idx {
                            if *idx_slot < rt.ic_slots.len() {
                                let c = &rt.ic_slots[*idx_slot];
                                if c.struct_ty_hash == s.ty_hash && c.key_hash == field_hash {
                                    if let Some(offset) = c.field_offset {
                                        val = Some(s.fields[offset]);
                                    }
                                }
                            }
                        }

                        if let Some(v) = val {
                            stack.push(v);
                        } else {
                            // Slow path
                            match rt.get_member_with_ic_raw(obj, field, *slot_idx) {
                                Ok(v) => stack.push(v),
                                Err(e) => {
                                    let err_val =
                                        Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                                    if let Some(flow) = throw_value(
                                        rt,
                                        &mut ip,
                                        &mut handlers,
                                        &mut stack,
                                        &mut iters,
                                        &mut pending,
                                        &mut thrown,
                                        err_val,
                                    ) {
                                        return Ok(flow);
                                    }
                                    ip += 1;
                                    continue;
                                }
                            }
                        }
                        ip += 1;
                        continue;
                    } else {
                        return Err("Not a struct".into());
                    }
                } else {
                    match rt.get_member_with_ic_raw(obj, field, *slot_idx) {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::GetIndex(slot_cell) => {
                let idx = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let obj = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let tag = obj.get_tag();

                if tag == TAG_DICT {
                    let id = obj.as_obj_id();
                    if let ManagedObject::Dict(me) = rt.heap.get(id) {
                        let cur_ver = me.ver;
                        let mut val = None;
                        if idx.is_int() {
                            let key = idx.as_i64();
                            if key >= 0 && key < 1024 {
                                let ui = key as usize;
                                if ui < me.elements.len() {
                                    let v = me.elements[ui];
                                    if v.get_tag() != crate::value::TAG_VOID {
                                        val = Some(v);
                                    }
                                }
                            }

                            if val.is_none() {
                                if let Some(c) = rt.dict_cache_int_last.as_ref() {
                                    if c.id == id.0 && c.ver == cur_ver && c.key == key {
                                        val = Some(c.value);
                                    }
                                }
                            }
                            if val.is_none() {
                                if let Some(v) = me.map.get(&crate::value::DictKey::Int(key)) {
                                    val = Some(*v);
                                    rt.dict_cache_int_last = Some(super::DictCacheIntLast {
                                        id: id.0,
                                        key,
                                        ver: cur_ver,
                                        value: *v,
                                    });
                                }
                            }
                        } else if idx.get_tag() == TAG_STR {
                            // Fast path for string key
                            let key_id = idx.as_obj_id();
                            if let ManagedObject::Str(key_text) = rt.heap.get(key_id) {
                                // Check shape-based cache first
                                if let Some(shape_id) = me.shape {
                                    if let ManagedObject::Shape(shape) = rt.heap.get(shape_id) {
                                        if let Some(&off) = shape.prop_map.get(key_text.as_str()) {
                                            val = Some(me.prop_values[off]);
                                        }
                                    }
                                }
                                // Check last cache
                                if val.is_none() {
                                    if let Some(c) = rt.dict_cache_last.as_ref() {
                                        if c.id == id.0 && c.ver == cur_ver && c.key.as_str() == key_text.as_str() {
                                            val = Some(c.value);
                                        }
                                    }
                                }
                                // Hash lookup
                                if val.is_none() {
                                    let key_hash = Runtime::hash_bytes(me.map.hasher(), key_text.as_bytes());
                                    if let Some(v) = Runtime::dict_get_by_str_with_hash(me, key_text.as_str(), key_hash) {
                                        val = Some(v);
                                        rt.dict_cache_last = Some(super::DictCacheLast {
                                            id: id.0,
                                            key_hash,
                                            ver: cur_ver,
                                            key: key_text.clone(),
                                            value: v,
                                        });
                                    }
                                }
                            }
                        }

                        if let Some(v) = val {
                            stack.push(v);
                            ip += 1;
                            continue;
                        }
                    }
                    // Slow path for Dict
                    match rt.get_index_with_ic_raw(obj, idx, *slot_cell) {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                        }
                    }
                    ip += 1;
                    continue;
                } else if tag == TAG_LIST && idx.is_int() {
                    let id = obj.as_obj_id();
                    let i = idx.as_i64();
                    if let ManagedObject::List(l) = rt.heap.get(id) {
                        if i >= 0 && (i as usize) < l.len() {
                            stack.push(l[i as usize]);
                            ip += 1;
                            continue;
                        } else {
                            match rt.get_index_with_ic_raw(obj, idx, *slot_cell) {
                                Ok(v) => stack.push(v),
                                Err(e) => {
                                    let err_val =
                                        Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                                    if let Some(flow) = throw_value(
                                        rt,
                                        &mut ip,
                                        &mut handlers,
                                        &mut stack,
                                        &mut iters,
                                        &mut pending,
                                        &mut thrown,
                                        err_val,
                                    ) {
                                        return Ok(flow);
                                    }
                                    ip += 1;
                                    continue;
                                }
                            }
                            ip += 1;
                            continue;
                        }
                    } else {
                        return Err("Not a list".into());
                    }
                } else {
                    match rt.get_index_with_ic_raw(obj, idx, *slot_cell) {
                        Ok(v) => stack.push(v),
                        Err(e) => {
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        }
                    }
                }
            }
            Op::DictGetStrConst(idx, k_hash, slot) => {
                let obj = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if obj.get_tag() != TAG_DICT {
                    let err_val = Value::str(
                        rt.heap.alloc(ManagedObject::Str(
                            rt.error(xu_syntax::DiagnosticKind::FormatDictRequired)
                                .into(),
                        )),
                    );
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
                let id = obj.as_obj_id();
                if let ManagedObject::Dict(me) = rt.heap.get(id) {
                    let cur_ver = me.ver;
                    let mut val = None;

                    if let Some(idx_slot) = slot {
                        if *idx_slot < rt.ic_slots.len() {
                            let c = &rt.ic_slots[*idx_slot];
                            if let Some(off) = c.field_offset {
                                if let Some(sid) = me.shape {
                                    if c.id == sid.0 {
                                        val = Some(me.prop_values[off]);
                                    }
                                }
                            } else {
                                if c.id == id.0 && c.ver == me.ver && c.key_hash == *k_hash {
                                    val = Some(c.value);
                                }
                            }
                        }
                    }

                    if let Some(v) = val {
                        stack.push(v);
                    } else {
                        let k = rt.get_const_str(*idx, &bc.constants);
                        let internal_hash = Runtime::hash_bytes(me.map.hasher(), k.as_bytes());
                        let out = Runtime::dict_get_by_str_with_hash(me, k, internal_hash);
                        let Some(out) = out else {
                            let err_val = Value::str(
                                rt.heap.alloc(ManagedObject::Str(
                                    rt.error(xu_syntax::DiagnosticKind::KeyNotFound(k.to_string()))
                                        .into(),
                                )),
                            );
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        };
                        if let Some(idx_slot) = slot {
                            while rt.ic_slots.len() <= *idx_slot {
                                rt.ic_slots.push(super::ICSlot::default());
                            }
                            let mut shape_info = (id.0, None);
                            if let Some(sid) = me.shape {
                                if let ManagedObject::Shape(shape) = rt.heap.get(sid) {
                                    if let Some(&off) = shape.prop_map.get(k) {
                                        shape_info = (sid.0, Some(off));
                                    }
                                }
                            }

                            rt.ic_slots[*idx_slot] = super::ICSlot {
                                id: shape_info.0,
                                key_hash: *k_hash,
                                ver: cur_ver,
                                value: out,
                                field_offset: shape_info.1,
                                ..Default::default()
                            };
                        }
                        stack.push(out);
                    }
                }
            }
            Op::DictGetIntConst(i, slot) => {
                let obj = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if obj.get_tag() != TAG_DICT {
                    let err_val = Value::str(
                        rt.heap.alloc(ManagedObject::Str(
                            rt.error(xu_syntax::DiagnosticKind::FormatDictRequired)
                                .into(),
                        )),
                    );
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
                let id = obj.as_obj_id();
                if let ManagedObject::Dict(me) = rt.heap.get(id) {
                    let mut val = None;
                    if *i >= 0 && *i < 1024 {
                        let ui = *i as usize;
                        if ui < me.elements.len() {
                            let v = me.elements[ui];
                            if v.get_tag() != crate::value::TAG_VOID {
                                val = Some(v);
                            }
                        }
                    }

                    let cur_ver = me.ver;
                    if val.is_none() {
                        if let Some(idx) = slot {
                            if *idx < rt.ic_slots.len() {
                                let c = &rt.ic_slots[*idx];
                                let key_hash = Runtime::hash_dict_key_int(me.map.hasher(), *i);
                                if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                                    val = Some(c.value);
                                }
                            }
                        }
                    }

                    if let Some(v) = val {
                        stack.push(v);
                    } else {
                        let key_hash = Runtime::hash_dict_key_int(me.map.hasher(), *i);
                        let out = me.map.get(&crate::value::DictKey::Int(*i)).cloned();
                        let Some(out) = out else {
                            let err_val = Value::str(
                                rt.heap.alloc(ManagedObject::Str(
                                    rt.error(xu_syntax::DiagnosticKind::KeyNotFound(i.to_string()))
                                        .into(),
                                )),
                            );
                            if let Some(flow) = throw_value(
                                rt,
                                &mut ip,
                                &mut handlers,
                                &mut stack,
                                &mut iters,
                                &mut pending,
                                &mut thrown,
                                err_val,
                            ) {
                                return Ok(flow);
                            }
                            continue;
                        };
                        if let Some(idx) = slot {
                            while rt.ic_slots.len() <= *idx {
                                rt.ic_slots.push(super::ICSlot::default());
                            }
                            rt.ic_slots[*idx] = super::ICSlot {
                                id: id.0,
                                key_hash,
                                ver: cur_ver,
                                value: out,
                                ..Default::default()
                            };
                        }
                        stack.push(out);
                    }
                }
            }
            Op::AssignMember(idx, op_type) => {
                let obj = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let rhs = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let field = rt.get_const_str(*idx, &bc.constants);
                if let Err(e) = rt.assign_member(obj, field, *op_type, rhs) {
                    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
            }
            Op::AssignIndex(op) => {
                let idxv = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let obj = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let rhs = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if let Err(e) = rt.assign_index(obj, idxv, *op, rhs) {
                    let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                    if let Some(flow) = throw_value(
                        rt,
                        &mut ip,
                        &mut handlers,
                        &mut stack,
                        &mut iters,
                        &mut pending,
                        &mut thrown,
                        err_val,
                    ) {
                        return Ok(flow);
                    }
                    continue;
                }
            }
            Op::ForEachInit(idx, var_idx, end) => {
                let iterable = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let tag = iterable.get_tag();
                let var = rt.get_const_str(*idx, &bc.constants);
                let first_val = if tag == TAG_LIST {
                    let id = iterable.as_obj_id();
                    let len = match rt.heap.get(id) {
                        ManagedObject::List(v) => v.len(),
                        _ => {
                            return Err(
                                rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into()))
                            );
                        }
                    };
                    if len == 0 {
                        ip = *end;
                        continue;
                    }
                    let first = match rt.heap.get(id) {
                        ManagedObject::List(v) => v.get(0).cloned().unwrap_or(Value::VOID),
                        _ => {
                            return Err(
                                rt.error(xu_syntax::DiagnosticKind::Raw("Not a list".into()))
                            );
                        }
                    };
                    iters.push(IterState::List { id, idx: 1, len });
                    first
                } else if tag == TAG_RANGE {
                    let id = iterable.as_obj_id();
                    let (start, r_end, inclusive) = match rt.heap.get(id) {
                        ManagedObject::Range(s, e, inc) => (*s, *e, *inc),
                        _ => {
                            return Err(
                                rt.error(xu_syntax::DiagnosticKind::Raw("Not a range".into()))
                            );
                        }
                    };
                    let step = if start <= r_end { 1 } else { -1 };
                    if !inclusive {
                        if (step > 0 && start >= r_end) || (step < 0 && start <= r_end) {
                            ip = *end;
                            continue;
                        }
                    }
                    let next = start.saturating_add(step);
                    iters.push(IterState::Range {
                        cur: next,
                        end: r_end,
                        step,
                        inclusive,
                    });
                    Value::from_i64(start)
                } else if tag == TAG_DICT {
                    let id = iterable.as_obj_id();
                    let raw_keys: Vec<DictKey> = match rt.heap.get(id) {
                        ManagedObject::Dict(d) => {
                            d.map.keys().cloned().collect()
                        }
                        _ => {
                            return Err(
                                rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into()))
                            );
                        }
                    };
                    if raw_keys.is_empty() {
                        ip = *end;
                        continue;
                    }
                    let keys: Vec<Value> = raw_keys.into_iter().map(|k| match k {
                        DictKey::Str { data, .. } => Value::str(rt.heap.alloc(ManagedObject::Str(Text::from_str(&data)))),
                        DictKey::Int(i) => Value::from_i64(i),
                    }).collect();
                    let first = keys[0];
                    iters.push(IterState::Dict { keys, idx: 1 });
                    first
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::InvalidIteratorType {
                        expected: "list, range, or dict".to_string(),
                        actual: iterable.type_name().to_string(),
                        iter_desc: "bytecode foreach".to_string(),
                    }));
                };

                if let Some(v_idx) = var_idx {
                    rt.set_local_by_index(*v_idx, first_val);
                } else if rt.locals.is_active() {
                    if !rt.set_local(var, first_val) {
                        rt.define_local(var.to_string(), first_val);
                    }
                } else {
                    rt.env.define(var.to_string(), first_val);
                }
                ip += 1;
                continue;
            }
            Op::ForEachNext(idx, var_idx, loop_start, end) => {
                let Some(state) = iters.last_mut() else {
                    return Err(
                        rt.error(xu_syntax::DiagnosticKind::Raw("Iterator underflow".into()))
                    );
                };
                let var = rt.get_const_str(*idx, &bc.constants);
                let next_val = match state {
                    IterState::List { id, idx, len, .. } => {
                        if *idx >= *len {
                            None
                        } else {
                            let item = match rt.heap.get(*id) {
                                ManagedObject::List(v) => {
                                    v.get(*idx).cloned().unwrap_or(Value::VOID)
                                }
                                _ => {
                                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw(
                                        "Not a list".into(),
                                    )));
                                }
                            };
                            *idx += 1;
                            Some(item)
                        }
                    }
                    IterState::Range {
                        cur,
                        end: r_end,
                        step,
                        inclusive,
                        ..
                    } => {
                        let done = if *inclusive {
                            (*step > 0 && *cur > *r_end) || (*step < 0 && *cur < *r_end)
                        } else {
                            (*step > 0 && *cur >= *r_end) || (*step < 0 && *cur <= *r_end)
                        };
                        if done {
                            None
                        } else {
                            let item = Value::from_i64(*cur);
                            *cur = cur.saturating_add(*step);
                            Some(item)
                        }
                    }
                    IterState::Dict { keys, idx } => {
                        if *idx >= keys.len() {
                            None
                        } else {
                            let item = keys[*idx];
                            *idx += 1;
                            Some(item)
                        }
                    }
                };

                if let Some(val) = next_val {
                    if let Some(v_idx) = var_idx {
                        rt.set_local_by_index(*v_idx, val);
                    } else if rt.locals.is_active() {
                        if !rt.set_local(var, val) {
                            rt.define_local(var.to_string(), val);
                        }
                    } else {
                        rt.env.define(var.to_string(), val);
                    }
                    ip = *loop_start;
                    continue;
                } else {
                    iters.pop();
                    ip = *end;
                    continue;
                }
            }
            Op::IterPop => {
                let _ = iters
                    .pop()
                    .ok_or_else(|| "Iterator underflow".to_string())?;
            }
            Op::EnvPush => rt.env.push(),
            Op::EnvPop => rt.env.pop(),
            Op::Break(to) => {
                ip = *to;
                continue;
            }
            Op::Continue(to) => {
                ip = *to;
                continue;
            }
            Op::Return => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                return Ok(Flow::Return(v));
            }
            Op::Throw => {
               return Err("Op::Throw not supported in v1.1".into());
            }
            Op::RunPending => {
                // No longer needed without try/catch
                ip += 1;
                continue;
            }
            Op::ListNew(n) => {
                let mut items: Vec<Value> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
                }
                items.reverse();
                let id = rt.heap.alloc(ManagedObject::List(items));
                stack.push(Value::list(id));
            }
            Op::TupleNew(n) => {
                if *n == 0 {
                    stack.push(Value::VOID);
                    ip += 1;
                    continue;
                }
                let mut items: Vec<Value> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
                }
                items.reverse();
                let id = rt.heap.alloc(ManagedObject::Tuple(items));
                stack.push(Value::tuple(id));
            }
            Op::DictNew(n) => {
                let mut map = crate::value::dict_with_capacity(*n);
                for _ in 0..*n {
                    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                    let k = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                    let key = if k.get_tag() == TAG_STR {
                        if let ManagedObject::Str(s) = rt.heap.get(k.as_obj_id()) {
                            DictKey::from_text(s)
                        } else {
                            return Err("Not a string".into());
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
            }
            Op::BuilderNewCap(cap) => {
                let s = rt.builder_pool_get(*cap);
                let id = rt.heap.alloc(ManagedObject::Builder(s));
                stack.push(Value::builder(id));
            }
            Op::BuilderAppend => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if b.get_tag() != TAG_BUILDER {
                    return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: "builder_push".to_string(),
                        ty: b.type_name().to_string(),
                    }));
                }
                let id = b.as_obj_id();
                // Optimized: check most common cases first (int and str)
                if v.is_int() {
                    let mut buf = itoa::Buffer::new();
                    let digits = buf.format(v.as_i64());
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(digits);
                    }
                } else if v.get_tag() == TAG_STR {
                    // Optimization: avoid clone by using raw pointer
                    let str_id = v.as_obj_id();
                    let ptr = if let ManagedObject::Str(s) = rt.heap.get(str_id) {
                        s.as_str().as_ptr()
                    } else {
                        "".as_ptr()
                    };
                    let len = if let ManagedObject::Str(s) = rt.heap.get(str_id) {
                        s.as_str().len()
                    } else {
                        0
                    };
                    if let ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
                        // SAFETY: ptr/len are valid, builder and string are different objects
                        let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
                        let s_ref = unsafe { std::str::from_utf8_unchecked(slice) };
                        sb.push_str(s_ref);
                    }
                } else if v.is_f64() {
                    let f = v.as_f64();
                    if f.fract() == 0.0 {
                        let mut buf = itoa::Buffer::new();
                        let digits = buf.format(f as i64);
                        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                            s.push_str(digits);
                        }
                    } else {
                        let mut buf = ryu::Buffer::new();
                        let digits = buf.format(f);
                        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                            s.push_str(digits);
                        }
                    }
                } else if v.is_bool() {
                    let piece = if v.as_bool() { "true" } else { "false" };
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(piece);
                    }
                } else if v.is_void() {
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str("()");
                    }
                } else {
                    let piece = super::util::value_to_string(&v, &rt.heap);
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(&piece);
                    }
                }
                stack.push(b);
            }
            Op::BuilderFinalize => {
                let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if b.get_tag() != TAG_BUILDER {
                    return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: "builder_finalize".to_string(),
                        ty: b.type_name().to_string(),
                    }));
                }
                let id = b.as_obj_id();
                // Take ownership of the builder string and return it to pool
                let (out, builder_str) = if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                    let text = crate::Text::from_str(s.as_str());
                    let taken = std::mem::take(s);
                    (text, Some(taken))
                } else {
                    return Err("Not a builder".into());
                };
                // Return the string to the pool for reuse
                if let Some(s) = builder_str {
                    rt.builder_pool_return(s);
                }
                let sid = rt.heap.alloc(ManagedObject::Str(out));
                stack.push(Value::str(sid));
            }
            Op::DictInsert => {
                dict_ops::op_dict_insert(rt, &mut stack)?;
            }
            Op::DictInsertStrConst(idx, _k_hash, slot) => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let dict = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if dict.get_tag() != TAG_DICT {
                    return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: "insert".to_string(),
                        ty: dict.type_name().to_string(),
                    }));
                }
                let id = dict.as_obj_id();

                // Get the key string first (before any mutable borrows)
                let k = rt.get_const_str(*idx, &bc.constants);
                let k_bytes = k.as_bytes();
                let k_len = k_bytes.len();

                // Try IC cache first - fast path for short keys (<=16 bytes)
                let mut cache_hit = false;
                if let Some(idx_slot) = slot {
                    if *idx_slot < rt.ic_slots.len() {
                        let c = &rt.ic_slots[*idx_slot];
                        // For short keys, compare directly; for long keys, compare hash
                        let key_match = if k_len <= 16 {
                            c.key_len == k_len as u8 && c.key_short[..k_len] == k_bytes[..]
                        } else {
                            c.key_len == k_len as u8 && c.key_hash != 0
                        };
                        if c.id == id.0 && key_match {
                            let cached_hash = c.key_hash;
                            if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
                                match d.map.raw_entry_mut().from_hash(cached_hash, |key| {
                                    match key {
                                        DictKey::Str { data, .. } => data.as_str() == k,
                                        _ => false,
                                    }
                                }) {
                                    hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                                        *o.get_mut() = v;
                                        d.ver += 1;
                                        rt.dict_version_last = Some((id.0, d.ver));
                                        cache_hit = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                if !cache_hit {
                    // Slow path - compute hash and insert
                    if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
                        let internal_hash = Runtime::hash_bytes(d.map.hasher(), k.as_bytes());
                        // Avoid creating DictKey for comparison - use closure with str comparison
                        match d.map.raw_entry_mut().from_hash(internal_hash, |key| {
                            match key {
                                DictKey::Str { data, .. } => data.as_str() == k,
                                _ => false,
                            }
                        }) {
                            hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                                *o.get_mut() = v;
                            }
                            hashbrown::hash_map::RawEntryMut::Vacant(vac) => {
                                // Only allocate key when actually inserting new entry
                                let key = DictKey::from_str(k);
                                vac.insert(key, v);
                            }
                        }
                        d.ver += 1;
                        rt.dict_version_last = Some((id.0, d.ver));

                        // Update IC cache with key info for fast comparison
                        if let Some(idx_slot) = slot {
                            while rt.ic_slots.len() <= *idx_slot {
                                rt.ic_slots.push(super::ICSlot::default());
                            }
                            let mut key_short = [0u8; 16];
                            let key_bytes = k.as_bytes();
                            let copy_len = key_bytes.len().min(16);
                            key_short[..copy_len].copy_from_slice(&key_bytes[..copy_len]);
                            rt.ic_slots[*idx_slot] = super::ICSlot {
                                id: id.0,
                                key_hash: internal_hash,
                                key_short,
                                key_len: key_bytes.len() as u8,
                                ver: d.ver,
                                value: Value::VOID,
                                ..Default::default()
                            };
                        }
                    }
                }
                stack.push(dict);
            }
            Op::DictMerge => {
                dict_ops::op_dict_merge(rt, &mut stack)?;
            }
            Op::ListAppend(n) => {
                let mut items: SmallVec<[Value; 8]> = SmallVec::with_capacity(*n);
                for _ in 0..*n {
                    items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
                }
                items.reverse();
                let list = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if list.get_tag() != TAG_LIST {
                    return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
                        method: "add".to_string(),
                        ty: list.type_name().to_string(),
                    }));
                }
                let id = list.as_obj_id();
                if let ManagedObject::List(vs) = rt.heap.get_mut(id) {
                    vs.reserve(items.len());
                    for v in items {
                        vs.push(v);
                    }
                }
                stack.push(list);
            }
            Op::Print => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                rt.write_output(&value_to_string(&v, &rt.heap));
            }
            Op::Dup => {
                let v = stack.last().cloned().ok_or_else(|| "Stack underflow".to_string())?;
                stack.push(v);
            }
            Op::JumpIfTrue(to) => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if v.is_bool() && v.as_bool() {
                    ip = *to;
                    continue;
                }
            }
            Op::MatchPattern(pat_idx) => {
                // Peek the value (don't pop it, we need it for MatchBindings)
                let v = stack.last().cloned().ok_or_else(|| "Stack underflow".to_string())?;
                let c = rt.get_constant(*pat_idx, &bc.constants);
                if let xu_ir::Constant::Pattern(pat) = c {
                    let matched = super::pattern::match_pattern(rt, pat, &v).is_some();
                    stack.push(Value::from_bool(matched));
                } else {
                    return Err("Expected pattern constant".into());
                }
            }
            Op::MatchBindings(pat_idx) => {
                // Pop the value that was matched
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let c = rt.get_constant(*pat_idx, &bc.constants);
                if let xu_ir::Constant::Pattern(pat) = c {
                    if let Some(bindings) = super::pattern::match_pattern(rt, pat, &v) {
                        // Push bindings onto stack in order
                        for (_, val) in bindings {
                            stack.push(val);
                        }
                    }
                } else {
                    return Err("Expected pattern constant".into());
                }
            }
            Op::LocalsPush => {
                rt.push_locals();
            }
            Op::LocalsPop => {
                rt.pop_locals();
            }
            Op::TryPush(_, _, _, _) | Op::TryPop | Op::SetThrown => {
                // try/catch not supported in v1.1
                return Err("try/catch not supported".into());
            }
            Op::Halt => return Ok(Flow::None),
        }
        ip += 1;
    }
    Ok(Flow::None)
}

///
///
pub struct VM {
    pub output: String,
    rt: Runtime,
}

impl VM {
    pub fn new() -> Self {
        Self {
            output: String::new(),
            rt: Runtime::new(),
        }
    }

    pub fn run(&mut self, bc: &Bytecode) -> Result<(), String> {
        self.rt.reset_for_entry_execution();
        let _ = run_bytecode(&mut self.rt, bc)?;
        self.output = std::mem::take(&mut self.rt.output);
        Ok(())
    }
}

impl Default for VM {
    fn default() -> Self {
        Self::new()
    }
}
