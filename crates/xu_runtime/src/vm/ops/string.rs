//! String operations for the VM.
//!
//! This module contains operations for:
//! - StrAppend*: String concatenation operations
//! - Builder*: String builder operations

use crate::core::heap::ManagedObject;
use crate::core::value::{TAG_BUILDER, TAG_STR};
use crate::core::Value;
use crate::errors::messages::NOT_A_STRING;
use crate::util::Appendable;
use crate::vm::exception::throw_value;
use crate::vm::stack::{add_with_heap, Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Execute Op::StrAppend - append any value to string
#[inline(always)]
pub(crate) fn op_str_append(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR {
        let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.clone()
        } else {
            return Err(NOT_A_STRING.into());
        };
        sa.append_value(&b, &rt.heap);
        stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
    } else {
        match add_with_heap(rt, a, b) {
            Ok(r) => stack.push(r),
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
    Ok(None)
}

/// Execute Op::StrAppendNull - append null to string
#[inline(always)]
pub(crate) fn op_str_append_null(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR {
        let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.clone()
        } else {
            return Err(NOT_A_STRING.into());
        };
        sa.append_null();
        stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
    } else {
        match add_with_heap(rt, a, Value::VOID) {
            Ok(r) => stack.push(r),
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
    Ok(None)
}

/// Execute Op::StrAppendBool - append bool to string
#[inline(always)]
pub(crate) fn op_str_append_bool(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.is_bool() {
        let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.clone()
        } else {
            return Err(NOT_A_STRING.into());
        };
        sa.append_bool(b.as_bool());
        stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
    } else {
        match add_with_heap(rt, a, b) {
            Ok(r) => stack.push(r),
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
    Ok(None)
}

/// Execute Op::StrAppendInt - append int to string
#[inline(always)]
pub(crate) fn op_str_append_int(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.is_int() {
        let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.clone()
        } else {
            return Err(NOT_A_STRING.into());
        };
        sa.append_i64(b.as_i64());
        stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
    } else {
        match add_with_heap(rt, a, b) {
            Ok(r) => stack.push(r),
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
    Ok(None)
}

/// Execute Op::StrAppendFloat - append float to string
#[inline(always)]
pub(crate) fn op_str_append_float(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.is_f64() {
        let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.clone()
        } else {
            return Err(NOT_A_STRING.into());
        };
        sa.append_f64(b.as_f64());
        stack.push(Value::str(rt.heap.alloc(ManagedObject::Str(sa))));
    } else {
        match add_with_heap(rt, a, b) {
            Ok(r) => stack.push(r),
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
    Ok(None)
}

/// Execute Op::StrAppendStr - append string to string
#[inline(always)]
pub(crate) fn op_str_append_str(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let a = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if a.get_tag() == TAG_STR && b.get_tag() == TAG_STR {
        let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
            s.clone()
        } else {
            return Err(NOT_A_STRING.into());
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
                    rt, ip, handlers, stack, iters, pending, thrown, err_val,
                ) {
                    return Ok(Some(flow));
                }
                return Ok(None);
            }
        }
    }
    Ok(None)
}

/// Execute Op::BuilderNewCap - create a new string builder with capacity
#[inline(always)]
pub(crate) fn op_builder_new_cap(rt: &mut Runtime, stack: &mut Vec<Value>, cap: usize) {
    let s = rt.builder_pool_get(cap);
    let id = rt.heap.alloc(ManagedObject::Builder(s));
    stack.push(Value::builder(id));
}

/// Execute Op::BuilderAppend - append value to string builder
#[inline(always)]
pub(crate) fn op_builder_append(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
) -> Result<(), String> {
    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if b.get_tag() != TAG_BUILDER {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "builder_push".to_string(),
            ty: b.type_name().to_string(),
        }));
    }
    let id = b.as_obj_id();
    // Optimized: check most common cases first (int and str)
    if v.is_int() {
        let mut buf = itoa::Buffer::new();
        let digits = buf.format(v.as_i64());
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str(digits);
        }
    } else if v.get_tag() == TAG_STR {
        // Optimization: avoid clone by using raw pointer
        let str_id = v.as_obj_id();
        let ptr = if let ManagedObject::Str(s) = rt.heap.get(str_id) {
            s.as_str().as_ptr()
        } else {
            "".as_ptr()
        };
        let len = if let ManagedObject::Str(s) = rt.heap.get(str_id) {
            s.as_str().len()
        } else {
            0
        };
        if let ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            // SAFETY: ptr/len are valid, builder and string are different objects
            let slice = unsafe { std::slice::from_raw_parts(ptr, len) };
            let s_ref = unsafe { std::str::from_utf8_unchecked(slice) };
            sb.push_str(s_ref);
        }
    } else if v.is_f64() {
        let f = v.as_f64();
        if f.fract() == 0.0 {
            let mut buf = itoa::Buffer::new();
            let digits = buf.format(f as i64);
            if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                s.push_str(digits);
            }
        } else {
            let mut buf = ryu::Buffer::new();
            let digits = buf.format(f);
            if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                s.push_str(digits);
            }
        }
    } else if v.is_bool() {
        let piece = if v.as_bool() { "true" } else { "false" };
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str(piece);
        }
    } else if v.is_void() {
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str("()");
        }
    } else {
        let piece = crate::util::value_to_string(&v, &rt.heap);
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str(&piece);
        }
    }
    stack.push(b);
    Ok(())
}

/// Execute Op::BuilderFinalize - finalize string builder to string
#[inline(always)]
pub(crate) fn op_builder_finalize(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if b.get_tag() != TAG_BUILDER {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "builder_finalize".to_string(),
            ty: b.type_name().to_string(),
        }));
    }
    let id = b.as_obj_id();
    // Take ownership of the builder string and return it to pool
    let (out, builder_str) = if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
        let text = crate::core::text::Text::from_str(s.as_str());
        let taken = std::mem::take(s);
        (text, Some(taken))
    } else {
        return Err("Not a builder".into());
    };
    // Return the string to the pool for reuse
    if let Some(s) = builder_str {
        rt.builder_pool_return(s);
    }
    let sid = rt.heap.alloc(ManagedObject::Str(out));
    stack.push(Value::str(sid));
    Ok(())
}
