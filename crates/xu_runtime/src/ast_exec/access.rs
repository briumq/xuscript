use crate::Value;
use crate::core::value::{DictKey, i64_to_text_fast};

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
            if matches!(field, "len" | "length" | "keys" | "values" | "items") {
                return self.get_member_with_ic_raw(obj, field, None);
            }

            let id = obj.as_obj_id();
            let (cur_ver, key_hash) = if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id)
            {
                (me.ver, Self::hash_bytes(me.map.hasher(), field.as_bytes()))
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            self.dict_version_last = Some((id.0, cur_ver));

            if let Some(idx) = slot_cell.get() {
                if idx < self.ic_slots.len() {
                    let c = &self.ic_slots[idx];
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
                if i0 < self.ic_slots.len() {
                    i0
                } else {
                    let ix = self.ic_slots.len();
                    self.ic_slots.push(crate::ICSlot::default());
                    slot_cell.set(Some(ix));
                    ix
                }
            } else {
                let ix = self.ic_slots.len();
                self.ic_slots.push(crate::runtime::ICSlot::default());
                slot_cell.set(Some(ix));
                ix
            };
            self.ic_slots[idx] = crate::runtime::ICSlot {
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
                    if idx < self.ic_slots.len() {
                        let c = &self.ic_slots[idx];
                        if c.struct_ty_hash == s.ty_hash && c.key_hash == xu_ir::stable_hash64(field)
                        {
                            if let Some(offset) = c.field_offset {
                                return Ok(s.fields[offset]);
                            }
                        }
                    }
                }

                let layout = self.struct_layouts.get(&s.ty).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                let pos = layout.iter().position(|f| f == field).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                })?;

                if let Some(idx) = slot_idx {
                    while self.ic_slots.len() <= idx {
                        self.ic_slots.push(crate::ICSlot::default());
                    }
                    self.ic_slots[idx] = crate::ICSlot {
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
        } else if tag == crate::core::value::TAG_LIST && field == "first" {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::List(v) = self.heap.get(id) {
                if let Some(first) = v.first().cloned() {
                    Ok(self.option_some(first))
                } else {
                    Ok(self.option_none())
                }
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        } else if tag == crate::core::value::TAG_LIST && (field == "len" || field == "length") {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::List(v) = self.heap.get(id) {
                Ok(Value::from_i64(v.len() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        } else if tag == crate::core::value::TAG_TUPLE && (field == "len" || field == "length") {
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
        } else if tag == crate::core::value::TAG_STR && (field == "len" || field == "length") {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(id) {
                Ok(Value::from_i64(s.as_str().chars().count() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw(
                    "Not a string".into(),
                )))
            }
        } else if tag == crate::core::value::TAG_DICT && (field == "len" || field == "length") {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Dict(v) = self.heap.get(id) {
                let mut n = v.map.len();
                n += v.prop_values.len();
                for ev in &v.elements {
                    if ev.get_tag() != crate::core::value::TAG_VOID {
                        n += 1;
                    }
                }
                Ok(Value::from_i64(n as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
            }
        } else if tag == crate::core::value::TAG_DICT && field == "keys" {
            let id = obj.as_obj_id();
            let keys_raw: Vec<crate::Text> = if let crate::core::heap::ManagedObject::Dict(db) =
                self.heap.get(id)
            {
                let mut out: Vec<crate::Text> = Vec::with_capacity(db.map.len() + db.prop_values.len());
                for k in db.map.keys() {
                    match k {
                        DictKey::Str { data, .. } => out.push(crate::Text::from_str(data)),
                        DictKey::Int(i) => out.push(i64_to_text_fast(*i)),
                    }
                }
                if let Some(sid) = db.shape {
                    if let crate::core::heap::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        for k in shape.prop_map.keys() {
                            out.push(crate::Text::from_str(k.as_str()));
                        }
                    }
                }
                for (i, v) in db.elements.iter().enumerate() {
                    if v.get_tag() != crate::core::value::TAG_VOID {
                        out.push(i64_to_text_fast(i as i64));
                    }
                }
                out
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            let mut keys: Vec<Value> = Vec::with_capacity(keys_raw.len());
            for s in keys_raw {
                keys.push(Value::str(self.heap.alloc(crate::core::heap::ManagedObject::Str(s))));
            }
            Ok(Value::list(self.heap.alloc(crate::core::heap::ManagedObject::List(keys))))
        } else if tag == crate::core::value::TAG_DICT && field == "values" {
            let id = obj.as_obj_id();
            let values: Vec<Value> = if let crate::core::heap::ManagedObject::Dict(db) = self.heap.get(id)
            {
                let mut out: Vec<Value> = Vec::with_capacity(db.map.len() + db.prop_values.len());
                out.extend(db.map.values().cloned());
                if let Some(sid) = db.shape {
                    if let crate::core::heap::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        for (_, off) in shape.prop_map.iter() {
                            if let Some(v) = db.prop_values.get(*off) {
                                out.push(*v);
                            }
                        }
                    }
                }
                for v in db.elements.iter() {
                    if v.get_tag() != crate::core::value::TAG_VOID {
                        out.push(*v);
                    }
                }
                out
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            Ok(Value::list(self.heap.alloc(crate::core::heap::ManagedObject::List(values))))
        } else if tag == crate::core::value::TAG_DICT && field == "items" {
            let id = obj.as_obj_id();
            let raw_items: Vec<(crate::Text, Value)> =
                if let crate::core::heap::ManagedObject::Dict(db) = self.heap.get(id) {
                    let mut out: Vec<(crate::Text, Value)> =
                        Vec::with_capacity(db.map.len() + db.prop_values.len());
                    for (k, v) in db.map.iter() {
                        let key = match k {
                            DictKey::Str { data, .. } => crate::Text::from_str(data),
                            DictKey::Int(i) => i64_to_text_fast(*i),
                        };
                        out.push((key, *v));
                    }
                    if let Some(sid) = db.shape {
                        if let crate::core::heap::ManagedObject::Shape(shape) = self.heap.get(sid) {
                            for (k, off) in shape.prop_map.iter() {
                                if let Some(v) = db.prop_values.get(*off) {
                                    out.push((crate::Text::from_str(k.as_str()), *v));
                                }
                            }
                        }
                    }
                    for (i, v) in db.elements.iter().enumerate() {
                        if v.get_tag() != crate::core::value::TAG_VOID {
                            out.push((i64_to_text_fast(i as i64), *v));
                        }
                    }
                    out
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
                };
            let mut items: Vec<Value> = Vec::with_capacity(raw_items.len());
            for (k, v) in raw_items {
                let key = Value::str(self.heap.alloc(crate::core::heap::ManagedObject::Str(k)));
                let pair = Value::tuple(
                    self.heap
                        .alloc(crate::core::heap::ManagedObject::Tuple(vec![key, v])),
                );
                items.push(pair);
            }
            Ok(Value::list(self.heap.alloc(crate::core::heap::ManagedObject::List(items))))
        } else if tag == crate::core::value::TAG_MODULE {
            let id = obj.as_obj_id();
            if let crate::core::heap::ManagedObject::Module(m) = self.heap.get(id) {
                if let Some(v) = m.exports.map.get(field) {
                    Ok(v.clone())
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
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            self.dict_version_last = Some((id.0, cur_ver));

            if let Some(idx) = slot_cell.get() {
                if idx < self.ic_slots.len() {
                    let c = &self.ic_slots[idx];
                    if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                        return Ok(c.value);
                    }
                }
            }

            let v = self.get_index_with_ic_raw(obj, index, None).map_err(|_| {
                self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string()))
            })?;

            let idx = if let Some(i0) = slot_cell.get() {
                if i0 < self.ic_slots.len() {
                    i0
                } else {
                    let ix = self.ic_slots.len();
                    self.ic_slots.push(crate::ICSlot::default());
                    slot_cell.set(Some(ix));
                    ix
                }
            } else {
                let ix = self.ic_slots.len();
                self.ic_slots.push(crate::runtime::ICSlot::default());
                slot_cell.set(Some(ix));
                ix
            };
            self.ic_slots[idx] = crate::runtime::ICSlot {
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
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
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
                        if idx < self.ic_slots.len() {
                            let c = &self.ic_slots[idx];
                            if c.id == id.0 && c.ver == me.ver && c.key_hash == key_hash {
                                return Ok(c.value);
                            }
                        }
                    }
                    let out_val = Self::dict_get_by_str_with_hash(me, &key, key_hash)
                        .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.clone())))?;
                    if let Some(idx) = slot_idx {
                        while self.ic_slots.len() <= idx {
                            self.ic_slots.push(crate::ICSlot::default());
                        }
                        self.ic_slots[idx] = crate::ICSlot {
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
                        if idx < self.ic_slots.len() {
                            let c = &self.ic_slots[idx];
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
                        while self.ic_slots.len() <= idx {
                            self.ic_slots.push(crate::ICSlot::default());
                        }
                        self.ic_slots[idx] = crate::ICSlot {
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
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
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
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())));
                };
                let ui = i as usize;
                let total = s.chars().count();
                if ui >= total {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let ch: String = s.chars().skip(ui).take(1).collect();
                Ok(Value::str(self.heap.alloc(crate::core::heap::ManagedObject::Str(
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
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())));
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
                Ok(Value::str(self.heap.alloc(crate::core::heap::ManagedObject::Str(
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
