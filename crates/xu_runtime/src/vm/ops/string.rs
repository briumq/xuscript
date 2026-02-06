//! String operations for the VM.
//!
//! This module contains operations for:
//! - StrAppend*: String concatenation operations
//! - Builder*: String builder operations

use crate::core::heap::ManagedObject;
use crate::core::text::Text;
use crate::core::value::{TAG_BUILDER, TAG_STR};
use crate::core::Value;
use crate::errors::messages::NOT_A_STRING;
use crate::util::Appendable;
use crate::vm::ops::helpers::{pop_stack, pop2_stack, try_throw_error};
use crate::vm::stack::{add_with_heap, Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// Handle fallback path for string operations when fast path doesn't apply.
/// Uses add_with_heap and handles errors by throwing exceptions.
#[inline(always)]
fn handle_str_op_fallback(
    rt: &mut Runtime,
    a: Value,
    b: Value,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
) -> Result<Option<Flow>, String> {
    match add_with_heap(rt, a, b) {
        Ok(r) => {
            stack.push(r);
            Ok(None)
        }
        Err(e) => {
            if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, e) {
                return Ok(Some(flow));
            }
            Ok(None)
        }
    }
}

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
    let (a, b) = pop2_stack(stack)?;
    if a.get_tag() == TAG_STR {
        // Fast path: both operands are strings - use concat2
        if b.get_tag() == TAG_STR {
            let result = {
                let sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                    s
                } else {
                    return Err(NOT_A_STRING.into());
                };
                let sb = if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                    s
                } else {
                    return Err(NOT_A_STRING.into());
                };
                Text::concat2(sa, sb)
            };
            stack.push(Value::str(rt.alloc(ManagedObject::Str(result))));
        } else {
            // Slow path: need to convert b to string
            let mut sa = if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s.clone()
            } else {
                return Err(NOT_A_STRING.into());
            };
            sa.append_value(&b, &rt.heap);
            stack.push(Value::str(rt.alloc(ManagedObject::Str(sa))));
        }
    } else {
        return handle_str_op_fallback(rt, a, b, stack, ip, handlers, iters, pending, thrown);
    }
    Ok(None)
}

/// Execute Op::BuilderNewCap - create a new string builder with capacity
#[inline(always)]
pub(crate) fn op_builder_new_cap(rt: &mut Runtime, stack: &mut Vec<Value>, cap: usize) {
    let s = rt.builder_pool_get(cap);
    let id = rt.alloc(ManagedObject::Builder(s));
    stack.push(Value::builder(id));
}

/// Execute Op::BuilderAppend - append value to string builder
#[inline(always)]
pub(crate) fn op_builder_append(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
) -> Result<(), String> {
    let v = pop_stack(stack)?;
    let b = pop_stack(stack)?;
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
        if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
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
        if let ManagedObject::Builder(sb) = rt.heap_get_mut(id) {
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
            if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
                s.push_str(digits);
            }
        } else {
            let mut buf = ryu::Buffer::new();
            let digits = buf.format(f);
            if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
                s.push_str(digits);
            }
        }
    } else if v.is_bool() {
        let piece = if v.as_bool() { "true" } else { "false" };
        if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
            s.push_str(piece);
        }
    } else if v.is_unit() {
        if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
            s.push_str("()");
        }
    } else {
        let piece = crate::util::value_to_string(&v, &rt.heap);
        if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
            s.push_str(&piece);
        }
    }
    stack.push(b);
    Ok(())
}

/// Execute Op::BuilderFinalize - finalize string builder to string
#[inline(always)]
pub(crate) fn op_builder_finalize(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let b = pop_stack(stack)?;
    if b.get_tag() != TAG_BUILDER {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "builder_finalize".to_string(),
            ty: b.type_name().to_string(),
        }));
    }
    let id = b.as_obj_id();
    // Take ownership of the builder string and return it to pool
    let (out, builder_str) = if let ManagedObject::Builder(s) = rt.heap_get_mut(id) {
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
    let sid = rt.alloc(ManagedObject::Str(out));
    stack.push(Value::str(sid));
    Ok(())
}
