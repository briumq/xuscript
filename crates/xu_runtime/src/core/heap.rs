//! Garbage collection and heap management.

use super::value::{Dict, DictStr, FileHandle, Function, ModuleInstance, StructInstance, Value};
use super::text::Text;

/// Handle to a heap-allocated object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub usize);

#[derive(Clone)]
pub enum ManagedObject {
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Dict(Dict),
    DictStr(DictStr),
    File(Box<FileHandle>),
    Builder(String),
    Struct(Box<StructInstance>),
    Module(Box<ModuleInstance>),
    Range(i64, i64, bool),
    Enum(Box<(Text, Text, Box<[Value]>)>),
    OptionSome(Value),
    Function(Function),
    Str(Text),
    Shape(Box<super::value::Shape>),
}

impl ManagedObject {
    #[inline]
    pub fn size(&self) -> usize {
        match self {
            ManagedObject::List(v) => 64 + v.capacity() * 8,
            ManagedObject::Tuple(v) => 64 + v.capacity() * 8,
            ManagedObject::Dict(d) => {
                let elements_cap = d.elements.as_ref().map_or(0, |e| e.capacity());
                let prop_values_cap = d.prop_values.as_ref().map_or(0, |pv| pv.capacity());
                128 + d.map.capacity() * 48 + elements_cap * 8 + prop_values_cap * 8
            }
            ManagedObject::DictStr(d) => 64 + d.map.capacity() * 48,
            ManagedObject::Builder(s) => 32 + s.capacity(),
            ManagedObject::Struct(s) => 64 + s.fields.len() * 8,
            ManagedObject::Str(s) => 32 + s.as_str().len(),
            ManagedObject::Function(_) => 256,
            ManagedObject::Enum(e) => 64 + e.2.len() * 8,
            ManagedObject::Module(_) => 256,
            ManagedObject::File(_) => 128,
            ManagedObject::Shape(s) => 64 + s.prop_map.capacity() * 32,
            ManagedObject::Range(_, _, _) => 32,
            ManagedObject::OptionSome(_) => 16,
        }
    }
}

pub struct Heap {
    pub(crate) objects: Vec<Option<ManagedObject>>,
    free_list: Vec<usize>,
    marks: Vec<u64>,
    pub(crate) alloc_count: usize,
    pub(crate) gc_threshold: usize,
    pub(crate) alloc_bytes: usize,
    pub(crate) gc_threshold_bytes: usize,
}

impl Heap {
    pub fn new() -> Self {
        Self {
            objects: Vec::with_capacity(1024),
            free_list: Vec::new(),
            marks: Vec::new(),
            alloc_count: 0,
            gc_threshold: 500_000,  // 50万对象触发GC
            alloc_bytes: 0,
            gc_threshold_bytes: 256 * 1024 * 1024,  // 256MB触发GC
        }
    }

    #[inline]
    pub fn alloc(&mut self, obj: ManagedObject) -> ObjectId {
        self.alloc_count += 1;
        self.alloc_bytes += obj.size();

        if let Some(id) = self.free_list.pop() {
            self.objects[id] = Some(obj);
            ObjectId(id)
        } else {
            let id = self.objects.len();
            self.objects.push(Some(obj));
            ObjectId(id)
        }
    }

    #[inline]
    pub fn should_gc(&self) -> bool {
        self.alloc_count >= self.gc_threshold || self.alloc_bytes >= self.gc_threshold_bytes
    }

    #[inline]
    pub fn get(&self, id: ObjectId) -> &ManagedObject {
        unsafe { self.objects.get_unchecked(id.0).as_ref().unwrap_unchecked() }
    }

    #[inline]
    pub fn get_mut(&mut self, id: ObjectId) -> &mut ManagedObject {
        unsafe { self.objects.get_unchecked_mut(id.0).as_mut().unwrap_unchecked() }
    }

    #[inline]
    pub fn is_marked(&self, id: ObjectId) -> bool {
        let word = id.0 >> 6;
        let bit = id.0 & 63;
        word < self.marks.len() && (self.marks[word] & (1u64 << bit)) != 0
    }

