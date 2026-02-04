//! Garbage collection and heap management.

use super::value::{Dict, DictStr, FileHandle, Function, ModuleInstance, StructInstance, DictKey, Value};
use super::text::Text;
use std::collections::HashSet;

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
    OptionSome(Value), // Optimized Option::some with single value
    Function(Function),
    Str(Text),
    Shape(Box<super::value::Shape>),
}

impl ManagedObject {
    pub fn size(&self) -> usize {
        // Base size of the enum variant + deep size estimation
        let base = std::mem::size_of::<ManagedObject>();
        let deep = match self {
            ManagedObject::List(v) => {
                // Count actual len + capacity overhead + allocator overhead
                v.len() * std::mem::size_of::<Value>()
                    + v.capacity() * std::mem::size_of::<Value>() / 4
                    + v.capacity() * 8 // Estimated allocator overhead
            }
            ManagedObject::Tuple(v) => {
                v.len() * std::mem::size_of::<Value>()
                    + v.capacity() * std::mem::size_of::<Value>() / 4
                    + v.capacity() * 8
            }
            ManagedObject::Dict(d) => {
                // More accurate sizing for hash maps
                let map_size = d.map.capacity()
                    * (
                        std::mem::size_of::<DictKey>()
                            + std::mem::size_of::<Value>()
                            + 16
                        // HashTable overhead per entry
                    );
                let elements_size = d.elements.len() * std::mem::size_of::<Value>();
                let props_size = d.prop_values.len() * std::mem::size_of::<Value>();
                map_size + elements_size + props_size + d.map.capacity() * 8
            }
            ManagedObject::DictStr(d) => {
                let map_size = d.map.capacity()
                    * (std::mem::size_of::<String>() + std::mem::size_of::<Value>() + 16);
                // Add actual string content size
                let string_size: usize = d.map.keys().map(|s| s.capacity()).sum();
                map_size + string_size + d.map.capacity() * 8
            }
            ManagedObject::File(f) => {
                f.path.capacity() + f.content.capacity() + 1024 // File handle overhead
            }
            ManagedObject::Builder(s) => {
                s.capacity() + s.capacity() / 4 + 32 // Builder overhead
            }
            ManagedObject::Struct(s) => {
                s.ty.capacity() + s.fields.len() * std::mem::size_of::<Value>() + 64
                // Struct instance overhead
            }
            ManagedObject::Module(m) => {
                m.exports.map.capacity()
                    * (std::mem::size_of::<String>() + std::mem::size_of::<Value>() + 16)
                    + 1024 // Module overhead
            }
            ManagedObject::Enum(e) => {
                let (ty, variant, payload) = e.as_ref();
                ty.as_str().len()
                    + variant.as_str().len()
                    + payload.len() * std::mem::size_of::<Value>()
                    + 64 // Enum overhead
            }
            ManagedObject::Str(s) => {
                s.as_str().len() + 32 // String/Text overhead
            }
            ManagedObject::Shape(s) => {
                s.prop_map.capacity()
                    * (std::mem::size_of::<String>() + std::mem::size_of::<usize>() + 16)
                    + 128 // Shape overhead
            }
            ManagedObject::Range(_, _, _) => 24, // Simple range size
            ManagedObject::Function(_) => 256,   // Approximate function size
            ManagedObject::OptionSome(_) => std::mem::size_of::<Value>(), // Single value
        };
        base + deep
    }
}

