use crate::core::Value;
use crate::Runtime;
use crate::core::gc::ManagedObject;
use crate::vm::stack::IterState;
use crate::Flow;

#[inline(always)]
pub(super) fn op_jump(ip: &mut usize, to: usize) {
    *ip = to;
}

#[inline(always)]
pub(super) fn op_jump_if_false(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    to: usize,
) -> Result<(), String> {
    let v = stack.pop().ok_or("Stack underflow")?;
    if v.is_bool() {
        if !v.as_bool() {
            *ip = to;
        } else {
            *ip += 1;
        }
        Ok(())
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::InvalidConditionType(
            v.type_name().to_string(),
        )))
    }
}

#[inline(always)]
pub(super) fn op_call(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    n: usize,
) -> Result<(), String> {
    if stack.len() < n + 1 {
        return Err("Stack underflow".to_string());
    }
    let args_start = stack.len() - n;
    // We need to extract args.
    // Optimization: check if we can avoid allocation for small args?
    // Current impl uses SmallVec in ir.rs, let's replicate that or use Vec.
    use smallvec::SmallVec;
    let args: SmallVec<[Value; 8]> = stack.drain(args_start..).collect();
    let callee = stack.pop().expect("Stack underflow"); // args_start - 1

    // Fast path logic for bytecode functions can be added here if needed,
    // but for simplicity/code size, we might delegate to rt.call_function first.
    // To keep it simple and correct:
    match rt.call_function(callee, &args) {
        Ok(v) => {
            stack.push(v);
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[inline(always)]
pub(super) fn op_return(
    stack: &mut Vec<Value>,
) -> Result<Flow, String> {
    let v = stack.pop().ok_or("Stack underflow")?;
    Ok(Flow::Return(v))
}

#[inline(always)]
pub(super) fn op_foreach_init(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    iters: &mut Vec<IterState>,
    ip: &mut usize,
    idx: usize,
    var_idx: Option<usize>,
    end: usize,
    constants: &[xu_ir::Constant],
) -> Result<(), String> {
    use crate::core::value::{TAG_LIST, TAG_RANGE};

    let iterable = stack.pop().ok_or("Stack underflow")?;
    let tag = iterable.get_tag();
    let var = rt.get_const_str(idx as u32, constants);

    let first_val = if tag == TAG_LIST {
        let id = iterable.as_obj_id();
        let len = match rt.heap.get(id) {
            ManagedObject::List(v) => v.len(),
            _ => return Err("Not a list".into()),
        };
        if len == 0 {
            *ip = end;
            return Ok(());
        }
        let first = match rt.heap.get(id) {
            ManagedObject::List(v) => v.get(0).cloned().unwrap_or(Value::VOID),
            _ => return Err("Not a list".into()),
        };
        iters.push(IterState::List { id, idx: 1, len });
        first
    } else if tag == TAG_RANGE {
        let id = iterable.as_obj_id();
        let (start, r_end, inclusive) = match rt.heap.get(id) {
            ManagedObject::Range(s, e, inc) => (*s, *e, *inc),
            _ => return Err("Not a range".into()),
        };
        let step = if start <= r_end { 1 } else { -1 };
        if !inclusive {
            if (step > 0 && start >= r_end) || (step < 0 && start <= r_end) {
                *ip = end;
                return Ok(());
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
        rt.set_local_by_index(v_idx, first_val);
    } else if rt.locals.is_active() {
        if !rt.set_local(var, first_val) {
            rt.define_local(var.to_string(), first_val);
        }
    } else {
        rt.env.define(var.to_string(), first_val);
    }
    *ip += 1;
    Ok(())
}

#[inline(always)]
pub(super) fn op_foreach_next(
    rt: &mut Runtime,
    iters: &mut Vec<IterState>,
    ip: &mut usize,
    idx: usize,
    var_idx: Option<usize>,
    loop_start: usize,
    end: usize,
    constants: &[xu_ir::Constant],
) -> Result<(), String> {
    let Some(state) = iters.last_mut() else {
        return Err("Iterator underflow".into());
    };
    let var = rt.get_const_str(idx as u32, constants);

    let next_val = match state {
        IterState::List { id, idx, len, .. } => {
            if *idx >= *len {
                None
            } else {
                let item = match rt.heap.get(*id) {
                    ManagedObject::List(v) => v.get(*idx).cloned().unwrap_or(Value::VOID),
                    _ => return Err("Not a list".into()),
                };
                *idx += 1;
                Some(item)
            }
        }
        IterState::Range { cur, end: r_end, step, inclusive, .. } => {
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
        IterState::Dict { .. } | IterState::DictKV { .. } => {
            return Err("Dict iteration not supported in bytecode foreach".into());
        }
    };

    if let Some(val) = next_val {
        if let Some(v_idx) = var_idx {
            rt.set_local_by_index(v_idx, val);
        } else if rt.locals.is_active() {
            if !rt.set_local(var, val) {
                rt.define_local(var.to_string(), val);
            }
        } else {
            rt.env.define(var.to_string(), val);
        }
        *ip = loop_start;
    } else {
        iters.pop();
        *ip = end;
    }
    Ok(())
}
