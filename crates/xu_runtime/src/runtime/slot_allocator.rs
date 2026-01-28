use crate::Value;
use crate::value::{FastHashMap, fast_map_new};

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
        let mut values = self.free_values.pop().unwrap_or_else(Vec::new);
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
        if let Some(map) = self.maps.last() {
            if let Some(values) = self.values.last() {
                if let Some(&idx) = map.get(name) {
                    return values.get(idx).cloned();
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

    pub fn get_index(&self, name: &str) -> Option<usize> {
        self.maps.last().and_then(|map| map.get(name).cloned())
    }

    pub fn set(&mut self, name: &str, value: Value) -> bool {
        if let Some(map) = self.maps.last() {
            if let Some(values) = self.values.last_mut() {
                if let Some(&idx) = map.get(name) {
                    if idx < values.len() {
                        values[idx] = value;
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn is_immutable(&self, name: &str) -> bool {
        if let Some(flags) = self.mut_flags.last() {
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
        Some(std::mem::replace(&mut values[idx], Value::NULL))
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
                        values.resize(max + 1, Value::NULL);
                    }
                }
                for (name, idx) in idxmap {
                    map.insert(name.clone(), *idx);
                }
            }
        }
    }

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
}

impl Default for LocalSlots {
    fn default() -> Self {
        Self::new()
    }
}