pub struct Heap {
    pub(crate) objects: Vec<Option<ManagedObject>>,
    free_list: Vec<usize>,
    marks: Vec<u64>,
    marked_frames: HashSet<usize>,
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
            marked_frames: HashSet::new(),
            alloc_count: 0,
            gc_threshold: 100000, // Lower threshold for better memory management with large datasets
            alloc_bytes: 0,
            // Lower threshold to trigger GC more frequently with large datasets
            gc_threshold_bytes: 32 * 1024 * 1024, // 32MB start instead of 128MB
        }
    }

    /// Allocate a managed object on the heap.
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

    pub fn get(&self, id: ObjectId) -> &ManagedObject {
        self.objects[id.0]
            .as_ref()
            .expect("Object was garbage collected")
    }

    pub fn get_mut(&mut self, id: ObjectId) -> &mut ManagedObject {
        self.objects[id.0]
            .as_mut()
            .expect("Object was garbage collected")
    }

    pub fn is_marked(&self, id: ObjectId) -> bool {
        let word = id.0 >> 6;
        let bit = id.0 & 63;
        self.marks
            .get(word)
            .map_or(false, |w| (w & (1 << bit)) != 0)
    }

    fn set_mark(&mut self, id: ObjectId) -> bool {
        let word = id.0 >> 6;
        let bit = id.0 & 63;
        if word >= self.marks.len() {
            self.marks.resize(word + 1, 0);
        }
        let w = &mut self.marks[word];
        let mask = 1 << bit;
        if (*w & mask) != 0 {
            return false;
        }
        *w |= mask;
        true
    }

    pub fn mark(&mut self, id: ObjectId) {
        self.mark_value(Value::list(id));
    }

    pub fn mark_value(&mut self, root: Value) {
        self.mark_all(&[root], &[], &[]);
    }

    /// Mark all reachable objects from roots, environments, and local slots.
    pub(crate) fn mark_all(
        &mut self,
        roots: &[Value],
        envs: &[&super::Env],
        locals: &[&super::slot_allocator::LocalSlots],
    ) {
        // Clear marks at the beginning to avoid duplicate marking
        self.marks.clear();
        self.marked_frames.clear();

        let mut pending_values: Vec<Value> = roots.to_vec();

        // Process environment stack values
        for env in envs {
            for val in &env.stack {
                pending_values.push(val.clone());
            }
        }

        // Process local slot values
        for ls in locals {
            for frame_values in &ls.values {
                for val in frame_values {
                    pending_values.push(val.clone());
                }
            }
        }

        while !pending_values.is_empty() {
            if let Some(val) = pending_values.pop() {
                if val.is_obj() {
                    let id = val.as_obj_id();
                    if id.0 < self.objects.len() {
                        // Only process if the object still exists
                        if self.objects[id.0].is_some() {
                            if self.set_mark(id) {
                                // Safely borrow the object
                                if let Some(obj) = &self.objects[id.0] {
                                    match obj {
                                        ManagedObject::List(list) => {
                                            for item in list {
                                                pending_values.push(item.clone());
                                            }
                                        }
                                        ManagedObject::Tuple(list) => {
                                            for item in list {
                                                pending_values.push(item.clone());
                                            }
                                        }
                                        ManagedObject::Dict(dict) => {
                                            for value in dict.map.values() {
                                                pending_values.push(value.clone());
                                            }
                                        }
                                        ManagedObject::DictStr(dict) => {
                                            for value in dict.map.values() {
                                                pending_values.push(value.clone());
                                            }
                                        }
                                        ManagedObject::Struct(s) => {
                                            for field in &s.fields {
                                                pending_values.push(field.clone());
                                            }
                                        }
                                        ManagedObject::Module(m) => {
                                            for value in m.exports.map.values() {
                                                pending_values.push(value.clone());
                                            }
                                        }
                                        ManagedObject::Enum(e) => {
                                            let (_, _, payload) = e.as_ref();
                                            for item in payload.iter() {
                                                pending_values.push(item.clone());
                                            }
                                        }
                                        ManagedObject::Function(func) => {
                                            // Mark values captured by closures
                                            match func {
                                                Function::User(uf) => {
                                                    // Mark values in the captured environment
                                                    for val in &uf.env.stack {
                                                        pending_values.push(val.clone());
                                                    }
                                                    for frame in &uf.env.frames {
                                                        let scope = frame.scope.borrow();
                                                        for val in &scope.values {
                                                            pending_values.push(val.clone());
                                                        }
                                                    }
                                                }
                                                Function::Bytecode(bf) => {
                                                    // Mark values in the captured environment
                                                    for val in &bf.env.stack {
                                                        pending_values.push(val.clone());
                                                    }
                                                    for frame in &bf.env.frames {
                                                        let scope = frame.scope.borrow();
                                                        for val in &scope.values {
                                                            pending_values.push(val.clone());
                                                        }
                                                    }
                                                }
                                                Function::Builtin(_) => {}
                                            }
                                        }
                                        // Skip Shape objects for now to avoid the crash
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Sweep unreachable objects and update thresholds.
    pub fn sweep(&mut self) {
        let mut live_bytes = 0;
        let mut live_count = 0;

        // Clear free_list before sweeping to rebuild it completely
        self.free_list.clear();

        for i in 0..self.objects.len() {
            if let Some(obj) = &self.objects[i] {
                if !self.is_marked(ObjectId(i)) {
                    // Object is unreachable, free it
                    self.objects[i] = None;
                    self.free_list.push(i);
                } else {
                    // Object is reachable, keep it
                    live_bytes += obj.size();
                    live_count += 1;
                }
            } else {
                // Slot is already empty, add to free_list
                self.free_list.push(i);
            }
        }

        // Truncate trailing empty slots to reduce memory usage
        while self.objects.last().map_or(false, |o| o.is_none()) {
            self.objects.pop();
        }
        // Remove truncated indices from free_list
        let new_len = self.objects.len();
        self.free_list.retain(|&i| i < new_len);
        // Shrink capacity if significantly oversized
        if self.objects.capacity() > self.objects.len() * 4 && self.objects.capacity() > 4096 {
            self.objects.shrink_to(self.objects.len() * 2);
        }

        self.marks.clear();
        self.marked_frames.clear();

        self.alloc_count = 0;
        self.alloc_bytes = live_bytes;

        // Do NOT compact the heap - this can cause issues with object IDs
        // self.compact_if_needed();

        // Adaptive strategy:
        // If heap is small, grow fast (2x).
        // If heap is large, grow slower (1.5x) to avoid massive pauses.
        let growth_factor = if live_bytes > 10 * 1024 * 1024 {
            1.5
        } else {
            2.0
        };

        self.gc_threshold = (live_count as f64 * growth_factor) as usize;
        self.gc_threshold = self.gc_threshold.max(32768);

        self.gc_threshold_bytes = (live_bytes as f64 * growth_factor) as usize;
        self.gc_threshold_bytes = self.gc_threshold_bytes.max(1024 * 1024); // Min 1MB
    }

    /// Get memory statistics by object type
    pub fn memory_stats(&self) -> String {
        let mut str_count = 0usize;
        let mut str_bytes = 0usize;
        let mut list_count = 0usize;
        let mut list_bytes = 0usize;
        let mut dict_count = 0usize;
        let mut dict_bytes = 0usize;
        let mut dict_capacity = 0usize;
        let mut dict_len = 0usize;
        let mut dict_str_count = 0usize;
        let mut dict_str_bytes = 0usize;
        let mut struct_count = 0usize;
        let mut struct_bytes = 0usize;
        let mut enum_count = 0usize;
        let mut enum_bytes = 0usize;
        let mut func_count = 0usize;
        let mut func_bytes = 0usize;
        let mut builder_count = 0usize;
        let mut builder_bytes = 0usize;
        let mut range_count = 0usize;
        let mut range_bytes = 0usize;
        let mut other_count = 0usize;
        let mut other_bytes = 0usize;

        for obj in self.objects.iter().flatten() {
            let size = obj.size();
            match obj {
                ManagedObject::Str(_) => { str_count += 1; str_bytes += size; }
                ManagedObject::List(_) => { list_count += 1; list_bytes += size; }
                ManagedObject::Dict(d) => {
                    dict_count += 1;
                    dict_bytes += size;
                    dict_capacity += d.map.capacity();
                    dict_len += d.map.len();
                }
                ManagedObject::DictStr(_) => { dict_str_count += 1; dict_str_bytes += size; }
                ManagedObject::Struct(_) => { struct_count += 1; struct_bytes += size; }
                ManagedObject::Enum(_) | ManagedObject::OptionSome(_) => { enum_count += 1; enum_bytes += size; }
                ManagedObject::Function(_) => { func_count += 1; func_bytes += size; }
                ManagedObject::Builder(_) => { builder_count += 1; builder_bytes += size; }
                ManagedObject::Range(_, _, _) => { range_count += 1; range_bytes += size; }
                _ => { other_count += 1; other_bytes += size; }
            }
        }

        let total_count = str_count + list_count + dict_count + dict_str_count + struct_count + enum_count + func_count + builder_count + range_count + other_count;
        let total_bytes = str_bytes + list_bytes + dict_bytes + dict_str_bytes + struct_bytes + enum_bytes + func_bytes + builder_bytes + range_bytes + other_bytes;
        let heap_overhead = self.objects.capacity() * std::mem::size_of::<Option<ManagedObject>>();

        format!(
            "=== Heap Memory Stats ===\n\
             Str:      {:>8} objects, {:>12} bytes ({:.1}%)\n\
             List:     {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Dict:     {:>8} objects, {:>12} bytes ({:.1}%) [cap={}, len={}]\n\
             DictStr:  {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Struct:   {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Enum:     {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Function: {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Builder:  {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Range:    {:>8} objects, {:>12} bytes ({:.1}%)\n\
             Other:    {:>8} objects, {:>12} bytes ({:.1}%)\n\
             --------------------------\n\
             Total:    {:>8} objects, {:>12} bytes\n\
             Heap vec: {:>8} slots,   {:>12} bytes overhead\n\
             Free:     {:>8} slots",
            str_count, str_bytes, if total_bytes > 0 { str_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            list_count, list_bytes, if total_bytes > 0 { list_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            dict_count, dict_bytes, if total_bytes > 0 { dict_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 }, dict_capacity, dict_len,
            dict_str_count, dict_str_bytes, if total_bytes > 0 { dict_str_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            struct_count, struct_bytes, if total_bytes > 0 { struct_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            enum_count, enum_bytes, if total_bytes > 0 { enum_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            func_count, func_bytes, if total_bytes > 0 { func_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            builder_count, builder_bytes, if total_bytes > 0 { builder_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            range_count, range_bytes, if total_bytes > 0 { range_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            other_count, other_bytes, if total_bytes > 0 { other_bytes as f64 / total_bytes as f64 * 100.0 } else { 0.0 },
            total_count, total_bytes,
            self.objects.capacity(), heap_overhead,
            self.free_list.len()
        )
    }

}