    #[inline]
    fn is_marked_idx(&self, id: usize) -> bool {
        let word = id >> 6;
        let bit = id & 63;
        word < self.marks.len() && (self.marks[word] & (1u64 << bit)) != 0
    }

    #[inline]
    fn set_mark(&mut self, id: usize) -> bool {
        let word = id >> 6;
        let bit = id & 63;
        if word >= self.marks.len() {
            self.marks.resize(word + 1, 0);
        }
        let mask = 1u64 << bit;
        if (self.marks[word] & mask) != 0 {
            return false;
        }
        self.marks[word] |= mask;
        true
    }

    pub fn mark(&mut self, id: ObjectId) {
        self.mark_value(Value::list(id));
    }

    pub fn mark_value(&mut self, root: Value) {
        self.mark_all(&[root], &[], &[]);
    }

    /// Optimized mark phase
    pub(crate) fn mark_all(
        &mut self,
        roots: &[Value],
        envs: &[&super::Env],
        locals: &[&super::slot_allocator::LocalSlots],
    ) {
        let needed_words = (self.objects.len() + 63) >> 6;
        self.marks.clear();
        self.marks.resize(needed_words, 0);

        let mut stack: Vec<usize> = Vec::with_capacity(2048);

        #[inline]
        fn push_if_obj(stack: &mut Vec<usize>, val: Value) {
            if val.is_obj() {
                stack.push(val.as_obj_id().0);
            }
        }

        for val in roots {
            push_if_obj(&mut stack, *val);
        }

        for env in envs {
            for val in &env.stack {
                push_if_obj(&mut stack, *val);
            }
        }

        for ls in locals {
            for frame_values in &ls.values {
                for val in frame_values {
                    push_if_obj(&mut stack, *val);
                }
            }
        }

        while let Some(id) = stack.pop() {
            if id >= self.objects.len() || !self.set_mark(id) {
                continue;
            }

            if let Some(obj) = &self.objects[id] {
                match obj {
                    ManagedObject::List(list) => {
                        for item in list.iter() {
                            push_if_obj(&mut stack, *item);
                        }
                    }
                    ManagedObject::Tuple(list) => {
                        for item in list.iter() {
                            push_if_obj(&mut stack, *item);
                        }
                    }
                    ManagedObject::Dict(dict) => {
                        // Mark string keys (DictKey::StrRef stores ObjectId)
                        for key in dict.map.keys() {
                            if let Some(obj_id) = key.str_obj_id() {
                                stack.push(obj_id.0);
                            }
                        }
                        for value in dict.map.values() {
                            push_if_obj(&mut stack, *value);
                        }
                        if let Some(pv) = &dict.prop_values {
                            for value in pv.as_ref() {
                                push_if_obj(&mut stack, *value);
                            }
                        }
                        if let Some(elements) = &dict.elements {
                            for value in elements.as_ref() {
                                push_if_obj(&mut stack, *value);
                            }
                        }
                    }
                    ManagedObject::DictStr(dict) => {
                        for value in dict.map.values() {
                            push_if_obj(&mut stack, *value);
                        }
                    }
                    ManagedObject::Struct(s) => {
                        for field in s.fields.iter() {
                            push_if_obj(&mut stack, *field);
                        }
                    }
                    ManagedObject::Module(m) => {
                        for value in m.exports.map.values() {
                            push_if_obj(&mut stack, *value);
                        }
                    }
                    ManagedObject::Enum(e) => {
                        for item in e.2.iter() {
                            push_if_obj(&mut stack, *item);
                        }
                    }
                    ManagedObject::OptionSome(v) => {
                        push_if_obj(&mut stack, *v);
                    }
                    ManagedObject::Function(func) => {
                        match func {
                            Function::User(uf) => {
                                for val in &uf.env.stack {
                                    push_if_obj(&mut stack, *val);
                                }
                                for frame in &uf.env.frames {
                                    let scope = frame.scope.borrow();
                                    for val in &scope.values {
                                        push_if_obj(&mut stack, *val);
                                    }
                                }
                            }
                            Function::Bytecode(bf) => {
                                for val in &bf.env.stack {
                                    push_if_obj(&mut stack, *val);
                                }
                                for frame in &bf.env.frames {
                                    let scope = frame.scope.borrow();
                                    for val in &scope.values {
                                        push_if_obj(&mut stack, *val);
                                    }
                                }
                            }
                            Function::Builtin(_) => {}
                        }
                    }
                    ManagedObject::Str(_) |
                    ManagedObject::Builder(_) |
                    ManagedObject::File(_) |
                    ManagedObject::Range(_, _, _) |
                    ManagedObject::Shape(_) => {}
                }
            }
        }
    }

