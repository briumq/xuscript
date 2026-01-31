use crate::Value;
use crate::Runtime;
use crate::gc::ManagedObject;
use xu_ir::BinaryOp;

#[inline(always)]
pub(super) fn op_add(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or("Stack underflow")?;
    let a = stack.last_mut().ok_or("Stack underflow")?;

    if a.is_int() && b.is_int() {
        let res = a.as_i64().wrapping_add(b.as_i64());
        *a = Value::from_i64(res);
        return Ok(());
    }

    match add_with_heap(rt, *a, b) {
        Ok(r) => {
            *a = r;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[inline(always)]
fn add_with_heap(rt: &mut Runtime, a: Value, b: Value) -> Result<Value, String> {
    use crate::value::TAG_STR;
    use crate::appendable::Appendable;

    let at = a.get_tag();
    let bt = b.get_tag();

    if at == TAG_STR || bt == TAG_STR {
        let a_len = if at == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                s.len()
            } else {
                return Err("Not a string".into());
            }
        } else {
            20
        };
        let b_len = if bt == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                s.len()
            } else {
                return Err("Not a string".into());
            }
        } else {
            20
        };

        let mut result = String::with_capacity(a_len + b_len);

        if at == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(a.as_obj_id()) {
                result.push_str(s.as_str());
            }
        } else {
            result.append_value(&a, &rt.heap);
        }

        if bt == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(b.as_obj_id()) {
                result.push_str(s.as_str());
            }
        } else {
            result.append_value(&b, &rt.heap);
        }

        Ok(Value::str(rt.heap.alloc(ManagedObject::Str(result.into()))))
    } else {
        a.bin_op(BinaryOp::Add, b)
    }
}

#[inline(always)]
pub(super) fn op_sub(stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or("Stack underflow")?;
    let a = stack.last_mut().ok_or("Stack underflow")?;
    *a = a.bin_op(BinaryOp::Sub, b)?;
    Ok(())
}

#[inline(always)]
pub(super) fn op_mul(stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or("Stack underflow")?;
    let a = stack.last_mut().ok_or("Stack underflow")?;
    *a = a.bin_op(BinaryOp::Mul, b)?;
    Ok(())
}

#[inline(always)]
pub(super) fn op_div(stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or("Stack underflow")?;
    let a = stack.last_mut().ok_or("Stack underflow")?;
    *a = a.bin_op(BinaryOp::Div, b)?;
    Ok(())
}

#[inline(always)]
pub(super) fn op_mod(stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or("Stack underflow")?;
    let a = stack.last_mut().ok_or("Stack underflow")?;
    *a = a.bin_op(BinaryOp::Mod, b)?;
    Ok(())
}

#[inline(always)]
pub(super) fn op_binary(op: BinaryOp, stack: &mut Vec<Value>) -> Result<(), String> {
    let b = stack.pop().ok_or("Stack underflow")?;
    let a = stack.last_mut().ok_or("Stack underflow")?;
    *a = a.bin_op(op, b)?;
    Ok(())
}

#[inline(always)]
pub(super) fn op_unary(op: xu_ir::UnaryOp, stack: &mut Vec<Value>) -> Result<(), String> {
    let a = stack.last_mut().ok_or("Stack underflow")?;
    match op {
        xu_ir::UnaryOp::Not => *a = Value::from_bool(!a.as_bool()),
        xu_ir::UnaryOp::Neg => {
            if a.is_int() {
                *a = Value::from_i64(-a.as_i64());
            } else if a.is_f64() {
                *a = Value::from_f64(-a.as_f64());
            } else {
                return Err("Operand must be a number".into());
            }
        }
    }
    Ok(())
}
