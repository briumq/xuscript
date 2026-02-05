//! Type system operations for the VM.
//!
//! This module contains operations for:
//! - DefineStruct: Define a struct type
//! - DefineEnum: Define an enum type
//! - StructInit: Initialize a struct instance
//! - EnumCtor: Create an enum variant (no payload)
//! - EnumCtorN: Create an enum variant (with payload)
//! - AssertType: Assert value matches expected type

use xu_ir::Bytecode;

use crate::core::heap::ManagedObject;
use crate::core::Value;
use crate::util::type_matches;
use crate::vm::ops::helpers::{pop_stack, peek_last, try_throw_error};
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Create a struct instance and push to stack
#[inline(always)]
fn create_struct_instance(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ty: &str,
    layout: &std::rc::Rc<[String]>,
    values: Vec<Value>,
) {
    let id = rt
        .heap
        .alloc(ManagedObject::Struct(Box::new(crate::core::value::StructInstance {
            ty: ty.to_string(),
            ty_hash: xu_ir::stable_hash64(ty),
            fields: values.into_boxed_slice(),
            field_names: layout.clone(),
        })));
    stack.push(Value::struct_obj(id));
}

/// Throw unknown struct error
#[inline(always)]
fn throw_unknown_struct(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    ty: &str,
) -> Result<Option<Flow>, String> {
    let err_msg = rt.error(xu_syntax::DiagnosticKind::UnknownStruct(ty.to_string()));
    if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, err_msg) {
        return Ok(Some(flow));
    }
    Ok(None)
}

/// Execute Op::DefineStruct - define a struct type
#[inline(always)]
pub(crate) fn op_define_struct(rt: &mut Runtime, bc: &Bytecode, idx: u32) {
    let c = rt.get_constant(idx, &bc.constants);
    if let xu_ir::Constant::Struct(def) = c {
        let layout: std::rc::Rc<[String]> = def.fields.iter().map(|f| f.name.clone()).collect();
        rt.types.struct_layouts.insert(def.name.clone(), layout);
        rt.types.structs.insert(def.name.clone(), def.clone());
    }
}

/// Execute Op::DefineEnum - define an enum type
#[inline(always)]
pub(crate) fn op_define_enum(rt: &mut Runtime, bc: &Bytecode, idx: u32) {
    let c = rt.get_constant(idx, &bc.constants);
    if let xu_ir::Constant::Enum(def) = c {
        rt.types.enums.insert(def.name.clone(), def.variants.to_vec());
    }
}

