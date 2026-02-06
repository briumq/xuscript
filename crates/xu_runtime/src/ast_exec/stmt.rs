use std::rc::Rc;

use xu_ir::{AssignOp, AssignStmt, BinaryOp, Expr, Stmt};

use crate::errors::messages::NOT_A_STRING;
use crate::Value;
use crate::core::heap::ObjectId;
use crate::core::value::{DictKey, Function, UserFunction, ValueExt};

use super::closure::{has_ident_assign, needs_env_frame, params_all_slotted};
use crate::util::Appendable;
use crate::util::type_matches;
use crate::{Flow, Runtime};

/// 将 DictKey 转换为 Value
#[inline]
fn dict_key_to_value(_rt: &mut Runtime, k: &DictKey) -> Value {
    match k {
        DictKey::StrRef { obj_id, .. } => {
            // Directly use the ObjectId - no string copy!
            Value::str(ObjectId(*obj_id))
        }
        DictKey::Int(i) => Value::from_i64(*i),
    }
}

impl Runtime {
    /// 创建错误值
    #[inline]
    fn throw_err(&mut self, e: String) -> Flow {
        Flow::Throw(Value::str(self.heap.alloc(crate::core::heap::ManagedObject::Str(e.into()))))
    }

    /// 设置循环变量
    #[inline]
    fn set_loop_var(&mut self, var: &str, local_idx: Option<usize>, use_local: bool, value: Value) {
        if use_local {
            if let Some(idx) = local_idx { let _ = self.set_local_by_index(idx, value); }
            else { let _ = self.set_local(var, value); }
        } else { self.env.define(var.to_string(), value); }
    }

    /// 执行循环体并处理 flow
    #[inline]
    fn exec_loop_body(&mut self, body: &[Stmt]) -> Option<Flow> {
        match self.exec_stmts(body) {
            Flow::None | Flow::Continue => None,
            Flow::Break => Some(Flow::Break),
            other => Some(other),
        }
    }

    /// 在作用域中定义变量
    #[inline]
    fn define_in_scope(&mut self, name: String, val: Value) {
        if self.locals.is_active() { self.define_local(name, val); }
        else { self.env.define(name, val); }
    }

    /// 在新作用域中执行闭包
    #[inline]
    fn with_scope<F: FnOnce(&mut Self) -> Flow>(&mut self, f: F) -> Flow {
        if self.locals.is_active() {
            self.push_locals();
            let flow = f(self);
            self.pop_locals();
            flow
        } else {
            self.env.push();
            let flow = f(self);
            self.env.pop();
            flow
        }
    }

