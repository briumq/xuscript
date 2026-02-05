//! Local variable management for Runtime.

use crate::core::Value;
use super::core::Runtime;

impl Runtime {
    pub(crate) fn push_locals(&mut self) {
        self.locals.push();
    }

    pub(crate) fn pop_locals(&mut self) {
        self.locals.pop();
    }

    pub(crate) fn get_local(&self, name: &str) -> Option<Value> {
        self.locals.get(name)
    }

    pub(crate) fn get_local_by_index(&self, idx: usize) -> Option<Value> {
        self.locals.get_by_index(idx)
    }

    pub(crate) fn get_local_by_depth_index(&self, depth_from_top: usize, idx: usize) -> Option<Value> {
        self.locals.get_by_depth_index(depth_from_top, idx)
    }

    pub(crate) fn set_local(&mut self, name: &str, value: Value) -> bool {
        if self.locals.set(name, value) {
            return true;
        }
        false
    }

    pub(crate) fn get_local_index(&self, name: &str) -> Option<usize> {
        self.locals.get_index(name)
    }

    pub(crate) fn set_local_by_index(&mut self, idx: usize, value: Value) -> bool {
        if self.locals.set_by_index(idx, value) {
            return true;
        }
        false
    }

    pub(crate) fn define_local(&mut self, name: String, value: Value) {
        let _ = self.locals.define(name, value);
    }

    pub(crate) fn define_local_with_mutability(&mut self, name: String, value: Value, immutable: bool) {
        let _ = self.locals.define_with_mutability(name, value, immutable);
    }

    pub(crate) fn is_local_immutable(&self, name: &str) -> bool {
        self.locals.is_immutable(name)
    }
}
