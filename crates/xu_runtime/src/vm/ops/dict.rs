use std::hash::{BuildHasher, Hash, Hasher};

use crate::core::Value;
use crate::core::heap::ManagedObject;
use crate::core::value::{DictKey, TAG_DICT, TAG_STR, TAG_VOID};

use crate::Runtime;

/// Maximum integer key to store in elements array (0-1023)
const ELEMENTS_MAX: i64 = 1024;

pub(crate) fn op_dict_insert(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let k = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let recv = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;

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
                    d.elements.resize(idx + 1, Value::VOID);
                }
                // Check if this is a new key (was VOID before)
                let was_void = d.elements[idx].get_tag() == TAG_VOID;
                d.elements[idx] = v;
                if was_void {
                    d.ver += 1;
                    rt.dict_version_last = Some((id.0, d.ver));
                }
            }
            stack.push(recv);
            return Ok(());
        }
    }

    // Slow path for string keys and large integer keys
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

    if let ManagedObject::Dict(d) = rt.heap.get_mut(id) {
        let mut hasher = d.map.hasher().build_hasher();
        key.hash(&mut hasher);
        let h = hasher.finish();
        match d.map.raw_entry_mut().from_hash(h, |k| k == &key) {
            hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                *o.get_mut() = v.clone();
            }
            hashbrown::hash_map::RawEntryMut::Vacant(vac) => {
                vac.insert(key, v.clone());
                d.ver += 1;
                rt.dict_version_last = Some((id.0, d.ver));
            }
        }
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
    }
    stack.push(recv);
    Ok(())
}

pub(crate) fn op_dict_merge(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let other = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let recv = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;

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
        return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
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
            rt.dict_version_last = Some((aid.0, a.ver));
        }
    }
    stack.push(recv);
    Ok(())
}
