//! Runtime values.
//!
//! Re-exports base types from xu_core and adds runtime-specific types.

// Re-export base types from xu_core
pub use xu_core::value::{
    DictKey, DictInstance, DictStrInstance, Dict, DictStr, Shape,
    FastHashMap, fast_hasher, fast_map_new, fast_map_with_capacity,
    dict_with_capacity, dict_str_new,
    write_i64_to_buf, i64_to_string_fast, i64_to_text_fast,
    QNAN, TAG_BASE, TAG_MASK, PAYLOAD_MASK,
    TAG_INT, TAG_BOOL, TAG_VOID, TAG_LIST, TAG_DICT, TAG_STR,
    TAG_STRUCT, TAG_MODULE, TAG_FUNC, TAG_FILE, TAG_RANGE,
    TAG_ENUM, TAG_BUILDER, TAG_TUPLE, TAG_OPTION,
    ModuleInstance, StructInstance, FileHandle,
};
pub use xu_core::value::Value;
pub use xu_core::gc::ObjectId;

use super::gc::{Heap, ManagedObject};
use std::rc::Rc;
use xu_ir::{Bytecode, FuncDef, BinaryOp};

// Runtime-specific function types

#[derive(Clone)]
pub enum Function {
    User(Rc<UserFunction>),
    Builtin(BuiltinFunction),
    Bytecode(Rc<BytecodeFunction>),
}

#[derive(Clone)]
pub struct UserFunction {
    pub def: FuncDef,
    pub env: super::Env,
    pub needs_env_frame: bool,
    pub fast_param_indices: Option<Box<[usize]>>,
    pub fast_locals_size: Option<usize>,
    pub skip_local_map: bool,
    pub type_sig_ic: std::cell::Cell<Option<u64>>,
}

#[derive(Clone)]
pub struct BytecodeFunction {
    pub def: FuncDef,
    pub bytecode: Rc<Bytecode>,
    pub env: super::Env,
    pub needs_env_frame: bool,
    pub locals_count: usize,
    pub type_sig_ic: std::cell::Cell<Option<u64>>,
}

pub type BuiltinFunction = fn(&mut crate::Runtime, &[Value]) -> Result<Value, String>;

/// Extension trait for Value with heap-dependent and runtime-specific methods.
pub trait ValueExt {
    fn mark_into(&self, heap: &mut Heap, pending: &mut Vec<ObjectId>);
    fn bin_op(&self, op: BinaryOp, other: Value) -> Result<Value, String>;
    fn bin_op_assign(&mut self, op: BinaryOp, other: Value, heap: &mut Heap) -> Result<(), String>;
    fn to_string_lossy(&self, heap: &Heap) -> String;
}

impl ValueExt for Value {
    fn mark_into(&self, heap: &mut Heap, pending: &mut Vec<ObjectId>) {
        if self.is_obj() {
            let id = self.as_obj_id();
            if !heap.is_marked(id) {
                pending.push(id);
            }
        }
    }

    fn bin_op(&self, op: BinaryOp, other: Value) -> Result<Value, String> {
        match op {
            BinaryOp::Add => add(*self, other),
            BinaryOp::Sub => sub(*self, other),
            BinaryOp::Mul => mul(*self, other),
            BinaryOp::Div => div(*self, other),
            BinaryOp::Mod => rem(*self, other),
            BinaryOp::Eq => Ok(Value::from_bool(self == &other)),
            BinaryOp::Ne => Ok(Value::from_bool(self != &other)),
            BinaryOp::And => and(*self, other),
            BinaryOp::Or => or(*self, other),
            BinaryOp::Gt
            | BinaryOp::Lt
            | BinaryOp::Ge
            | BinaryOp::Le => cmp(*self, op, other),
        }
    }

    fn bin_op_assign(
        &mut self,
        op: BinaryOp,
        other: Value,
        heap: &mut Heap,
    ) -> Result<(), String> {
        match op {
            BinaryOp::Add => {
                if self.get_tag() == TAG_STR {
                    let id = self.as_obj_id();
                    if other.get_tag() == TAG_STR {
                        let other_id = other.as_obj_id();
                        let other_s = if let ManagedObject::Str(s) = heap.get(other_id) {
                            s.as_str().to_string()
                        } else {
                            return Err("Not a string".to_string());
                        };
                        let s_ptr = if let ManagedObject::Str(s) = heap.get_mut(id) {
                            s
                        } else {
                            return Err("Not a string".to_string());
                        };
                        s_ptr.push_str(&other_s);
                        Ok(())
                    } else {
                        let bs = self.to_string_lossy(heap);
                        let other_bs = other.to_string_lossy(heap);
                        let s_ptr = if let ManagedObject::Str(s) = heap.get_mut(id) {
                            s
                        } else {
                            return Err("Not a string".to_string());
                        };
                        // Actually we need to append other to self, not bs
                        let _ = bs; // unused
                        s_ptr.push_str(&other_bs);
                        Ok(())
                    }
                } else {
                    *self = self.bin_op(op, other)?;
                    Ok(())
                }
            }
            _ => {
                *self = self.bin_op(op, other)?;
                Ok(())
            }
        }
    }

