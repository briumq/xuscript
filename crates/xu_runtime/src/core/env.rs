use std::cell::RefCell;
use std::rc::Rc;

use super::Value;
use super::value::{FastHashMap, fast_map_new};

#[derive(Clone, Debug)]
pub struct Scope {
    pub names: FastHashMap<String, usize>,
    pub values: Vec<Value>, // Used only when detached
    pub mut_flags: FastHashMap<String, bool>,
}

impl Scope {
    pub fn new() -> Self {
        Self {
            names: fast_map_new(),
            values: Vec::new(),
            mut_flags: fast_map_new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub scope: Rc<RefCell<Scope>>,
    pub base: usize,
    pub attached: bool,
}

impl Frame {
    fn new_attached(base: usize) -> Self {
        Self {
            scope: Rc::new(RefCell::new(Scope::new())),
            base,
            attached: true,
        }
    }

    fn new_detached() -> Self {
        Self {
            scope: Rc::new(RefCell::new(Scope::new())),
            base: 0,
            attached: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Env {
    pub stack: Vec<Value>,
    pub frames: Vec<Frame>,
    name_cache: FastHashMap<String, (u32, u32)>, // (depth_from_top, index)
}

impl Env {
    pub fn new() -> Self {
        // Global frame is always detached to persist across stack clears
        let global = Frame::new_detached();
        Self {
            stack: Vec::with_capacity(1024),
            frames: vec![global],
            name_cache: fast_map_new(),
        }
    }

    pub(crate) fn freeze(&mut self) -> Self {
        // "Promote" all attached frames to heap (Upvalue mechanism)
        // Key insight: We need to share the scope between the original env and the closure
        // so that modifications in the closure are visible in the original env and vice versa.
        for frame in &mut self.frames {
            if frame.attached {
                let mut scope = frame.scope.borrow_mut();
                // Move values from stack to scope.values
                // After this, both the original env and the closure will use scope.values
                // through the shared Rc<RefCell<Scope>>.
                let max_idx = scope.names.values().copied().max().unwrap_or(0);
                let count = if scope.names.is_empty() {
                    0
                } else {
                    max_idx + 1
                };

                scope.values.clear();
                scope.values.reserve(count);
                for i in 0..count {
                    if let Some(val) = self.stack.get(frame.base + i) {
                        scope.values.push(*val);
                    } else {
                        scope.values.push(Value::UNIT);
                    }
                }
                // Mark as detached - now both original and closure use scope.values
                frame.attached = false;
            }
        }

        // Clear name_cache since we changed attached status
        self.name_cache.clear();

        // Return a new Env with shared frames (same Rc<RefCell<Scope>>)
        // Both envs now point to the same scopes, so modifications are shared.
        Self {
            stack: Vec::new(), // New env has its own execution stack
            frames: self.frames.clone(), // Clone increments Rc refcount, shares the scope
            name_cache: fast_map_new(),
        }
    }

    /// Fast freeze that only shares the global frame.
    /// Use this when the closure doesn't capture any local variables.
    #[inline]
    pub(crate) fn freeze_global_only(&self) -> Self {
        // Only share the global frame (first frame)
        Self {
            stack: Vec::new(),
            frames: vec![self.frames[0].clone()],
            name_cache: fast_map_new(),
        }
    }

    pub fn fork_for_call(&self) -> Self {
        // When calling a function, we usually want to start with the same environment base.
        // If this is a closure call, self is the closure's env (already detached).
        Self {
            stack: Vec::with_capacity(1024),
            frames: self.frames.clone(),
            name_cache: fast_map_new(),
        }
    }

    pub fn reset_for_call_from(&mut self, base: &Env) {
        self.stack.clear();
        self.name_cache.clear();
        if self.frames.len() == 1
            && base.frames.len() == 1
            && Rc::ptr_eq(&self.frames[0].scope, &base.frames[0].scope)
            && self.frames[0].attached == base.frames[0].attached
        {
            return;
        }
        self.frames.clear();
        self.frames.extend(base.frames.iter().cloned());
    }

    pub fn push(&mut self) {
        let base = self.stack.len();
        self.frames.push(Frame::new_attached(base));
        self.name_cache.clear();
    }

    /// Push a new detached frame. Values will be stored in scope.values instead of stack.
    /// This is useful for closures where the captured values need to persist.
    pub fn push_detached(&mut self) {
        self.frames.push(Frame::new_detached());
        self.name_cache.clear();
    }

    pub fn pop(&mut self) {
        if let Some(frame) = self.frames.pop() {
            if frame.attached {
                // Restore stack to base (discard locals)
                self.stack.truncate(frame.base);
            } else {
                // For detached frames, clear scope.values to release references
                // This allows GC to collect objects that were only referenced by this scope
                let mut scope = frame.scope.borrow_mut();
                scope.values.clear();
                scope.names.clear();
                scope.mut_flags.clear();
            }
        }
        self.name_cache.clear();
    }

    /// Pop a frame without clearing the scope.
    /// This is used when the scope is shared with a closure (via freeze()).
    pub fn pop_without_clear(&mut self) {
        if let Some(frame) = self.frames.pop() {
            if frame.attached {
                // Restore stack to base (discard locals)
                self.stack.truncate(frame.base);
            }
            // For detached frames, don't clear scope - it's shared with a closure
        }
        self.name_cache.clear();
    }

    pub fn local_depth(&self) -> usize {
        self.frames.len().saturating_sub(1) // Excluding global frame
    }

    pub fn pop_to(&mut self, target_depth: usize) {
        // target_depth is the number of local frames we want to keep
        // Total frames = 1 (global) + target_depth
        let target_len = 1 + target_depth;
        while self.frames.len() > target_len {
            self.pop();
        }
        self.name_cache.clear();
    }

    pub fn define(&mut self, name: String, value: Value) {
        if let Some(frame) = self.frames.last_mut() {
            let mut scope = frame.scope.borrow_mut();
            if let Some(&idx) = scope.names.get(&name) {
                if frame.attached {
                    self.stack[frame.base + idx] = value;
                } else {
                    scope.values[idx] = value;
                }
                scope.mut_flags.entry(name.clone()).or_insert(false);
            } else {
                let idx = if frame.attached {
                    let i = self.stack.len() - frame.base;
                    self.stack.push(value);
                    i
                } else {
                    let i = scope.values.len();
                    scope.values.push(value);
                    i
                };
                scope.names.insert(name.clone(), idx);
                self.name_cache.insert(name.clone(), (0, idx as u32));
                scope.mut_flags.insert(name.clone(), false);
            }
        }
    }

    /// Batch define multiple bindings at once.
    /// More efficient than calling define() multiple times.
    #[inline]
    pub fn define_batch(&mut self, bindings: Vec<(String, Value)>) {
        if bindings.is_empty() {
            return;
        }
        if let Some(frame) = self.frames.last_mut() {
            let mut scope = frame.scope.borrow_mut();
            // Pre-allocate capacity
            scope.values.reserve(bindings.len());
            scope.names.reserve(bindings.len());

            for (name, value) in bindings {
                // For detached frames (which is what we use for closures),
                // we can skip the attached check
                let idx = scope.values.len();
                scope.values.push(value);
                scope.names.insert(name, idx);
            }
        }
    }

    pub fn define_with_mutability(&mut self, name: String, value: Value, immutable: bool) {
        if let Some(frame) = self.frames.last_mut() {
            let mut scope = frame.scope.borrow_mut();
            if let Some(&idx) = scope.names.get(&name) {
                if frame.attached {
                    self.stack[frame.base + idx] = value;
                } else {
                    scope.values[idx] = value;
                }
                scope.mut_flags.insert(name.clone(), immutable);
            } else {
                let idx = if frame.attached {
                    let i = self.stack.len() - frame.base;
                    self.stack.push(value);
                    i
                } else {
                    let i = scope.values.len();
                    scope.values.push(value);
                    i
                };
                scope.names.insert(name.clone(), idx);
                self.name_cache.insert(name.clone(), (0, idx as u32));
                scope.mut_flags.insert(name.clone(), immutable);
            }
        }
    }

    pub fn assign(&mut self, name: &str, value: Value) -> bool {
        // Check cache
        if let Some(&(depth, idx)) = self.name_cache.get(name) {
            let frame_idx = self.frames.len().checked_sub(1 + depth as usize);
            if let Some(f_idx) = frame_idx {
                let frame = &self.frames[f_idx];
                if frame.attached {
                    if let Some(slot) = self.stack.get_mut(frame.base + idx as usize) {
                        *slot = value;
                        return true;
                    }
                } else {
                    let mut scope = frame.scope.borrow_mut();
                    if (idx as usize) < scope.values.len() {
                        scope.values[idx as usize] = value;
                        return true;
                    }
                }
            }
        }

        // Slow path
        for (depth, frame) in self.frames.iter().rev().enumerate() {
            let mut scope = frame.scope.borrow_mut(); // Borrow mut for potential cache update or lazy creation? No, just checking.
            // Actually assign needs mut access to values.
            if let Some(&idx) = scope.names.get(name) {
                if frame.attached {
                    if let Some(slot) = self.stack.get_mut(frame.base + idx) {
                        *slot = value;
                    }
                } else if idx < scope.values.len() {
                    scope.values[idx] = value;
                }
                self.name_cache
                    .insert(name.to_string(), (depth as u32, idx as u32));
                return true;
            }
        }
        false
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(&(depth, idx)) = self.name_cache.get(name) {
            let frame_idx = self.frames.len().checked_sub(1 + depth as usize)?;
            let frame = &self.frames[frame_idx];
            if frame.attached {
                return self.stack.get(frame.base + idx as usize).cloned();
            } else {
                let scope = frame.scope.borrow();
                return scope.values.get(idx as usize).cloned();
            }
        }

        for frame in self.frames.iter().rev() {
            let scope = frame.scope.borrow();
            if let Some(&idx) = scope.names.get(name) {
                let val = if frame.attached {
                    self.stack.get(frame.base + idx).cloned()
                } else {
                    scope.values.get(idx).cloned()
                };
                // Can't update cache here due to &self
                return val;
            }
        }
        None
    }

    pub fn get_cached(&mut self, name: &str) -> Option<Value> {
        if let Some(&(depth, idx)) = self.name_cache.get(name) {
            let frame_idx = self.frames.len().checked_sub(1 + depth as usize)?;
            let frame = &self.frames[frame_idx];
            if frame.attached {
                return self.stack.get(frame.base + idx as usize).cloned();
            } else {
                let scope = frame.scope.borrow();
                return scope.values.get(idx as usize).cloned();
            }
        }

        for (depth, frame) in self.frames.iter().rev().enumerate() {
            let scope = frame.scope.borrow();
            if let Some(&idx) = scope.names.get(name) {
                let val = if frame.attached {
                    self.stack.get(frame.base + idx).cloned()
                } else {
                    scope.values.get(idx).cloned()
                };
                self.name_cache
                    .insert(name.to_string(), (depth as u32, idx as u32));
                return val;
            }
        }
        None
    }

    pub fn is_immutable(&self, name: &str) -> bool {
        for frame in self.frames.iter().rev() {
            let scope = frame.scope.borrow();
            if scope.names.contains_key(name) {
                if let Some(&flag) = scope.mut_flags.get(name) {
                    return flag;
                }
                return false;
            }
        }
        false
    }

    pub fn get_at(&self, idx: usize) -> Option<Value> {
        if let Some(frame) = self.frames.last() {
            if frame.attached {
                return self.stack.get(frame.base + idx).cloned();
            } else {
                let scope = frame.scope.borrow();
                return scope.values.get(idx).cloned();
            }
        }
        None
    }

    pub fn set_at(&mut self, idx: usize, value: Value) -> bool {
        if let Some(frame) = self.frames.last_mut() {
            if frame.attached {
                if let Some(slot) = self.stack.get_mut(frame.base + idx) {
                    *slot = value;
                    return true;
                }
            } else {
                let mut scope = frame.scope.borrow_mut();
                if idx < scope.values.len() {
                    scope.values[idx] = value;
                    return true;
                }
            }
        }
        false
    }

    pub fn take(&mut self, name: &str) -> Option<Value> {
        for (depth, frame) in self.frames.iter().rev().enumerate() {
            let mut scope = frame.scope.borrow_mut();
            if let Some(&idx) = scope.names.get(name) {
                let val = if frame.attached {
                    if let Some(slot) = self.stack.get_mut(frame.base + idx) {
                        std::mem::replace(slot, Value::UNIT)
                    } else {
                        Value::UNIT
                    }
                } else if idx < scope.values.len() {
                    std::mem::replace(&mut scope.values[idx], Value::UNIT)
                } else {
                    Value::UNIT
                };
                self.name_cache
                    .insert(name.to_string(), (depth as u32, idx as u32));
                return Some(val);
            }
        }
        None
    }

    // Helper to access global frame directly (compatibility)
    pub(crate) fn global_frame(&self) -> Rc<RefCell<Scope>> {
        // SAFETY: Env::new() always creates a global frame, so frames is never empty
        self.frames.first().expect("Env must have at least one frame").scope.clone()
    }

    // Helper for debugging/inspection
    pub fn all_frames(&self) -> Vec<Rc<RefCell<Scope>>> {
        self.frames.iter().map(|f| f.scope.clone()).collect()
    }
}

impl Default for Env {
    fn default() -> Self {
        Self::new()
    }
}
