use xu_ir::{Bytecode, Op};

use crate::core::Value;
use crate::core::heap::ManagedObject;

use crate::util::value_to_string;
use crate::{Flow, Runtime};
use super::exception::throw_value;

use super::ops::dict as dict_ops;
use super::ops::{access, assign, call, collection, compare, iter, math, string, types};
use super::stack::{stack_underflow, Handler, IterState, Pending};

pub(crate) fn run_bytecode(rt: &mut Runtime, bc: &Bytecode) -> Result<Flow, String> {
    let mut stack = rt.pools.get_stack();
    stack.clear();
    let mut iters = rt.pools.get_iters();
    iters.clear();
    let mut handlers = rt.pools.get_handlers();
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
                (*self.rt).pools.return_stack(std::mem::take(&mut *self.stack));
                (*self.rt).pools.return_iters(std::mem::take(&mut *self.iters));
                (*self.rt).pools.return_handlers(std::mem::take(&mut *self.handlers));
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
    let mut stmt_count: usize = 0;

    while ip < ops_len {
        // SAFETY: ip is always < ops_len due to the loop condition above,
        // so this index is always in bounds.
        let op = unsafe { ops.get_unchecked(ip) };
        stmt_count = stmt_count.wrapping_add(1);
        // Check GC every 1024 instructions
        if stmt_count & 1023 == 0 {
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
            Op::ConstNull => stack.push(Value::UNIT),
            Op::Pop => {
                let _ = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
            }
            Op::Dup => {
                let v = stack.last().cloned().ok_or_else(|| "Stack underflow".to_string())?;
                stack.push(v);
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
                    while rt.get_local_by_index(*idx).is_none() {
                        rt.define_local(format!("_tmp_{}", idx), Value::UNIT);
                    }
                    rt.set_local_by_index(*idx, val);
                }
            }
            Op::LoadName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                let v = if rt.locals.is_active() {
                    if let Some(v) = rt.get_local(name) {
                        v
                    } else {
                        rt.env.get_cached(name).ok_or_else(|| {
                            rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(name.to_string()))
                        })?
                    }
                } else {
                    rt.env.get_cached(name).ok_or_else(|| {
                        rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(name.to_string()))
                    })?
                };
                stack.push(v);
            }
            Op::StoreName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                let v = stack.pop().ok_or_else(|| stack_underflow(ip, op))?;
                if rt.locals.is_active() {
                    if !rt.set_local(name, v) && !rt.env.assign(name, v) {
                        rt.define_local(name.to_string(), v);
                    }
                } else if !rt.env.assign(name, v) {
                    rt.env.define(name.to_string(), v);
                }
            }
            Op::Use(path_idx, alias_idx) => {
                let path = rt.get_const_str(*path_idx, &bc.constants);
                let alias = rt.get_const_str(*alias_idx, &bc.constants);
                match crate::modules::import_path(rt, path) {
                    Ok(module_obj) => rt.env.define(alias.to_string(), module_obj),
                    Err(e) => {
                        let err_val = Value::str(rt.alloc(ManagedObject::Str(e.into())));
                        if let Some(flow) = throw_value(rt, &mut ip, &mut handlers, &mut stack, &mut iters, &mut pending, &mut thrown, err_val) {
                            return Ok(flow);
                        }
                        ip += 1;
                        continue;
                    }
                }
            }
            // Arithmetic operations
            Op::Add => {
                if let Some(flow) = math::op_add(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Sub => {
                if let Some(flow) = math::op_sub(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Mul => {
                if let Some(flow) = math::op_mul(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Div => {
                if let Some(flow) = math::op_div(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Mod => {
                if let Some(flow) = math::op_mod(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            // Logical operations
            Op::And => {
                if let Some(flow) = math::op_and(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Or => {
                if let Some(flow) = math::op_or(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Not => {
                if let Some(flow) = math::op_not(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            // Comparison operations
            Op::Eq => compare::op_eq(rt, &mut stack)?,
            Op::Ne => compare::op_ne(rt, &mut stack)?,
            Op::Gt => {
                if let Some(flow) = compare::op_gt(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Lt => {
                if let Some(flow) = compare::op_lt(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Ge => {
                if let Some(flow) = compare::op_ge(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::Le => {
                if let Some(flow) = compare::op_le(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            // String operations
            Op::StrAppend => {
                if let Some(flow) = string::op_str_append(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown)? {
                    return Ok(flow);
                }
            }
            Op::BuilderNewCap(cap) => string::op_builder_new_cap(rt, &mut stack, *cap),
            Op::BuilderAppend => string::op_builder_append(rt, &mut stack)?,
            Op::BuilderFinalize => string::op_builder_finalize(rt, &mut stack)?,
            // Assignment operations
            Op::AddAssignName(idx) => {
                if let Some(flow) = assign::op_add_assign_name(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *idx)? {
                    return Ok(flow);
                }
            }
            Op::AddAssignLocal(idx) => {
                if let Some(flow) = assign::op_add_assign_local(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *idx)? {
                    return Ok(flow);
                }
            }
            Op::IncLocal(idx) => {
                if let Some(flow) = assign::op_inc_local(rt, &mut ip, &mut handlers, &mut stack, &mut iters, &mut pending, &mut thrown, *idx)? {
                    return Ok(flow);
                }
            }
            // Type operations
            Op::AssertType(idx) => {
                if let Some(flow) = types::op_assert_type(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *idx)? {
                    return Ok(flow);
                }
            }
            Op::DefineStruct(idx) => types::op_define_struct(rt, bc, *idx),
            Op::DefineEnum(idx) => types::op_define_enum(rt, bc, *idx),
            Op::StructInit(t_idx, n_idx) => {
                if let Some(flow) = types::op_struct_init(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *t_idx, *n_idx)? {
                    return Ok(flow);
                }
            }
            Op::StructInitSpread(t_idx, n_idx) => {
                if let Some(flow) = types::op_struct_init_spread(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *t_idx, *n_idx)? {
                    return Ok(flow);
                }
            }
            Op::EnumCtor(t_idx, v_idx) => types::op_enum_ctor(rt, bc, &mut stack, *t_idx, *v_idx)?,
            Op::EnumCtorN(t_idx, v_idx, argc) => types::op_enum_ctor_n(rt, bc, &mut stack, *t_idx, *v_idx, *argc)?,
            // Function operations
            Op::MakeFunction(f_idx) => call::op_make_function(rt, bc, &mut stack, *f_idx)?,
            Op::Call(n) => {
                if let Some(flow) = call::op_call(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *n)? {
                    return Ok(flow);
                }
            }
            Op::CallMethod(m_idx, method_hash, n, slot_idx) => {
                if let Some(flow) = call::op_call_method(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *m_idx, *method_hash, *n, *slot_idx)? {
                    return Ok(flow);
                }
            }
            Op::CallStaticOrMethod(type_idx, m_idx, method_hash, n, slot_idx) => {
                if let Some(flow) = call::op_call_static_or_method(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *type_idx, *m_idx, *method_hash, *n, *slot_idx)? {
                    return Ok(flow);
                }
            }
            Op::Return => return call::op_return(&mut stack),
            // Collection operations
            Op::ListNew(n) => collection::op_list_new(rt, &mut stack, *n)?,
            Op::TupleNew(n) => {
                if collection::op_tuple_new(rt, &mut stack, *n)? {
                    ip += 1;
                    continue;
                }
            }
            Op::DictNew(n) => collection::op_dict_new(rt, &mut stack, *n)?,
            Op::MakeRange(inclusive) => collection::op_make_range(rt, &mut stack, *inclusive)?,
            Op::ListAppend(n) => collection::op_list_append(rt, &mut stack, *n)?,
            Op::DictInsert => dict_ops::op_dict_insert(rt, &mut stack)?,
            Op::DictMerge => dict_ops::op_dict_merge(rt, &mut stack)?,
            // Access operations
            Op::GetMember(idx, slot_idx) => {
                if let Some(flow) = access::op_get_member(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *idx, *slot_idx)? {
                    return Ok(flow);
                }
            }
            Op::GetIndex(slot_cell) => {
                if let Some(flow) = access::op_get_index(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *slot_cell)? {
                    return Ok(flow);
                }
            }
            Op::AssignMember(idx, op_type) => {
                if let Some(flow) = access::op_assign_member(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *idx, *op_type)? {
                    return Ok(flow);
                }
            }
            Op::AssignIndex(aop) => {
                if let Some(flow) = access::op_assign_index(rt, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *aop)? {
                    return Ok(flow);
                }
            }
            // Static field operations
            Op::GetStaticField(type_idx, field_idx) => {
                if let Some(flow) = access::op_get_static_field(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *type_idx, *field_idx)? {
                    return Ok(flow);
                }
            }
            Op::SetStaticField(type_idx, field_idx) => {
                if let Some(flow) = access::op_set_static_field(rt, bc, &mut stack, &mut ip, &mut handlers, &mut iters, &mut pending, &mut thrown, *type_idx, *field_idx)? {
                    return Ok(flow);
                }
            }
            Op::InitStaticField(type_idx, field_idx) => {
                access::op_init_static_field(rt, bc, &mut stack, *type_idx, *field_idx)?;
            }
            // Iterator operations
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
            Op::IterPop => iter::op_iter_pop(&mut iters)?,
            // Control flow
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
                    let msg = rt.error(xu_syntax::DiagnosticKind::InvalidConditionType(v.type_name().to_string()));
                    let err_val = Value::str(rt.alloc(ManagedObject::Str(msg.into())));
                    if let Some(flow) = throw_value(rt, &mut ip, &mut handlers, &mut stack, &mut iters, &mut pending, &mut thrown, err_val) {
                        return Ok(flow);
                    }
                    continue;
                }
            }
            Op::JumpIfTrue(to) => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                if v.is_bool() && v.as_bool() {
                    ip = *to;
                    continue;
                }
            }
            Op::Break(to) | Op::Continue(to) => {
                ip = *to;
                continue;
            }
            // Environment operations
            Op::EnvPush => rt.env.push(),
            Op::EnvPop => rt.env.pop(),
            Op::LocalsPush => rt.push_locals(),
            Op::LocalsPop => rt.pop_locals(),
            // Pattern matching
            Op::MatchPattern(pat_idx) => {
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
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                let c = rt.get_constant(*pat_idx, &bc.constants);
                if let xu_ir::Constant::Pattern(pat) = c {
                    if let Some(bindings) = crate::util::match_pattern(rt, pat, &v) {
                        for (_, val) in bindings {
                            stack.push(val);
                        }
                    }
                } else {
                    return Err("Expected pattern constant".into());
                }
            }
            // I/O
            Op::Print => {
                let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
                rt.write_output(&value_to_string(&v, &rt.heap));
            }
            // Unsupported/special
            Op::RunPending => {
                ip += 1;
                continue;
            }
            Op::Halt => return Ok(Flow::None),
        }
        ip += 1;
    }
    Ok(Flow::None)
}

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
