//! Iteration operations for the VM.
//!
//! This module contains operations for:
//! - ForEachInit: Initialize a foreach loop
//! - ForEachNext: Get next iteration value
//! - IterPop: Pop iterator state

use xu_ir::Bytecode;

use crate::core::heap::{ManagedObject, ObjectId};
use crate::core::value::{DictKey, TAG_DICT, TAG_LIST, TAG_RANGE};
use crate::core::Value;
use crate::errors::messages::{NOT_A_DICT, NOT_A_LIST};
use crate::vm::ops::helpers::pop_stack;
use crate::vm::stack::IterState;
use crate::Runtime;

/// Set loop variable value
#[inline(always)]
fn set_loop_var(rt: &mut Runtime, var: &str, var_idx: Option<usize>, val: Value) {
    if let Some(v_idx) = var_idx {
        rt.set_local_by_index(v_idx, val);
    } else if rt.locals.is_active() {
        if !rt.set_local(var, val) {
            rt.define_local(var.to_string(), val);
        }
    } else {
        rt.env.define(var.to_string(), val);
    }
}

/// Execute Op::ForEachInit - initialize a foreach loop
#[inline(always)]
pub(crate) fn op_foreach_init(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    iters: &mut Vec<IterState>,
    ip: &mut usize,
    idx: u32,
    var_idx: Option<usize>,
    end: usize,
) -> Result<bool, String> {
    let iterable = pop_stack(stack)?;
    let tag = iterable.get_tag();
    let var = rt.get_const_str(idx, &bc.constants);

    let first_val = if tag == TAG_LIST {
        let id = iterable.as_obj_id();
        let len = match rt.heap.get(id) {
            ManagedObject::List(v) => v.len(),
            _ => {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw(NOT_A_LIST.into())));
            }
        };
        if len == 0 {
            *ip = end;
            return Ok(true); // Signal to continue (skip loop)
        }
        let first = match rt.heap.get(id) {
            ManagedObject::List(v) => v.first().copied().unwrap_or(Value::UNIT),
            _ => {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw(NOT_A_LIST.into())));
            }
        };
        iters.push(IterState::List { id, idx: 1, len });
        first
    } else if tag == TAG_RANGE {
        let id = iterable.as_obj_id();
        let (start, r_end, inclusive) = match rt.heap.get(id) {
            ManagedObject::Range(s, e, inc) => (*s, *e, *inc),
            _ => {
                return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a range".into())));
            }
        };
        let step = if start <= r_end { 1 } else { -1 };
        if !inclusive {
            if (step > 0 && start >= r_end) || (step < 0 && start <= r_end) {
                *ip = end;
                return Ok(true); // Signal to continue (skip loop)
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
        // Check if this is a key-value pair loop (parser-transformed)
        let is_kv_loop = var.starts_with("__tmp_foreach_");

        if is_kv_loop {
            // Key-value pair loop: return (key, value) tuples
            // First collect raw data to avoid borrow conflicts
            let (raw_pairs, shape_keys, elements): (Vec<(DictKey, Value)>, Vec<(String, Value)>, Vec<(i64, Value)>) = match rt.heap.get(id) {
                ManagedObject::Dict(d) => {
                    let mut result = Vec::with_capacity(d.map.len());
                    for (k, v) in d.map.iter() {
                        result.push((*k, *v));
                    }
                    // Collect shape keys as strings
                    let shape_keys: Vec<(String, Value)> = if let Some(sid) = d.shape {
                        if let ManagedObject::Shape(shape) = rt.heap.get(sid) {
                            if let Some(pv) = d.prop_values() {
                                shape.prop_map.iter()
                                    .filter_map(|(k, off)| {
                                        pv.get(*off).map(|v| (k.clone(), *v))
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };
                    // Collect elements
                    let elements: Vec<(i64, Value)> = d.elements().map_or(Vec::new(), |e| {
                        e.iter().enumerate()
                            .filter(|(_, v)| v.get_tag() != crate::core::value::TAG_UNIT)
                            .map(|(i, v)| (i as i64, *v))
                            .collect()
                    });
                    (result, shape_keys, elements)
                }
                _ => {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())));
                }
            };

            // Now allocate strings for shape keys (outside the borrow)
            let mut all_pairs = raw_pairs;
            for (k, v) in shape_keys {
                let key = DictKey::from_str_alloc(&k, &mut rt.heap);
                all_pairs.push((key, v));
            }
            for (i, v) in elements {
                all_pairs.push((DictKey::Int(i), v));
            }

            if all_pairs.is_empty() {
                *ip = end;
                return Ok(true); // Signal to continue (skip loop)
            }
            // Now allocate memory to create tuples
            let items: Vec<Value> = all_pairs
                .into_iter()
                .map(|(k, v)| {
                    let key_val = match k {
                        DictKey::StrRef { obj_id, .. } => {
                            // Directly use the ObjectId - no string copy!
                            Value::str(ObjectId(obj_id))
                        }
                        DictKey::Int(i) => Value::from_i64(i),
                    };
                    Value::tuple(rt.alloc(ManagedObject::Tuple(vec![key_val, v])))
                })
                .collect();
            let first = items[0];
            iters.push(IterState::DictKV { items, idx: 1 });
            first
        } else {
            // Normal dict loop: only return keys
            let raw_keys: Vec<DictKey> = match rt.heap.get(id) {
                ManagedObject::Dict(d) => d.map.keys().copied().collect(),
                _ => {
                    return Err(rt.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())));
                }
            };
            if raw_keys.is_empty() {
                *ip = end;
                return Ok(true); // Signal to continue (skip loop)
            }
            let keys: Vec<Value> = raw_keys
                .into_iter()
                .map(|k| match k {
                    DictKey::StrRef { obj_id, .. } => {
                        // Directly use the ObjectId - no string copy!
                        Value::str(ObjectId(obj_id))
                    }
                    DictKey::Int(i) => Value::from_i64(i),
                })
                .collect();
            let first = keys[0];
            iters.push(IterState::Dict { keys, idx: 1 });
            first
        }
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::InvalidIteratorType {
            expected: "list, range, or dict".to_string(),
            actual: iterable.type_name().to_string(),
            iter_desc: "bytecode foreach".to_string(),
        }));
    };

    set_loop_var(rt, var, var_idx, first_val);
    Ok(false) // Normal continuation
}

