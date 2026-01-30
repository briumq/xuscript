use std::rc::Rc;

use xu_ir::{AssignOp, AssignStmt, BinaryOp, Expr, Stmt};

use crate::Text;
use crate::Value;
use crate::value::{DictKey, Function, UserFunction};

use super::closure_analysis::{has_ident_assign, needs_env_frame, params_all_slotted};
use super::super::appendable::Appendable;
use super::super::util::type_matches;
use super::super::{Flow, Runtime};

impl Runtime {
    pub(crate) fn exec_stmts(&mut self, stmts: &[Stmt]) -> Flow {
        for s in stmts {
            let f = self.exec_stmt(s);
            if !matches!(f, Flow::None) {
                return f;
            }
        }
        Flow::None
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Flow {
        self.stmt_count += 1;
        if self.stmt_count >= 64 {
            self.stmt_count = 0;
            self.maybe_gc();
        }
        match stmt {
            Stmt::StructDef(def) => {
                self.structs.insert(def.name.clone(), (**def).clone());
                let layout = def
                    .fields
                    .iter()
                    .map(|f| f.name.clone())
                    .collect::<Vec<_>>();
                self.struct_layouts
                    .insert(def.name.clone(), std::rc::Rc::from(layout));
                Flow::None
            }
            Stmt::EnumDef(def) => {
                self.enums.insert(def.name.clone(), def.variants.to_vec());
                Flow::None
            }
            Stmt::FuncDef(def) => {
                let mut captured_env = self.env.clone();
                if self.locals.is_active() {
                    captured_env.push();
                    for (name, value) in self.locals.current_bindings() {
                        captured_env.define(name, value);
                    }
                    if let Some(bindings) = self.current_param_bindings.as_ref() {
                        for (name, idx) in bindings {
                            if let Some(value) = self.get_local_by_index(*idx) {
                                captured_env.define(name.clone(), value);
                            }
                        }
                    }
                }
                let needs_env_frame = needs_env_frame(&def.body);
                let fast_param_indices =
                    if let Some(idxmap) = self.compiled_locals_idx.get(&def.name) {
                        let mut out: Vec<usize> = Vec::with_capacity(def.params.len());
                        let mut ok = true;
                        for p in def.params.iter() {
                            if let Some(i) = idxmap.get(p.name.as_str()).copied() {
                                out.push(i);
                            } else {
                                ok = false;
                                break;
                            }
                        }
                        if ok {
                            Some(out.into_boxed_slice())
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                let fast_locals_size = self
                    .compiled_locals_idx
                    .get(&def.name)
                    .and_then(|idxmap| idxmap.values().copied().max().map(|m| m + 1));
                let skip_local_map = !needs_env_frame
                    && !def.params.is_empty()
                    && fast_param_indices.is_some()
                    && fast_locals_size.is_some()
                    && !has_ident_assign(&def.body)
                    && params_all_slotted(&def.body, &def.params);
                let func = UserFunction {
                    def: (**def).clone(),
                    env: captured_env.freeze(),
                    needs_env_frame,
                    fast_param_indices,
                    fast_locals_size,
                    skip_local_map,
                    type_sig_ic: std::cell::Cell::new(None),
                };
                let func_val = Value::function(self.heap.alloc(
                    crate::gc::ManagedObject::Function(Function::User(Rc::new(func))),
                ));
                self.env.define(def.name.clone(), func_val);
                Flow::None
            }
            Stmt::DoesBlock(def) => {
                for f in def.funcs.iter() {
                    let s = Stmt::FuncDef(Box::new(f.clone()));
                    match self.exec_stmt(&s) {
                        Flow::None => {}
                        other => return other,
                    }
                }
                Flow::None
            }
            Stmt::Use(u) => match super::super::modules::import_path(self, &u.path) {
                Ok(module_obj) => {
                    let alias = u
                        .alias
                        .clone()
                        .unwrap_or_else(|| super::super::modules::infer_module_alias(&u.path));
                    self.env.define(alias, module_obj);
                    Flow::None
                }
                Err(e) => {
                    let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    Flow::Throw(err_val)
                }
            },
            Stmt::Assign(s) => match self.exec_assign(s) {
                Ok(()) => Flow::None,
                Err(e) => {
                    let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    Flow::Throw(err_val)
                }
            },
            Stmt::If(s) => self.exec_if_branches(s.branches.as_ref(), s.else_branch.as_ref()),
            Stmt::Match(s) => {
                let v = match self.eval_expr(&s.expr) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                        return Flow::Throw(err_val);
                    }
                };
                for (pat, body) in s.arms.iter() {
                    if let Some(bindings) = super::super::pattern::match_pattern(self, pat, &v) {
                        if self.locals.is_active() {
                            self.push_locals();
                            for (name, val) in bindings {
                                self.define_local(name, val);
                            }
                            let flow = self.exec_stmts(body);
                            self.pop_locals();
                            return flow;
                        } else {
                            self.env.push();
                            for (name, val) in bindings {
                                self.env.define(name, val);
                            }
                            let flow = self.exec_stmts(body);
                            self.env.pop();
                            return flow;
                        }
                    }
                }
                if let Some(body) = &s.else_branch {
                    self.exec_stmts(body)
                } else {
                    Flow::None
                }
            }
            Stmt::While(s) => self.exec_while_loop(&s.cond, &s.body),
            Stmt::ForEach(s) => {
                let iter = match self.eval_expr(&s.iter) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                        return Flow::Throw(err_val);
                    }
                };
                let iter_desc = match &s.iter {
                    Expr::Ident(n, _) => n.clone(),
                    Expr::Member(m) => format!("*.{}", m.field),
                    Expr::Index(_) => "*[*]".to_string(),
                    Expr::Call(_) => "*()".to_string(),
                    _ => "*".to_string(),
                };
                let use_local = self.locals.is_active();
                let mut local_idx: Option<usize> = None;
                if use_local {
                    // First try to get the index from compiled_locals_idx
                    if let Some(func_name) = self.current_func.as_deref() {
                        if let Some(idxmap) = self.compiled_locals_idx.get(func_name) {
                            if let Some(&idx) = idxmap.get(&s.var) {
                                local_idx = Some(idx);
                                // Ensure the slot exists
                                if let Some(values) = self.locals.values.last_mut() {
                                    if values.len() <= idx {
                                        values.resize(idx + 1, Value::VOID);
                                    }
                                }
                                if let Some(map) = self.locals.maps.last_mut() {
                                    map.insert(s.var.clone(), idx);
                                }
                            }
                        }
                    }
                    // Fallback to define_local if not found in compiled_locals_idx
                    if local_idx.is_none() {
                        if self.get_local(&s.var).is_none() {
                            self.define_local(s.var.clone(), Value::VOID);
                        }
                        local_idx = self.get_local_index(&s.var);
                    }
                } else {
                    self.env.define(s.var.clone(), Value::VOID);
                }

                let tag = iter.get_tag();
                if tag == crate::value::TAG_LIST {
                    let id = iter.as_obj_id();
                    let items = if let crate::gc::ManagedObject::List(list) = self.heap.get(id) {
                        list.clone()
                    } else {
                        Vec::new()
                    };
                    for item in items {
                        if use_local {
                            if let Some(idx) = local_idx {
                                let _ = self.set_local_by_index(idx, item);
                            } else {
                                let _ = self.set_local(&s.var, item);
                            }
                        } else {
                            self.env.define(s.var.clone(), item);
                        }
                        let flow = self.exec_stmts(&s.body);
                        match flow {
                            Flow::None => {}
                            Flow::Continue => continue,
                            Flow::Break => break,
                            other => return other,
                        }
                    }
                } else if tag == crate::value::TAG_DICT {
                    let id = iter.as_obj_id();
                    let raw_keys = if let crate::gc::ManagedObject::Dict(dict) = self.heap.get(id) {
                        dict.map.keys().cloned().collect::<Vec<_>>()
                    } else {
                        Vec::new()
                    };

                    let mut items = Vec::with_capacity(raw_keys.len());
                    for k in raw_keys {
                        match k {
                            DictKey::Str { data, .. } => items.push(Value::str(
                                self.heap.alloc(crate::gc::ManagedObject::Str(Text::from_str(&data))),
                            )),
                            DictKey::Int(i) => items.push(Value::from_i64(i)),
                        }
                    }
                    for item in items {
                        if use_local {
                            if let Some(idx) = local_idx {
                                let _ = self.set_local_by_index(idx, item);
                            } else {
                                let _ = self.set_local(&s.var, item);
                            }
                        } else {
                            self.env.define(s.var.clone(), item);
                        }
                        let flow = self.exec_stmts(&s.body);
                        match flow {
                            Flow::None => {}
                            Flow::Continue => continue,
                            Flow::Break => break,
                            other => return other,
                        }
                    }
                } else if tag == crate::value::TAG_RANGE {
                    let id = iter.as_obj_id();
                    let (start, end, inclusive) =
                        if let crate::gc::ManagedObject::Range(s, e, inc) = self.heap.get(id) {
                            (*s, *e, *inc)
                        } else {
                            (0, 0, false)
                        };

                    let step: i64 = if start <= end { 1 } else { -1 };
                    let mut i = start;
                    loop {
                        if !inclusive && i == end {
                            break;
                        }
                        let item = Value::from_i64(i);
                        if use_local {
                            if let Some(idx) = local_idx {
                                let _ = self.set_local_by_index(idx, item);
                            } else {
                                let _ = self.set_local(&s.var, item);
                            }
                        } else {
                            self.env.define(s.var.clone(), item);
                        }
                        let flow = self.exec_stmts(&s.body);
                        match flow {
                            Flow::None => {}
                            Flow::Continue => {}
                            Flow::Break => break,
                            other => return other,
                        }
                        if i == end {
                            break;
                        }
                        i = i.saturating_add(step);
                    }
                } else {
                    let err_msg = self.error(xu_syntax::DiagnosticKind::InvalidIteratorType {
                        expected: "list".to_string(),
                        actual: iter.type_name().to_string(),
                        iter_desc,
                    });
                    let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(err_msg.into())));
                    return Flow::Throw(err_val);
                }
                Flow::None
            }
            Stmt::Return(v) => match v {
                None => Flow::Return(Value::VOID),
                Some(e) => match self.eval_expr(e) {
                    Ok(v) => Flow::Return(v),
                    Err(e) => {
                        let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                        Flow::Throw(err_val)
                    }
                },
            },
            Stmt::Break => Flow::Break,
            Stmt::Continue => Flow::Continue,
            Stmt::Expr(e) => match self.eval_expr(e) {
                Ok(_) => Flow::None,
                Err(e) => {
                    let err_val = Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    Flow::Throw(err_val)
                }
            },
            Stmt::Block(stmts) => {
                // Execute block in a new scope
                if self.locals.is_active() {
                    self.push_locals();
                    let flow = self.exec_stmts(stmts);
                    self.pop_locals();
                    flow
                } else {
                    self.env.push();
                    let flow = self.exec_stmts(stmts);
                    self.env.pop();
                    flow
                }
            }
            Stmt::Error(_) => Flow::None,
        }
    }

    fn exec_assign(&mut self, stmt: &AssignStmt) -> Result<(), String> {
        let rhs = self.eval_expr(&stmt.value)?;
        match &stmt.target {
            Expr::Ident(name, _slot) => {
                if stmt.decl.is_some() || stmt.ty.is_some() {
                    if let Some(ty) = stmt.ty.as_ref().map(|t| t.name.as_str()) {
                        if !type_matches(ty, &rhs, &self.heap) {
                            return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                                expected: ty.to_string(),
                                actual: rhs.type_name().to_string(),
                            }));
                        }
                    }
                    let immutable = matches!(stmt.decl, Some(xu_ir::DeclKind::Let));
                    if self.locals.is_active() {
                        if !self.set_local(name, rhs.clone()) {
                            self.define_local_with_mutability(name.clone(), rhs, immutable);
                        } else if immutable {
                            self.define_local_with_mutability(name.clone(), rhs, immutable);
                        }
                    } else {
                        self.env.define_with_mutability(name.clone(), rhs, immutable);
                    }
                } else {
                    if stmt.op == AssignOp::Add {
                        if self.locals.is_active() {
                            if let Some(idx) = self.locals.get_index(name) {
                                let mut val =
                                    self.locals.take_local_by_index(idx).unwrap_or(Value::VOID);
                                val.bin_op_assign(BinaryOp::Add, rhs, &mut self.heap)?;
                                self.locals.set_by_index(idx, val);
                                return Ok(());
                            }
                        } else if let Some(mut val) = self.env.take(name) {
                            val.bin_op_assign(BinaryOp::Add, rhs, &mut self.heap)?;
                            if self.env.is_immutable(name) {
                                return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                                    "Cannot reassign immutable variable".into(),
                                )));
                            }
                            self.env.define(name.clone(), val);
                            return Ok(());
                        }
                    }

                    if let Some(cur) = self.get_local(name) {
                        if self.is_local_immutable(name) {
                            return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                                "Cannot reassign immutable variable".into(),
                            )));
                        }
                        let v = self.apply_assign_op(Some(cur), stmt.op, rhs)?;
                        let _ = self.set_local(name, v);
                    } else if self.locals.is_active() {
                        let v = self.apply_assign_op(None, stmt.op, rhs)?;
                        self.define_local(name.clone(), v);
                    } else {
                        let cur = self.env.get(name);
                        if self.config.strict_vars && cur.is_none() {
                            return Err(self.error(
                                xu_syntax::DiagnosticKind::UndefinedIdentifier(name.clone()),
                            ));
                        }
                        if self.env.is_immutable(name) {
                            return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                                "Cannot reassign immutable variable".into(),
                            )));
                        }
                        let v = self.apply_assign_op(cur, stmt.op, rhs)?;
                        let assigned = self.env.assign(name, v);
                        if !assigned {
                            self.env.define(name.clone(), v);
                        }
                    }
                }
                Ok(())
            }
            Expr::Member(m) => {
                let obj = self.eval_expr(&m.object)?;
                self.assign_member(obj, &m.field, stmt.op, rhs)
            }
            Expr::Index(m) => {
                let obj = self.eval_expr(&m.object)?;
                let idx = self.eval_expr(&m.index)?;
                self.assign_index(obj, idx, stmt.op, rhs)
            }
            _ => Err(self.error(xu_syntax::DiagnosticKind::InvalidAssignmentTarget)),
        }
    }

    fn apply_assign_op(
        &mut self,
        cur: Option<Value>,
        op: AssignOp,
        rhs: Value,
    ) -> Result<Value, String> {
        match op {
            AssignOp::Set => Ok(rhs),
            AssignOp::Add => {
                if let Some(v) = cur {
                    if v.get_tag() == crate::value::TAG_STR {
                        let mut s = if let crate::gc::ManagedObject::Str(s) =
                            self.heap.get(v.as_obj_id())
                        {
                            s.clone()
                        } else {
                            return Err("Not a string".to_string());
                        };
                        s.append_value(&rhs, &self.heap);
                        return Ok(Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(s))));
                    }
                }
                let mut v = cur.unwrap_or(Value::from_i64(0));
                v.bin_op_assign(BinaryOp::Add, rhs, &mut self.heap)?;
                Ok(v)
            }
            AssignOp::Sub => cur.unwrap_or(Value::from_i64(0)).bin_op(BinaryOp::Sub, rhs),
            AssignOp::Mul => cur.unwrap_or(Value::from_i64(0)).bin_op(BinaryOp::Mul, rhs),
            AssignOp::Div => cur.unwrap_or(Value::from_i64(0)).bin_op(BinaryOp::Div, rhs),
        }
    }

    pub(crate) fn assign_member(
        &mut self,
        obj: Value,
        field: &str,
        op: AssignOp,
        rhs: Value,
    ) -> Result<(), String> {
        if obj.get_tag() == crate::value::TAG_STRUCT {
            let id = obj.as_obj_id();
            let mut prev = None;
            let mut pos = 0;
            if let crate::gc::ManagedObject::Struct(s) = self.heap.get(id) {
                let layout = self.struct_layouts.get(&s.ty).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                pos = layout.iter().position(|f| f == field).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                })?;
                prev = Some(s.fields[pos]);
            }
            let v = self.apply_assign_op(prev, op, rhs)?;
            if let crate::gc::ManagedObject::Struct(s) = self.heap.get_mut(id) {
                s.fields[pos] = v;
            }
            Ok(())
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::InvalidMemberAccess {
                field: field.to_string(),
                ty: obj.type_name().to_string(),
            }))
        }
    }

    pub(crate) fn assign_index(
        &mut self,
        obj: Value,
        idx: Value,
        op: AssignOp,
        rhs: Value,
    ) -> Result<(), String> {
        let tag = obj.get_tag();
        if tag == crate::value::TAG_LIST {
            let id = obj.as_obj_id();
            let i = if idx.is_int() {
                idx.as_i64()
            } else if idx.is_f64() {
                idx.as_f64() as i64
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::ListIndexRequired));
            };
            let ui = i as usize;

            // Fast path for simple assignment (no need to read old value)
            if op == AssignOp::Set {
                if let crate::gc::ManagedObject::List(list) = self.heap.get_mut(id) {
                    if ui >= list.len() {
                        return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                    }
                    list[ui] = rhs;
                }
                return Ok(());
            }

            // Compound assignment needs old value
            let mut prev = None;
            if let crate::gc::ManagedObject::List(list) = self.heap.get(id) {
                if ui >= list.len() {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                prev = list.get(ui).cloned();
            }

            let v = self.apply_assign_op(prev, op, rhs)?;
            if let crate::gc::ManagedObject::List(list) = self.heap.get_mut(id) {
                list[ui] = v;
            }
            Ok(())
        } else if tag == crate::value::TAG_DICT {
            let id = obj.as_obj_id();
            let key = if idx.get_tag() == crate::value::TAG_STR {
                let s = if let crate::gc::ManagedObject::Str(s) = self.heap.get(idx.as_obj_id()) {
                    s.clone()
                } else {
                    return Err("Not a string".to_string());
                };
                DictKey::from_text(&s)
            } else if idx.is_int() {
                DictKey::Int(idx.as_i64())
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::DictKeyRequired));
            };

            // Fast path for simple assignment
            if op == AssignOp::Set {
                if let crate::gc::ManagedObject::Dict(me) = self.heap.get_mut(id) {
                    me.map.insert(key, rhs);
                    me.ver += 1;
                    self.dict_version_last = Some((id.0, me.ver));
                }
                return Ok(());
            }

            // Compound assignment needs old value
            let mut prev = None;
            if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                prev = me.map.get(&key).cloned();
            }

            let v = self.apply_assign_op(prev, op, rhs)?;

            if let crate::gc::ManagedObject::Dict(me) = self.heap.get_mut(id) {
                let prev = me.map.insert(key, v);
                if prev.as_ref() != Some(&v) {
                    me.ver += 1;
                    self.dict_version_last = Some((id.0, me.ver));
                }
            }

            Ok(())
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::InvalidIndexAccess {
                expected: "list or dict".to_string(),
                actual: obj.type_name().to_string(),
            }))
        }
    }
}
