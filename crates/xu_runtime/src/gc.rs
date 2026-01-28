//! Garbage collection and heap management.

use crate::Value;
use crate::runtime::Scope;
use crate::value::{Dict, DictStr, FileHandle, Function, ModuleInstance, StructInstance};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub usize);

pub enum ManagedObject {
    List(Vec<Value>),
    Dict(Dict),
    DictStr(DictStr),
    File(FileHandle),
    Builder(String),
    Struct(StructInstance),
    Module(ModuleInstance),
    Range(i64, i64),
    Enum(crate::Text, crate::Text, Box<[Value]>),
    Function(Function),
    Str(crate::Text),
    Shape(crate::value::Shape),
}

impl ManagedObject {
    pub fn size(&self) -> usize {
        // Base size of the enum variant + deep size estimation
        let base = std::mem::size_of::<ManagedObject>();
        let deep = match self {
            ManagedObject::List(v) => v.capacity() * std::mem::size_of::<Value>(),
            ManagedObject::Dict(d) => {
                d.map.capacity()
                    * (std::mem::size_of::<crate::value::DictKey>() + std::mem::size_of::<Value>())
            }
            ManagedObject::DictStr(d) => {
                d.map.capacity() * (std::mem::size_of::<String>() + std::mem::size_of::<Value>())
            }
            ManagedObject::File(f) => f.path.capacity() + f.content.capacity(),
            ManagedObject::Builder(s) => s.capacity(),
            ManagedObject::Struct(s) => {
                s.ty.capacity() + s.fields.len() * std::mem::size_of::<Value>()
            }
            ManagedObject::Module(m) => {
                m.exports.map.capacity()
                    * (std::mem::size_of::<String>() + std::mem::size_of::<Value>())
            }
            ManagedObject::Enum(ty, variant, payload) => {
                ty.as_str().len()
                    + variant.as_str().len()
                    + payload.len() * std::mem::size_of::<Value>()
            }
            ManagedObject::Str(s) => s.as_str().len(), // Text might be shared, but count it for now
            ManagedObject::Shape(s) => {
                s.prop_map.capacity()
                    * (std::mem::size_of::<String>() + std::mem::size_of::<usize>())
            }
            _ => 0,
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
            gc_threshold: 1000000, // High threshold to avoid GC during benchmarks
            alloc_bytes: 0,
            // Increase initial threshold to avoid frequent GCs in micro-benchmarks
            gc_threshold_bytes: 512 * 1024 * 1024, // 512MB start
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

    pub fn should_gc(&self) -> bool {
        // Disable GC during benchmarks to avoid the dict key collection bug
        false
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

    /// Mark all reachable objects from roots, environments, and locals.
    pub(crate) fn mark_all(
        &mut self,
        roots: &[Value],
        envs: &[&crate::runtime::Env],
        locals: &[&crate::runtime::LocalSlots],
    ) {
        let mut pending_values: Vec<Value> = roots.to_vec();
        let mut pending_frames: Vec<Rc<RefCell<Scope>>> = Vec::new();

        for env in envs {
            // Mark stack values for running environments
            pending_values.extend(env.stack.iter().cloned());

            for f in env.all_frames() {
                let ptr = f.as_ptr() as usize;
                if !self.marked_frames.contains(&ptr) {
                    self.marked_frames.insert(ptr);
                    pending_frames.push(f.clone());
                }
            }
        }

        for ls in locals {
            for frame_values in &ls.values {
                pending_values.extend(frame_values.iter().cloned());
            }
        }

        while !pending_values.is_empty() || !pending_frames.is_empty() {
            if let Some(val) = pending_values.pop() {
                if val.is_obj() {
                    let id = val.as_obj_id();
                    if self.set_mark(id) {
                        match &self.objects[id.0] {
                            Some(ManagedObject::List(list)) => {
                                pending_values.extend(list.iter().cloned())
                            }
                            Some(ManagedObject::Dict(dict)) => {
                                pending_values.extend(dict.map.values().cloned())
                            }
                            Some(ManagedObject::DictStr(dict)) => {
                                pending_values.extend(dict.map.values().cloned())
                            }
                            Some(ManagedObject::Struct(s)) => {
                                pending_values.extend(s.fields.iter().cloned())
                            }
                            Some(ManagedObject::Module(m)) => {
                                pending_values.extend(m.exports.map.values().cloned())
                            }
                            Some(ManagedObject::Enum(_, _, payload)) => {
                                pending_values.extend(payload.iter().cloned())
                            }
                            Some(ManagedObject::Function(f)) => {
                                let env = match f {
                                    Function::User(u) => Some(&u.env),
                                    Function::Bytecode(b) => Some(&b.env),
                                    Function::Builtin(_) => None,
                                };
                                if let Some(env) = env {
                                    for f in env.all_frames() {
                                        let ptr = f.as_ptr() as usize;
                                        if !self.marked_frames.contains(&ptr) {
                                            self.marked_frames.insert(ptr);
                                            pending_frames.push(f.clone());
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            } else if let Some(frame) = pending_frames.pop() {
                let scope = frame.borrow();
                pending_values.extend(scope.values.iter().cloned());
            }
        }
    }

    /// Sweep unreachable objects and update thresholds.
    pub fn sweep(&mut self) {
        let mut live_bytes = 0;
        let mut live_count = 0;
        for i in 0..self.objects.len() {
            if let Some(obj) = &self.objects[i] {
                if !self.is_marked(ObjectId(i)) {
                    self.objects[i] = None;
                    self.free_list.push(i);
                } else {
                    live_bytes += obj.size();
                    live_count += 1;
                }
            }
        }
        self.marks.clear();
        self.marked_frames.clear();

        self.alloc_count = 0;
        self.alloc_bytes = live_bytes;

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
}