/// Execute Op::ForEachNext - get next iteration value
#[inline(always)]
pub(crate) fn op_foreach_next(
    rt: &mut Runtime,
    bc: &Bytecode,
    iters: &mut Vec<IterState>,
    ip: &mut usize,
    idx: u32,
    var_idx: Option<usize>,
    loop_start: usize,
    end: usize,
) -> Result<bool, String> {
    let Some(state) = iters.last_mut() else {
        return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Iterator underflow".into())));
    };
    let var = rt.get_const_str(idx, &bc.constants);

    let next_val = match state {
        IterState::List {
            id,
            idx: list_idx,
            len,
            ..
        } => {
            if *list_idx >= *len {
                None
            } else {
                let item = match rt.heap.get(*id) {
                    ManagedObject::List(v) => v.get(*list_idx).cloned().unwrap_or(Value::UNIT),
                    _ => {
                        return Err(rt.error(xu_syntax::DiagnosticKind::Raw(NOT_A_LIST.into())));
                    }
                };
                *list_idx += 1;
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
        IterState::Dict {
            keys,
            idx: dict_idx,
        } => {
            if *dict_idx >= keys.len() {
                None
            } else {
                let item = keys[*dict_idx];
                *dict_idx += 1;
                Some(item)
            }
        }
        IterState::DictKV {
            items,
            idx: dict_idx,
        } => {
            if *dict_idx >= items.len() {
                None
            } else {
                let item = items[*dict_idx];
                *dict_idx += 1;
                Some(item)
            }
        }
    };

    if let Some(val) = next_val {
        set_loop_var(rt, var, var_idx, val);
        *ip = loop_start;
        Ok(true) // Signal to continue (loop back)
    } else {
        iters.pop();
        *ip = end;
        Ok(true) // Signal to continue (exit loop)
    }
}

/// Execute Op::IterPop - pop iterator state
#[inline(always)]
pub(crate) fn op_iter_pop(iters: &mut Vec<IterState>) -> Result<(), String> {
    let _ = iters
        .pop()
        .ok_or_else(|| "Iterator underflow".to_string())?;
    Ok(())
}
