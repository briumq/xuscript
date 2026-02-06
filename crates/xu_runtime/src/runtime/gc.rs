//! Garbage collection operations for the Runtime.
//!
//! This module contains:
//! - gc: Full garbage collection
//! - maybe_gc_with_roots: Conditional GC with extra roots

use crate::core::Value;
use crate::Runtime;

impl Runtime {
    /// Perform a full garbage collection cycle.
    pub fn gc(&mut self, extra_roots: &[Value]) {
        // Clear runtime caches that don't hold heap references
        self.caches.method_cache.clear();
        self.caches.dict_cache_last = None;
        self.caches.dict_cache_int_last = None;
        self.caches.dict_version_last = None;
        self.caches.ic_slots.clear();
        self.caches.ic_method_slots.clear();

        // Clear object pools that may hold references to heap objects
        // These pools are just for performance optimization and can be rebuilt
        self.pools.env_pool.clear();
        self.pools.vm_stack_pool.clear();
        self.pools.small_list_pool.clear();

        // Estimate capacity for roots vector
        // Include string caches as roots to preserve them across GC
        let small_int_count = self.caches.small_int_strings.iter().filter(|v| v.is_some()).count();
        let bytecode_cache_count = self.caches.bytecode_string_cache.len();
        let estimated_roots = extra_roots.len()
            + self.gc_temp_roots.len()
            + self.env.stack.len()
            + self.locals.values.iter().map(|v| v.len()).sum::<usize>()
            + small_int_count
            + bytecode_cache_count
            + 256;

        let mut roots: Vec<Value> = Vec::with_capacity(estimated_roots);
        roots.extend_from_slice(extra_roots);
        roots.extend_from_slice(&self.gc_temp_roots);

        for stack_ptr in &self.active_vm_stacks {
            let stack = unsafe { &**stack_ptr };
            roots.extend_from_slice(stack);
        }

        roots.extend_from_slice(&self.env.stack);

        for frame in &self.env.frames {
            let scope = frame.scope.borrow();
            roots.extend_from_slice(&scope.values);
        }

        for frame_values in &self.locals.values {
            roots.extend_from_slice(frame_values);
        }

        for val in self.types.static_fields.values() {
            roots.push(*val);
        }

        // Add string caches as roots - these are performance-critical
        // and should be preserved across GC cycles
        for val in self.caches.small_int_strings.iter().flatten() {
            roots.push(*val);
        }
        for cache_vec in self.caches.bytecode_string_cache.values() {
            for val in cache_vec.iter().flatten() {
                roots.push(*val);
            }
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
