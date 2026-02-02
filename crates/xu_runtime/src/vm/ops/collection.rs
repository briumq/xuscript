//! Collection operations for the VM.
//!
//! This module contains operations for:
//! - ListNew: Create a new list
//! - TupleNew: Create a new tuple
//! - DictNew: Create a new dictionary
//! - ListAppend: Append items to a list
//! - DictInsertStrConst: Insert with string constant key

use smallvec::SmallVec;
use xu_ir::Bytecode;

use crate::core::heap::ManagedObject;
use crate::core::value::{DictKey, TAG_DICT, TAG_LIST, TAG_STR};
use crate::core::Value;
use crate::Runtime;

/// Execute Op::ListNew - create a new list
#[inline(always)]
pub(crate) fn op_list_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    let mut items: Vec<Value> = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
    }
    items.reverse();
    let id = rt.heap.alloc(ManagedObject::List(items));
    stack.push(Value::list(id));
    Ok(())
}

/// Execute Op::TupleNew - create a new tuple
#[inline(always)]
pub(crate) fn op_tuple_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<bool, String> {
    if n == 0 {
        stack.push(Value::VOID);
        return Ok(true); // Signal to continue (skip ip increment)
    }
    let mut items: Vec<Value> = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
    }
    items.reverse();
    let id = rt.heap.alloc(ManagedObject::Tuple(items));
    stack.push(Value::tuple(id));
    Ok(false)
}

/// Execute Op::DictNew - create a new dictionary
#[inline(always)]
pub(crate) fn op_dict_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    let mut map = crate::core::value::dict_with_capacity(n);
    for _ in 0..n {
        let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
        let k = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
        let key = if k.get_tag() == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(k.as_obj_id()) {
                DictKey::from_text(s)
            } else {
                return Err("Not a string".into());
            }
        } else if k.is_int() {
            DictKey::Int(k.as_i64())
        } else {
            return Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired));
        };
        map.map.insert(key, v);
    }
    let id = rt.heap.alloc(ManagedObject::Dict(map));
    stack.push(Value::dict(id));
    Ok(())
}

/// Execute Op::ListAppend - append items to a list
#[inline(always)]
pub(crate) fn op_list_append(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    n: usize,
) -> Result<(), String> {
    let mut items: SmallVec<[Value; 8]> = SmallVec::with_capacity(n);
    for _ in 0..n {
        items.push(stack.pop().ok_or_else(|| "Stack underflow".to_string())?);
    }
    items.reverse();
    let list = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if list.get_tag() != TAG_LIST {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "add".to_string(),
            ty: list.type_name().to_string(),
        }));
    }
    let id = list.as_obj_id();
    if let ManagedObject::List(vs) = rt.heap.get_mut(id) {
        vs.reserve(items.len());
        for v in items {
            vs.push(v);
        }
    }
    stack.push(list);
    Ok(())
}

/// Execute Op::DictInsertStrConst - insert with string constant key
#[inline(always)]
pub(crate) fn op_dict_insert_str_const(
    rt: &mut Runtime,
    bc: &Bytecode,
    stack: &mut Vec<Value>,
    idx: u32,
    _k_hash: u64,
    slot: Option<usize>,
) -> Result<(), String> {
    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let dict = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if dict.get_tag() != TAG_DICT {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "insert".to_string(),
            ty: dict.type_name().to_string(),
        }));
    }
    let id = dict.as_obj_id();

    // Get the key string first (before any mutable borrows)
    let k = rt.get_const_str(idx, &bc.constants);
    let k_bytes = k.as_bytes();
    let k_len = k_bytes.len();

    // Try IC cache first - fast path for short keys (<=16 bytes)
    let mut cache_hit = false;
    if let Some(idx_slot) = slot {
        if idx_slot < rt.ic_slots.len() {
            let c = &rt.ic_slots[idx_slot];
            // For short keys, compare directly; for long keys, compare hash
            let key_match = if k_len <= 16 {
                c.key_len == k_len as u8 && c.key_short[..k_len] == k_bytes[..]
            } else {
                c.key_len == k_len as u8 && c.key_hash != 0
            };
            if c.id == id.0 && key_match {
                let cached_hash = c.key_hash;
                if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
                    match d.map.raw_entry_mut().from_hash(cached_hash, |key| {
                        key.is_str() && key.as_str() == k
                    }) {
                        hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                            // 值更新 - 不增加版本号
                            *o.get_mut() = v;
                            cache_hit = true;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    if !cache_hit {
        // Slow path - compute hash and insert
        if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
            let internal_hash = Runtime::hash_bytes(d.map.hasher(), k.as_bytes());
            // Avoid creating DictKey for comparison - use closure with str comparison
            let mut is_new_key = false;
            match d.map.raw_entry_mut().from_hash(internal_hash, |key| {
                key.is_str() && key.as_str() == k
            }) {
                hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                    // 值更新 - 不增加版本号
                    *o.get_mut() = v;
                }
                hashbrown::hash_map::RawEntryMut::Vacant(vac) => {
                    // 新 key - 增加版本号
                    let key = DictKey::from_str(k);
                    vac.insert(key, v);
                    is_new_key = true;
                }
            }
            if is_new_key {
                d.ver += 1;
                rt.dict_version_last = Some((id.0, d.ver));
            }

            // Update IC cache with key info for fast comparison
            if let Some(idx_slot) = slot {
                while rt.ic_slots.len() <= idx_slot {
                    rt.ic_slots.push(crate::ICSlot::default());
                }
                let mut key_short = [0u8; 16];
                let key_bytes = k.as_bytes();
                let copy_len = key_bytes.len().min(16);
                key_short[..copy_len].copy_from_slice(&key_bytes[..copy_len]);
                rt.ic_slots[idx_slot] = crate::ICSlot {
                    id: id.0,
                    key_hash: internal_hash,
                    key_short,
                    key_len: key_bytes.len() as u8,
                    ver: d.ver,
                    value: Value::VOID,
                    ..Default::default()
                };
            }
        }
    }
    stack.push(dict);
    Ok(())
}
