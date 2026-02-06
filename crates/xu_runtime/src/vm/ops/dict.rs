use std::hash::{BuildHasher, Hash, Hasher};

use crate::core::Value;
use crate::core::heap::ManagedObject;
use crate::core::value::{DictKey, TAG_DICT, TAG_STR, TAG_UNIT, ELEMENTS_MAX};
use crate::errors::messages::{NOT_A_DICT, NOT_A_STRING};
use crate::vm::ops::helpers::pop_stack;

use crate::Runtime;

pub(crate) fn op_dict_insert(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let v = pop_stack(stack)?;
    let k = pop_stack(stack)?;
    let recv = pop_stack(stack)?;

    if recv.get_tag() != TAG_DICT {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "insert_int".to_string(),
            ty: recv.type_name().to_string(),
        }));
    }

    let id = recv.as_obj_id();

    // Fast path for small integer keys - use elements array
    if k.is_int() {
        let key_int = k.as_i64();
        if key_int >= 0 && key_int < ELEMENTS_MAX {
            let idx = key_int as usize;
            if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
                // Ensure elements array is large enough
                if d.elements.len() <= idx {
                    d.elements.resize(idx + 1, Value::UNIT);
                }
                // Check if this is a new key (was VOID before)
                let was_unit = d.elements[idx].get_tag() == TAG_UNIT;
                d.elements[idx] = v;
                if was_unit {
                    d.ver += 1;
                    rt.caches.dict_version_last = Some((id.0, d.ver));
                }
            }
            stack.push(recv);
            return Ok(());
        }
    }

    // Fast path for string keys - avoid creating DictKey for existing keys
    if k.get_tag() == TAG_STR {
        // Get key string and compute hash first
        let (key_ptr, key_len, hash, dict_key_hash) = {
            let key_str = if let ManagedObject::Str(s) = rt.heap.get(k.as_obj_id()) {
                s
            } else {
                return Err(NOT_A_STRING.into());
            };
            let d = if let ManagedObject::Dict(d) = rt.heap.get(id) {
                d
            } else {
                return Err(NOT_A_DICT.into());
            };
            // Compute hash for HashMap lookup
            let hash = {
                let mut h = d.map.hasher().build_hasher();
                h.write_u8(0); // String discriminant
                key_str.as_bytes().hash(&mut h);
                h.finish()
            };
            // Pre-compute DictKey hash (using ahash, different from HashMap hash)
            let dict_key_hash = DictKey::hash_str(key_str.as_str());
            // Store pointer and length for later use
            (key_str.as_str().as_ptr(), key_str.len(), hash, dict_key_hash)
        };

        let d = if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
            d
        } else {
            return Err(NOT_A_DICT.into());
        };

        // SAFETY: key_ptr/key_len are valid, we haven't modified the heap
        let key_str = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(key_ptr, key_len)) };

        match d.map.raw_entry_mut().from_hash(hash, |dk| dk.is_str() && dk.as_str() == key_str) {
            hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                *o.get_mut() = v;
            }
            hashbrown::hash_map::RawEntryMut::Vacant(vac) => {
                // Use pre-computed hash to avoid re-hashing
                let key = DictKey::from_str_with_hash(key_str, dict_key_hash);
                vac.insert(key, v);
                d.ver += 1;
                rt.caches.dict_version_last = Some((id.0, d.ver));
            }
        }
        stack.push(recv);
        return Ok(());
    }

    // Slow path for large integer keys
    if k.is_int() {
        let key = DictKey::Int(k.as_i64());
        if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
            let h = d.map.hasher().hash_one(&key);
            match d.map.raw_entry_mut().from_hash(h, |kk| kk == &key) {
                hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                    *o.get_mut() = v;
                }
                hashbrown::hash_map::RawEntryMut::Vacant(vac) => {
                    vac.insert(key, v);
                    d.ver += 1;
                    rt.caches.dict_version_last = Some((id.0, d.ver));
                }
            }
        }
        stack.push(recv);
        return Ok(());
    }

    Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired))
}

pub(crate) fn op_dict_merge(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let other = pop_stack(stack)?;
    let recv = pop_stack(stack)?;

    if recv.get_tag() != TAG_DICT {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "merge".to_string(),
            ty: recv.type_name().to_string(),
        }));
    }

    if other.get_tag() != TAG_DICT {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "merge".to_string(),
            ty: recv.type_name().to_string(),
        }));
    }

    let aid = recv.as_obj_id();
    let bid = other.as_obj_id();
    if aid == bid {
        stack.push(recv);
        return Ok(());
    }

    let other_dict_map = if let ManagedObject::Dict(b) = rt.heap.get(bid) {
        b.map.clone()
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())));
    };

    if let ManagedObject::Dict(a) = rt.heap.get_mut(aid) {
        a.map.reserve(other_dict_map.len());
        let mut changed = false;
        for (k, v) in other_dict_map {
            match a.map.entry(k) {
                hashbrown::hash_map::Entry::Vacant(e) => {
                    e.insert(v);
                    changed = true;
                }
                hashbrown::hash_map::Entry::Occupied(mut e) => {
                    *e.get_mut() = v;
                }
            }
        }
        if changed {
            a.ver += 1;
            rt.caches.dict_version_last = Some((aid.0, a.ver));
        }
    }
    stack.push(recv);
    Ok(())
}
