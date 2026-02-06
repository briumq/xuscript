use std::rc::Rc;

use xu_ir::{BinaryOp, Expr, UnaryOp};

use crate::Text;
use crate::Value;
use crate::core::value::{Dict, DictKey, Function, StructInstance, UserFunction, i64_to_text_fast};

use super::closure::needs_env_frame;
use crate::util::Appendable;
use crate::util::to_i64;
use crate::Runtime;

impl Runtime {
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Ident(s, slot) => {
                if self.locals.is_active() {
                    if let Some((depth, idx)) = slot.get() {
                        // Calculate the maximum valid depth for slot lookup within current function
                        // func_entry_frame_depth is the frame count after the function's frame was pushed
                        // current_frames - func_entry_frame_depth gives the number of nested frames
                        // within the function (e.g., from match/if/while statements)
                        // Adding 1 accounts for the function's own frame
                        let current_frames = self.locals.maps.len();
                        let nested_frames = current_frames.saturating_sub(self.func_entry_frame_depth);
                        // max_valid_depth includes the function's frame (depth=0) plus any nested frames
                        let max_valid_depth = nested_frames + 1;

                        if (depth as usize) < max_valid_depth {
                            if let Some(v) =
                                self.get_local_by_depth_index(depth as usize, idx as usize)
                            {
                                return Ok(v);
                            }
                        }
                        // depth >= max_valid_depth means the variable is outside current function
                        // Fall through to env lookup
                    } else if let Some(func_name) = self.current_func.as_deref() {
                        if let Some(idxmap) = self.compiled_locals_idx.get(func_name) {
                            if let Some(&idx) = idxmap.get(s) {
                                slot.set(Some((0, idx as u32)));
                                if let Some(v) = self.get_local_by_depth_index(0, idx) {
                                    return Ok(v);
                                }
                            }
                        }
                    }
                }
                if let Some(v) = self.get_local(s) {
                    return Ok(v);
                }
                if let Some(v) = self.env.get_cached(s) {
                    return Ok(v);
                }
                Err(self.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(s.clone())))
            }
            Expr::Int(v) => Ok(Value::from_i64(*v)),
            Expr::Float(v) => Ok(Value::from_f64(*v)),
            Expr::Bool(v) => Ok(Value::from_bool(*v)),
            Expr::Str(s) => Ok(Value::str(
                self.heap
                    .alloc(crate::core::heap::ManagedObject::Str(s.clone().into())),
            )),
            Expr::InterpolatedString(parts) => {
                let mut cap = 0;
                for p in parts {
                    if let Expr::Str(s) = p {
                        cap += s.len();
                    }
                }
                let mut sb = if cap > 0 {
                    String::with_capacity(cap)
                } else {
                    String::new()
                };
                for p in parts {
                    let v = self.eval_expr(p)?;
                    sb.append_value(&v, &self.heap);
                }
                Ok(Value::str(
                    self.alloc(crate::core::heap::ManagedObject::Str(sb.into())),
                ))
            }
            Expr::Group(e) => self.eval_expr(e),
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => {
                        if v.is_int() {
                            Ok(Value::from_i64(-v.as_i64()))
                        } else if v.is_f64() {
                            Ok(Value::from_f64(-v.as_f64()))
                        } else {
                            Err(self.error(xu_syntax::DiagnosticKind::InvalidUnaryOperand {
                                op: '-',
                                expected: "number".to_string(),
                            }))
                        }
                    }
                    UnaryOp::Not => {
                        if v.is_bool() {
                            Ok(Value::from_bool(!v.as_bool()))
                        } else {
                            Err(self.error(xu_syntax::DiagnosticKind::InvalidUnaryOperand {
                                op: '!',
                                expected: "?".to_string(),
                            }))
                        }
                    }
                }
            }
            Expr::Binary { op, left, right } => {
                if *op == BinaryOp::Add {
                    if let Expr::Str(prefix) = left.as_ref() {
                        if let Expr::Call(c) = right.as_ref() {
                            if let Expr::Ident(name, _) = c.callee.as_ref() {
                                if name == "to_text" && c.args.len() == 1 {
                                    let av = self.eval_expr(&c.args[0])?;
                                    let digits = if av.is_int() {
                                        Some(i64_to_text_fast(av.as_i64()))
                                    } else if av.is_f64() && av.as_f64().fract() == 0.0 {
                                        Some(i64_to_text_fast(av.as_f64() as i64))
                                    } else if av.get_tag() == crate::core::value::TAG_STR {
                                        if let crate::core::heap::ManagedObject::Str(s) =
                                            self.heap.get(av.as_obj_id())
                                        {
                                            Some(s.clone())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    };
                                    if let Some(d) = digits {
                                        let mut out = Text::from_str(prefix.as_str());
                                        out.push_str(d.as_str());
                                        return Ok(Value::str(
                                            self.alloc(crate::core::heap::ManagedObject::Str(out)),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                // Short-circuit evaluation for && and ||
                if *op == BinaryOp::And {
                    let a = self.eval_expr(left)?;
                    if !a.is_bool() {
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                            expected: "bool".to_string(),
                            actual: a.type_name().to_string(),
                        }));
                    }
                    if !a.as_bool() {
                        // Short-circuit: false && _ => false
                        return Ok(Value::from_bool(false));
                    }
                    let b = self.eval_expr(right)?;
                    if !b.is_bool() {
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                            expected: "bool".to_string(),
                            actual: b.type_name().to_string(),
                        }));
                    }
                    return Ok(Value::from_bool(b.as_bool()));
                }
                if *op == BinaryOp::Or {
                    let a = self.eval_expr(left)?;
                    if !a.is_bool() {
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                            expected: "bool".to_string(),
                            actual: a.type_name().to_string(),
                        }));
                    }
                    if a.as_bool() {
                        // Short-circuit: true || _ => true
                        return Ok(Value::from_bool(true));
                    }
                    let b = self.eval_expr(right)?;
                    if !b.is_bool() {
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                            expected: "bool".to_string(),
                            actual: b.type_name().to_string(),
                        }));
                    }
                    return Ok(Value::from_bool(b.as_bool()));
                }
                let a = self.eval_expr(left)?;
                let b = self.eval_expr(right)?;
                self.eval_binary(*op, a, b)
            }
            Expr::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for e in items {
                    v.push(self.eval_expr(e)?);
                }
                Ok(Value::list(self.alloc(crate::core::heap::ManagedObject::List(v))))
            }
            Expr::Tuple(items) => {
                if items.is_empty() {
                    return Ok(Value::UNIT);
                }
                let mut v = Vec::with_capacity(items.len());
                for e in items {
                    v.push(self.eval_expr(e)?);
                }
                Ok(Value::tuple(self.alloc(crate::core::heap::ManagedObject::Tuple(v))))
            }
            Expr::Range(r) => {
                let a = self.eval_expr(&r.start)?;
                let b = self.eval_expr(&r.end)?;
                let start = to_i64(&a)?;
                let end = to_i64(&b)?;
                Ok(Value::range(
                    self.heap
                        .alloc(crate::core::heap::ManagedObject::Range(start, end, r.inclusive)),
                ))
            }
            Expr::IfExpr(e) => {
                let cv = self.eval_expr(&e.cond)?;
                if !cv.is_bool() {
                    return Err(self.error(xu_syntax::DiagnosticKind::InvalidConditionType(
                        cv.type_name().to_string(),
                    )));
                }
                if cv.as_bool() {
                    self.eval_expr(&e.then_expr)
                } else {
                    self.eval_expr(&e.else_expr)
                }
            }
            Expr::Match(m) => {
                let v = self.eval_expr(&m.expr)?;
                for (pat, body) in m.arms.iter() {
                    if let Some(binds) = crate::util::match_pattern(self, pat, &v) {
                        if self.locals.is_active() {
                            self.push_locals();
                            for (name, value) in binds {
                                self.define_local(name, value);
                            }
                            let out = self.eval_expr(body);
                            self.pop_locals();
                            return out;
                        } else {
                            self.env.push();
                            for (name, value) in binds {
                                self.env.define(name, value);
                            }
                            let out = self.eval_expr(body);
                            self.env.pop();
                            return out;
                        }
                    }
                }
                if let Some(e) = m.else_expr.as_ref() {
                    self.eval_expr(e)
                } else {
                    Err(self.error(xu_syntax::DiagnosticKind::Raw(
                        "Non-exhaustive match expression".into(),
                    )))
                }
            }
            Expr::FuncLit(def) => {
                // First, freeze the current environment to promote attached frames to heap.
                // This ensures that both the original env and the closure share the same scope.
                let captured_env = self.env.freeze();

                // If locals are active, we need to capture them too
                let captured_env = if self.locals.is_active() {
                    let mut env = captured_env;
                    // Use push_detached so values are stored in scope.values, not stack
                    // This is important because the closure's stack will be empty when called
                    env.push_detached();
                    for (name, value) in self.locals.current_bindings() {
                        env.define(name, value);
                    }
                    if let Some(bindings) = self.current_param_bindings.as_ref() {
                        for (name, idx) in bindings {
                            if let Some(value) = self.get_local_by_index(*idx) {
                                env.define(name.clone(), value);
                            }
                        }
                    }
                    env
                } else {
                    captured_env
                };

                let needs_env_frame = needs_env_frame(&def.body);
                let skip_local_map = false;
                let func = UserFunction {
                    def: (**def).clone(),
                    env: captured_env,
                    needs_env_frame,
                    fast_param_indices: None,
                    fast_locals_size: None,
                    skip_local_map,
                    type_sig_ic: std::cell::Cell::new(None),
                };
                Ok(Value::function(self.alloc(
                    crate::core::heap::ManagedObject::Function(Function::User(Rc::new(func))),
                )))
            }
            Expr::Dict(entries) => {
                let mut map: Dict = crate::core::value::dict_with_capacity(entries.len());
                for (k, v) in entries {
                    // Allocate string key on heap
                    let key = DictKey::from_str_alloc(k, &mut self.heap);
                    map.map.insert(key, self.eval_expr(v)?);
                }
                Ok(Value::dict(self.alloc(crate::core::heap::ManagedObject::Dict(map))))
            }
            Expr::StructInit(s) => {
                let layout = self.types.struct_layouts.get(&s.ty).cloned().ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                let mut values = vec![Value::UNIT; layout.len()];
                if let Some(def) = self.types.structs.get(&s.ty) {
                    let defaults = def
                        .fields
                        .iter()
                        .map(|f| f.default.clone())
                        .collect::<Vec<_>>();
                    for (i, d) in defaults.into_iter().enumerate() {
                        if let Some(d) = d {
                            if i < values.len() {
                                values[i] = self.eval_expr(&d)?;
                            }
                        }
                    }
                }
                for item in s.items.iter() {
                    match item {
                        xu_ir::StructInitItem::Field(k, v) => {
                            if let Some(pos) = layout.iter().position(|f| f == k) {
                                values[pos] = self.eval_expr(v)?;
                            }
                        }
                        xu_ir::StructInitItem::Spread(e) => {
                            let base = self.eval_expr(e)?;
                            if base.get_tag() == crate::core::value::TAG_STRUCT {
                                let id = base.as_obj_id();
                                if let crate::core::heap::ManagedObject::Struct(si) = self.heap.get(id) {
                                    if si.ty.as_str() != s.ty.as_str() {
                                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                                            expected: s.ty.clone(),
                                            actual: si.ty.as_str().to_string(),
                                        }));
                                    }
                                    for (i, fname) in si.field_names.iter().enumerate() {
                                        if let Some(pos) = layout.iter().position(|f| f == fname) {
                                            values[pos] = si.fields[i];
                                        }
                                    }
                                }
                            } else if base.get_tag() == crate::core::value::TAG_DICT {
                                let id = base.as_obj_id();
                                if let crate::core::heap::ManagedObject::Dict(db) = self.heap.get(id) {
                                    for (pos, fname) in layout.iter().enumerate() {
                                        if let Some(sid) = db.shape {
                                            if let crate::core::heap::ManagedObject::Shape(shape) =
                                                self.heap.get(sid)
                                            {
                                                if let Some(&off) = shape.prop_map.get(fname.as_str())
                                                {
                                                    if let Some(v) = db.prop_values.get(off) {
                                                        values[pos] = *v;
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                        let hash = Self::hash_bytes(db.map.hasher(), fname.as_bytes());
                                        if let Some(v) = Self::dict_get_by_str_with_hash(
                                            db,
                                            fname.as_str(),
                                            hash,
                                        ) {
                                            values[pos] = v;
                                        }
                                    }
                                }
                            } else {
                                return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                                    "Struct spread expects struct or dict".into(),
                                )));
                            }
                        }
                    }
                }
                Ok(Value::struct_obj(self.alloc(
                    crate::core::heap::ManagedObject::Struct(Box::new(StructInstance {
                        ty: s.ty.clone(),
                        ty_hash: xu_ir::stable_hash64(s.ty.as_str()),
                        fields: values.into_boxed_slice(),
                        field_names: layout.clone(),
                    })),
                )))
            }
            Expr::EnumCtor { module, ty, variant, args } => {
                let payload = self.eval_args(args)?;
                // For cross-module enums, we just need to evaluate the module expression
                // to ensure the module is loaded. The enum type is registered globally
                // when the module is imported.
                if let Some(mod_expr) = module {
                    let mod_val = self.eval_expr(mod_expr)?;
                    if mod_val.get_tag() != crate::core::value::TAG_MODULE {
                        return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                            "Expected module".into(),
                        )));
                    }
                }
                self.enum_new_checked(ty, variant, payload.into_boxed_slice())
            }
            Expr::Member(m) => {
                // Check if this is a static field access (Type.field)
                if let Expr::Ident(type_name, _) = m.object.as_ref() {
                    // First check if it's a static field
                    let key = (type_name.clone(), m.field.clone());
                    if let Some(value) = self.types.static_fields.get(&key) {
                        return Ok(*value);
                    }
                }
                let obj = self.eval_expr(&m.object)?;
                self.get_member_with_ic(obj, &m.field, &m.ic_slot)
            }
            Expr::Index(m) => {
                let obj = self.eval_expr(&m.object)?;
                let idx = self.eval_expr(&m.index)?;
                self.get_index_with_ic(obj, idx, &m.ic_slot)
            }
            Expr::Call(c) => {
                let f = self.eval_expr(&c.callee)?;
                let args = self.eval_args(&c.args)?;
                self.call_function(f, &args)
            }
            Expr::MethodCall(m) => {
                // Check if this might be a static method call (Type.method())
                // If receiver is an Ident, try static method first before evaluating receiver
                if let Expr::Ident(type_name, _) = m.receiver.as_ref() {
                    let static_name = format!("__static__{}__{}", type_name, m.method);
                    // Try to find static method in env
                    if let Some(func) = self.env.get(&static_name) {
                        let args = self.eval_args(&m.args)?;
                        return self.call_function(func, &args);
                    }
                    // If no static method found, fall through to try as instance method
                }
                // Check for cross-module static method (module.Type.method())
                if let Expr::Member(inner_m) = m.receiver.as_ref() {
                    // Try to get module and check for static method
                    if let Ok(mod_val) = self.eval_expr(&inner_m.object) {
                        if mod_val.get_tag() == crate::core::value::TAG_MODULE {
                            let static_name = format!("__static__{}__{}", inner_m.field, m.method);
                            let id = mod_val.as_obj_id();
                            let func_opt = if let crate::core::heap::ManagedObject::Module(module) = self.heap.get(id) {
                                module.exports.map.get(&static_name).copied()
                            } else {
                                None
                            };
                            if let Some(func) = func_opt {
                                let args = self.eval_args(&m.args)?;
                                return self.call_function(func, &args);
                            }
                        }
                    }
                }
                // Fall back to instance method call
                let recv = self.eval_expr(&m.receiver)?;
                let args = self.eval_args(&m.args)?;
                let slot_idx = if let Some(idx) = m.ic_slot.get() {
                    Some(idx)
                } else {
                    let idx = self.caches.ic_method_slots.len();
                    m.ic_slot.set(Some(idx));
                    Some(idx)
                };
                self.call_method_with_ic_raw(
                    recv,
                    &m.method,
                    xu_ir::stable_hash64(&m.method),
                    &args,
                    slot_idx,
                )
            }
            _ => Err(self.error(xu_syntax::DiagnosticKind::ExpectedExpression)),
        }
    }
}
