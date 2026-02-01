use crate::core::Value;
use crate::Runtime;
use crate::core::gc::ManagedObject;
use crate::core::value::{StructInstance, TAG_STR, DictKey, TAG_BUILDER};

#[inline(always)]
pub(super) fn op_load_local(rt: &mut Runtime, stack: &mut Vec<Value>, idx: usize) -> Result<(), String> {
    let val = rt.get_local_by_index(idx).ok_or_else(|| format!("Undefined local variable index: {}", idx))?;
    stack.push(val);
    Ok(())
}

#[inline(always)]
pub(super) fn op_store_local(rt: &mut Runtime, stack: &mut Vec<Value>, idx: usize) -> Result<(), String> {
    let val = stack.pop().ok_or("Stack underflow")?;
    if !rt.set_local_by_index(idx, val) {
        while rt.get_local_by_index(idx).is_none() {
            rt.define_local(format!("_tmp_{}", idx), Value::VOID);
        }
        rt.set_local_by_index(idx, val);
    }
    Ok(())
}

#[inline(always)]
pub(super) fn op_load_name(rt: &mut Runtime, stack: &mut Vec<Value>, name: &str) -> Result<(), String> {
    let v = if rt.locals.is_active() {
        if let Some(v) = rt.get_local(name) {
            v
        } else {
            rt.env.get_cached(name).ok_or_else(|| {
                rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(name.to_string()))
            })?
        }
    } else {
        rt.env.get_cached(name).ok_or_else(|| {
            rt.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(name.to_string()))
        })?
    };
    stack.push(v);
    Ok(())
}

#[inline(always)]
pub(super) fn op_store_name(rt: &mut Runtime, stack: &mut Vec<Value>, name: &str) -> Result<(), String> {
    let v = stack.pop().ok_or("Stack underflow")?;
    if rt.locals.is_active() {
        if !rt.set_local(name, v) {
            rt.define_local(name.to_string(), v);
        }
    } else {
        if !rt.env.assign(name, v) {
            rt.env.define(name.to_string(), v);
        }
    }
    Ok(())
}

#[inline(always)]
pub(super) fn op_struct_init(
    rt: &mut Runtime,
    stack: &mut Vec<Value>,
    ty: &str,
    field_names: &[String],
) -> Result<(), String> {
    let layout = rt.struct_layouts.get(ty).ok_or_else(|| {
        rt.error(xu_syntax::DiagnosticKind::UnknownStruct(ty.to_string()))
    })?.clone();

    let mut values = vec![Value::VOID; layout.len()];
    for k in field_names.iter().rev() {
        let v = stack.pop().ok_or("Stack underflow")?;
        if let Some(pos) = layout.iter().position(|f| f == k) {
            values[pos] = v;
        }
    }

    let id = rt.heap.alloc(ManagedObject::Struct(Box::new(StructInstance {
        ty: ty.to_string(),
        ty_hash: xu_ir::stable_hash64(ty),
        fields: values.into_boxed_slice(),
        field_names: layout,
    })));
    stack.push(Value::struct_obj(id));
    Ok(())
}

#[inline(always)]
pub(super) fn op_list_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    let mut items: Vec<Value> = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(stack.pop().ok_or("Stack underflow")?);
    }
    items.reverse();
    let id = rt.heap.alloc(ManagedObject::List(items));
    stack.push(Value::list(id));
    Ok(())
}

#[inline(always)]
pub(super) fn op_tuple_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    if n == 0 {
        stack.push(Value::VOID);
        return Ok(());
    }
    let mut items: Vec<Value> = Vec::with_capacity(n);
    for _ in 0..n {
        items.push(stack.pop().ok_or("Stack underflow")?);
    }
    items.reverse();
    let id = rt.heap.alloc(ManagedObject::Tuple(items));
    stack.push(Value::tuple(id));
    Ok(())
}

#[inline(always)]
pub(super) fn op_dict_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
    let mut map = crate::core::value::dict_with_capacity(n);
    for _ in 0..n {
        let v = stack.pop().ok_or("Stack underflow")?;
        let k = stack.pop().ok_or("Stack underflow")?;
        let key = if k.get_tag() == TAG_STR {
            if let ManagedObject::Str(s) = rt.heap.get(k.as_obj_id()) {
                DictKey::from_text(s)
            } else {
                return Err("Not a string".into());
            }
        } else if k.is_int() {
            DictKey::Int(k.as_i64())
        } else {
            return Err(rt.error(xu_syntax::DiagnosticKind::DictKeyRequired));
        };
        map.map.insert(key, v);
    }
    let id = rt.heap.alloc(ManagedObject::Dict(map));
    stack.push(Value::dict(id));
    Ok(())
}

// TODO: Set type not yet implemented
// #[inline(always)]
// pub(super) fn op_set_new(rt: &mut Runtime, stack: &mut Vec<Value>, n: usize) -> Result<(), String> {
//     ...
// }

#[inline(always)]
pub(super) fn op_builder_append(rt: &mut Runtime, stack: &mut Vec<Value>) -> Result<(), String> {
    let v = stack.pop().ok_or("Stack underflow")?;
    let b = stack.pop().ok_or("Stack underflow")?;
    if b.get_tag() != TAG_BUILDER {
        return Err(rt.error(xu_syntax::DiagnosticKind::UnsupportedMethod {
            method: "builder_push".to_string(),
            ty: b.type_name().to_string(),
        }));
    }
    let id = b.as_obj_id();

    // Check common cases
    if v.is_int() {
        let mut buf = itoa::Buffer::new();
        let digits = buf.format(v.as_i64());
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str(digits);
        }
    } else if v.get_tag() == TAG_STR {
        let str_id = v.as_obj_id();
        let s_copy = if let ManagedObject::Str(s) = rt.heap.get(str_id) {
            s.as_str().to_string()
        } else {
            String::new()
        };
        if let ManagedObject::Builder(sb) = rt.heap.get_mut(id) {
            sb.push_str(&s_copy);
        }
    } else if v.is_f64() {
        let f = v.as_f64();
        if f.fract() == 0.0 {
            let mut buf = itoa::Buffer::new();
            let digits = buf.format(f as i64);
            if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                s.push_str(digits);
            }
        } else {
            let mut buf = ryu::Buffer::new();
            let digits = buf.format(f);
            if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
                s.push_str(digits);
            }
        }
    } else if v.is_bool() {
        let piece = if v.as_bool() { "true" } else { "false" };
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str(piece);
        }
    } else {
        let piece = crate::util::value_to_string(&v, &rt.heap);
        if let ManagedObject::Builder(s) = rt.heap.get_mut(id) {
            s.push_str(&piece);
        }
    }
    stack.push(b);
    Ok(())
}
