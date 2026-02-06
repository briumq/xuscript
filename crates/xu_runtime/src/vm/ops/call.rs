//! Function call operations for the VM.

use indexmap::map::RawEntryApiV1;
use smallvec::SmallVec;
use xu_ir::{Bytecode, Op};

use crate::core::heap::ManagedObject;
use crate::core::value::{DictKey, Function, TAG_DICT, TAG_STR};
use crate::core::Value;
use crate::vm::ops::helpers::{pop_stack, try_throw_error};
use crate::vm::fast::run_bytecode_fast_params_only;
use crate::vm::stack::{Handler, IterState, Pending};
use crate::{Flow, Runtime};

/// 处理调用结果，成功时推入栈，失败时尝试抛出错误
#[inline(always)]
fn handle_call_result(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    result: Result<Value, String>,
) -> Result<Option<Flow>, String> {
    match result {
        Ok(v) => { stack.push(v); Ok(None) }
        Err(e) => {
            if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, e) {
                Ok(Some(flow))
            } else {
                Ok(None)
            }
        }
    }
}

/// Execute Op::Call - direct function call
#[inline(always)]
pub(crate) fn op_call(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    n: usize,
) -> Result<Option<Flow>, String> {
    if stack.len() < n + 1 {
        return Err("Stack underflow in Call".to_string());
    }
    let args_start = stack.len() - n;
    let callee = stack[args_start - 1];

    // Fast path for bytecode functions
    let mut fast_res = None;
    if callee.get_tag() == crate::core::value::TAG_FUNC {
        let func_id = callee.as_obj_id();
        if let ManagedObject::Function(crate::core::value::Function::Bytecode(f)) =
            rt.heap.get(func_id)
        {
            let f = f.clone();
            if !f.needs_env_frame
                && f.def.params.len() == n
                && f.def.params.iter().all(|p| p.default.is_none())
            {
                let args = &stack[args_start..];
                if let Some(res) =
                    run_bytecode_fast_params_only(rt, &f.bytecode, &f.def.params, args)
                {
                    fast_res = Some(res);
                }
            }
        }
    }

    let result = if let Some(res) = fast_res {
        stack.truncate(args_start - 1);
        res
    } else {
        let args: SmallVec<[Value; 8]> = stack.drain(args_start..).collect();
        let callee = stack.pop().expect("Stack underflow in Call (callee)");
        stack.truncate(args_start - 1);
        rt.call_function(callee, &args)
    };
    handle_call_result(rt, stack, ip, handlers, iters, pending, thrown, result)
}

