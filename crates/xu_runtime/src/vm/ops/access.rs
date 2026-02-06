//! Member access operations for the VM.
//!
//! This module contains operations for:
//! - GetMember: Access struct/object members
//! - GetIndex: Array/dict indexing
//! - AssignMember: Assign to struct/object members
//! - AssignIndex: Assign to array/dict elements
//! - GetStaticField: Access static fields
//! - SetStaticField: Assign to static fields
//! - InitStaticField: Initialize static fields

use xu_ir::Bytecode;

use crate::core::heap::ManagedObject;
use crate::core::value::{TAG_DICT, TAG_LIST, TAG_STR, TAG_TUPLE, ELEMENTS_MAX};
use crate::core::Value;
use crate::errors::messages::NOT_A_LIST;
use crate::runtime::DictCacheIntLast;
use crate::vm::ops::helpers::{pop_stack, pop2_stack, try_throw_error, handle_result, handle_result_push};
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Execute Op::GetMember - access struct/object member
#[inline(always)]
pub(crate) fn op_get_member(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    idx: u32,
    slot_idx: Option<usize>,
) -> Result<Option<Flow>, String> {
    let obj = pop_stack(stack)?;
    let tag = obj.get_tag();
    let field = rt.get_const_str(idx, &bc.constants);

    if tag == crate::core::value::TAG_STRUCT {
        let id = obj.as_obj_id();
        if let ManagedObject::Struct(s) = rt.heap.get(id) {
            let field_hash = xu_ir::stable_hash64(field);
            // IC check - fast path
            if let Some(idx_slot) = slot_idx {
                if idx_slot < rt.caches.ic_slots.len() {
                    let c = &rt.caches.ic_slots[idx_slot];
                    if c.struct_ty_hash == s.ty_hash && c.key_hash == field_hash {
                        if let Some(offset) = c.field_offset {
                            stack.push(s.fields[offset]);
                            return Ok(None);
                        }
                    }
                }
            }
            // Slow path
            let result = rt.get_member_with_ic_raw(obj, field, slot_idx);
            return handle_result_push(rt, ip, handlers, stack, iters, pending, thrown, result);
        } else {
            return Err("Not a struct".into());
        }
    }

    let result = rt.get_member_with_ic_raw(obj, field, slot_idx);
    handle_result_push(rt, ip, handlers, stack, iters, pending, thrown, result)
}

