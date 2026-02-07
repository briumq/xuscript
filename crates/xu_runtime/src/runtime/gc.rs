//! Garbage collection operations for the Runtime.
//!
//! This module contains:
//! - gc: Full garbage collection
//! - maybe_gc_with_roots: Conditional GC with extra roots
//! - Generational GC support (when feature enabled)

use crate::core::Value;
use crate::Runtime;

impl Runtime {
    /// Collect all GC roots from the runtime state
    fn collect_gc_roots(&self, extra_roots: &[Value]) -> Vec<Value> {
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

        for val in self.caches.small_int_strings.iter().flatten() {
            roots.push(*val);
        }
        for cache_vec in self.caches.bytecode_string_cache.values() {
            for val in cache_vec.iter().flatten() {
                roots.push(*val);
            }
        }

        roots
    }

    /// Clear runtime caches before GC
    fn clear_caches_for_gc(&mut self) {
        self.caches.method_cache.clear();
        self.caches.dict_cache_last = None;
        self.caches.dict_cache_int_last = None;
        self.caches.dict_version_last = None;
        self.caches.ic_slots.clear();
        self.caches.ic_method_slots.clear();
        self.pools.env_pool.clear();
        self.pools.vm_stack_pool.clear();
        self.pools.small_list_pool.clear();
    }

    /// Clean up string intern cache after GC.
    /// Removes entries pointing to objects that were garbage collected.
    fn cleanup_intern_cache(&mut self) {
        self.caches.string_value_intern.retain(|_, val| {
            let id = val.as_obj_id();
            // Check if the object still exists (not freed by GC)
            id.0 < self.heap.objects.len() && self.heap.objects[id.0].is_some()
        });
    }

    /// Perform a full garbage collection cycle.
    #[cfg(not(feature = "generational-gc"))]
    pub fn gc(&mut self, extra_roots: &[Value]) {
        self.clear_caches_for_gc();
        let roots = self.collect_gc_roots(extra_roots);
        self.heap.mark_all(&roots, &[&self.env], &[&self.locals]);
        self.heap.sweep();
        self.cleanup_intern_cache();
    }

    /// Perform garbage collection (generational GC version).
    /// Currently only uses full GC - young GC is disabled for stability.
    #[cfg(feature = "generational-gc")]
    pub fn gc(&mut self, extra_roots: &[Value]) {
        self.clear_caches_for_gc();
        let roots = self.collect_gc_roots(extra_roots);
        self.full_gc(&roots);
        self.cleanup_intern_cache();
    }

    /// Perform full garbage collection (generational GC version)
    #[cfg(feature = "generational-gc")]
    fn full_gc(&mut self, roots: &[Value]) {
        // Standard mark-sweep
        self.heap.mark_all(roots, &[&self.env], &[&self.locals]);
        self.heap.sweep();

        // Update gen_heap after full GC
        let max_heap_id = self.heap.objects.len();
        self.gen_heap.after_full_gc(max_heap_id);
    }

    /// Perform garbage collection if the heap has grown enough.
    #[cfg(not(feature = "generational-gc"))]
    pub(crate) fn maybe_gc_with_roots(&mut self, roots: &[Value]) {
        if self.heap.should_gc() {
            self.gc(roots);
        }
    }

    /// Perform garbage collection if needed (generational GC version).
    #[cfg(feature = "generational-gc")]
    pub(crate) fn maybe_gc_with_roots(&mut self, roots: &[Value]) {
        if self.heap.should_gc() {
            self.gc(roots);
        }
    }
}