    /// 执行函数定义列表
    #[inline]
    fn exec_func_defs(&mut self, funcs: &[xu_ir::FuncDef]) -> Flow {
        for f in funcs {
            let s = Stmt::FuncDef(Box::new(f.clone()));
            if let other @ (Flow::Return(_) | Flow::Throw(_) | Flow::Break | Flow::Continue) = self.exec_stmt(&s) {
                return other;
            }
        }
        Flow::None
    }

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
        match stmt {
            Stmt::StructDef(def) => {
                self.types.structs.insert(def.name.clone(), (**def).clone());
                let layout = def.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
                self.types.struct_layouts.insert(def.name.clone(), std::rc::Rc::from(layout));

                for sf in def.static_fields.iter() {
                    let value = match self.eval_expr(&sf.default) {
                        Ok(v) => v,
                        Err(e) => return self.throw_err(e),
                    };
                    self.types.static_fields.insert((def.name.clone(), sf.name.clone()), value);
                }
                self.exec_func_defs(&def.methods)
            }
            Stmt::EnumDef(def) => {
                self.types.enums.insert(def.name.clone(), def.variants.to_vec());
                Flow::None
            }
            Stmt::FuncDef(def) => {
                let captured_env = self.env.freeze();
                let captured_env = if self.locals.is_active() {
                    let mut env = captured_env;
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
                let fast_param_indices = self.compiled_locals_idx.get(&def.name).and_then(|idxmap| {
                    let mut out = Vec::with_capacity(def.params.len());
                    for p in def.params.iter() {
                        out.push(*idxmap.get(p.name.as_str())?);
                    }
                    Some(out.into_boxed_slice())
                });
                let fast_locals_size = self.compiled_locals_idx.get(&def.name)
                    .and_then(|idxmap| idxmap.values().copied().max().map(|m| m + 1));
                let skip_local_map = !needs_env_frame
                    && !def.params.is_empty()
                    && fast_param_indices.is_some()
                    && fast_locals_size.is_some()
                    && !has_ident_assign(&def.body)
                    && params_all_slotted(&def.body, &def.params);
                let func = UserFunction {
                    def: (**def).clone(),
                    env: captured_env,
                    needs_env_frame,
                    fast_param_indices,
                    fast_locals_size,
                    skip_local_map,
                    type_sig_ic: std::cell::Cell::new(None),
                };
                let func_val = Value::function(self.heap.alloc(
                    crate::core::heap::ManagedObject::Function(Function::User(Rc::new(func))),
                ));
                self.env.define(def.name.clone(), func_val);
                Flow::None
            }
            Stmt::DoesBlock(def) => self.exec_func_defs(&def.funcs),
            Stmt::Use(u) => match crate::modules::import_path(self, &u.path) {
                Ok(module_obj) => {
                    let alias = u.alias.clone().unwrap_or_else(|| crate::modules::infer_module_alias(&u.path));
                    self.env.define(alias, module_obj);
                    Flow::None
                }
                Err(e) => self.throw_err(e),
            },
            Stmt::Assign(s) => match self.exec_assign(s) {
                Ok(()) => Flow::None,
                Err(e) => self.throw_err(e),
            },
            Stmt::If(s) => self.exec_if_branches(s.branches.as_ref(), s.else_branch.as_deref()),
            Stmt::Match(s) => {
                let v = match self.eval_expr(&s.expr) {
                    Ok(v) => v,
                    Err(e) => return self.throw_err(e),
                };
                for (pat, body) in s.arms.iter() {
                    if let Some(bindings) = crate::util::match_pattern(self, pat, &v) {
                        return self.with_scope(|rt| {
                            for (name, val) in bindings { rt.define_in_scope(name, val); }
                            rt.exec_stmts(body)
                        });
                    }
                }
                s.else_branch.as_ref().map_or(Flow::None, |body| self.exec_stmts(body))
            }
            Stmt::While(s) => self.exec_while_loop(&s.cond, &s.body),
            Stmt::ForEach(s) => {
                let iter = match self.eval_expr(&s.iter) {
                    Ok(v) => v,
                    Err(e) => return self.throw_err(e),
                };
                let iter_desc = match &s.iter {
                    Expr::Ident(n, _) => n.clone(),
                    Expr::Member(m) => format!("*.{}", m.field),
                    Expr::Index(_) => "*[*]".to_string(),
                    Expr::Call(_) => "*()".to_string(),
                    _ => "*".to_string(),
                };
                let use_local = self.locals.is_active();
                let local_idx = self.prepare_loop_var(&s.var, use_local);

                let tag = iter.get_tag();
                if tag == crate::core::value::TAG_LIST {
                    let id = iter.as_obj_id();
                    let items = if let crate::core::heap::ManagedObject::List(list) = self.heap.get(id) {
                        list.clone()
                    } else {
                        Vec::new()
                    };
                    for item in items {
                        self.set_loop_var(&s.var, local_idx, use_local, item);
                        if let Some(flow) = self.exec_loop_body(&s.body) {
                            if matches!(flow, Flow::Break) { break; }
                            return flow;
                        }
                    }
                } else if tag == crate::core::value::TAG_DICT {
                    let id = iter.as_obj_id();
                    let is_kv_loop = s.var.starts_with('(') && s.var.ends_with(')') && s.var.contains(',');
                    let is_parser_kv = s.var.starts_with("__tmp_foreach_");

                    if is_kv_loop {
                        let var_str = s.var.trim_matches(|c| c == '(' || c == ')');
                        let parts: Vec<&str> = var_str.split(',').map(|p| p.trim()).collect();
                        if parts.len() == 2 {
                            let (key_var, value_var) = (parts[0], parts[1]);
                            let pairs = self.collect_dict_pairs(id, false);
                            for (k, v) in pairs {
                                let key_val = dict_key_to_value(self, &k);
                                if use_local {
                                    if self.get_local(key_var).is_none() { self.define_local(key_var.to_string(), Value::UNIT); }
                                    if self.get_local(value_var).is_none() { self.define_local(value_var.to_string(), Value::UNIT); }
                                    let _ = self.set_local(key_var, key_val);
                                    let _ = self.set_local(value_var, v);
                                } else {
                                    self.env.define(key_var.to_string(), key_val);
                                    self.env.define(value_var.to_string(), v);
                                }
                                if let Some(flow) = self.exec_loop_body(&s.body) {
                                    if matches!(flow, Flow::Break) { break; }
                                    return flow;
                                }
                            }
                        }
                    } else if is_parser_kv {
                        let pairs = self.collect_dict_pairs(id, true);
                        for (k, v) in pairs {
                            let key_val = dict_key_to_value(self, &k);
                            let tuple = Value::tuple(self.heap.alloc(crate::core::heap::ManagedObject::Tuple(vec![key_val, v])));
                            self.set_loop_var(&s.var, local_idx, use_local, tuple);
                            match self.exec_stmts(&s.body) {
                                Flow::None | Flow::Continue => {}
                                Flow::Break => break,
                                other => return other,
                            }
                        }
                    } else {
                        let raw_keys = if let crate::core::heap::ManagedObject::Dict(dict) = self.heap.get(id) {
                            dict.map.keys().cloned().collect::<Vec<_>>()
                        } else {
                            Vec::new()
                        };
                        for k in raw_keys {
                            let item = dict_key_to_value(self, &k);
                            self.set_loop_var(&s.var, local_idx, use_local, item);
                            if let Some(flow) = self.exec_loop_body(&s.body) {
                                if matches!(flow, Flow::Break) { break; }
                                return flow;
                            }
                        }
                    }
                } else if tag == crate::core::value::TAG_RANGE {
                    let id = iter.as_obj_id();
                    let (start, end, inclusive) = if let crate::core::heap::ManagedObject::Range(s, e, inc) = self.heap.get(id) {
                        (*s, *e, *inc)
                    } else {
                        (0, 0, false)
                    };
                    let step: i64 = if start <= end { 1 } else { -1 };
                    let mut i = start;
                    loop {
                        if !inclusive && i == end { break; }
                        self.set_loop_var(&s.var, local_idx, use_local, Value::from_i64(i));
                        if let Some(flow) = self.exec_loop_body(&s.body) {
                            if matches!(flow, Flow::Break) { break; }
                            return flow;
                        }
                        if i == end { break; }
                        i = i.saturating_add(step);
                    }
                } else {
                    return self.throw_err(self.error(xu_syntax::DiagnosticKind::InvalidIteratorType {
                        expected: "list".to_string(),
                        actual: iter.type_name().to_string(),
                        iter_desc,
                    }));
                }
                Flow::None
            }
            Stmt::Return(v) => match v {
                None => Flow::Return(Value::UNIT),
                Some(e) => match self.eval_expr(e) {
                    Ok(v) => Flow::Return(v),
                    Err(e) => self.throw_err(e),
                },
            },
            Stmt::Break => Flow::Break,
            Stmt::Continue => Flow::Continue,
            Stmt::Expr(e) => match self.eval_expr(e) {
                Ok(_) => Flow::None,
                Err(e) => self.throw_err(e),
            },
            Stmt::Block(stmts) => self.with_scope(|rt| rt.exec_stmts(stmts)),
            Stmt::Error(_) => Flow::None,
        }
    }

