//! Method call with inline caching for Runtime.

use std::hash::{BuildHasher, Hash, Hasher};

use hashbrown::hash_map::RawEntryMut;
use smallvec::SmallVec;

use crate::core::Value;
use crate::methods;

use super::cache::MethodICSlot;
use super::core::Runtime;

pub(crate) use crate::methods::MethodKind;

impl Runtime {
    pub(crate) fn call_method_with_ic_raw(
        &mut self,
        recv: Value,
        method: &str,
        method_hash: u64,
        args: &[Value],
        slot_idx: Option<usize>,
    ) -> Result<Value, String> {
        let tag = recv.get_tag();

        // IC check
        if let Some(idx) = slot_idx {
            if idx < self.caches.ic_method_slots.len() {
                let slot = &self.caches.ic_method_slots[idx];
                if slot.tag == tag && slot.method_hash == method_hash {
                    if tag == crate::core::value::TAG_STRUCT {
                        let id = recv.as_obj_id();
                        if let crate::core::heap::ManagedObject::Struct(s) = self.heap.get(id) {
                            if slot.struct_ty_hash == s.ty_hash {
                                if let Some(f) = slot.cached_bytecode.as_ref() {
                                    if args.is_empty() {
                                        return self.call_bytecode_function(f.clone(), &[recv]);
                                    }
                                    if args.len() == 1 {
                                        let all = [recv, args[0]];
                                        return self.call_bytecode_function(f.clone(), &all);
                                    }
                                }
                                let mut all_args: SmallVec<[Value; 4]> =
                                    SmallVec::with_capacity(args.len() + 1);
                                all_args.push(recv);
                                all_args.extend(args.iter().cloned());
                                if let Some(f) = slot.cached_user.as_ref() {
                                    return self.call_user_function(f.clone(), &all_args);
                                }
                                return self.call_function(slot.cached_func, &all_args);
                            }
                        }
                    } else if tag == crate::core::value::TAG_ENUM {
                        let id = recv.as_obj_id();
                        if let crate::core::heap::ManagedObject::Enum(e) =
                            self.heap.get(id)
                        {
                            let (ty, _variant, _payload) = e.as_ref();
                            let ty_hash = xu_ir::stable_hash64(ty.as_str());
                            if slot.struct_ty_hash == ty_hash {
                                if let Some(f) = slot.cached_bytecode.as_ref() {
                                    if args.is_empty() {
                                        return self.call_bytecode_function(f.clone(), &[recv]);
                                    }
                                    if args.len() == 1 {
                                        let all = [recv, args[0]];
                                        return self.call_bytecode_function(f.clone(), &all);
                                    }
                                }
                                let mut all_args: SmallVec<[Value; 4]> =
                                    SmallVec::with_capacity(args.len() + 1);
                                all_args.push(recv);
                                all_args.extend(args.iter().cloned());
                                if let Some(f) = slot.cached_user.as_ref() {
                                    return self.call_user_function(f.clone(), &all_args);
                                }
                                return self.call_function(slot.cached_func, &all_args);
                            }
                        }
                    } else if slot.kind != MethodKind::Unknown {
                        return methods::dispatch_builtin_method(
                            self, recv, slot.kind, args, method,
                        );
                    }
                }
            }
        }

        if tag == crate::core::value::TAG_MODULE {
            let id = recv.as_obj_id();
            let callee = if let crate::core::heap::ManagedObject::Module(m) = self.heap.get(id) {
                m.exports.map.get(method).cloned().ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(method.to_string()))
                })?
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-module object".into())));
            };
            if callee.get_tag() != crate::core::value::TAG_FUNC {
                return Err(self.error(xu_syntax::DiagnosticKind::NotCallable(method.to_string())));
            }
            // Update IC
            if let Some(idx) = slot_idx {
                while self.caches.ic_method_slots.len() <= idx {
                    self.caches.ic_method_slots.push(MethodICSlot::default());
                }
                self.caches.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: 0,
                    kind: MethodKind::Unknown,
                    cached_func: callee,
                    cached_user: if let crate::core::heap::ManagedObject::Function(
                        crate::core::value::Function::User(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                    cached_bytecode: if let crate::core::heap::ManagedObject::Function(
                        crate::core::value::Function::Bytecode(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                };
            }
            self.call_function(callee, args)
        } else if tag == crate::core::value::TAG_STRUCT {
            let id = recv.as_obj_id();
            let callee = match if let crate::core::heap::ManagedObject::Struct(s) = self.heap.get(id) {
                let ty = s.ty.as_str();
                let hash = {
                    let mut h = self.caches.method_cache.hasher().build_hasher();
                    ty.hash(&mut h);
                    method.hash(&mut h);
                    h.finish()
                };
                match self
                    .caches.method_cache
                    .raw_entry_mut()
                    .from_hash(hash, |(t, m)| t == ty && m == method)
                {
                    RawEntryMut::Occupied(o) => {
                        Ok(o.get().clone())
                    }
                    RawEntryMut::Vacant(vac) => {
                        let name = format!("__method__{}__{}", ty, method);
                        if let Some(v) = self.env.get_cached(&name) {
                            vac.insert((s.ty.clone(), method.to_string()), v.clone());
                            Ok(v)
                        } else {
                            // Search in loaded modules for cross-module method calls
                            let mut found = None;
                            for (_, module_val) in self.loaded_modules.iter() {
                                if module_val.get_tag() == crate::core::value::TAG_MODULE {
                                    if let crate::core::heap::ManagedObject::Module(m) =
                                        self.heap.get(module_val.as_obj_id())
                                    {
                                        if let Some(v) = m.exports.map.get(&name) {
                                            found = Some(v.clone());
                                            break;
                                        }
                                    }
                                }
                            }
                            if let Some(v) = found {
                                vac.insert((s.ty.clone(), method.to_string()), v.clone());
                                Ok(v)
                            } else {
                                Err(xu_syntax::DiagnosticKind::UnknownMember(method.to_string()))
                            }
                        }
                    }
                }
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-struct object".into())));
            } {
                Ok(v) => v,
                Err(kind) => return Err(self.error(kind)),
            };

            // Update IC
            if let Some(idx) = slot_idx {
                while self.caches.ic_method_slots.len() <= idx {
                    self.caches.ic_method_slots.push(MethodICSlot::default());
                }
                self.caches.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: if let crate::core::heap::ManagedObject::Struct(s) = self.heap.get(id) {
                        s.ty_hash
                    } else {
                        0
                    },
                    kind: MethodKind::Unknown,
                    cached_func: callee,
                    cached_user: if let crate::core::heap::ManagedObject::Function(
                        crate::core::value::Function::User(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                    cached_bytecode: if let crate::core::heap::ManagedObject::Function(
                        crate::core::value::Function::Bytecode(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                };
            }

            let mut all_args: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len() + 1);
            all_args.push(recv);
            all_args.extend(args.iter().cloned());
            self.call_function(callee, &all_args)
        } else if tag == crate::core::value::TAG_ENUM {
            let id = recv.as_obj_id();
            let (callee, ty_hash) =
                match if let crate::core::heap::ManagedObject::Enum(e) =
                    self.heap.get(id)
                {
                    let (ty, _variant, _payload) = e.as_ref();
                    let ty_str = ty.as_str();
                    let hash = {
                        let mut h = self.caches.method_cache.hasher().build_hasher();
                        ty_str.hash(&mut h);
                        method.hash(&mut h);
                        h.finish()
                    };
                    match self
                        .caches.method_cache
                        .raw_entry_mut()
                        .from_hash(hash, |(t, m)| t == ty_str && m == method)
                    {
                        RawEntryMut::Occupied(o) => Ok((o.get().clone(), xu_ir::stable_hash64(ty_str))),
                        RawEntryMut::Vacant(vac) => {
                            let name = format!("__method__{}__{}", ty_str, method);
                            if let Some(v) = self.env.get_cached(&name) {
                                vac.insert((ty.to_string(), method.to_string()), v.clone());
                                Ok((v, xu_ir::stable_hash64(ty_str)))
                            } else {
                                // Search in loaded modules for cross-module method calls
                                let mut found = None;
                                for (_, module_val) in self.loaded_modules.iter() {
                                    if module_val.get_tag() == crate::core::value::TAG_MODULE {
                                        if let crate::core::heap::ManagedObject::Module(m) =
                                            self.heap.get(module_val.as_obj_id())
                                        {
                                            if let Some(v) = m.exports.map.get(&name) {
                                                found = Some(v.clone());
                                                break;
                                            }
                                        }
                                    }
                                }
                                if let Some(v) = found {
                                    vac.insert((ty.to_string(), method.to_string()), v.clone());
                                    Ok((v, xu_ir::stable_hash64(ty_str)))
                                } else {
                                    Err(())
                                }
                            }
                        }
                    }
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Non-enum object".into())));
                } {
                    Ok(v) => v,
                    Err(()) => {
                        let kind = MethodKind::from_str(method);
                        if kind == MethodKind::Unknown {
                            return Err(self.error(xu_syntax::DiagnosticKind::UnknownMember(
                                method.to_string(),
                            )));
                        }
                        if let Some(idx) = slot_idx {
                            while self.caches.ic_method_slots.len() <= idx {
                                self.caches.ic_method_slots.push(MethodICSlot::default());
                            }
                            self.caches.ic_method_slots[idx] = MethodICSlot {
                                tag,
                                method_hash,
                                struct_ty_hash: 0,
                                kind,
                                cached_func: Value::UNIT,
                                cached_user: None,
                                cached_bytecode: None,
                            };
                        }
                        return methods::dispatch_builtin_method(self, recv, kind, args, method);
                    }
                };

            if let Some(idx) = slot_idx {
                while self.caches.ic_method_slots.len() <= idx {
                    self.caches.ic_method_slots.push(MethodICSlot::default());
                }
                self.caches.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: ty_hash,
                    kind: MethodKind::Unknown,
                    cached_func: callee,
                    cached_user: if let crate::core::heap::ManagedObject::Function(crate::core::value::Function::User(
                        f,
                    )) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                    cached_bytecode: if let crate::core::heap::ManagedObject::Function(
                        crate::core::value::Function::Bytecode(f),
                    ) = self.heap.get(callee.as_obj_id())
                    {
                        Some(f.clone())
                    } else {
                        None
                    },
                };
            }

            if callee.get_tag() != crate::core::value::TAG_FUNC {
                return Err(self.error(xu_syntax::DiagnosticKind::NotCallable(method.to_string())));
            }
            let mut all_args: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len() + 1);
            all_args.push(recv);
            all_args.extend(args.iter().cloned());
            self.call_function(callee, &all_args)
        } else {
            let kind = MethodKind::from_str(method);
            if kind == MethodKind::Unknown {
                return Err(self.error(xu_syntax::DiagnosticKind::UnsupportedReceiver(
                    recv.type_name().to_string(),
                )));
            }

            // Update IC
            if let Some(idx) = slot_idx {
                while self.caches.ic_method_slots.len() <= idx {
                    self.caches.ic_method_slots.push(MethodICSlot::default());
                }
                self.caches.ic_method_slots[idx] = MethodICSlot {
                    tag,
                    method_hash,
                    struct_ty_hash: 0,
                    kind,
                    cached_func: Value::UNIT,
                    cached_user: None,
                    cached_bytecode: None,
                };
            }

            methods::dispatch_builtin_method(self, recv, kind, args, method)
        }
    }
}
