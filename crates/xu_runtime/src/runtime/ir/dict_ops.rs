use std::hash::{BuildHasher, Hash, Hasher};

use crate::Value;
use crate::gc::ManagedObject;
use crate::value::{DictKey, TAG_DICT, TAG_STR};

use super::Runtime;

pub(super) fn op_dict_insert(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let v = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let k = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let dict = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;

    if dict.get_tag() == TAG_DICT {
        let id = dict.as_obj_id();
        let key = if k.get_tag() == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(k.as_obj_id()) {
                DictKey::Str(s.clone())
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
            let mut changed = false;
            match d.map.raw_entry_mut().from_hash(h, |k| k == &key) {
                hashbrown::hash_map::RawEntryMut::Occupied(mut o) => {
                    let prev = o.get().clone();
                    *o.get_mut() = v.clone();
                    if prev != v {
                        changed = true;
                    }
                }
                hashbrown::hash_map::RawEntryMut::Vacant(vac) => {
                    vac.insert(key, v.clone());
                    changed = true;
                }
            }
            if changed {
                d.ver += 1;
                rt.dict_version_last = Some((id.0, d.ver));
            }
        } else {
            return Err(rt.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
        }
        stack.push(dict);
        Ok(())
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "insert".to_string(),
            ty: dict.type_name().to_string(),
        }))
    }
}

pub(super) fn op_dict_merge(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let other = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    let dict = stack.pop().ok_or_else(|| "Stack underflow".to_string())?;
    if dict.get_tag() == TAG_DICT && other.get_tag() == TAG_DICT {
        let aid = dict.as_obj_id();
        let bid = other.as_obj_id();
        if aid == bid {
            stack.push(dict);
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
                        let prev = e.get().clone();
                        if prev != v {
                            *e.get_mut() = v;
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                a.ver += 1;
                rt.dict_version_last = Some((aid.0, a.ver));
            }
        }
        stack.push(dict);
        Ok(())
    } else {
        Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "merge".to_string(),
            ty: dict.type_name().to_string(),
        }))
    }
}