/// Execute Op::StructInit - initialize a struct instance
#[inline(always)]
pub(crate) fn op_struct_init(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    t_idx: u32,
    n_idx: u32,
) -> Result<Option<Flow>, String> {
    let ty = rt.get_const_str(t_idx, &bc.constants).to_string();
    let fields = rt.get_const_names(n_idx, &bc.constants).to_vec();
    let layout = if let Some(l) = rt.types.struct_layouts.get(&ty).cloned() {
        l
    } else {
        return throw_unknown_struct(rt, stack, ip, handlers, iters, pending, thrown, &ty);
    };

    let mut values = vec![Value::UNIT; layout.len()];

    // Apply default values from struct definition
    if let Some(def) = rt.types.structs.get(&ty).cloned() {
        for (i, field) in def.fields.iter().enumerate() {
            if let Some(ref default_expr) = field.default {
                if i < values.len() {
                    match rt.eval_expr(default_expr) {
                        Ok(v) => values[i] = v,
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
            }
        }
    }

    // Override with explicitly provided field values
    for k in fields.iter().rev() {
        let v = pop_stack(stack)?;
        if let Some(pos) = layout.iter().position(|f| f == k) {
            values[pos] = v;
        }
    }

    create_struct_instance(rt, stack, &ty, &layout, values);
    Ok(None)
}

/// Execute Op::StructInitSpread - initialize a struct instance with spread
/// Stack layout: [spread_src, field_values...] (spread_src at bottom)
#[inline(always)]
pub(crate) fn op_struct_init_spread(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    t_idx: u32,
    n_idx: u32,
) -> Result<Option<Flow>, String> {
    let ty = rt.get_const_str(t_idx, &bc.constants).to_string();
    let explicit_fields = rt.get_const_names(n_idx, &bc.constants).to_vec();
    let layout = if let Some(l) = rt.types.struct_layouts.get(&ty).cloned() {
        l
    } else {
        return throw_unknown_struct(rt, stack, ip, handlers, iters, pending, thrown, &ty);
    };

    let mut values = vec![Value::UNIT; layout.len()];

    // Pop explicit field values first (they're on top of stack)
    let mut explicit_values: Vec<Value> = Vec::with_capacity(explicit_fields.len());
    for _ in 0..explicit_fields.len() {
        explicit_values.push(pop_stack(stack)?);
    }
    explicit_values.reverse();

    // Pop spread source (at bottom)
    let spread_src = pop_stack(stack)?;

    // Apply values from spread source
    let spread_tag = spread_src.get_tag();
    if spread_tag == crate::core::value::TAG_STRUCT {
        let id = spread_src.as_obj_id();
        if let ManagedObject::Struct(si) = rt.heap.get(id) {
            if si.ty.as_str() != ty.as_str() {
                let err_msg = rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                    expected: ty.clone(),
                    actual: si.ty.as_str().to_string(),
                });
                if let Some(flow) = try_throw_error(
                    rt, ip, handlers, stack, iters, pending, thrown, err_msg,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
            for (i, fname) in si.field_names.iter().enumerate() {
                if let Some(pos) = layout.iter().position(|f| f.as_str() == fname.as_str()) {
                    values[pos] = si.fields[i];
                }
            }
        }
    } else if spread_tag == crate::core::value::TAG_DICT {
        let id = spread_src.as_obj_id();
        if let ManagedObject::Dict(db) = rt.heap.get(id) {
            for (pos, fname) in layout.iter().enumerate() {
                // Check shape properties first
                if let Some(sid) = db.shape {
                    if let ManagedObject::Shape(shape) = rt.heap.get(sid) {
                        if let Some(&off) = shape.prop_map.get(fname.as_str()) {
                            if let Some(v) = db.prop_values.get(off) {
                                values[pos] = *v;
                                continue;
                            }
                        }
                    }
                }
                // Check map
                let hash = crate::Runtime::hash_bytes(db.map.hasher(), fname.as_bytes());
                if let Some(v) = crate::Runtime::dict_get_by_str_with_hash(db, fname.as_str(), hash) {
                    values[pos] = v;
                }
            }
        }
    }

    // Override with explicit field values
    for (k, v) in explicit_fields.iter().zip(explicit_values.into_iter()) {
        if let Some(pos) = layout.iter().position(|f| f == k) {
            values[pos] = v;
        }
    }

    create_struct_instance(rt, stack, &ty, &layout, values);
    Ok(None)
}

/// Execute Op::EnumCtor - create an enum variant (no payload)
#[inline(always)]
pub(crate) fn op_enum_ctor(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    t_idx: u32,
    v_idx: u32,
) -> Result<(), String> {
    let ty = rt.get_const_str(t_idx, &bc.constants);
    let variant = rt.get_const_str(v_idx, &bc.constants);
    let v = rt.enum_new_checked(ty, variant, Box::new([]))?;
    stack.push(v);
    Ok(())
}

/// Execute Op::EnumCtorN - create an enum variant (with payload)
#[inline(always)]
pub(crate) fn op_enum_ctor_n(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    t_idx: u32,
    v_idx: u32,
    argc: usize,
) -> Result<(), String> {
    let ty = rt.get_const_str(t_idx, &bc.constants);
    let variant = rt.get_const_str(v_idx, &bc.constants);
    let mut payload: Vec<Value> = Vec::with_capacity(argc);
    for _ in 0..argc {
        payload.push(pop_stack(stack)?);
    }
    payload.reverse();
    let v = rt.enum_new_checked(ty, variant, payload.into_boxed_slice())?;
    stack.push(v);
    Ok(())
}

/// Execute Op::AssertType - assert value matches expected type
#[inline(always)]
pub(crate) fn op_assert_type(
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
    let name = rt.get_const_str(idx, &bc.constants);
    let v = peek_last(stack)?;
    if !type_matches(name, v, &rt.heap) {
        let msg = rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: name.to_string(),
            actual: v.type_name().to_string(),
        });
        if let Some(flow) = try_throw_error(
            rt, ip, handlers, stack, iters, pending, thrown, msg,
        ) {
            return Ok(Some(flow));
        }
        return Ok(None);
    }
    Ok(None)
}
