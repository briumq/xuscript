//! Binary operations and value equality for the Runtime.
//!
//! This module contains:
//! - eval_binary: Evaluate binary operations
//! - values_equal: Check if two values are equal
//! - values_equal_inner: Recursive equality check with cycle detection

use std::collections::HashSet;

use xu_ir::BinaryOp;

use crate::core::gc::ManagedObject;
use crate::core::value::ValueExt;
use crate::core::Value;
use crate::runtime::Text;
use crate::util::value_to_string;
use crate::Runtime;

impl Runtime {
    /// Evaluate a binary operation on two values.
    pub(crate) fn eval_binary(&mut self, op: BinaryOp, a: Value, b: Value) -> Result<Value, String> {
        let debug_err =
            |e: String, a: &Value, b: &Value, op: BinaryOp, heap: &crate::core::gc::Heap| {
                let sa = value_to_string(a, heap);
                let sb = value_to_string(b, heap);
                println!("BinaryOp Error: {} {:?} {} -> {}", sa, op, sb, e);
                Err(e)
            };

        match op {
            BinaryOp::Eq => Ok(Value::from_bool(self.values_equal(&a, &b))),
            BinaryOp::Ne => Ok(Value::from_bool(!self.values_equal(&a, &b))),
            BinaryOp::Add => {
                let at = a.get_tag();
                let bt = b.get_tag();
                if at == crate::core::value::TAG_STR && bt == crate::core::value::TAG_STR {
                    // Fast path: both are strings - use Text::concat2
                    let ta = if let ManagedObject::Str(s) = self.heap.get(a.as_obj_id()) {
                        s.clone()
                    } else {
                        Text::new()
                    };
                    let tb = if let ManagedObject::Str(s) = self.heap.get(b.as_obj_id()) {
                        s.clone()
                    } else {
                        Text::new()
                    };
                    let result = Text::concat2(&ta, &tb);
                    Ok(Value::str(self.heap.alloc(ManagedObject::Str(result))))
                } else if at == crate::core::value::TAG_STR || bt == crate::core::value::TAG_STR {
                    let sa = value_to_string(&a, &self.heap);
                    let sb = value_to_string(&b, &self.heap);
                    // Pre-allocate capacity to avoid intermediate allocations
                    let mut result = String::with_capacity(sa.len() + sb.len());
                    result.push_str(&sa);
                    result.push_str(&sb);
                    Ok(Value::str(
                        self.heap.alloc(ManagedObject::Str(result.into())),
                    ))
                } else {
                    a.bin_op(op, b)
                        .or_else(|e| debug_err(e, &a, &b, op, &self.heap))
                }
            }
            BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => {
                if a.get_tag() == crate::core::value::TAG_STR
                    && b.get_tag() == crate::core::value::TAG_STR
                {
                    let sa = if let ManagedObject::Str(s) = self.heap.get(a.as_obj_id()) {
                        s.as_str().to_string()
                    } else {
                        String::new()
                    };
                    let sb = if let ManagedObject::Str(s) = self.heap.get(b.as_obj_id()) {
                        s.as_str().to_string()
                    } else {
                        String::new()
                    };
                    let res = match op {
                        BinaryOp::Gt => sa > sb,
                        BinaryOp::Lt => sa < sb,
                        BinaryOp::Ge => sa >= sb,
                        BinaryOp::Le => sa <= sb,
                        _ => unreachable!(),
                    };
                    Ok(Value::from_bool(res))
                } else {
                    a.bin_op(op, b)
                        .or_else(|e| debug_err(e, &a, &b, op, &self.heap))
                }
            }
            _ => a
                .bin_op(op, b)
                .or_else(|e| debug_err(e, &a, &b, op, &self.heap)),
        }
    }

    /// Check if two values are equal.
    #[inline(always)]
    pub(crate) fn values_equal(&self, a: &Value, b: &Value) -> bool {
        if a == b {
            return true;
        }
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        self.values_equal_inner(a, b, &mut seen)
    }

    /// Recursive equality check with cycle detection.
    fn values_equal_inner(
        &self,
        a: &Value,
        b: &Value,
        seen: &mut HashSet<(usize, usize)>,
    ) -> bool {
        if a == b {
            return true;
        }

        if a.is_int() && b.is_f64() {
            return (a.as_i64() as f64) == b.as_f64();
        }
        if a.is_f64() && b.is_int() {
            return a.as_f64() == (b.as_i64() as f64);
        }

        let at = a.get_tag();
        let bt = b.get_tag();
        if at != bt {
            return false;
        }

        if a.is_obj() {
            let aid = a.as_obj_id();
            let bid = b.as_obj_id();
            let key = (aid.0, bid.0);
            if !seen.insert(key) {
                return true;
            }

            match (self.heap.get(aid), self.heap.get(bid)) {
                (ManagedObject::Str(x), ManagedObject::Str(y)) => x == y,
                (ManagedObject::List(a_list), ManagedObject::List(b_list)) => {
                    if a_list.len() != b_list.len() {
                        return false;
                    }
                    for (x, y) in a_list.iter().zip(b_list.iter()) {
                        if !self.values_equal_inner(x, y, seen) {
                            return false;
                        }
                    }
                    true
                }
                (ManagedObject::Dict(a_dict), ManagedObject::Dict(b_dict)) => {
                    if a_dict.map.len() != b_dict.map.len() {
                        return false;
                    }
                    for (k, av) in a_dict.map.iter() {
                        let Some(bv) = b_dict.map.get(k) else {
                            return false;
                        };
                        if !self.values_equal_inner(av, bv, seen) {
                            return false;
                        }
                    }
                    true
                }
                (ManagedObject::Range(a1, a2, ai), ManagedObject::Range(b1, b2, bi)) => {
                    a1 == b1 && a2 == b2 && ai == bi
                }
                (ManagedObject::Enum(ea), ManagedObject::Enum(eb)) => {
                    let (ta, va, pa) = ea.as_ref();
                    let (tb, vb, pb) = eb.as_ref();
                    if ta != tb || va != vb || pa.len() != pb.len() {
                        return false;
                    }
                    for (x, y) in pa.iter().zip(pb.iter()) {
                        if !self.values_equal_inner(x, y, seen) {
                            return false;
                        }
                    }
                    true
                }
                (ManagedObject::Struct(as_), ManagedObject::Struct(bs)) => {
                    if as_.ty != bs.ty || as_.fields.len() != bs.fields.len() {
                        return false;
                    }
                    for i in 0..as_.fields.len() {
                        if !self.values_equal_inner(&as_.fields[i], &bs.fields[i], seen) {
                            return false;
                        }
                    }
                    true
                }
                _ => false,
            }
        } else {
            false
        }
    }
}
