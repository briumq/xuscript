use crate::errors::messages::{NOT_A_DICT, NOT_A_LIST, NOT_A_STRING};
use crate::Value;
use crate::core::value::DictKey;

use crate::Runtime;

impl Runtime {
    pub(crate) fn get_member_with_ic(
        &mut self,
        obj: Value,
        field: &str,
        slot_cell: &std::cell::Cell<Option<usize>>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::core::value::TAG_DICT {
            // length 同时支持属性和方法，需要走通用路径
            if field == "length" {
                return self.get_member(obj, field);
            }

            let id = obj.as_obj_id();
            let (cur_ver, key_hash) = if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id)
            {
                (me.ver, Self::hash_bytes(me.map.hasher(), field.as_bytes()))
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())));
            };
            self.caches.dict_version_last = Some((id.0, cur_ver));

            if let Some(idx) = slot_cell.get() {
                if idx < self.caches.ic_slots.len() {
                    let c = &self.caches.ic_slots[idx];
                    if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                        return Ok(c.value);
                    }
                }
            }

            let v = if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id) {
                Self::dict_get_by_str_with_hash(me, field, key_hash)
            } else {
                None
            }
            .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string())))?;

            let idx = if let Some(i0) = slot_cell.get() {
                if i0 < self.caches.ic_slots.len() {
                    i0
                } else {
                    let ix = self.caches.ic_slots.len();
                    self.caches.ic_slots.push(crate::ICSlot::default());
                    slot_cell.set(Some(ix));
                    ix
                }
            } else {
                let ix = self.caches.ic_slots.len();
                self.caches.ic_slots.push(crate::runtime::ICSlot::default());
                slot_cell.set(Some(ix));
                ix
            };
            self.caches.ic_slots[idx] = crate::runtime::ICSlot {
                id: id.0,
                key_hash,
                ver: cur_ver,
                value: v,
                ..Default::default()
            };

            Ok(v)
        } else {
            self.get_member(obj, field)
        }
    }

    pub(crate) fn get_member(&mut self, obj: Value, field: &str) -> Result<Value, String> {
        self.get_member_with_ic_raw(obj, field, None)
    }

    pub(crate) fn get_member_with_ic_raw(
        &mut self,
        obj: Value,
        field: &str,
        slot_idx: Option<usize>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::core::value::TAG_STRUCT {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Struct(s) = self.heap.get(id) {
                if let Some(idx) = slot_idx {
                    if idx < self.caches.ic_slots.len() {
                        let c = &self.caches.ic_slots[idx];
                        if c.struct_ty_hash == s.ty_hash && c.key_hash == xu_ir::stable_hash64(field)
                        {
                            if let Some(offset) = c.field_offset {
                                return Ok(s.fields[offset]);
                            }
                        }
                    }
                }

                let layout = self.types.struct_layouts.get(&s.ty).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                let pos = layout.iter().position(|f| f == field).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                })?;

                if let Some(idx) = slot_idx {
                    while self.caches.ic_slots.len() <= idx {
                        self.caches.ic_slots.push(crate::ICSlot::default());
                    }
                    self.caches.ic_slots[idx] = crate::ICSlot {
                        struct_ty_hash: s.ty_hash,
                        key_hash: xu_ir::stable_hash64(field),
                        field_offset: Some(pos),
                        ..Default::default()
                    };
                }

                Ok(s.fields[pos])
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a struct".into())))
            }
        } else if tag == crate::core::value::TAG_ENUM && (field == "name" || field == "type_name") {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Enum(e) = self.heap.get(id) {
                let (ty, variant, _) = e.as_ref();
                let s = if field == "name" {
                    variant.clone()
                } else {
                    ty.clone()
                };
                Ok(Value::str(self.alloc(crate::core::heap::ManagedObject::Str(s))))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not an enum".into())))
            }
        } else if tag == crate::core::value::TAG_ENUM && (field == "has" || field == "none") {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Enum(e) = self.heap.get(id) {
                let (ty, variant, _) = e.as_ref();
                if ty.as_str() != "Option" {
                    return Err(self.error(xu_syntax::DiagnosticKind::UnknownMember(
                        field.to_string(),
                    )));
                }
                let b = if field == "has" {
                    variant.as_str() == "some"
                } else {
                    variant.as_str() == "none"
                };
                Ok(Value::from_bool(b))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not an enum".into())))
            }
        } else if tag == crate::core::value::TAG_OPTION && (field == "has" || field == "name" || field == "type_name") {
            // Option#some has TAG_OPTION, supports .has, .name, .type_name
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::OptionSome(_inner) = self.heap.get(id) {
                match field {
                    "has" => Ok(Value::from_bool(true)),
                    "name" => Ok(Value::str(self.alloc(crate::core::heap::ManagedObject::Str("some".into())))),
                    "type_name" => Ok(Value::str(self.alloc(crate::core::heap::ManagedObject::Str("Option".into())))),
                    _ => unreachable!(),
                }
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not an Option".into())))
            }
        } else if tag == crate::core::value::TAG_LIST && field == "length" {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::List(v) = self.heap.get(id) {
                Ok(Value::from_i64(v.len() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_LIST.into())))
            }
        } else if tag == crate::core::value::TAG_STR && field == "length" {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(id) {
                Ok(Value::from_i64(s.as_str().chars().count() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_STRING.into())))
            }
        } else if tag == crate::core::value::TAG_DICT && field == "length" {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Dict(v) = self.heap.get(id) {
                Ok(Value::from_i64(v.len() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())))
            }
        } else if tag == crate::core::value::TAG_TUPLE && field == "length" {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Tuple(v) = self.heap.get(id) {
                Ok(Value::from_i64(v.len() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a tuple".into())))
            }
        } else if tag == crate::core::value::TAG_TUPLE {
            let idx = field.parse::<usize>().ok();
            if let Some(i) = idx {
                let id = obj.as_obj_id();
                if let crate::core::heap::ManagedObject::Tuple(v) = self.heap.get(id) {
                    v.get(i)
                        .cloned()
                        .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::IndexOutOfRange))
                } else {
                    Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a tuple".into())))
                }
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::UnknownMember(
                    field.to_string(),
                )))
            }
        } else if tag == crate::core::value::TAG_DICT {
            // Generic dict field access - treat field as a string key
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id) {
                let key_hash = Self::hash_bytes(me.map.hasher(), field.as_bytes());
                Self::dict_get_by_str_with_hash(me, field, key_hash)
                    .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::KeyNotFound(field.to_string())))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())))
            }
        } else if tag == crate::core::value::TAG_MODULE {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Module(m) = self.heap.get(id) {
                if let Some(v) = m.exports.map.get(field) {
                    Ok(*v)
                } else {
                    Err(self.error(xu_syntax::DiagnosticKind::UnknownMember(
                        field.to_string(),
                    )))
                }
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a module".into())))
            }
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::InvalidMemberAccess {
                field: field.to_string(),
                ty: obj.type_name().to_string(),
            }))
        }
    }

    pub(crate) fn get_index_with_ic(
        &mut self,
        obj: Value,
        index: Value,
        slot_cell: &std::cell::Cell<Option<usize>>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::core::value::TAG_DICT {
            let id = obj.as_obj_id();
            let (cur_ver, key_hash, key) = if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id)
            {
                if index.get_tag() == crate::core::value::TAG_STR {
                    if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(index.as_obj_id()) {
                        let hash = Self::hash_bytes(me.map.hasher(), s.as_str().as_bytes());
                        (me.ver, hash, s.as_str().to_string())
                    } else {
                        return Err(self.error(xu_syntax::DiagnosticKind::GetKeyRequired));
                    }
                } else if index.is_int() {
                    let hash = Self::hash_dict_key_int(me.map.hasher(), index.as_i64());
                    (me.ver, hash, index.as_i64().to_string())
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::GetKeyRequired));
                }
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())));
            };
            self.caches.dict_version_last = Some((id.0, cur_ver));

            if let Some(idx) = slot_cell.get() {
                if idx < self.caches.ic_slots.len() {
                    let c = &self.caches.ic_slots[idx];
                    if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                        return Ok(c.value);
                    }
                }
            }

            let v = self.get_index_with_ic_raw(obj, index, None).map_err(|_| {
                self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string()))
            })?;

            let idx = if let Some(i0) = slot_cell.get() {
                if i0 < self.caches.ic_slots.len() {
                    i0
                } else {
                    let ix = self.caches.ic_slots.len();
                    self.caches.ic_slots.push(crate::ICSlot::default());
                    slot_cell.set(Some(ix));
                    ix
                }
            } else {
                let ix = self.caches.ic_slots.len();
                self.caches.ic_slots.push(crate::runtime::ICSlot::default());
                slot_cell.set(Some(ix));
                ix
            };
            self.caches.ic_slots[idx] = crate::runtime::ICSlot {
                id: id.0,
                key_hash,
                ver: cur_ver,
                value: v,
                ..Default::default()
            };
            Ok(v)
        } else {
            self.get_index_with_ic_raw(obj, index, slot_cell.get())
        }
    }

    pub(crate) fn get_index_with_ic_raw(
        &mut self,
        obj: Value,
        index: Value,
        slot_idx: Option<usize>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::core::value::TAG_LIST {
            let i = super::super::util::to_i64(&index)?;
            if i < 0 {
                return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
            }
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::List(list) = self.heap.get(id) {
                let ui = i as usize;
                if ui >= list.len() {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                Ok(list[ui])
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_LIST.into())))
            }
        } else if tag == crate::core::value::TAG_DICT {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id) {
                if index.get_tag() == crate::core::value::TAG_STR {
                    let key = if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(index.as_obj_id())
                    {
                        s.as_str().to_string()
                    } else {
                        return Err(self.error(xu_syntax::DiagnosticKind::GetKeyRequired));
                    };
                    let key_hash = Self::hash_bytes(me.map.hasher(), key.as_bytes());
                    if let Some(idx) = slot_idx {
                        if idx < self.caches.ic_slots.len() {
                            let c = &self.caches.ic_slots[idx];
                            if c.id == id.0 && c.ver == me.ver && c.key_hash == key_hash {
                                return Ok(c.value);
                            }
                        }
                    }
                    let out_val = Self::dict_get_by_str_with_hash(me, &key, key_hash)
                        .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.clone())))?;
                    if let Some(idx) = slot_idx {
                        while self.caches.ic_slots.len() <= idx {
                            self.caches.ic_slots.push(crate::ICSlot::default());
                        }
                        self.caches.ic_slots[idx] = crate::ICSlot {
                            id: id.0,
                            key_hash,
                            ver: me.ver,
                            value: out_val,
                            ..Default::default()
                        };
                    }
                    Ok(out_val)
                } else if index.is_int() {
                    let key = index.as_i64();
                    let key_hash = Self::hash_dict_key_int(me.map.hasher(), key);
                    if let Some(idx) = slot_idx {
                        if idx < self.caches.ic_slots.len() {
                            let c = &self.caches.ic_slots[idx];
                            if c.id == id.0 && c.ver == me.ver && c.key_hash == key_hash {
                                return Ok(c.value);
                            }
                        }
                    }
                    let out_val = me
                        .map
                        .get(&DictKey::Int(key))
                        .cloned()
                        .ok_or_else(|| {
                            self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string()))
                        })?;
                    if let Some(idx) = slot_idx {
                        while self.caches.ic_slots.len() <= idx {
                            self.caches.ic_slots.push(crate::ICSlot::default());
                        }
                        self.caches.ic_slots[idx] = crate::ICSlot {
                            id: id.0,
                            key_hash,
                            ver: me.ver,
                            value: out_val,
                            ..Default::default()
                        };
                    }
                    Ok(out_val)
                } else {
                    Err(self.error(xu_syntax::DiagnosticKind::GetKeyRequired))
                }
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_DICT.into())))
            }
        } else if tag == crate::core::value::TAG_STR {
            if index.is_int() {
                let i = super::super::util::to_i64(&index)?;
                if i < 0 {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let id = obj.as_obj_id();
                let s = if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(id) {
                    s.as_str().to_string()
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_STRING.into())));
                };
                let ui = i as usize;
                let total = s.chars().count();
                if ui >= total {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let ch: String = s.chars().skip(ui).take(1).collect();
                Ok(Value::str(self.alloc(crate::core::heap::ManagedObject::Str(
                    crate::Text::from_string(ch),
                ))))
            } else if index.get_tag() == crate::core::value::TAG_RANGE {
                let id = index.as_obj_id();
                let (start, end, inclusive) = if let crate::core::heap::ManagedObject::Range(a, b, inc) =
                    self.heap.get(id)
                {
                    (*a, *b, *inc)
                } else {
                    return Err("Not a range".to_string());
                };

                if start < 0 || end < 0 || end < start {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let start = start as usize;
                let end = end as usize;
                let sid = obj.as_obj_id();
                let s = if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(sid) {
                    s.as_str().to_string()
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw(NOT_A_STRING.into())));
                };
                let total = s.chars().count();
                let len = if inclusive {
                    if end >= total {
                        return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                    }
                    end - start + 1
                } else {
                    if end > total {
                        return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                    }
                    end - start
                };
                let sub: String = s.chars().skip(start).take(len).collect();
                Ok(Value::str(self.alloc(crate::core::heap::ManagedObject::Str(
                    crate::Text::from_string(sub),
                ))))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::InvalidIndexAccess {
                    expected: "int or range".to_string(),
                    actual: index.type_name().to_string(),
                }))
            }
        } else if tag == crate::core::value::TAG_TUPLE {
            let i = super::super::util::to_i64(&index)?;
            if i < 0 {
                return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
            }
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Tuple(tuple) = self.heap.get(id) {
                let ui = i as usize;
                if ui >= tuple.len() {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                Ok(tuple[ui])
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a tuple".into())))
            }
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::InvalidIndexAccess {
                expected: "list, dict, tuple, or text".to_string(),
                actual: obj.type_name().to_string(),
            }))
        }
    }
}