/// Execute Op::GetIndex - array/dict indexing
#[inline(always)]
pub(crate) fn op_get_index(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    slot_cell: Option<usize>,
) -> Result<Option<Flow>, String> {
    let (obj, idx) = pop2_stack(stack)?;
    let tag = obj.get_tag();

    if tag == TAG_DICT {
        let id = obj.as_obj_id();
        if let ManagedObject::Dict(me) = rt.heap.get(id) {
            let cur_ver = me.ver;
            let mut val = None;
            if idx.is_int() {
                let key = idx.as_i64();
                if key >= 0 && key < ELEMENTS_MAX {
                    let ui = key as usize;
                    if ui < me.elements.len() {
                        let v = me.elements[ui];
                        if v.get_tag() != crate::core::value::TAG_UNIT {
                            val = Some(v);
                        }
                    }
                }

                if val.is_none() {
                    if let Some(c) = rt.caches.dict_cache_int_last.as_ref() {
                        if c.id == id.0 && c.ver == cur_ver && c.key == key {
                            val = Some(c.value);
                        }
                    }
                }
                if val.is_none() {
                    // Use raw_entry to avoid creating DictKey
                    let hash = Runtime::hash_dict_key_int(me.map.hasher(), key);
                    if let Some((_, v)) = me.map.raw_entry().from_hash(hash, |k| matches!(k, crate::core::value::DictKey::Int(i) if *i == key)) {
                        val = Some(*v);
                        rt.caches.dict_cache_int_last = Some(DictCacheIntLast {
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
                        if let Some(c) = rt.caches.dict_cache_last.as_ref() {
                            if c.id == id.0
                                && c.ver == cur_ver
                                && c.key.as_str() == key_text.as_str()
                            {
                                val = Some(c.value);
                            }
                        }
                    }
                    // Hash lookup - don't update cache for every lookup
                    // (cache is only useful for repeated access to same key)
                    if val.is_none() {
                        let key_hash = Runtime::hash_bytes(me.map.hasher(), key_text.as_bytes());
                        if let Some(v) =
                            Runtime::dict_get_by_str_with_hash(me, key_text.as_str(), key_hash)
                        {
                            val = Some(v);
                        }
                    }
                }
            }

            if let Some(v) = val {
                stack.push(v);
                return Ok(None);
            }
        }
        // Slow path for Dict
        match rt.get_index_with_ic_raw(obj, idx, slot_cell) {
            Ok(v) => stack.push(v),
            Err(e) => {
                if let Some(flow) = try_throw_error(
                    rt, ip, handlers, stack, iters, pending, thrown, e,
                ) {
                    return Ok(Some(flow));
                }
            }
        }
        return Ok(None);
    } else if tag == TAG_LIST && idx.is_int() {
        let id = obj.as_obj_id();
        let i = idx.as_i64();
        if let ManagedObject::List(l) = rt.heap.get(id) {
            if i >= 0 && (i as usize) < l.len() {
                stack.push(l[i as usize]);
                return Ok(None);
            } else {
                match rt.get_index_with_ic_raw(obj, idx, slot_cell) {
                    Ok(v) => stack.push(v),
                    Err(e) => {
                        if let Some(flow) = try_throw_error(
                            rt, ip, handlers, stack, iters, pending, thrown, e,
                        ) {
                            return Ok(Some(flow));
                        }
                        return Ok(None);
                    }
                }
                return Ok(None);
            }
        } else {
            return Err(NOT_A_LIST.into());
        }
    } else if tag == TAG_TUPLE && idx.is_int() {
        let id = obj.as_obj_id();
        let i = idx.as_i64();
        if let ManagedObject::Tuple(t) = rt.heap.get(id) {
            if i >= 0 && (i as usize) < t.len() {
                stack.push(t[i as usize]);
                return Ok(None);
            }
        }
        match rt.get_index_with_ic_raw(obj, idx, slot_cell) {
            Ok(v) => stack.push(v),
            Err(e) => {
                if let Some(flow) = try_throw_error(
                    rt, ip, handlers, stack, iters, pending, thrown, e,
                ) {
                    return Ok(Some(flow));
                }
            }
        }
    } else {
        match rt.get_index_with_ic_raw(obj, idx, slot_cell) {
            Ok(v) => stack.push(v),
            Err(e) => {
                if let Some(flow) = try_throw_error(
                    rt, ip, handlers, stack, iters, pending, thrown, e,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Execute Op::AssignMember - assign to struct/object member
#[inline(always)]
pub(crate) fn op_assign_member(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    idx: u32,
    op_type: xu_ir::AssignOp,
) -> Result<Option<Flow>, String> {
    let (rhs, obj) = pop2_stack(stack)?;
    let field = rt.get_const_str(idx, &bc.constants);
    let result = rt.assign_member(obj, field, op_type, rhs);
    handle_result(rt, ip, handlers, stack, iters, pending, thrown, result)
}

/// Execute Op::AssignIndex - assign to array/dict element
#[inline(always)]
pub(crate) fn op_assign_index(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    op: xu_ir::AssignOp,
) -> Result<Option<Flow>, String> {
    let idxv = pop_stack(stack)?;
    let obj = pop_stack(stack)?;
    let rhs = pop_stack(stack)?;
    let result = rt.assign_index(obj, idxv, op, rhs);
    handle_result(rt, ip, handlers, stack, iters, pending, thrown, result)
}

/// Execute Op::GetStaticField - get a static field value
/// Falls back to instance member access if not a static field
#[inline(always)]
pub(crate) fn op_get_static_field(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    type_idx: u32,
    field_idx: u32,
) -> Result<Option<Flow>, String> {
    let type_name = rt.get_const_str(type_idx, &bc.constants);
    let field_name = rt.get_const_str(field_idx, &bc.constants);

    // First check if it's a static field
    let key = (type_name.to_string(), field_name.to_string());
    if let Some(value) = rt.types.static_fields.get(&key) {
        stack.push(*value);
        return Ok(None);
    }

    // Not a static field - try as instance member access on a global variable
    if let Some(obj) = rt.env.get(type_name) {
        let result = rt.get_member_with_ic_raw(obj, field_name, None);
        return handle_result_push(rt, ip, handlers, stack, iters, pending, thrown, result);
    }

    // Neither static field nor global variable found
    let err_msg = format!("Undefined static field: {}.{}", type_name, field_name);
    if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, err_msg) {
        return Ok(Some(flow));
    }
    Ok(None)
}

/// Execute Op::SetStaticField - set a static field value
/// Falls back to instance member assignment if not a static field
#[inline(always)]
pub(crate) fn op_set_static_field(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    type_idx: u32,
    field_idx: u32,
) -> Result<Option<Flow>, String> {
    let type_name = rt.get_const_str(type_idx, &bc.constants);
    let field_name = rt.get_const_str(field_idx, &bc.constants);
    let value = pop_stack(stack)?;

    // First check if it's a static field
    let key = (type_name.to_string(), field_name.to_string());
    if rt.types.static_fields.contains_key(&key) {
        rt.types.static_fields.insert(key, value);
        return Ok(None);
    }

    // Not a static field - try as instance member assignment on a global variable
    if let Some(obj) = rt.env.get(type_name) {
        let result = rt.assign_member(obj, field_name, xu_ir::AssignOp::Set, value);
        return handle_result(rt, ip, handlers, stack, iters, pending, thrown, result);
    }

    // Neither static field nor global variable found
    let err_msg = format!("Undefined static field: {}.{}", type_name, field_name);
    if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, err_msg) {
        return Ok(Some(flow));
    }
    Ok(None)
}

/// Execute Op::InitStaticField - initialize a static field (used during struct definition)
#[inline(always)]
pub(crate) fn op_init_static_field(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    type_idx: u32,
    field_idx: u32,
) -> Result<(), String> {
    let type_name = rt.get_const_str(type_idx, &bc.constants);
    let field_name = rt.get_const_str(field_idx, &bc.constants);
    let value = pop_stack(stack)?;

    let key = (type_name.to_string(), field_name.to_string());
    rt.types.static_fields.insert(key, value);
    Ok(())
}