    /// 准备循环变量，返回 local_idx
    fn prepare_loop_var(&mut self, var: &str, use_local: bool) -> Option<usize> {
        if use_local {
            if let Some(func_name) = self.current_func.as_deref() {
                if let Some(idxmap) = self.compiled_locals_idx.get(func_name) {
                    if let Some(&idx) = idxmap.get(var) {
                        if let Some(values) = self.locals.values.last_mut() {
                            if values.len() <= idx { values.resize(idx + 1, Value::UNIT); }
                        }
                        if let Some(map) = self.locals.maps.last_mut() {
                            map.insert(var.to_string(), idx);
                        }
                        return Some(idx);
                    }
                }
            }
            if self.get_local(var).is_none() {
                self.define_local(var.to_string(), Value::UNIT);
            }
            self.get_local_index(var)
        } else {
            self.env.define(var.to_string(), Value::UNIT);
            None
        }
    }

    /// 收集字典键值对
    fn collect_dict_pairs(&mut self, id: crate::core::ObjectId, include_all: bool) -> Vec<(DictKey, Value)> {
        let mut pairs = Vec::new();
        // First collect shape keys as strings (to avoid borrow issues)
        let shape_keys: Vec<(String, Value)> = if let crate::core::heap::ManagedObject::Dict(dict) = self.heap.get(id) {
            for (k, v) in dict.map.iter() {
                pairs.push((*k, *v));
            }
            if include_all {
                // Collect elements
                for (i, v) in dict.elements.iter().enumerate() {
                    if v.get_tag() != crate::core::value::TAG_UNIT {
                        pairs.push((DictKey::Int(i as i64), *v));
                    }
                }
                // Collect shape keys
                if let Some(sid) = dict.shape {
                    if let crate::core::heap::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        shape.prop_map.iter()
                            .filter_map(|(k, off)| {
                                dict.prop_values.get(*off).map(|v| (k.clone(), *v))
                            })
                            .collect()
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        // Now allocate strings for shape keys
        for (k, v) in shape_keys {
            let key = DictKey::from_str_alloc(&k, &mut self.heap);
            pairs.push((key, v));
        }
        pairs
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
                        // 如果变量不存在或者是不可变声明，都需要定义新变量
                        if !self.set_local(name, rhs) || immutable {
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
                                    self.locals.take_local_by_index(idx).unwrap_or(Value::UNIT);
                                val.bin_op_assign(BinaryOp::Add, rhs, &mut self.heap)?;
                                self.locals.set_by_index(idx, val);
                                return Ok(());
                            }
                            // Check env for captured variables
                            if let Some(mut val) = self.env.take(name) {
                                val.bin_op_assign(BinaryOp::Add, rhs, &mut self.heap)?;
                                if self.env.is_immutable(name) {
                                    return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                                        "Cannot reassign immutable variable".into(),
                                    )));
                                }
                                self.env.define(name.clone(), val);
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
                        // Check if the variable exists in env (captured from outer scope)
                        if let Some(cur) = self.env.get(name) {
                            // Variable exists in env, assign to it
                            if self.env.is_immutable(name) {
                                return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                                    "Cannot reassign immutable variable".into(),
                                )));
                            }
                            let v = self.apply_assign_op(Some(cur), stmt.op, rhs)?;
                            let assigned = self.env.assign(name, v);
                            if !assigned {
                                self.env.define(name.clone(), v);
                            }
                        } else {
                            // Variable doesn't exist anywhere, create new local
                            let v = self.apply_assign_op(None, stmt.op, rhs)?;
                            self.define_local(name.clone(), v);
                        }
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
                // Check if this is a static field assignment (Type.field = value)
                if let Expr::Ident(type_name, _) = m.object.as_ref() {
                    let key = (type_name.clone(), m.field.clone());
                    if self.types.static_fields.contains_key(&key) {
                        if stmt.op == AssignOp::Set {
                            self.types.static_fields.insert(key, rhs);
                        } else {
                            let cur = self.types.static_fields.get(&key).copied();
                            let v = self.apply_assign_op(cur, stmt.op, rhs)?;
                            self.types.static_fields.insert(key, v);
                        }
                        return Ok(());
                    }
                }
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
                    if v.get_tag() == crate::core::value::TAG_STR {
                        let mut s = if let crate::core::heap::ManagedObject::Str(s) =
                            self.heap.get(v.as_obj_id())
                        {
                            s.clone()
                        } else {
                            return Err(NOT_A_STRING.to_string());
                        };
                        s.append_value(&rhs, &self.heap);
                        return Ok(Value::str(self.heap.alloc(crate::core::heap::ManagedObject::Str(s))));
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
        if obj.get_tag() == crate::core::value::TAG_STRUCT {
            let id = obj.as_obj_id();
            let mut prev = None;
            let mut pos = 0;
            if let crate::core::heap::ManagedObject::Struct(s) = self.heap.get(id) {
                let layout = self.types.struct_layouts.get(&s.ty).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                pos = layout.iter().position(|f| f == field).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                })?;
                prev = Some(s.fields[pos]);
            }
            let v = self.apply_assign_op(prev, op, rhs)?;
            if let crate::core::heap::ManagedObject::Struct(s) = self.heap.get_mut(id) {
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
        if tag == crate::core::value::TAG_LIST {
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
                if let crate::core::heap::ManagedObject::List(list) = self.heap.get_mut(id) {
                    if ui >= list.len() {
                        return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                    }
                    list[ui] = rhs;
                }
                return Ok(());
            }

            // Compound assignment needs old value
            let mut prev = None;
            if let crate::core::heap::ManagedObject::List(list) = self.heap.get(id) {
                if ui >= list.len() {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                prev = list.get(ui).cloned();
            }

            let v = self.apply_assign_op(prev, op, rhs)?;
            if let crate::core::heap::ManagedObject::List(list) = self.heap.get_mut(id) {
                list[ui] = v;
            }
            Ok(())
        } else if tag == crate::core::value::TAG_DICT {
            let id = obj.as_obj_id();
            let key = if idx.get_tag() == crate::core::value::TAG_STR {
                let key_id = idx.as_obj_id();
                let hash = if let crate::core::heap::ManagedObject::Str(s) = self.heap.get(key_id) {
                    DictKey::hash_str(s.as_str())
                } else {
                    return Err(NOT_A_STRING.to_string());
                };
                // Use ObjectId directly - no string copy!
                DictKey::from_str_obj(key_id, hash)
            } else if idx.is_int() {
                DictKey::Int(idx.as_i64())
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::DictKeyRequired));
            };

            // Fast path for simple assignment
            if op == AssignOp::Set {
                if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get_mut(id) {
                    me.map.insert(key, rhs);
                    me.ver += 1;
                    self.caches.dict_version_last = Some((id.0, me.ver));
                }
                return Ok(());
            }

            // Compound assignment needs old value
            let mut prev = None;
            if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get(id) {
                prev = me.map.get(&key).cloned();
            }

            let v = self.apply_assign_op(prev, op, rhs)?;

            if let crate::core::heap::ManagedObject::Dict(me) = self.heap.get_mut(id) {
                let prev = me.map.insert(key, v);
                if prev.as_ref() != Some(&v) {
                    me.ver += 1;
                    self.caches.dict_version_last = Some((id.0, me.ver));
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

    pub(crate) fn exec_if_branches(
        &mut self,
        branches: &[(Expr, Box<[Stmt]>)],
        else_branch: Option<&[Stmt]>,
    ) -> Flow {
        for (cond, body) in branches {
            match self.eval_expr(cond) {
                Ok(v) if v.is_bool() && v.as_bool() => return self.exec_stmts(body.as_ref()),
                Ok(v) if v.is_bool() => continue,
                Ok(v) => return self.throw_err(self.error(xu_syntax::DiagnosticKind::InvalidConditionType(v.type_name().to_string()))),
                Err(e) => return self.throw_err(e),
            }
        }
        else_branch.map_or(Flow::None, |body| self.exec_stmts(body))
    }

    pub(crate) fn exec_while_loop(&mut self, cond: &Expr, body: &[Stmt]) -> Flow {
        loop {
            let cond_v = match self.eval_expr(cond) {
                Ok(v) if v.is_bool() => v.as_bool(),
                Ok(v) => return self.throw_err(self.error(xu_syntax::DiagnosticKind::InvalidConditionType(v.type_name().to_string()))),
                Err(e) => return self.throw_err(e),
            };
            if !cond_v { break; }
            if let Some(flow) = self.exec_loop_body(body.as_ref()) {
                if matches!(flow, Flow::Break) { break; }
                return flow;
            }
        }
        Flow::None
    }
}