/// Execute Op::CallMethod - method call with IC caching
#[inline(always)]
pub(crate) fn op_call_method(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    m_idx: u32,
    method_hash: u64,
    n: usize,
    slot_idx: Option<usize>,
) -> Result<Option<Flow>, String> {
    if stack.len() < n + 1 {
        return Err("Stack underflow in CallMethod".to_string());
    }
    let args_start = stack.len() - n;
    let recv = stack[args_start - 1];
    let tag = recv.get_tag();

    // Fast paths for dict operations
    if tag == TAG_DICT {
        // dict.get (n==1)
        if n == 1 {
            if let Some(result) = try_dict_get_fast(rt, stack, args_start, recv, method_hash, &slot_idx) {
                stack.truncate(args_start - 1);
                stack.push(result);
                return Ok(None);
            }

            // dict.contains (n==1)
            if let Some(result) = try_dict_contains_fast(rt, stack, args_start, recv, method_hash) {
                stack.truncate(args_start - 1);
                stack.push(result);
                return Ok(None);
            }
        }

        // dict.insert / dict.insert_int (n==2)
        if n == 2 {
            // dict.insert_int with integer key
            if let Some(result) = try_dict_insert_int_fast(rt, stack, args_start, recv, method_hash) {
                stack.truncate(args_start - 1);
                stack.push(result);
                return Ok(None);
            }

            // dict.insert with string key
            if try_dict_insert_fast(rt, stack, args_start, recv, method_hash, &slot_idx) {
                stack.truncate(args_start - 1);
                stack.push(Value::UNIT);
                return Ok(None);
            }
        }
    }

    // IC check (Hot path for bytecode methods)
    let mut fast_res = None;
    if let Some(idx) = slot_idx {
        if idx < rt.caches.ic_method_slots.len() {
            let slot = &rt.caches.ic_method_slots[idx];
            if slot.tag == tag && slot.method_hash == method_hash {
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

    let result = if let Some(res) = fast_res {
        stack.truncate(args_start - 1);
        res
    } else {
        let method = rt.get_const_str(m_idx, &bc.constants);
        let res = rt.call_method_with_ic_raw(recv, method, method_hash, &stack[args_start..], slot_idx);
        stack.truncate(args_start - 1);
        res
    };
    handle_call_result(rt, stack, ip, handlers, iters, pending, thrown, result)
}

/// Try fast path for dict.get with string key
#[inline(always)]
fn try_dict_get_fast(
    rt: &mut Runtime,
    stack: &[Value],
    args_start: usize,
    recv: Value,
    method_hash: u64,
    slot_idx: &Option<usize>,
) -> Option<Value> {
    static GET_HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let get_hash = *GET_HASH.get_or_init(|| xu_ir::stable_hash64("get"));
    if method_hash != get_hash {
        return None;
    }

    let key_val = stack[args_start];
    if key_val.get_tag() != TAG_STR {
        return None;
    }

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
    if let Some(idx) = slot_idx {
        if *idx < rt.caches.ic_slots.len() {
            let c = &rt.caches.ic_slots[*idx];
            if c.id == dict_id.0 && c.key_len as usize == key_bytes.len() && key_bytes.len() <= 16 {
                // Fast compare short keys
                if &c.key_short[..key_bytes.len()] == key_bytes {
                    if let ManagedObject::Dict(me) = rt.heap.get(dict_id) {
                        if c.ver == me.ver {
                            return Some(c.option_some_cached);
                        }
                    }
                }
            }
        }
    }

    // SAFETY: key_ptr still valid
    let key_str = unsafe { std::str::from_utf8_unchecked(key_bytes) };
    if let ManagedObject::Dict(me) = rt.heap.get(dict_id) {
        let cur_ver = me.ver;
        let key_hash = Runtime::hash_bytes(me.map.hasher(), key_bytes);
        if let Some(v) = Runtime::dict_get_by_str_with_hash(me, key_str, key_hash) {
            // Create Option::some and cache it
            let opt = rt.option_some(v);
            if let Some(idx) = slot_idx {
                while rt.caches.ic_slots.len() <= *idx {
                    rt.caches.ic_slots.push(crate::ICSlot::default());
                }
                let mut key_short = [0u8; 16];
                let klen = key_bytes.len().min(16);
                key_short[..klen].copy_from_slice(&key_bytes[..klen]);
                rt.caches.ic_slots[*idx] = crate::ICSlot {
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
            return Some(opt);
        } else {
            return Some(rt.option_none());
        }
    }
    None
}

/// Try fast path for dict.contains with string key
#[inline(always)]
fn try_dict_contains_fast(
    rt: &mut Runtime,
    stack: &[Value],
    args_start: usize,
    recv: Value,
    method_hash: u64,
) -> Option<Value> {
    static CONTAINS_HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let contains_hash = *CONTAINS_HASH.get_or_init(|| xu_ir::stable_hash64("contains"));
    if method_hash != contains_hash {
        return None;
    }

    let key_val = stack[args_start];
    if key_val.get_tag() != TAG_STR {
        return None;
    }

    let dict_id = recv.as_obj_id();
    let key_id = key_val.as_obj_id();

    // Get key pointer/len without cloning
    let (key_ptr, key_len, dict_key_hash) = if let ManagedObject::Str(s) = rt.heap.get(key_id) {
        (s.as_str().as_ptr(), s.as_str().len(), DictKey::hash_str(s.as_str()))
    } else {
        return None;
    };

    // SAFETY: key_ptr is valid during this operation
    let key_bytes = unsafe { std::slice::from_raw_parts(key_ptr, key_len) };

    if let ManagedObject::Dict(me) = rt.heap.get(dict_id) {
        // Compute hash using DictKey hash (not string bytes)
        let key_hash = {
            use std::hash::{BuildHasher, Hasher};
            let mut h = me.map.hasher().build_hasher();
            h.write_u8(0); // String discriminant
            h.write_u64(dict_key_hash);
            h.finish()
        };

        // SAFETY: key_ptr still valid
        let key_str = unsafe { std::str::from_utf8_unchecked(key_bytes) };

        // Use raw_entry for efficient lookup - compare by hash
        let found = me
            .map
            .raw_entry_v1()
            .from_hash(key_hash, |k| k.eq_str(key_str, dict_key_hash, &rt.heap))
            .is_some();

        return Some(Value::from_bool(found));
    }
    None
}

/// Try fast path for dict.insert with string key
#[inline(always)]
fn try_dict_insert_fast(
    rt: &mut Runtime,
    stack: &[Value],
    args_start: usize,
    recv: Value,
    method_hash: u64,
    slot_idx: &Option<usize>,
) -> bool {
    static INSERT_HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let insert_hash = *INSERT_HASH.get_or_init(|| xu_ir::stable_hash64("insert"));
    if method_hash != insert_hash {
        return false;
    }

    let key_val = stack[args_start];
    let value = stack[args_start + 1];
    if key_val.get_tag() != TAG_STR {
        return false;
    }

    let dict_id = recv.as_obj_id();
    let key_id = key_val.as_obj_id();

    // Get key hash
    let dict_key_hash = if let ManagedObject::Str(s) = rt.heap.get(key_id) {
        DictKey::hash_str(s.as_str())
    } else {
        return false;
    };

    // IC optimization for insert
    let mut cached_hash = None;
    if let Some(idx) = slot_idx {
        if *idx < rt.caches.ic_slots.len() {
            let c = &rt.caches.ic_slots[*idx];
            if c.id == dict_id.0 && c.key_id == key_id.0 {
                // Cache hit: same dict and same key object (e.g. constant string)
                cached_hash = Some(c.key_hash);
            }
        }
    }

    // Write barrier for generational GC (before getting mutable reference)
    rt.write_barrier(dict_id);

    if let ManagedObject::Dict(me) = rt.heap.get_mut(dict_id) {
        let key_hash = if let Some(h) = cached_hash {
            h
        } else {
            // Compute HashMap hash from DictKey hash
            use std::hash::{BuildHasher, Hasher};
            let mut h = me.map.hasher().build_hasher();
            h.write_u8(0); // String discriminant
            h.write_u64(dict_key_hash);
            let hash = h.finish();
            // Update IC cache
            if let Some(idx) = slot_idx {
                while rt.caches.ic_slots.len() <= *idx {
                    rt.caches.ic_slots.push(crate::ICSlot::default());
                }
                rt.caches.ic_slots[*idx] = crate::ICSlot {
                    id: dict_id.0,
                    key_hash: hash,
                    key_id: key_id.0,
                    key_len: 0,
                    ver: 0,
                    value: Value::UNIT,
                    ..Default::default()
                };
            }
            hash
        };

        use indexmap::map::raw_entry_v1::RawEntryMut;
        match me.map.raw_entry_mut_v1().from_hash(key_hash, |kk| {
            // Compare by hash - if hash matches, it's the same key
            if let DictKey::StrRef { hash, obj_id } = kk {
                if *hash != dict_key_hash {
                    return false;
                }
                // Same ObjectId or same hash means same key
                *obj_id == key_id.0 || *hash == dict_key_hash
            } else {
                false
            }
        }) {
            RawEntryMut::Occupied(mut o) => {
                *o.get_mut() = value;
            }
            RawEntryMut::Vacant(vac) => {
                // Use ObjectId directly - no string copy!
                let key = DictKey::from_str_obj(key_id, dict_key_hash);
                vac.insert(key, value);
            }
        }
        me.ver += 1;
        return true;
    }
    false
}

/// Try fast path for dict.insert_int with integer key
#[inline(always)]
fn try_dict_insert_int_fast(
    rt: &mut Runtime,
    stack: &[Value],
    args_start: usize,
    recv: Value,
    method_hash: u64,
) -> Option<Value> {
    static INSERT_INT_HASH: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let insert_int_hash = *INSERT_INT_HASH.get_or_init(|| xu_ir::stable_hash64("insert_int"));
    if method_hash != insert_int_hash {
        return None;
    }

    let key_val = stack[args_start];
    let value = stack[args_start + 1];
    if !key_val.is_int() {
        return None;
    }

    let dict_id = recv.as_obj_id();
    let key_int = key_val.as_i64();

    // Fast path for small integers - use elements array
    if key_int >= 0 && key_int < 1024 {
        let idx = key_int as usize;
        // Write barrier for generational GC (before getting mutable reference)
    rt.write_barrier(dict_id);

    if let ManagedObject::Dict(me) = rt.heap.get_mut(dict_id) {
            let was_unit = me.set_element(idx, value);
            if was_unit {
                me.ver += 1;
            }
        }
        return Some(Value::UNIT);
    }

    // Slow path for large integers
    // Write barrier for generational GC (before getting mutable reference)
    rt.write_barrier(dict_id);

    if let ManagedObject::Dict(me) = rt.heap.get_mut(dict_id) {
        let key_hash = Runtime::hash_dict_key_int(me.map.hasher(), key_int);
        let key = DictKey::Int(key_int);

        use indexmap::map::raw_entry_v1::RawEntryMut;
        match me.map.raw_entry_mut_v1().from_hash(key_hash, |kk| kk == &key) {
            RawEntryMut::Occupied(mut o) => {
                *o.get_mut() = value;
            }
            RawEntryMut::Vacant(vac) => {
                vac.insert(key, value);
                me.ver += 1;
            }
        }
    }
    Some(Value::UNIT)
}

/// Execute Op::MakeFunction - create a function value
#[inline(always)]
pub(crate) fn op_make_function(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    f_idx: u32,
) -> Result<(), String> {
    let c = rt.get_constant(f_idx, &bc.constants);
    if let xu_ir::Constant::Func(func_bc) = c {
        let def = &func_bc.def;
        let bytecode = &func_bc.bytecode;
        let locals_count = func_bc.locals_count;

        // 创建独立的环境帧用于捕获，避免污染外部环境
        rt.env.push();

        if rt.locals.is_active() {
            let bindings = rt.locals.current_bindings();
            if !bindings.is_empty() {
                let env = &mut rt.env;
                for (name, value) in bindings {
                    // 在新帧中定义变量，不会覆盖外部同名变量
                    env.define(name, value);
                }
            }
        }
        if let Some(bindings) = rt.current_param_bindings.as_ref() {
            if !bindings.is_empty() {
                let mut captured: Vec<(String, Value)> = Vec::with_capacity(bindings.len());
                for (name, idx) in bindings {
                    if let Some(value) = rt.get_local_by_index(*idx) {
                        captured.push((name.clone(), value));
                    }
                }
                let env = &mut rt.env;
                for (name, value) in captured {
                    // 在新帧中定义变量，不会覆盖外部同名变量
                    env.define(name, value);
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

        // 弹出临时帧，恢复原环境
        rt.env.pop();

        let id = rt
            .heap
            .alloc(ManagedObject::Function(Function::Bytecode(std::rc::Rc::new(
                fun,
            ))));
        stack.push(Value::function(id));
    }
    Ok(())
}

/// Execute Op::CallStaticOrMethod - try static method first, fall back to instance method
/// Stack layout: [args...] (no receiver on stack - receiver is looked up by name)
#[inline(always)]
pub(crate) fn op_call_static_or_method(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    ip: &mut usize,
    handlers: &mut Vec<Handler>,
    iters: &mut Vec<IterState>,
    pending: &mut Option<Pending>,
    thrown: &mut Option<Value>,
    type_idx: u32,
    m_idx: u32,
    method_hash: u64,
    n: usize,
    slot_idx: Option<usize>,
) -> Result<Option<Flow>, String> {
    let type_name = rt.get_const_str(type_idx, &bc.constants);
    let method = rt.get_const_str(m_idx, &bc.constants);

    // First try static method: __static__{type_name}__{method}
    let static_name = format!("__static__{type_name}__{method}");
    if let Some(func) = rt.env.get(&static_name) {
        // Static method found - call it directly
        if stack.len() < n {
            return Err("Stack underflow in CallStaticOrMethod".to_string());
        }
        let args: smallvec::SmallVec<[Value; 8]> = stack.drain(stack.len() - n..).collect();
        let result = rt.call_function(func, &args);
        return handle_call_result(rt, stack, ip, handlers, iters, pending, thrown, result);
    }

    // Static method not found - try as instance method
    // First check locals, then env
    let recv = if rt.locals.is_active() {
        rt.get_local(type_name).or_else(|| rt.env.get(type_name))
    } else {
        rt.env.get(type_name)
    };

    if let Some(recv) = recv {
        if stack.len() < n {
            return Err("Stack underflow in CallStaticOrMethod".to_string());
        }
        let args_start = stack.len() - n;
        let res = rt.call_method_with_ic_raw(recv, method, method_hash, &stack[args_start..], slot_idx);
        stack.truncate(args_start);
        return handle_call_result(rt, stack, ip, handlers, iters, pending, thrown, res);
    }

    // Neither static method nor variable found - error
    let err = format!("Undefined identifier: {type_name}");
    if let Some(flow) = try_throw_error(rt, ip, handlers, stack, iters, pending, thrown, err) {
        return Ok(Some(flow));
    }
    Ok(None)
}

/// Execute Op::Return - return from function
#[inline(always)]
pub(crate) fn op_return(stack: &mut Vec<Value>) -> Result<Flow, String> {
    let v = pop_stack(stack)?;
    Ok(Flow::Return(v))
}
