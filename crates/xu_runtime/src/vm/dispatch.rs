use crate::core::value::ValueExt;

use xu_ir::{Bytecode, Op};

use crate::core::Value;
use crate::core::heap::ManagedObject;

use crate::util::{to_i64, value_to_string};
use crate::{Flow, Runtime};
use super::exception::throw_value;

use super::ops::dict as dict_ops;
use super::ops::{access, assign, call, collection, compare, iter, string, types};
use super::stack::{add_with_heap, stack_underflow, Handler, IterState, Pending};

pub(crate) fn run_bytecode(rt: &mut Runtime, bc: &Bytecode) -> Result<Flow, String> {
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
                match crate::modules::import_path(rt, path) {
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
                if let Some(flow) = assign::op_add_assign_name(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                )? {
                    return Ok(flow);
                }
            }
            Op::AddAssignLocal(idx) => {
                if let Some(flow) = assign::op_add_assign_local(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                )? {
                    return Ok(flow);
                }
            }
            Op::IncLocal(idx) => {
                if let Some(flow) = assign::op_inc_local(
                    rt,
                    &mut ip,
                    &mut handlers,
                    &mut stack,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                )? {
                    return Ok(flow);
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
                if let Some(flow) = string::op_str_append(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::StrAppendNull => {
                if let Some(flow) = string::op_str_append_null(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::StrAppendBool => {
                if let Some(flow) = string::op_str_append_bool(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::StrAppendInt => {
                if let Some(flow) = string::op_str_append_int(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::StrAppendFloat => {
                if let Some(flow) = string::op_str_append_float(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::StrAppendStr => {
                if let Some(flow) = string::op_str_append_str(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::Eq => {
                compare::op_eq(rt, &mut stack)?;
            }
            Op::Ne => {
                compare::op_ne(rt, &mut stack)?;
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
                if let Some(flow) = compare::op_gt(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::Lt => {
                if let Some(flow) = compare::op_lt(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::Ge => {
                if let Some(flow) = compare::op_ge(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
                }
            }
            Op::Le => {
                if let Some(flow) = compare::op_le(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                )? {
                    return Ok(flow);
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
                if let Some(flow) = types::op_assert_type(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                )? {
                    return Ok(flow);
                }
            }
            Op::DefineStruct(idx) => {
                types::op_define_struct(rt, bc, *idx);
            }
            Op::DefineEnum(idx) => {
                types::op_define_enum(rt, bc, *idx);
            }
            Op::StructInit(t_idx, n_idx) => {
                if let Some(flow) = types::op_struct_init(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *t_idx,
                    *n_idx,
                )? {
                    return Ok(flow);
                }
            }
            Op::EnumCtor(t_idx, v_idx) => {
                types::op_enum_ctor(rt, bc, &mut stack, *t_idx, *v_idx)?;
            }
            Op::EnumCtorN(t_idx, v_idx, argc) => {
                types::op_enum_ctor_n(rt, bc, &mut stack, *t_idx, *v_idx, *argc)?;
            }
            Op::MakeFunction(f_idx) => {
                call::op_make_function(rt, bc, &mut stack, *f_idx)?;
            }
            Op::Call(n) => {
                if let Some(flow) = call::op_call(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *n,
                )? {
                    return Ok(flow);
                }
            }
            Op::CallMethod(m_idx, method_hash, n, slot_idx) => {
                if let Some(flow) = call::op_call_method(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *m_idx,
                    *method_hash,
                    *n,
                    slot_idx.clone(),
                )? {
                    return Ok(flow);
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
                if let Some(flow) = access::op_get_member(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                    *slot_idx,
                )? {
                    return Ok(flow);
                }
            }
            Op::GetIndex(slot_cell) => {
                if let Some(flow) = access::op_get_index(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *slot_cell,
                )? {
                    return Ok(flow);
                }
            }
            Op::DictGetStrConst(idx, k_hash, slot) => {
                if let Some(flow) = access::op_dict_get_str_const(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                    *k_hash,
                    *slot,
                )? {
                    return Ok(flow);
                }
            }
            Op::DictGetIntConst(i, slot) => {
                if let Some(flow) = access::op_dict_get_int_const(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *i,
                    *slot,
                )? {
                    return Ok(flow);
                }
            }
            Op::AssignMember(idx, op_type) => {
                if let Some(flow) = access::op_assign_member(
                    rt,
                    bc,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *idx,
                    *op_type,
                )? {
                    return Ok(flow);
                }
            }
            Op::AssignIndex(op) => {
                if let Some(flow) = access::op_assign_index(
                    rt,
                    &mut stack,
                    &mut ip,
                    &mut handlers,
                    &mut iters,
                    &mut pending,
                    &mut thrown,
                    *op,
                )? {
                    return Ok(flow);
                }
            }
            Op::ForEachInit(idx, var_idx, end) => {
                if iter::op_foreach_init(rt, bc, &mut stack, &mut iters, &mut ip, *idx, *var_idx, *end)? {
                    continue;
                }
            }
            Op::ForEachNext(idx, var_idx, loop_start, end) => {
                if iter::op_foreach_next(rt, bc, &mut iters, &mut ip, *idx, *var_idx, *loop_start, *end)? {
                    continue;
                }
            }
            Op::IterPop => {
                iter::op_iter_pop(&mut iters)?;
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
                return call::op_return(&mut stack);
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
                collection::op_list_new(rt, &mut stack, *n)?;
            }
            Op::TupleNew(n) => {
                if collection::op_tuple_new(rt, &mut stack, *n)? {
                    ip += 1;
                    continue;
                }
            }
            Op::DictNew(n) => {
                collection::op_dict_new(rt, &mut stack, *n)?;
            }
            Op::BuilderNewCap(cap) => {
                string::op_builder_new_cap(rt, &mut stack, *cap);
            }
            Op::BuilderAppend => {
                string::op_builder_append(rt, &mut stack)?;
            }
            Op::BuilderFinalize => {
                string::op_builder_finalize(rt, &mut stack)?;
            }
            Op::DictInsert => {
                dict_ops::op_dict_insert(rt, &mut stack)?;
            }
            Op::DictInsertStrConst(idx, k_hash, slot) => {
                collection::op_dict_insert_str_const(rt, bc, &mut stack, *idx, *k_hash, *slot)?;
            }
            Op::DictMerge => {
                dict_ops::op_dict_merge(rt, &mut stack)?;
            }
            Op::ListAppend(n) => {
                collection::op_list_append(rt, &mut stack, *n)?;
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
                    let matched = crate::util::match_pattern(rt, pat, &v).is_some();
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
                    if let Some(bindings) = crate::util::match_pattern(rt, pat, &v) {
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
