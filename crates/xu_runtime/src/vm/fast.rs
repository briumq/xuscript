use crate::core::Value;
use crate::core::value::ValueExt;
use crate::core::gc::ManagedObject;

use xu_ir::{Bytecode, Op};

use super::stack::add_with_heap;
use crate::ICSlot;
use crate::{Flow, Runtime};

pub(crate) fn run_bytecode_fast(
    rt: &mut Runtime,
    bc: &Bytecode,
) -> Option<Result<Flow, String>> {
    if bc.ops.len() > 16 {
        return None;
    }
    for op in bc.ops.iter() {
        match op {
            Op::ConstInt(_)
            | Op::ConstFloat(_)
            | Op::Const(_)
            | Op::ConstBool(_)
            | Op::ConstNull
            | Op::Add
            | Op::AddAssignName(_)
            | Op::AddAssignLocal(_)
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Inc
            | Op::IncLocal(_)
            | Op::LoadName(_)
            | Op::LoadLocal(_)
            | Op::StoreName(_)
            | Op::StoreLocal(_)
            | Op::GetMember(_, _)
            | Op::Return
            | Op::Pop => {}
            _ => return None,
        }
    }

    fn load_name(rt: &mut Runtime, name: &str) -> Result<Value, String> {
        if rt.locals.is_active() {
            if let Some(func_name) = &rt.current_func {
                if let Some(idxmap) = rt.compiled_locals_idx.get(func_name) {
                    if let Some(idx) = idxmap.get(name) {
                        if let Some(v) = rt.get_local_by_index(*idx) {
                            return Ok(v);
                        }
                    }
                }
            }
            if let Some(v) = rt.get_local(name) {
                return Ok(v);
            }
        }
        rt.env.get_cached(name).ok_or_else(|| {
            rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(name.to_string()))
        })
    }

    fn store_name(rt: &mut Runtime, name: &str, v: Value) {
        if rt.locals.is_active() {
            let mut stored = false;
            if let Some(func_name) = &rt.current_func {
                if let Some(idxmap) = rt.compiled_locals_idx.get(func_name) {
                    if let Some(idx) = idxmap.get(name) {
                        if rt.set_local_by_index(*idx, v) {
                            stored = true;
                        }
                    }
                }
            }
            if !stored {
                if !rt.set_local(name, v) {
                    rt.define_local(name.to_string(), v);
                }
            }
        } else {
            let assigned = rt.env.assign(name, v);
            if !assigned {
                rt.env.define(name.to_string(), v);
            }
        }
    }

    let mut stack: [Value; 8] = [Value::VOID; 8];
    let mut sp: usize = 0;
    for op in bc.ops.iter() {
        match op {
            Op::ConstInt(i) => {
                stack[sp] = Value::from_i64(*i);
                sp += 1;
            }
            Op::ConstFloat(f) => {
                stack[sp] = Value::from_f64(*f);
                sp += 1;
            }
            Op::Const(idx) => {
                let c = &bc.constants[*idx as usize];
                match c {
                    xu_ir::Constant::Str(s) => {
                        let bc_ptr = bc as *const Bytecode as usize;
                        stack[sp] = rt.get_string_const(bc_ptr, *idx, s);
                    }
                    xu_ir::Constant::Int(i) => stack[sp] = Value::from_i64(*i),
                    xu_ir::Constant::Float(f) => stack[sp] = Value::from_f64(*f),
                    _ => return Some(Err("Unexpected constant type in fast path".into())),
                }
                sp += 1;
            }
            Op::ConstBool(b) => {
                stack[sp] = Value::from_bool(*b);
                sp += 1;
            }
            Op::ConstNull => {
                stack[sp] = Value::VOID;
                sp += 1;
            }
            Op::Pop => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
            }
            Op::LoadName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                stack[sp] = match load_name(rt, name) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                sp += 1;
            }
            Op::LoadLocal(idx) => {
                stack[sp] = match rt.get_local_by_index(*idx) {
                    Some(v) => v,
                    None => return Some(Err(format!("Undefined local variable index: {}", idx))),
                };
                sp += 1;
            }
            Op::StoreName(idx) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
                let name = rt.get_const_str(*idx, &bc.constants);
                store_name(rt, name, stack[sp]);
            }
            Op::StoreLocal(idx) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
                let val = stack[sp];
                if !rt.set_local_by_index(*idx, val) {
                    while rt.get_local_by_index(*idx).is_none() {
                        rt.define_local(format!("_tmp_{}", idx), Value::VOID);
                    }
                    rt.set_local_by_index(*idx, val);
                }
            }
            Op::AddAssignName(idx) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
                let rhs = stack[sp];
                let name = rt.get_const_str(*idx, &bc.constants);
                let mut cur = match load_name(rt, name) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                if cur.is_int() && rhs.is_int() {
                    cur = Value::from_i64(cur.as_i64().wrapping_add(rhs.as_i64()));
                } else {
                    cur = match cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap) {
                        Ok(_) => cur,
                        Err(e) => return Some(Err(e)),
                    };
                }
                store_name(rt, name, cur);
            }
            Op::AddAssignLocal(idx) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
                let rhs = stack[sp];
                let mut cur = match rt.get_local_by_index(*idx) {
                    Some(v) => v,
                    None => return Some(Err(format!("Undefined local variable index: {}", idx))),
                };
                if cur.is_int() && rhs.is_int() {
                    cur = Value::from_i64(cur.as_i64().wrapping_add(rhs.as_i64()));
                } else {
                    cur = match cur.bin_op_assign(xu_ir::BinaryOp::Add, rhs, &mut rt.heap) {
                        Ok(_) => cur,
                        Err(e) => return Some(Err(e)),
                    };
                }
                rt.set_local_by_index(*idx, cur);
            }
            Op::IncLocal(idx) => {
                let mut cur = match rt.get_local_by_index(*idx) {
                    Some(v) => v,
                    None => return Some(Err(format!("Undefined local variable index: {}", idx))),
                };
                if cur.is_int() {
                    cur = Value::from_i64(cur.as_i64().wrapping_add(1));
                } else {
                    cur = match cur.bin_op_assign(
                        xu_ir::BinaryOp::Add,
                        Value::from_i64(1),
                        &mut rt.heap,
                    ) {
                        Ok(_) => cur,
                        Err(e) => return Some(Err(e)),
                    };
                }
                rt.set_local_by_index(*idx, cur);
            }
            Op::Add => {
                if sp < 2 {
                    return Some(Err("Stack underflow".into()));
                }
                let b = stack[sp - 1];
                let a = stack[sp - 2];
                sp -= 2;
                if a.is_int() && b.is_int() {
                    stack[sp] = Value::from_i64(a.as_i64().wrapping_add(b.as_i64()));
                } else {
                    stack[sp] = match add_with_heap(rt, a, b) {
                        Ok(v) => v,
                        Err(e) => return Some(Err(e)),
                    };
                }
                sp += 1;
            }
            Op::Sub | Op::Mul | Op::Div => return None,
            Op::Inc => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                let a = &mut stack[sp - 1];
                if a.is_int() {
                    *a = Value::from_i64(a.as_i64().saturating_add(1));
                } else if a.is_f64() {
                    *a = Value::from_f64(a.as_f64() + 1.0);
                } else {
                    return Some(Err(rt.error(
                        xu_syntax::DiagnosticKind::InvalidUnaryOperand {
                            op: '+',
                            expected: "number".to_string(),
                        },
                    )));
                }
            }
            Op::GetMember(idx, slot) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
                let obj = stack[sp];
                let field = rt.get_const_str(*idx, &bc.constants);
                let v = match rt.get_member_with_ic_raw(obj, field, *slot) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                stack[sp] = v;
                sp += 1;
            }
            Op::Return => {
                let v = if sp == 0 { Value::VOID } else { stack[sp - 1] };
                return Some(Ok(Flow::Return(v)));
            }
            _ => return None,
        }
    }
    Some(Ok(Flow::None))
}

