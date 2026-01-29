use crate::value::TAG_BUILDER;

use smallvec::SmallVec;
use xu_ir::{Bytecode, Op};

use super::appendable::Appendable;
use crate::Value;
use crate::gc::ManagedObject;
use crate::value::{DictKey, Function, TAG_DICT, TAG_LIST, TAG_RANGE, TAG_STR, set_with_capacity};

use super::util::{to_i64, type_matches, value_to_string};
use super::{Flow, Runtime};
use super::ir_throw::{dispatch_throw, throw_value, unwind_to_finally};

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
}

pub(super) struct Handler {
    pub(super) catch_ip: Option<usize>,
    pub(super) finally_ip: Option<usize>,
    pub(super) stack_len: usize,
    pub(super) iter_len: usize,
    pub(super) env_depth: usize,
}

pub(super) enum Pending {
    Jump(usize),
    Return(Value),
    Throw(Value),
}

#[inline(always)]
fn add_with_heap(rt: &mut Runtime, a: Value, b: Value) -> Result<Value, String> {
    let at = a.get_tag();
    let bt = b.get_tag();
    if at == TAG_STR || bt == TAG_STR {
        let mut sa = if at == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s.clone()
            } else {
                return Err("Not a string".into());
            }
        } else {
            let mut s = String::new();
            s.append_value(&a, &rt.heap);
            crate::Text::from_string(s)
        };

        sa.append_value(&b, &rt.heap);
        Ok(Value::str(rt.heap.alloc(ManagedObject::Str(sa))))
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

    struct VmScratchReturn {
        rt: *mut Runtime,
        stack: *mut Vec<Value>,
        iters: *mut Vec<IterState>,
        handlers: *mut Vec<Handler>,
    }

    impl Drop for VmScratchReturn {
        fn drop(&mut self) {
            unsafe {
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
                        let text = rt.intern_string(s);
                        stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(text))));
                    }
                    xu_ir::Constant::Int(i) => stack.push(Value::from_i64(*i)),
                    xu_ir::Constant::Float(f) => stack.push(Value::from_f64(*f)),
                    _ => return Err("Unexpected constant type in VM loop".into()),
                }
            }
            Op::ConstBool(b) => stack.push(Value::from_bool(*b)),
            Op::ConstNull => stack.push(Value::UNIT),
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
                        rt.define_local(format!("_tmp_{}", idx), Value::UNIT);
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
                    match add_with_heap(rt, a, Value::UNIT) {
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
                let ty = rt.get_const_str(*t_idx, &bc.constants);
                let fields = rt.get_const_names(*n_idx, &bc.constants);
                let layout = if let Some(l) = rt.struct_layouts.get(ty) {
                    l
                } else {
                    let err_val = Value::str(
                        rt.heap.alloc(ManagedObject::Str(
                            rt.error(xu_syntax::DiagnosticKind::UnknownStruct(ty.to_string()))
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

                let mut values = vec![Value::UNIT; layout.len()];
                for k in fields.iter().rev() {
                    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                    if let Some(pos) = layout.iter().position(|f| f == k) {
                        values[pos] = v;
                    }
                }

                let id = rt
                    .heap
                    .alloc(ManagedObject::Struct(crate::value::StructInstance {
                        ty: ty.to_string(),
                        ty_hash: xu_ir::stable_hash64(ty),
                        fields: values.into_boxed_slice(),
                        field_names: layout.clone(),
                    }));
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
                let args: Vec<Value> = stack.drain(stack.len() - n..).collect();
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
            Op::CallMethod(m_idx, method_hash, n, slot_idx) => {
                let n = *n;
                if stack.len() < n + 1 {
                    return Err(stack_underflow(ip, op));
                }
                let args_start = stack.len() - n;
                let recv = stack[args_start - 1];
                let tag = recv.get_tag();

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
                                    if v.get_tag() != crate::value::TAG_NULL {
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
                            if v.get_tag() != crate::value::TAG_UNIT {
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
                        ManagedObject::List(v) => v.get(0).cloned().unwrap_or(Value::UNIT),
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
                } else {
                    return Err(rt.error(xu_syntax::DiagnosticKind::InvalidIteratorType {
                        expected: "list".to_string(),
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
                                    v.get(*idx).cloned().unwrap_or(Value::UNIT)
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
            Op::TryPush(catch_ip, finally_ip, _end_ip, _catch_var) => {
                handlers.push(Handler {
                    catch_ip: *catch_ip,
                    finally_ip: *finally_ip,
                    stack_len: stack.len(),
                    iter_len: iters.len(),
                    env_depth: rt.env.local_depth(),
                });
            }
            Op::TryPop => {
                let _ = handlers
                    .pop()
                    .ok_or_else(|| "Handler underflow".to_string())?;
            }
            Op::SetThrown => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                thrown = Some(v);
            }
            Op::PushThrown => stack.push(thrown.unwrap_or(Value::UNIT)),
            Op::ClearThrown => thrown = None,
            Op::SetPendingNone => pending = None,
            Op::SetPendingJump(to) => pending = Some(Pending::Jump(*to)),
            Op::SetPendingReturn => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                pending = Some(Pending::Return(v));
            }
            Op::SetPendingThrow => {
                let v = thrown.ok_or_else(|| "No thrown value".to_string())?;
                pending = Some(Pending::Throw(v));
            }
            Op::Break(to) => {
                pending = Some(Pending::Jump(*to));
                if let Some(fin_ip) = unwind_to_finally(rt, &mut handlers, &mut stack) {
                    ip = fin_ip;
                    continue;
                }
                ip = *to;
                pending = None;
                continue;
            }
            Op::Continue(to) => {
                pending = Some(Pending::Jump(*to));
                if let Some(fin_ip) = unwind_to_finally(rt, &mut handlers, &mut stack) {
                    ip = fin_ip;
                    continue;
                }
                ip = *to;
                pending = None;
                continue;
            }
            Op::Return => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                pending = Some(Pending::Return(v));
                if let Some(fin_ip) = unwind_to_finally(rt, &mut handlers, &mut stack) {
                    ip = fin_ip;
                    continue;
                }
                let Pending::Return(v) = pending.take().expect("Pending return lost") else {
                    unreachable!();
                };
                return Ok(Flow::Return(v));
            }
            Op::Throw => {
                if thrown.is_none() {
                    return Err("No thrown value".into());
                }
                if let Some(next_ip) = dispatch_throw(
                    rt,
                    &mut handlers,
                    &mut stack,
                    &mut iters,
                    &mut pending,
                    &thrown,
                ) {
                    ip = next_ip;
                    continue;
                }
                return Ok(Flow::Throw(thrown.take().expect("Thrown value lost")));
            }
            Op::RunPending => {
                if pending.is_none() {
                    ip += 1;
                    continue;
                }
                if let Some(fin_ip) = unwind_to_finally(rt, &mut handlers, &mut stack) {
                    ip = fin_ip;
                    continue;
                }
                match pending.take().unwrap() {
                    Pending::Jump(to) => {
                        ip = to;
                        continue;
                    }
                    Pending::Return(v) => return Ok(Flow::Return(v)),
                    Pending::Throw(v) => {
                        thrown = Some(v);
                        if let Some(next_ip) = dispatch_throw(
                            rt,
                            &mut handlers,
                            &mut stack,
                            &mut iters,
                            &mut pending,
                            &thrown,
                        ) {
                            ip = next_ip;
                            continue;
                        }
                        return Ok(Flow::Throw(thrown.take().expect("Thrown value lost")));
                    }
                }
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
                    stack.push(Value::UNIT);
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
                            DictKey::Str(s.clone())
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
            Op::SetNew(n) => {
                let mut items: Vec<Value> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
                }
                items.reverse();
                let mut set = set_with_capacity(*n);
                for v in items {
                    let key = if v.get_tag() == TAG_STR {
                        if let ManagedObject::Str(s) = rt.heap.get(v.as_obj_id()) {
                            DictKey::Str(s.clone())
                        } else {
                            return Err("Not a string".into());
                        }
                    } else if v.is_int() {
                        DictKey::Int(v.as_i64())
                    } else {
                        return Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired));
                    };
                    set.map.insert(key, ());
                }
                let id = rt.heap.alloc(ManagedObject::Set(set));
                stack.push(Value::set(id));
            }
            Op::BuilderNewCap(cap) => {
                let s = String::with_capacity(*cap);
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
                if v.is_int() {
                    let mut buf = itoa::Buffer::new();
                    let digits = buf.format(v.as_i64());
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(digits);
                    }
                } else if v.is_f64() {
                    let f = v.as_f64();
                    let piece = if f.fract() == 0.0 {
                        let mut buf = itoa::Buffer::new();
                        buf.format(f as i64).to_string()
                    } else {
                        f.to_string()
                    };
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(&piece);
                    }
                } else if v.is_bool() {
                    let piece = if v.as_bool() { "true" } else { "false" };
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(piece);
                    }
                } else if v.is_unit() {
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str("()");
                    }
                } else if v.get_tag() == TAG_STR {
                    let text = if let ManagedObject::Str(s) = rt.heap.get(v.as_obj_id()) {
                        s.clone()
                    } else {
                        crate::Text::from_str("")
                    };
                    if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                        s.push_str(text.as_str());
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
                let out = if let ManagedObject::Builder(s) = rt.heap.get(id) {
                    crate::Text::from_str(s.as_str())
                } else {
                    return Err("Not a builder".into());
                };
                let sid = rt.heap.alloc(ManagedObject::Str(out));
                stack.push(Value::str(sid));
            }
            Op::DictInsert => {
                dict_ops::op_dict_insert(rt, &mut stack)?;
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
