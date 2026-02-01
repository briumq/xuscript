use crate::core::value::ValueExt;

use smallvec::SmallVec;
use xu_ir::{Bytecode, Op};

use crate::core::Value;
use crate::core::gc::ManagedObject;
use crate::core::value::{DictKey, Function, TAG_DICT, TAG_STR};

use crate::util::{to_i64, value_to_string};
use crate::{Flow, Runtime};
use super::exception::throw_value;

use super::fast::run_bytecode_fast_params_only;
use super::ops::dict as dict_ops;
use super::ops::{access, assign, collection, compare, iter, string, types};
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
                    let fun = crate::core::value::BytecodeFunction {
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
                if callee.get_tag() == crate::core::value::TAG_FUNC {
                    let func_id = callee.as_obj_id();
                    if let ManagedObject::Function(crate::core::value::Function::Bytecode(f)) = rt.heap.get(func_id) {
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
                                            rt.ic_slots.push(crate::ICSlot::default());
                                        }
                                        let mut key_short = [0u8; 16];
                                        let klen = key_bytes.len().min(16);
                                        key_short[..klen].copy_from_slice(&key_bytes[..klen]);
                                        rt.ic_slots[*idx] = crate::ICSlot {
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
                                            rt.ic_slots.push(crate::ICSlot::default());
                                        }
                                        rt.ic_slots[*idx] = crate::ICSlot {
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
                                        // 值更新 - 不增加版本号
                                        *o.get_mut() = value;
                                    }
                                    RawEntryMut::Vacant(vac) => {
                                        // 新 key - 增加版本号
                                        let key = DictKey::from_str(key_str);
                                        vac.insert(key, value);
                                        me.ver += 1;
                                    }
                                }
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
                            if tag == crate::core::value::TAG_STRUCT {
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
