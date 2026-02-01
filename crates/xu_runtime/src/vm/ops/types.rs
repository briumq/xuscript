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
use crate::vm::exception::throw_value;
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Execute Op::DefineStruct - define a struct type
#[inline(always)]
pub(crate) fn op_define_struct(rt: &mut Runtime, bc: &Bytecode, idx: u32) {
    let c = rt.get_constant(idx, &bc.constants);
    if let xu_ir::Constant::Struct(def) = c {
        let layout: std::rc::Rc<[String]> = def.fields.iter().map(|f| f.name.clone()).collect();
        rt.struct_layouts.insert(def.name.clone(), layout);
        rt.structs.insert(def.name.clone(), def.clone());
    }
}

/// Execute Op::DefineEnum - define an enum type
#[inline(always)]
pub(crate) fn op_define_enum(rt: &mut Runtime, bc: &Bytecode, idx: u32) {
    let c = rt.get_constant(idx, &bc.constants);
    if let xu_ir::Constant::Enum(def) = c {
        rt.enums.insert(def.name.clone(), def.variants.to_vec());
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
            rt, ip, handlers, stack, iters, pending, thrown, err_val,
        ) {
            return Ok(Some(flow));
        }
        return Ok(None);
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
                            let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(e.into())));
                            if let Some(flow) = throw_value(
                                rt, ip, handlers, stack, iters, pending, thrown, err_val,
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
        let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
        if let Some(pos) = layout.iter().position(|f| f == k) {
            values[pos] = v;
        }
    }

    let id = rt
        .heap
        .alloc(ManagedObject::Struct(Box::new(crate::core::value::StructInstance {
            ty: ty.clone(),
            ty_hash: xu_ir::stable_hash64(&ty),
            fields: values.into_boxed_slice(),
            field_names: layout.clone(),
        })));
    stack.push(Value::struct_obj(id));
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
        payload.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
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
    let v = stack.last().ok_or_else(|| "Stack underflow".to_string())?;
    if !type_matches(name, v, &rt.heap) {
        let msg = rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: name.to_string(),
            actual: v.type_name().to_string(),
        });
        let err_val = Value::str(rt.heap.alloc(ManagedObject::Str(msg.into())));
        if let Some(flow) = throw_value(
            rt, ip, handlers, stack, iters, pending, thrown, err_val,
        ) {
            return Ok(Some(flow));
        }
        return Ok(None);
    }
    Ok(None)
}
