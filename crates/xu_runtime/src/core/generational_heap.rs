//! Generational Garbage Collection for Xu Runtime
//!
//! Two-generation GC design using lazy range tracking:
//! - Young generation: Objects allocated after last full GC (ID >= young_start)
//! - Old generation: Objects that survived a full GC (ID < young_start)
//!
//! This approach has minimal overhead:
//! - track_alloc: just increment counter (no bitmap)
//! - is_young: simple comparison (ID >= young_start)
//! - Memory: only card_table for write barrier

use super::heap::ObjectId;

/// Configuration
const CARD_SHIFT: usize = 9;  // 每个card覆盖512个对象

/// Generational GC Tracker with range-based young object tracking
pub struct GenerationalHeap {
    /// Start of young generation (object IDs >= this are young)
    young_start: usize,
    /// Card table: each bit represents whether a card is dirty
    card_table: Vec<u64>,
    /// Mark bits for young GC
    young_marks: Vec<u64>,
    /// Allocation count since last GC
    pub young_alloc_count: usize,
    /// Total allocation count since last full GC
    pub total_alloc_count: usize,
    /// Statistics
    pub young_gc_count: usize,
    pub full_gc_count: usize,
}

impl GenerationalHeap {
    pub fn new() -> Self {
        Self {
            young_start: 0,
            card_table: Vec::new(),
            young_marks: Vec::new(),
            young_alloc_count: 0,
            total_alloc_count: 0,
            young_gc_count: 0,
            full_gc_count: 0,
        }
    }

    /// Track a new allocation - just increment counter
    #[inline(always)]
    pub fn track_alloc(&mut self, _id: ObjectId) {
        self.young_alloc_count += 1;
        self.total_alloc_count += 1;
    }

    /// Check if young GC should be triggered
    #[inline]
    pub fn should_young_gc(&self) -> bool {
        // Disabled - only use full GC for now
        false
    }

    /// Mark a card as dirty (write barrier)
    #[inline(always)]
    pub fn write_barrier(&mut self, container_id: usize) {
        // Only track writes to old objects
        if container_id < self.young_start {
            let card_idx = container_id >> CARD_SHIFT;
            let word = card_idx >> 6;
            let bit = card_idx & 63;
            if word < self.card_table.len() {
                self.card_table[word] |= 1u64 << bit;
            } else {
                self.card_table.resize(word + 1, 0);
                self.card_table[word] |= 1u64 << bit;
            }
        }
    }

    /// Check if an object is young
    #[inline(always)]
    pub fn is_young(&self, idx: usize) -> bool {
        idx >= self.young_start
    }

    /// Check if an object is old
    #[inline(always)]
    pub fn is_old(&self, idx: usize) -> bool {
        idx < self.young_start
    }

    /// Mark a young object during young GC
    #[inline]
    fn mark_young(&mut self, idx: usize) {
        let word = idx >> 6;
        let bit = idx & 63;
        if word >= self.young_marks.len() {
            self.young_marks.resize(word + 1, 0);
        }
        self.young_marks[word] |= 1u64 << bit;
    }

    #[inline]
    fn is_young_marked(&self, idx: usize) -> bool {
        let word = idx >> 6;
        let bit = idx & 63;
        word < self.young_marks.len() && (self.young_marks[word] & (1u64 << bit)) != 0
    }

    /// Clear all dirty cards
    fn clear_card_table(&mut self) {
        for word in &mut self.card_table {
            *word = 0;
        }
    }

    /// Perform young generation GC
    /// Returns (objects_to_free, objects_promoted)
    pub fn young_gc<F>(
        &mut self,
        root_ids: &[usize],
        trace_refs: F,
        max_heap_id: usize,
        is_live: impl Fn(usize) -> bool,
    ) -> (Vec<usize>, usize)
    where
        F: Fn(usize, &mut Vec<usize>),
    {
        self.young_gc_count += 1;

        // Quick check: if no young objects, nothing to do
        if self.young_start >= max_heap_id {
            self.young_alloc_count = 0;
            self.clear_card_table();
            return (Vec::new(), 0);
        }

        let young_count = max_heap_id - self.young_start;

        // Prepare mark bits
        let needed_words = (max_heap_id + 64) >> 6;
        self.young_marks.clear();
        self.young_marks.resize(needed_words, 0);

        let mut stack: Vec<usize> = Vec::with_capacity(1024);
        let mut refs_buf: Vec<usize> = Vec::with_capacity(64);

        // 1. Mark young objects directly reachable from roots
        for &root_id in root_ids {
            if self.is_young(root_id) && !self.is_young_marked(root_id) {
                stack.push(root_id);
            }
        }

        // 2. Scan dirty cards for old->young references
        for (word_idx, &card_word) in self.card_table.iter().enumerate() {
            if card_word == 0 {
                continue;
            }
            let mut word = card_word;
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                let card_idx = (word_idx << 6) | bit;
                word &= word - 1;

                let start = card_idx << CARD_SHIFT;
                let end = ((card_idx + 1) << CARD_SHIFT).min(self.young_start);
                for idx in start..end {
                    if is_live(idx) {
                        refs_buf.clear();
                        trace_refs(idx, &mut refs_buf);
                        for &ref_id in &refs_buf {
                            if self.is_young(ref_id) && !self.is_young_marked(ref_id) {
                                stack.push(ref_id);
                            }
                        }
                    }
                }
            }
        }

        // 3. Trace through young generation
        while let Some(idx) = stack.pop() {
            if !self.is_young(idx) || self.is_young_marked(idx) {
                continue;
            }
            self.mark_young(idx);

            refs_buf.clear();
            trace_refs(idx, &mut refs_buf);
            for &ref_id in &refs_buf {
                if self.is_young(ref_id) && !self.is_young_marked(ref_id) {
                    stack.push(ref_id);
                }
            }
        }

        // 4. Sweep: collect dead young objects
        let mut to_free = Vec::with_capacity(young_count / 2);

        for idx in self.young_start..max_heap_id {
            if is_live(idx) && !self.is_young_marked(idx) {
                to_free.push(idx);
            }
        }

        // All surviving young objects are promoted to old
        let promoted = young_count - to_free.len();
        self.young_start = max_heap_id;

        self.young_alloc_count = 0;
        self.young_marks.clear();
        self.clear_card_table();

        (to_free, promoted)
    }

    /// Called after a full GC to reset young tracking
    pub fn after_full_gc(&mut self, max_heap_id: usize) {
        self.full_gc_count += 1;

        // All survivors are now old, new allocations will be young
        self.young_start = max_heap_id;

        // Reset counters
        self.young_alloc_count = 0;
        self.total_alloc_count = 0;

        // Clear and shrink card table
        self.card_table.clear();
        if self.card_table.capacity() > 64 {
            self.card_table.shrink_to(32);
        }
    }

    /// Get count of young objects
    pub fn young_count(&self) -> usize {
        self.young_alloc_count
    }

    pub fn memory_stats(&self) -> String {
        format!(
            "GenHeap: ~{} young (start={}), {} young GCs, {} full GCs",
            self.young_alloc_count, self.young_start, self.young_gc_count, self.full_gc_count
        )
    }
}

impl Default for GenerationalHeap {
    fn default() -> Self {
        Self::new()
    }
}