    /// Sweep phase - free unreachable objects and aggressively reclaim memory
    pub fn sweep(&mut self) {
        let mut live_count = 0usize;
        let mut live_bytes = 0usize;
        let mut last_live_idx = 0usize;

        // First pass: free dead objects and count live ones
        for i in 0..self.objects.len() {
            if let Some(ref obj) = self.objects[i] {
                if self.is_marked_idx(i) {
                    live_count += 1;
                    live_bytes += obj.size();
                    last_live_idx = i;
                } else {
                    // Free the object
                    self.objects[i] = None;
                }
            }
        }

        // Truncate trailing None slots
        if last_live_idx + 1 < self.objects.len() {
            self.objects.truncate(last_live_idx + 1);
        }

        // Rebuild free list from remaining None slots
        self.free_list.clear();
        for i in 0..self.objects.len() {
            if self.objects[i].is_none() {
                self.free_list.push(i);
            }
        }

        // Aggressively shrink if we have too many free slots
        // If more than half of the slots are free, try to compact
        let used = self.objects.len();
        let free_count = self.free_list.len();
        let live_objects = used - free_count;
        let cap = self.objects.capacity();

        // Shrink if: capacity > 4096 AND (capacity > live_objects * 4 OR free_count > live_objects)
        if cap > 4096 && (cap > live_objects * 4 || free_count > live_objects) {
            // Target capacity: 2x live objects
            let target = (live_objects * 2).max(4096);
            self.objects.shrink_to(target);
        }

        // Shrink free_list if too large
        if self.free_list.capacity() > self.free_list.len() * 4 && self.free_list.capacity() > 1024 {
            self.free_list.shrink_to(self.free_list.len() * 2);
        }

        self.marks.clear();
        if self.marks.capacity() > 1024 {
            self.marks.shrink_to(256);
        }

        self.alloc_count = 0;
        self.alloc_bytes = 0;

        // Set next GC threshold based on live data
        let growth = if live_count > 50000 { 1.5 } else { 2.0 };
        self.gc_threshold = ((live_count as f64 * growth) as usize).max(16384);
        self.gc_threshold_bytes = ((live_bytes as f64 * growth) as usize).max(16 * 1024 * 1024);
    }

    pub fn memory_stats(&self) -> String {
        let mut counts: [usize; 14] = [0; 14];
        for obj in self.objects.iter().flatten() {
            let idx = match obj {
                ManagedObject::Str(_) => 0,
                ManagedObject::List(_) => 1,
                ManagedObject::Dict(_) => 2,
                ManagedObject::DictStr(_) => 3,
                ManagedObject::Struct(_) => 4,
                ManagedObject::Enum(_) | ManagedObject::OptionSome(_) => 5,
                ManagedObject::Function(_) => 6,
                ManagedObject::Builder(_) => 7,
                ManagedObject::Range(_, _, _) => 8,
                ManagedObject::Tuple(_) => 9,
                ManagedObject::File(_) => 10,
                ManagedObject::Module(_) => 11,
                ManagedObject::Shape(_) => 12,
            };
            counts[idx] += 1;
        }
        let total: usize = counts.iter().sum();
        format!(
            "Heap: {} objects ({} Str, {} List, {} Dict, {} Struct, {} Enum, {} Func, {} other), {} free slots",
            total, counts[0], counts[1], counts[2], counts[4], counts[5], counts[6],
            counts[3] + counts[7] + counts[8] + counts[9] + counts[10] + counts[11] + counts[12],
            self.free_list.len()
        )
    }