    fn to_string_lossy(&self, heap: &Heap) -> String {
        if self.is_void() {
            "()".to_string()
        } else if self.is_bool() {
            if self.as_bool() {
                "true".to_string()
            } else {
                "false".to_string()
            }
        } else if self.is_int() {
            i64_to_string_fast(self.as_i64())
        } else if self.is_f64() {
            let f = self.as_f64();
            if f.fract() == 0.0 {
                format!("{}", f as i64)
            } else {
                f.to_string()
            }
        } else if self.get_tag() == TAG_STR {
            if let ManagedObject::Str(s) = heap.get(self.as_obj_id()) {
                s.as_str().to_string()
            } else {
                format!("{:?}", self)
            }
        } else {
            format!("{:?}", self)
        }
    }
}

// Helper functions for binary operations

fn add(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        return Ok(Value::from_i64(
            a.as_i64().saturating_add(b.as_i64()),
        ));
    }
    if a.is_f64() || b.is_f64() {
        let x = if a.is_f64() {
            a.as_f64()
        } else if a.is_int() {
            a.as_i64() as f64
        } else {
            return Err("Invalid operand".into());
        };
        let y = if b.is_f64() {
            b.as_f64()
        } else if b.is_int() {
            b.as_i64() as f64
        } else {
            return Err("Invalid operand".into());
        };
        return Ok(Value::from_f64(x + y));
    }

    Err("Operand mismatch for add (String concat requires heap access)".into())
}

fn sub(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        return Ok(Value::from_i64(
            a.as_i64().saturating_sub(b.as_i64()),
        ));
    }
    let (x, y) = coerce_f64(a, b)?;
    Ok(Value::from_f64(x - y))
}

fn mul(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        return Ok(Value::from_i64(
            a.as_i64().saturating_mul(b.as_i64()),
        ));
    }
    let (x, y) = coerce_f64(a, b)?;
    Ok(Value::from_f64(x * y))
}

fn div(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        let bv = b.as_i64();
        if bv == 0 {
            return Err("Division by zero".to_string());
        }
        return a
            .as_i64()
            .checked_div(bv)
            .map(Value::from_i64)
            .ok_or_else(|| "Integer division overflow".to_string());
    }
    let (x, y) = coerce_f64(a, b)?;
    if y == 0.0 {
        return Err("Division by zero".to_string());
    }
    Ok(Value::from_f64(x / y))
}

fn rem(a: Value, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        let bv = b.as_i64();
        if bv == 0 {
            return Err("Division by zero".to_string());
        }
        return Ok(Value::from_i64(a.as_i64() % bv));
    }
    let (x, y) = coerce_f64(a, b)?;
    if y == 0.0 {
        return Err("Division by zero".to_string());
    }
    Ok(Value::from_f64(x % y))
}

fn and(a: Value, b: Value) -> Result<Value, String> {
    if a.is_bool() && b.is_bool() {
        return Ok(Value::from_bool(a.as_bool() && b.as_bool()));
    }
    Err("Logical AND requires both operands to be of type ?".to_string())
}

fn or(a: Value, b: Value) -> Result<Value, String> {
    if a.is_bool() && b.is_bool() {
        return Ok(Value::from_bool(a.as_bool() || b.as_bool()));
    }
    Err("Logical OR requires both operands to be of type ?".to_string())
}

fn cmp(a: Value, op: BinaryOp, b: Value) -> Result<Value, String> {
    if a.is_int() && b.is_int() {
        let av = a.as_i64();
        let bv = b.as_i64();
        let res = match op {
            BinaryOp::Gt => av > bv,
            BinaryOp::Lt => av < bv,
            BinaryOp::Ge => av >= bv,
            BinaryOp::Le => av <= bv,
            _ => unreachable!(),
        };
        return Ok(Value::from_bool(res));
    }
    let (x, y) = coerce_f64(a, b)?;
    let res = match op {
        BinaryOp::Gt => x > y,
        BinaryOp::Lt => x < y,
        BinaryOp::Ge => x >= y,
        BinaryOp::Le => x <= y,
        _ => unreachable!(),
    };
    Ok(Value::from_bool(res))
}

fn coerce_f64(a: Value, b: Value) -> Result<(f64, f64), String> {
    let av = if a.is_f64() {
        a.as_f64()
    } else if a.is_int() {
        a.as_i64() as f64
    } else {
        return Err(format!(
            "[E0003] Expected numeric type, got {}",
            a.type_name()
        ));
    };
    let bv = if b.is_f64() {
        b.as_f64()
    } else if b.is_int() {
        b.as_i64() as f64
    } else {
        return Err(format!(
            "[E0003] Expected numeric type, got {}",
            b.type_name()
        ));
    };
    Ok((av, bv))
}
