use super::Value;
use super::value::{FastHashMap, fast_map_new};

pub(crate) struct LocalSlots {
    pub(crate) maps: Vec<FastHashMap<String, usize>>,
    pub(crate) values: Vec<Vec<Value>>,
    pub(crate) mut_flags: Vec<FastHashMap<String, bool>>,
    free_maps: Vec<FastHashMap<String, usize>>,
    free_values: Vec<Vec<Value>>,
}

impl LocalSlots {
    pub fn new() -> Self {
        Self {
            maps: Vec::new(),
            values: Vec::new(),
            mut_flags: Vec::new(),
            free_maps: Vec::new(),
            free_values: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.maps.clear();
        self.values.clear();
        self.mut_flags.clear();
        self.free_maps.clear();
        self.free_values.clear();
    }

    pub fn is_active(&self) -> bool {
        !self.maps.is_empty()
    }

    pub fn push(&mut self) {
        let mut map = self.free_maps.pop().unwrap_or_else(fast_map_new);
        map.clear();
        let mut values = self.free_values.pop().unwrap_or_default();
        values.clear();
        self.maps.push(map);
        self.values.push(values);
        self.mut_flags.push(fast_map_new());
    }

    pub fn pop(&mut self) {
        if let Some(mut map) = self.maps.pop() {
            map.clear();
            self.free_maps.push(map);
        }
        if let Some(mut values) = self.values.pop() {
            values.clear();
            self.free_values.push(values);
        }
        self.mut_flags.pop();
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        for (map, values) in self.maps.iter().zip(self.values.iter()).rev() {
            if let Some(&idx) = map.get(name) {
                if let Some(v) = values.get(idx).cloned() {
                    return Some(v);
                }
            }
        }
        None
    }

    pub fn get_by_index(&self, idx: usize) -> Option<Value> {
        self.values
            .last()
            .and_then(|values| values.get(idx).cloned())
    }

    pub fn get_by_depth_index(&self, depth_from_top: usize, idx: usize) -> Option<Value> {
        if depth_from_top >= self.values.len() {
            return None;
        }
        let frame_idx = self.values.len() - 1 - depth_from_top;
        self.values
            .get(frame_idx)
            .and_then(|values| values.get(idx).cloned())
    }

    pub fn get_index(&self, name: &str) -> Option<usize> {
        self.maps.last().and_then(|map| map.get(name).cloned())
    }

    pub fn set(&mut self, name: &str, value: Value) -> bool {
        for (map, values) in self.maps.iter().zip(self.values.iter_mut()).rev() {
            if let Some(&idx) = map.get(name) {
                if idx < values.len() {
                    values[idx] = value;
                    return true;
                }
            }
        }
        false
    }

    pub fn is_immutable(&self, name: &str) -> bool {
        for flags in self.mut_flags.iter().rev() {
            if let Some(&flag) = flags.get(name) {
                return flag;
            }
        }
        false
    }

    pub fn set_by_index(&mut self, idx: usize, value: Value) -> bool {
        if let Some(values) = self.values.last_mut() {
            if idx < values.len() {
                values[idx] = value;
                return true;
            }
        }
        false
    }

    pub fn take_local_by_index(&mut self, idx: usize) -> Option<Value> {
        let values = self.values.last_mut()?;
        if idx >= values.len() {
            return None;
        }
        Some(std::mem::replace(&mut values[idx], Value::UNIT))
    }

    pub fn define(&mut self, name: String, value: Value) -> Option<usize> {
        if let Some(map) = self.maps.last_mut() {
            if let Some(values) = self.values.last_mut() {
                if let Some(&idx) = map.get(&name) {
                    if idx < values.len() {
                        values[idx] = value;
                        return Some(idx);
                    }
                }
                let idx = values.len();
                values.push(value);
                map.insert(name.clone(), idx);
                if let Some(flags) = self.mut_flags.last_mut() {
                    flags.insert(name.clone(), false);
                }
                return Some(idx);
            }
        }
        None
    }

    pub fn define_with_mutability(
        &mut self,
        name: String,
        value: Value,
        immutable: bool,
    ) -> Option<usize> {
        if let Some(map) = self.maps.last_mut() {
            if let Some(values) = self.values.last_mut() {
                if let Some(&idx) = map.get(&name) {
                    if idx < values.len() {
                        values[idx] = value;
                        if let Some(flags) = self.mut_flags.last_mut() {
                            flags.insert(name, immutable);
                        }
                        return Some(idx);
                    }
                }
                let idx = values.len();
                values.push(value);
                map.insert(name.clone(), idx);
                if let Some(flags) = self.mut_flags.last_mut() {
                    flags.insert(name.clone(), immutable);
                }
                return Some(idx);
            }
        }
        None
    }

    pub fn init_from_index_map(&mut self, idxmap: &FastHashMap<String, usize>) {
        if let Some(map) = self.maps.last_mut() {
            if let Some(values) = self.values.last_mut() {
                if let Some(max) = idxmap.values().copied().max() {
                    if values.len() <= max {
                        values.resize(max + 1, Value::UNIT);
                    }
                }
                for (name, idx) in idxmap {
                    map.insert(name.clone(), *idx);
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn current_bindings(&self) -> Vec<(String, Value)> {
        let Some(map) = self.maps.last() else {
            return Vec::new();
        };
        let Some(values) = self.values.last() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(map.len());
        for (name, idx) in map {
            if let Some(v) = values.get(*idx).cloned() {
                out.push((name.clone(), v));
            }
        }
        out
    }

    /// Capture all bindings from all frames for closure capture.
    /// Inner scopes shadow outer scopes (later frames take precedence).
    pub fn all_bindings(&self) -> Vec<(String, Value)> {
        // Fast path: if no bindings at all, return empty
        if self.maps.iter().all(|m| m.is_empty()) {
            return Vec::new();
        }

        // Fast path: single frame - no shadowing possible
        if self.maps.len() == 1 {
            let map = &self.maps[0];
            let values = &self.values[0];
            let mut out = Vec::with_capacity(map.len());
            for (name, idx) in map {
                if let Some(v) = values.get(*idx).cloned() {
                    out.push((name.clone(), v));
                }
            }
            return out;
        }

        let mut seen: FastHashMap<String, Value> = fast_map_new();
        // Iterate from innermost to outermost, so inner shadows outer
        for (map, values) in self.maps.iter().zip(self.values.iter()).rev() {
            for (name, idx) in map {
                // Only add if not already seen (inner scope shadows outer)
                if !seen.contains_key(name) {
                    if let Some(v) = values.get(*idx).cloned() {
                        seen.insert(name.clone(), v);
                    }
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Check if there are any bindings to capture (fast check without allocation)
    #[inline]
    pub fn has_bindings(&self) -> bool {
        self.maps.iter().any(|m| !m.is_empty())
    }

    /// Get total number of bindings across all frames (for capacity hint)
    #[inline]
    #[allow(dead_code)]
    pub fn total_bindings_count(&self) -> usize {
        self.maps.iter().map(|m| m.len()).sum()
    }
}

impl Default for LocalSlots {
    fn default() -> Self {
        Self::new()
    }
}
