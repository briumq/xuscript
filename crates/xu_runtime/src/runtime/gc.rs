//! Garbage collection operations for the Runtime.
//!
//! This module contains:
//! - gc: Full garbage collection
//! - maybe_gc_with_roots: Conditional GC with extra roots

use crate::core::Value;
use crate::Runtime;

impl Runtime {
    /// Perform a full garbage collection cycle.
    ///
    /// This clears caches, collects roots from all sources, marks reachable objects,
    /// and sweeps unreachable objects.
    pub fn gc(&mut self, extra_roots: &[Value]) {
        // Clear caches that are safe to clear (don't affect correctness)
        self.caches.method_cache.clear();
        self.caches.dict_cache_last = None;
        self.caches.dict_cache_int_last = None;
        self.caches.dict_version_last = None;
        self.caches.ic_slots.clear();
        self.caches.ic_method_slots.clear();

        // Create roots vector
        let mut roots: Vec<Value> = Vec::new();
        roots.extend_from_slice(extra_roots);

        // Add temporary GC roots (e.g., function arguments being evaluated)
        roots.extend_from_slice(&self.gc_temp_roots);

        // Add values from active VM stacks
        for stack_ptr in &self.active_vm_stacks {
            // SAFETY: The stack pointer is valid as long as the VM frame is active
            let stack = unsafe { &**stack_ptr };
            for val in stack {
                roots.push(*val);
            }
        }

        // Add stack values as roots
        for val in &self.env.stack {
            roots.push(*val);
        }

        // Add all frame values as roots (not just global frame)
        for frame in &self.env.frames {
            let scope = frame.scope.borrow();
            for val in &scope.values {
                roots.push(*val);
            }
        }

        // Add local slot values as roots
        for frame_values in &self.locals.values {
            for val in frame_values {
                roots.push(*val);
            }
        }

        // Add bytecode string cache values as roots
        for cache in self.caches.bytecode_string_cache.values() {
            for val in cache.iter().flatten() {
                roots.push(*val);
            }
        }

        // Add small integer string cache values as roots
        for val in self.caches.small_int_strings.iter().flatten() {
            roots.push(*val);
        }

        // Add static field values as roots
        for val in self.types.static_fields.values() {
            roots.push(*val);
        }

        // Mark all reachable objects
        self.heap.mark_all(&roots, &[&self.env], &[&self.locals]);

        // Sweep phase
        self.heap.sweep();
    }

    /// Perform garbage collection if the heap has grown enough, with extra roots.
    pub(crate) fn maybe_gc_with_roots(&mut self, roots: &[Value]) {
        if self.heap.should_gc() {
            self.gc(roots);
        }
    }
}