    /// Get the set of marked (live) object IDs after mark phase
    /// Must be called after mark_all() and before sweep()
    #[cfg(feature = "generational-gc")]
    pub fn get_marked_ids(&self) -> std::collections::HashSet<usize> {
        let mut live_ids = std::collections::HashSet::with_capacity(self.objects.len() / 2);
        for i in 0..self.objects.len() {
            if self.objects[i].is_some() && self.is_marked_idx(i) {
                live_ids.insert(i);
            }
        }
        live_ids
    }

    /// Free specific objects by their IDs (for young GC)
    #[cfg(feature = "generational-gc")]
    pub fn free_objects(&mut self, ids: &[usize]) {
        for &id in ids {
            if id < self.objects.len() {
                if let Some(ref obj) = self.objects[id] {
                    let size = obj.size();
                    if self.alloc_bytes >= size {
                        self.alloc_bytes -= size;
                    }
                }
                self.objects[id] = None;
                self.free_list.push(id);
            }
        }
        if self.alloc_count >= ids.len() {
            self.alloc_count -= ids.len();
        }
    }

    /// Collect references from an object (for young GC tracing)
    #[cfg(feature = "generational-gc")]
    pub fn collect_refs(&self, id: usize, out: &mut Vec<usize>) {
        if id >= self.objects.len() {
            return;
        }
        if let Some(obj) = &self.objects[id] {
            match obj {
                ManagedObject::List(list) | ManagedObject::Tuple(list) => {
                    for item in list {
                        if item.is_obj() {
                            out.push(item.as_obj_id().0);
                        }
                    }
                }
                ManagedObject::Dict(dict) => {
                    for key in dict.map.keys() {
                        if let Some(obj_id) = key.str_obj_id() {
                            out.push(obj_id.0);
                        }
                    }
                    for value in dict.map.values() {
                        if value.is_obj() {
                            out.push(value.as_obj_id().0);
                        }
                    }
                    if let Some(pv) = &dict.prop_values {
                        for value in pv.as_ref() {
                            if value.is_obj() {
                                out.push(value.as_obj_id().0);
                            }
                        }
                    }
                    if let Some(elements) = &dict.elements {
                        for value in elements.as_ref() {
                            if value.is_obj() {
                                out.push(value.as_obj_id().0);
                            }
                        }
                    }
                }
                ManagedObject::DictStr(dict) => {
                    for value in dict.map.values() {
                        if value.is_obj() {
                            out.push(value.as_obj_id().0);
                        }
                    }
                }
                ManagedObject::Struct(s) => {
                    for field in s.fields.iter() {
                        if field.is_obj() {
                            out.push(field.as_obj_id().0);
                        }
                    }
                }
                ManagedObject::Enum(e) => {
                    for item in e.2.iter() {
                        if item.is_obj() {
                            out.push(item.as_obj_id().0);
                        }
                    }
                }
                ManagedObject::OptionSome(v) => {
                    if v.is_obj() {
                        out.push(v.as_obj_id().0);
                    }
                }
                ManagedObject::Function(func) => {
                    match func {
                        Function::User(uf) => {
                            for val in &uf.env.stack {
                                if val.is_obj() {
                                    out.push(val.as_obj_id().0);
                                }
                            }
                        }
                        Function::Bytecode(bf) => {
                            for val in &bf.env.stack {
                                if val.is_obj() {
                                    out.push(val.as_obj_id().0);
                                }
                            }
                        }
                        Function::Builtin(_) => {}
                    }
                }
                ManagedObject::Module(m) => {
                    for value in m.exports.map.values() {
                        if value.is_obj() {
                            out.push(value.as_obj_id().0);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
