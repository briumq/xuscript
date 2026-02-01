use super::helpers::value_to_string;
use crate::Text;
use crate::Value;
use crate::core::gc::Heap;

pub trait Appendable {
    fn append_str(&mut self, s: &str);
    fn append_i64(&mut self, i: i64);
    fn append_f64(&mut self, f: f64);
    fn append_bool(&mut self, b: bool);
    fn append_null(&mut self);
    fn append_value(&mut self, v: &Value, heap: &Heap);
}

impl Appendable for String {
    fn append_str(&mut self, s: &str) {
        self.push_str(s);
    }
    fn append_i64(&mut self, i: i64) {
        let mut buf = itoa::Buffer::new();
        self.push_str(buf.format(i));
    }
    fn append_f64(&mut self, f: f64) {
        if f.fract() == 0.0 {
            self.append_i64(f as i64);
        } else {
            use std::fmt::Write;
            write!(self, "{}", f).ok();
        }
    }
    fn append_bool(&mut self, b: bool) {
        self.push_str(if b { "true" } else { "false" });
    }
    fn append_null(&mut self) {
        self.push_str("()");
    }
    fn append_value(&mut self, v: &Value, heap: &Heap) {
        if v.is_int() {
            self.append_i64(v.as_i64());
        } else if v.is_f64() {
            self.append_f64(v.as_f64());
        } else if v.is_bool() {
            self.append_bool(v.as_bool());
        } else if v.is_void() {
            self.append_null();
        } else if v.get_tag() == crate::core::value::TAG_STR {
            if let crate::core::gc::ManagedObject::Str(s) = heap.get(v.as_obj_id()) {
                self.append_str(s.as_str());
            }
        } else {
            self.append_str(&value_to_string(v, heap));
        }
    }
}

impl Appendable for Text {
    fn append_str(&mut self, s: &str) {
        self.push_str(s);
    }
    fn append_i64(&mut self, i: i64) {
        let mut buf = itoa::Buffer::new();
        self.push_str(buf.format(i));
    }
    fn append_f64(&mut self, f: f64) {
        if f.fract() == 0.0 {
            self.append_i64(f as i64);
        } else {
            self.push_str(&f.to_string());
        }
    }
    fn append_bool(&mut self, b: bool) {
        self.push_str(if b { "true" } else { "false" });
    }
    fn append_null(&mut self) {
        self.push_str("()");
    }
    fn append_value(&mut self, v: &Value, heap: &Heap) {
        if v.is_int() {
            self.append_i64(v.as_i64());
        } else if v.is_f64() {
            self.append_f64(v.as_f64());
        } else if v.is_bool() {
            self.append_bool(v.as_bool());
        } else if v.is_void() {
            self.append_null();
        } else if v.get_tag() == crate::core::value::TAG_STR {
            if let crate::core::gc::ManagedObject::Str(s) = heap.get(v.as_obj_id()) {
                self.append_str(s.as_str());
            }
        } else {
            self.append_str(&value_to_string(v, heap));
        }
    }
}