pub(crate) fn run_bytecode_fast_params_only(
    rt: &mut Runtime,
    bc: &Bytecode,
    params: &[xu_ir::Param],
    args: &[Value],
) -> Option<Result<Value, String>> {
    if params.len() != args.len() {
        return None;
    }
    if bc.ops.len() > 32 {
        return None;
    }
    for op in bc.ops.iter() {
        match op {
            Op::LoadName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                if !params.iter().any(|p| p.name.as_str() == name) {
                    return None;
                }
            }
            Op::LoadLocal(idx) => {
                if *idx >= args.len() {
                    return None;
                }
            }
            Op::DictGetStrConst(_, _, _)
            | Op::DictGetIntConst(_, _)
            | Op::ConstInt(_)
            | Op::ConstFloat(_)
            | Op::Const(_)
            | Op::ConstBool(_)
            | Op::ConstNull
            | Op::Add
            | Op::Sub
            | Op::Mul
            | Op::Div
            | Op::Gt
            | Op::Lt
            | Op::Ge
            | Op::Le
            | Op::Eq
            | Op::Ne
            | Op::GetMember(_, _)
            | Op::Return
            | Op::Pop => {}
            _ => return None,
        }
    }

    let mut stack: [Value; 16] = [Value::VOID; 16];
    let mut sp: usize = 0;
    for op in bc.ops.iter() {
        match op {
            Op::ConstInt(i) => {
                stack[sp] = Value::from_i64(*i);
                sp += 1;
            }
            Op::ConstFloat(f) => {
                stack[sp] = Value::from_f64(*f);
                sp += 1;
            }
            Op::Const(idx) => {
                let c = &bc.constants[*idx as usize];
                match c {
                    xu_ir::Constant::Str(s) => {
                        let bc_ptr = bc as *const Bytecode as usize;
                        stack[sp] = rt.get_string_const(bc_ptr, *idx, s);
                    }
                    xu_ir::Constant::Int(i) => stack[sp] = Value::from_i64(*i),
                    xu_ir::Constant::Float(f) => stack[sp] = Value::from_f64(*f),
                    _ => return Some(Err("Unexpected constant type in fast path".into())),
                }
                sp += 1;
            }
            Op::ConstBool(b) => {
                stack[sp] = Value::from_bool(*b);
                sp += 1;
            }
            Op::ConstNull => {
                stack[sp] = Value::VOID;
                sp += 1;
            }
            Op::Pop => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
            }
            Op::DictGetStrConst(idx, k_hash, slot) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                let obj = stack[sp - 1];
                if obj.get_tag() != crate::core::value::TAG_DICT {
                    return None;
                }
                let id = obj.as_obj_id();
                if let ManagedObject::Dict(me) = rt.heap.get(id) {
                    let cur_ver = me.ver;
                    let mut val = None;

                    if let Some(idx_ic) = slot {
                        if *idx_ic < rt.ic_slots.len() {
                            let c = &rt.ic_slots[*idx_ic];
                            if let Some(off) = c.field_offset {
                                if let Some(sid) = me.shape {
                                    if c.id == sid.0 {
                                        val = Some(me.prop_values[off]);
                                    }
                                }
                            } else if c.id == id.0 && c.ver == me.ver && c.key_hash == *k_hash {
                                val = Some(c.value);
                            }
                        }
                    }

                    if let Some(v) = val {
                        stack[sp - 1] = v;
                    } else {
                        let k = rt.get_const_str(*idx, &bc.constants);
                        let mut out = None;
                        if let Some(sid) = me.shape {
                            if let ManagedObject::Shape(shape) = rt.heap.get(sid) {
                                if let Some(&off) = shape.prop_map.get(k) {
                                    out = Some(me.prop_values[off]);
                                }
                            }
                        }
                        if out.is_none() {
                            let internal_hash = Runtime::hash_bytes(me.map.hasher(), k.as_bytes());
                            out = Runtime::dict_get_by_str_with_hash(me, k, internal_hash);
                        }

                        let Some(out_v) = out else {
                            return None;
                        };
                        if let Some(idx_ic) = slot {
                            while rt.ic_slots.len() <= *idx_ic {
                                rt.ic_slots.push(ICSlot::default());
                            }
                            let mut shape_info = (id.0, None);
                            if let Some(sid) = me.shape {
                                if let ManagedObject::Shape(shape) = rt.heap.get(sid) {
                                    if let Some(&off) = shape.prop_map.get(k) {
                                        shape_info = (sid.0, Some(off));
                                    }
                                }
                            }

                            rt.ic_slots[*idx_ic] = ICSlot {
                                id: shape_info.0,
                                key_hash: *k_hash,
                                ver: cur_ver,
                                value: out_v,
                                field_offset: shape_info.1,
                                ..Default::default()
                            };
                        }
                        stack[sp - 1] = out_v;
                    }
                }
            }
            Op::DictGetIntConst(i, slot) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                let obj = stack[sp - 1];
                if obj.get_tag() != crate::core::value::TAG_DICT {
                    return None;
                }
                let id = obj.as_obj_id();
                let (cur_ver, key_hash) = if let ManagedObject::Dict(me) = rt.heap.get(id) {
                    (me.ver, Runtime::hash_dict_key_int(me.map.hasher(), *i))
                } else {
                    return None;
                };

                let mut val = None;
                if let Some(idx_ic) = slot {
                    if *idx_ic < rt.ic_slots.len() {
                        let c = &rt.ic_slots[*idx_ic];
                        if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                            val = Some(c.value);
                        }
                    }
                }

                if let Some(v) = val {
                    stack[sp - 1] = v;
                } else {
                    let out = if let ManagedObject::Dict(me) = rt.heap.get(id) {
                        me.map.get(&crate::core::value::DictKey::Int(*i)).cloned()
                    } else {
                        None
                    };
                    let Some(out) = out else {
                        return None;
                    };
                    if let Some(idx_ic) = slot {
                        while rt.ic_slots.len() <= *idx_ic {
                            rt.ic_slots.push(ICSlot::default());
                        }
                        rt.ic_slots[*idx_ic] = ICSlot {
                            id: id.0,
                            key_hash,
                            ver: cur_ver,
                            value: out,
                            ..Default::default()
                        };
                    }
                    stack[sp - 1] = out;
                }
            }
            Op::LoadLocal(idx) => {
                stack[sp] = args[*idx];
                sp += 1;
            }
            Op::LoadName(idx) => {
                let name = rt.get_const_str(*idx, &bc.constants);
                let mut found = None;
                for (i, p) in params.iter().enumerate() {
                    if p.name.as_str() == name {
                        found = Some(args[i]);
                        break;
                    }
                }
                let Some(v) = found else {
                    return None;
                };
                stack[sp] = v;
                sp += 1;
            }
            Op::Add => {
                if sp < 2 {
                    return Some(Err("Stack underflow".into()));
                }
                let b = stack[sp - 1];
                let a = stack[sp - 2];
                sp -= 2;
                stack[sp] = match add_with_heap(rt, a, b) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                sp += 1;
            }
            Op::Sub => {
                if sp < 2 {
                    return Some(Err("Stack underflow".into()));
                }
                let b = stack[sp - 1];
                let a = stack[sp - 2];
                sp -= 2;
                stack[sp] = match a.bin_op(xu_ir::BinaryOp::Sub, b) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                sp += 1;
            }
            Op::Mul => {
                if sp < 2 {
                    return Some(Err("Stack underflow".into()));
                }
                let b = stack[sp - 1];
                let a = stack[sp - 2];
                sp -= 2;
                stack[sp] = match a.bin_op(xu_ir::BinaryOp::Mul, b) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                sp += 1;
            }
            Op::Div => {
                if sp < 2 {
                    return Some(Err("Stack underflow".into()));
                }
                let b = stack[sp - 1];
                let a = stack[sp - 2];
                sp -= 2;
                stack[sp] = match a.bin_op(xu_ir::BinaryOp::Div, b) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                sp += 1;
            }
            Op::Gt | Op::Lt | Op::Ge | Op::Le | Op::Eq | Op::Ne => {
                if sp < 2 {
                    return Some(Err("Stack underflow".into()));
                }
                let b = stack[sp - 1];
                let a = stack[sp - 2];
                sp -= 2;
                let bop = match op {
                    Op::Gt => xu_ir::BinaryOp::Gt,
                    Op::Lt => xu_ir::BinaryOp::Lt,
                    Op::Ge => xu_ir::BinaryOp::Ge,
                    Op::Le => xu_ir::BinaryOp::Le,
                    Op::Eq => xu_ir::BinaryOp::Eq,
                    Op::Ne => xu_ir::BinaryOp::Ne,
                    _ => unreachable!(),
                };
                stack[sp] = match a.bin_op(bop, b) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                sp += 1;
            }
            Op::GetMember(idx, slot) => {
                if sp == 0 {
                    return Some(Err("Stack underflow".into()));
                }
                sp -= 1;
                let obj = stack[sp];
                let field = rt.get_const_str(*idx, &bc.constants);
                let v = match rt.get_member_with_ic_raw(obj, field, *slot) {
                    Ok(v) => v,
                    Err(e) => return Some(Err(e)),
                };
                stack[sp] = v;
                sp += 1;
            }
            Op::Return => {
                if sp == 0 {
                    return Some(Ok(Value::VOID));
                }
                return Some(Ok(stack[sp - 1]));
            }
            _ => return None,
        }
    }
    Some(Ok(Value::VOID))
}
