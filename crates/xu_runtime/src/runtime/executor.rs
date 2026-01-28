use smallvec::SmallVec;
use std::collections::HashSet;
use std::rc::Rc;
use xu_ir::{AssignOp, AssignStmt, BinaryOp, Expr, Pattern, Stmt, TryStmt, UnaryOp};

use super::appendable::Appendable;
use crate::Text;
use crate::Value;
use crate::value::{
    BytecodeFunction, Dict, DictKey, Function, StructInstance, UserFunction, i64_to_text_fast,
};

use super::util::{to_i64, type_matches};
use super::{Flow, Runtime};

fn needs_env_frame(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match s {
            Stmt::FuncDef(_) => return true,
            Stmt::If(x) => {
                for (_, body) in x.branches.iter() {
                    if needs_env_frame(body) {
                        return true;
                    }
                }
                if let Some(b) = &x.else_branch {
                    if needs_env_frame(b) {
                        return true;
                    }
                }
            }
            Stmt::While(x) => {
                if needs_env_frame(&x.body) {
                    return true;
                }
            }
            Stmt::ForEach(x) => {
                if needs_env_frame(&x.body) {
                    return true;
                }
            }
            Stmt::Try(x) => {
                if needs_env_frame(&x.body) {
                    return true;
                }
                if let Some(c) = &x.catch {
                    if needs_env_frame(&c.body) {
                        return true;
                    }
                }
                if let Some(f) = &x.finally {
                    if needs_env_frame(f) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn has_ident_assign(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match s {
            Stmt::Assign(a) => {
                if matches!(&a.target, Expr::Ident(_, _)) {
                    return true;
                }
            }
            Stmt::If(x) => {
                for (_, body) in x.branches.iter() {
                    if has_ident_assign(body) {
                        return true;
                    }
                }
                if let Some(b) = &x.else_branch {
                    if has_ident_assign(b) {
                        return true;
                    }
                }
            }
            Stmt::While(x) => {
                if has_ident_assign(&x.body) {
                    return true;
                }
            }
            Stmt::ForEach(x) => {
                if has_ident_assign(&x.body) {
                    return true;
                }
            }
            Stmt::Try(x) => {
                if has_ident_assign(&x.body) {
                    return true;
                }
                if let Some(c) = &x.catch {
                    if has_ident_assign(&c.body) {
                        return true;
                    }
                }
                if let Some(f) = &x.finally {
                    if has_ident_assign(f) {
                        return true;
                    }
                }
            }
            Stmt::FuncDef(_) => {}
            _ => {}
        }
    }
    false
}

fn params_all_slotted(stmts: &[Stmt], params: &[xu_ir::Param]) -> bool {
    let mut names: HashSet<String> = HashSet::new();
    for p in params {
        names.insert(p.name.clone());
    }
    if names.is_empty() {
        return false;
    }

    fn check_expr(e: &Expr, names: &HashSet<String>) -> bool {
        match e {
            Expr::Ident(n, slot) => !(slot.get().is_none() && names.contains(n)),
            Expr::List(items) => items.iter().all(|x| check_expr(x, names)),
            Expr::Range(a, b) => check_expr(a, names) && check_expr(b, names),
            Expr::Dict(entries) => entries.iter().all(|(_, v)| check_expr(v, names)),
            Expr::StructInit(s) => s.fields.iter().all(|(_, v)| check_expr(v, names)),
            Expr::Member(m) => check_expr(&m.object, names),
            Expr::Index(m) => check_expr(&m.object, names) && check_expr(&m.index, names),
            Expr::Call(c) => {
                check_expr(&c.callee, names) && c.args.iter().all(|a| check_expr(a, names))
            }
            Expr::MethodCall(m) => {
                check_expr(&m.receiver, names) && m.args.iter().all(|a| check_expr(a, names))
            }
            Expr::Unary { expr, .. } => check_expr(expr, names),
            Expr::Binary { left, right, .. } => check_expr(left, names) && check_expr(right, names),
            _ => true,
        }
    }

    fn check_stmts(stmts: &[Stmt], names: &HashSet<String>) -> bool {
        for s in stmts {
            match s {
                Stmt::If(x) => {
                    for (cond, body) in x.branches.iter() {
                        if !check_expr(cond, names) || !check_stmts(body, names) {
                            return false;
                        }
                    }
                    if let Some(b) = &x.else_branch {
                        if !check_stmts(b, names) {
                            return false;
                        }
                    }
                }
                Stmt::While(x) => {
                    if !check_expr(&x.cond, names) || !check_stmts(&x.body, names) {
                        return false;
                    }
                }
                Stmt::ForEach(x) => {
                    if !check_expr(&x.iter, names) || !check_stmts(&x.body, names) {
                        return false;
                    }
                }
                Stmt::Try(x) => {
                    if !check_stmts(&x.body, names) {
                        return false;
                    }
                    if let Some(c) = &x.catch {
                        if !check_stmts(&c.body, names) {
                            return false;
                        }
                    }
                    if let Some(f) = &x.finally {
                        if !check_stmts(f, names) {
                            return false;
                        }
                    }
                }
                Stmt::Return(Some(e)) => {
                    if !check_expr(e, names) {
                        return false;
                    }
                }
                Stmt::Throw(e) => {
                    if !check_expr(e, names) {
                        return false;
                    }
                }
                Stmt::Assign(a) => {
                    if !check_expr(&a.target, names) || !check_expr(&a.value, names) {
                        return false;
                    }
                }
                Stmt::Expr(e) => {
                    if !check_expr(e, names) {
                        return false;
                    }
                }
                Stmt::FuncDef(_) => {}
                _ => {}
            }
        }
        true
    }

    check_stmts(stmts, &names)
}

impl Runtime {
    pub(super) fn exec_stmts(&mut self, stmts: &[Stmt]) -> Flow {
        for s in stmts {
            let f = self.exec_stmt(s);
            if !matches!(f, Flow::None) {
                return f;
            }
        }
        Flow::None
    }

    ///
    ///
    ///
    ///
    ///
    ///
    fn match_pattern(&mut self, pat: &Pattern, v: &Value) -> Option<Vec<(String, Value)>> {
        match pat {
            Pattern::Wildcard => Some(Vec::new()),
            Pattern::Bind(name) => Some(vec![(name.clone(), v.clone())]),
            Pattern::Int(i) => {
                if v.is_int() && v.as_i64() == *i {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Pattern::Float(f) => {
                if v.is_f64() && v.as_f64() == *f {
                    Some(Vec::new())
                } else if v.is_int() && (v.as_i64() as f64) == *f {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Pattern::Str(s) => {
                if v.get_tag() != crate::value::TAG_STR {
                    return None;
                }
                if let crate::gc::ManagedObject::Str(x) = self.heap.get(v.as_obj_id()) {
                    if x.as_str() == s.as_str() {
                        Some(Vec::new())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Pattern::Bool(b) => {
                if v.is_bool() && v.as_bool() == *b {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Pattern::Null => {
                if v.is_null() {
                    Some(Vec::new())
                } else {
                    None
                }
            }
            Pattern::EnumVariant { ty, variant, args } => {
                if v.get_tag() != crate::value::TAG_ENUM {
                    return None;
                }
                let payload_vals: Vec<Value> =
                    if let crate::gc::ManagedObject::Enum(ety, ev, payload) =
                        self.heap.get(v.as_obj_id())
                    {
                        if ety.as_str() != ty.as_str() || ev.as_str() != variant.as_str() {
                            return None;
                        }
                        if payload.len() != args.len() {
                            return None;
                        }
                        payload.iter().cloned().collect()
                    } else {
                        return None;
                    };

                let mut out: Vec<(String, Value)> = Vec::new();
                for (p, val) in args.iter().zip(payload_vals.iter()) {
                    let bindings = self.match_pattern(p, val)?;
                    out.extend(bindings);
                }
                Some(out)
            }
        }
    }

    ///
    ///
    ///
    ///
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
                if self.locals.is_active() {
                    let bindings = self.locals.current_bindings();
                    if !bindings.is_empty() {
                        let env = &mut self.env;
                        for (name, value) in bindings {
                            let assigned = env.assign(&name, value);
                            if !assigned {
                                env.define(name, value);
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
                    env: self.env.freeze(),
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
            Stmt::Assign(s) => match self.exec_assign(s) {
                Ok(()) => Flow::None,
                Err(e) => {
                    let err_val =
                        Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    Flow::Throw(err_val)
                }
            },
            Stmt::If(s) => {
                for (cond, body) in &s.branches {
                    match self.eval_expr(cond) {
                        Ok(v) if v.is_bool() && v.as_bool() => return self.exec_stmts(body),
                        Ok(v) if v.is_bool() && !v.as_bool() => continue,
                        Ok(v) => {
                            let err_msg =
                                self.error(xu_syntax::DiagnosticKind::InvalidConditionType(
                                    v.type_name().to_string(),
                                ));
                            let err_val = Value::str(
                                self.heap
                                    .alloc(crate::gc::ManagedObject::Str(err_msg.into())),
                            );
                            return Flow::Throw(err_val);
                        }
                        Err(e) => {
                            let err_val = Value::str(
                                self.heap.alloc(crate::gc::ManagedObject::Str(e.into())),
                            );
                            return Flow::Throw(err_val);
                        }
                    }
                }
                if let Some(body) = &s.else_branch {
                    self.exec_stmts(body)
                } else {
                    Flow::None
                }
            }
            Stmt::When(s) => {
                let v = match self.eval_expr(&s.expr) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_val =
                            Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                        return Flow::Throw(err_val);
                    }
                };
                for (pat, body) in s.arms.iter() {
                    if let Some(bindings) = self.match_pattern(pat, &v) {
                        self.env.push();
                        for (name, val) in bindings {
                            self.env.define(name, val);
                        }
                        let flow = self.exec_stmts(body);
                        self.env.pop();
                        return flow;
                    }
                }
                if let Some(body) = &s.else_branch {
                    self.exec_stmts(body)
                } else {
                    Flow::None
                }
            }
            Stmt::While(s) => {
                loop {
                    let cond = match self.eval_expr(&s.cond) {
                        Ok(v) if v.is_bool() => v.as_bool(),
                        Ok(v) => {
                            let err_msg =
                                self.error(xu_syntax::DiagnosticKind::InvalidConditionType(
                                    v.type_name().to_string(),
                                ));
                            let err_val = Value::str(
                                self.heap
                                    .alloc(crate::gc::ManagedObject::Str(err_msg.into())),
                            );
                            return Flow::Throw(err_val);
                        }
                        Err(e) => {
                            let err_val = Value::str(
                                self.heap.alloc(crate::gc::ManagedObject::Str(e.into())),
                            );
                            return Flow::Throw(err_val);
                        }
                    };
                    if !cond {
                        break;
                    }
                    match self.exec_stmts(&s.body) {
                        Flow::None => {}
                        Flow::Continue => continue,
                        Flow::Break => break,
                        other => return other,
                    }
                }
                Flow::None
            }
            Stmt::ForEach(s) => {
                let iter = match self.eval_expr(&s.iter) {
                    Ok(v) => v,
                    Err(e) => {
                        let err_val =
                            Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
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
                    if self.get_local(&s.var).is_none() {
                        self.define_local(s.var.clone(), Value::NULL);
                    }
                    local_idx = self.get_local_index(&s.var);
                } else {
                    self.env.define(s.var.clone(), Value::NULL);
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
                            DictKey::Str(s) => items.push(Value::str(
                                self.heap.alloc(crate::gc::ManagedObject::Str(s)),
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
                    let (start, end) =
                        if let crate::gc::ManagedObject::Range(s, e) = self.heap.get(id) {
                            (*s, *e)
                        } else {
                            (0, 0)
                        };

                    if start <= end {
                        for i in start..=end {
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
                                Flow::Continue => continue,
                                Flow::Break => break,
                                other => return other,
                            }
                        }
                    } else {
                        for i in (end..=start).rev() {
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
                                Flow::Continue => continue,
                                Flow::Break => break,
                                other => return other,
                            }
                        }
                    }
                } else {
                    let err_msg = self.error(xu_syntax::DiagnosticKind::InvalidIteratorType {
                        expected: "list".to_string(),
                        actual: iter.type_name().to_string(),
                        iter_desc,
                    });
                    let err_val = Value::str(
                        self.heap
                            .alloc(crate::gc::ManagedObject::Str(err_msg.into())),
                    );
                    return Flow::Throw(err_val);
                }
                Flow::None
            }
            Stmt::Try(s) => self.exec_try(s),
            Stmt::Return(v) => match v {
                None => Flow::Return(Value::NULL),
                Some(e) => match self.eval_expr(e) {
                    Ok(v) => Flow::Return(v),
                    Err(e) => {
                        let err_val =
                            Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                        Flow::Throw(err_val)
                    }
                },
            },
            Stmt::Break => Flow::Break,
            Stmt::Continue => Flow::Continue,
            Stmt::Throw(e) => match self.eval_expr(e) {
                Ok(v) => Flow::Throw(v),
                Err(e) => {
                    let err_val =
                        Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    Flow::Throw(err_val)
                }
            },
            Stmt::Expr(e) => match self.eval_expr(e) {
                Ok(_) => Flow::None,
                Err(e) => {
                    let err_val =
                        Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(e.into())));
                    Flow::Throw(err_val)
                }
            },
            Stmt::Error(_) => Flow::None,
        }
    }

    fn exec_try(&mut self, stmt: &TryStmt) -> Flow {
        let mut thrown: Option<Value> = None;
        let mut flow = self.exec_stmts(&stmt.body);

        if let Flow::Throw(v) = flow {
            thrown = Some(v);
            flow = Flow::None;
        }

        if thrown.is_some() {
            if let Some(catch) = &stmt.catch {
                self.env.push();
                if let Some(var) = &catch.var {
                    let val = thrown.clone().unwrap();
                    self.env.define(var.clone(), val.clone());
                    if self.locals.is_active() && self.get_local(var).is_some() {
                        let _ = self.set_local(var, val.clone());
                    }
                }
                let catch_flow = self.exec_stmts(&catch.body);
                self.env.pop();

                match catch_flow {
                    Flow::None => {
                        thrown = None;
                    }
                    other => {
                        thrown = None;
                        flow = other;
                    }
                }
            }
        }

        if let Some(fin) = &stmt.finally {
            let fin_flow = self.exec_stmts(fin);
            match fin_flow {
                Flow::None => {}
                other => {
                    // Finally block control flow overrides everything
                    return other;
                }
            }
        }

        if let Some(v) = thrown {
            Flow::Throw(v)
        } else {
            flow
        }
    }

    fn exec_assign(&mut self, stmt: &AssignStmt) -> Result<(), String> {
        let rhs = self.eval_expr(&stmt.value)?;
        match &stmt.target {
            Expr::Ident(name, _slot) => {
                if stmt.ty.is_some() {
                    let ty = stmt.ty.as_ref().unwrap().name.as_str();
                    if !type_matches(ty, &rhs, &self.heap) {
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatch {
                            expected: ty.to_string(),
                            actual: rhs.type_name().to_string(),
                        }));
                    }
                    let immutable = matches!(stmt.decl, Some(xu_ir::DeclKind::Let));
                    if self.locals.is_active() {
                        if !self.set_local(name, rhs.clone()) {
                            self.define_local_with_mutability(name.clone(), rhs, immutable);
                        } else if immutable {
                            // Even if already exists, enforce immutability flag
                            self.define_local_with_mutability(name.clone(), rhs, immutable);
                        }
                    } else {
                        self.env
                            .define_with_mutability(name.clone(), rhs, immutable);
                    }
                } else {
                    if stmt.op == AssignOp::Add {
                        if self.locals.is_active() {
                            if let Some(idx) = self.locals.get_index(name) {
                                let mut val =
                                    self.locals.take_local_by_index(idx).unwrap_or(Value::NULL);
                                val.bin_op_assign(BinaryOp::Add, rhs, &mut self.heap)?;
                                self.locals.set_by_index(idx, val);
                                return Ok(());
                            }
                        } else {
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
                        return Ok(Value::str(
                            self.heap.alloc(crate::gc::ManagedObject::Str(s)),
                        ));
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

    pub(super) fn assign_member(
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

    pub(super) fn assign_index(
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
                DictKey::Str(s)
            } else if idx.is_int() {
                DictKey::Int(idx.as_i64())
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::DictKeyRequired));
            };

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

    ///
    ///
    ///
    ///
    ///
    ///
    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, String> {
        match expr {
            Expr::Ident(s, slot) => {
                if self.locals.is_active() {
                    if let Some((depth, idx)) = slot.get() {
                        if depth == 0 {
                            if let Some(v) = self.get_local_by_index(idx as usize) {
                                return Ok(v);
                            }
                        }
                    } else if let Some(func_name) = self.current_func.as_deref() {
                        if let Some(idxmap) = self.compiled_locals_idx.get(func_name) {
                            if let Some(&idx) = idxmap.get(s) {
                                slot.set(Some((0, idx as u32)));
                                if let Some(v) = self.get_local_by_index(idx) {
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
                if s == "import" {
                    return Ok(Value::function(self.heap.alloc(
                        crate::gc::ManagedObject::Function(Function::Builtin(
                            super::modules::builtin_import,
                        )),
                    )));
                }
                Err(self.error(xu_syntax::DiagnosticKind::UndefinedIdentifier(s.clone())))
            }
            Expr::Int(v) => Ok(Value::from_i64(*v)),
            Expr::Float(v) => Ok(Value::from_f64(*v)),
            Expr::Bool(v) => Ok(Value::from_bool(*v)),
            Expr::Null => Ok(Value::NULL),
            Expr::Str(s) => Ok(Value::str(
                self.heap
                    .alloc(crate::gc::ManagedObject::Str(s.clone().into())),
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
                    self.heap.alloc(crate::gc::ManagedObject::Str(sb.into())),
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
                                    } else if av.get_tag() == crate::value::TAG_STR {
                                        if let crate::gc::ManagedObject::Str(s) =
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
                                            self.heap.alloc(crate::gc::ManagedObject::Str(out)),
                                        ));
                                    }
                                }
                            }
                        }
                    }
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
                Ok(Value::list(
                    self.heap.alloc(crate::gc::ManagedObject::List(v)),
                ))
            }
            Expr::Range(a, b) => {
                let a = self.eval_expr(a)?;
                let b = self.eval_expr(b)?;
                let start = to_i64(&a)?;
                let end = to_i64(&b)?;
                Ok(Value::range(
                    self.heap.alloc(crate::gc::ManagedObject::Range(start, end)),
                ))
            }
            Expr::Dict(entries) => {
                let mut map: Dict = crate::value::dict_with_capacity(entries.len());
                for (k, v) in entries {
                    map.map
                        .insert(DictKey::Str(k.clone().into()), self.eval_expr(v)?);
                }
                Ok(Value::dict(
                    self.heap.alloc(crate::gc::ManagedObject::Dict(map)),
                ))
            }
            Expr::StructInit(s) => {
                let layout = self.struct_layouts.get(&s.ty).cloned().ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                let mut values = vec![Value::NULL; layout.len()];
                for (k, v) in s.fields.iter() {
                    if let Some(pos) = layout.iter().position(|f| f == k) {
                        values[pos] = self.eval_expr(v)?;
                    }
                }
                Ok(Value::struct_obj(self.heap.alloc(
                    crate::gc::ManagedObject::Struct(StructInstance {
                        ty: s.ty.clone(),
                        ty_hash: xu_ir::stable_hash64(s.ty.as_str()),
                        fields: values.into_boxed_slice(),
                        field_names: layout.clone(),
                    }),
                )))
            }
            Expr::EnumCtor { ty, variant, args } => {
                let payload = self.eval_args(args)?;
                self.enum_new_checked(ty, variant, payload.into_boxed_slice())
            }
            Expr::Member(m) => {
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
                let recv = self.eval_expr(&m.receiver)?;
                let args = self.eval_args(&m.args)?;
                let slot_idx = if let Some(idx) = m.ic_slot.get() {
                    Some(idx)
                } else {
                    let idx = self.ic_method_slots.len();
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

    fn eval_args(&mut self, args: &[Expr]) -> Result<SmallVec<[Value; 4]>, String> {
        let mut out: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len());
        for a in args {
            out.push(self.eval_expr(a)?);
        }
        Ok(out)
    }

    pub(super) fn get_member_with_ic(
        &mut self,
        obj: Value,
        field: &str,
        slot_cell: &std::cell::Cell<Option<usize>>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::value::TAG_DICT {
            if matches!(field, "len" | "length" | "keys" | "values" | "items") {
                return self.get_member_with_ic_raw(obj, field, None);
            }

            let id = obj.as_obj_id();
            let (cur_ver, key_hash) = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id)
            {
                (me.ver, Self::hash_bytes(me.map.hasher(), field.as_bytes()))
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            self.dict_version_last = Some((id.0, cur_ver));

            // Use IC slot
            if let Some(idx) = slot_cell.get() {
                if idx < self.ic_slots.len() {
                    let c = &self.ic_slots[idx];
                    if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                        return Ok(c.value);
                    }
                }
            }

            let v = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                Self::dict_get_by_str_with_hash(me, field, key_hash)
            } else {
                None
            }
            .ok_or_else(|| {
                self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
            })?;

            // Update IC slot
            let idx = if let Some(i0) = slot_cell.get() {
                if i0 < self.ic_slots.len() {
                    i0
                } else {
                    let ix = self.ic_slots.len();
                    self.ic_slots.push(crate::runtime::ICSlot::default());
                    slot_cell.set(Some(ix));
                    ix
                }
            } else {
                let ix = self.ic_slots.len();
                self.ic_slots.push(crate::runtime::ICSlot::default());
                slot_cell.set(Some(ix));
                ix
            };
            self.ic_slots[idx] = crate::runtime::ICSlot {
                id: id.0,
                key_hash,
                ver: cur_ver,
                value: v,
                ..Default::default()
            };

            Ok(v)
        } else {
            self.get_member(obj, field)
        }
    }

    pub(super) fn get_member(&mut self, obj: Value, field: &str) -> Result<Value, String> {
        self.get_member_with_ic_raw(obj, field, None)
    }

    pub(super) fn get_member_with_ic_raw(
        &mut self,
        obj: Value,
        field: &str,
        slot_idx: Option<usize>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::value::TAG_STRUCT {
            let id = obj.as_obj_id();
            if let crate::gc::ManagedObject::Struct(s) = self.heap.get(id) {
                // IC check
                if let Some(idx) = slot_idx {
                    if idx < self.ic_slots.len() {
                        let c = &self.ic_slots[idx];
                        if c.struct_ty_hash == s.ty_hash
                            && c.key_hash == xu_ir::stable_hash64(field)
                        {
                            if let Some(offset) = c.field_offset {
                                return Ok(s.fields[offset]);
                            }
                        }
                    }
                }

                // Slow path
                let layout = self.struct_layouts.get(&s.ty).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownStruct(s.ty.clone()))
                })?;
                let pos = layout.iter().position(|f| f == field).ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                })?;

                // Update IC
                if let Some(idx) = slot_idx {
                    while self.ic_slots.len() <= idx {
                        self.ic_slots.push(crate::runtime::ICSlot::default());
                    }
                    self.ic_slots[idx] = crate::runtime::ICSlot {
                        struct_ty_hash: s.ty_hash,
                        key_hash: xu_ir::stable_hash64(field),
                        field_offset: Some(pos),
                        ..Default::default()
                    };
                }

                Ok(s.fields[pos])
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a struct".into())))
            }
        } else if tag == crate::value::TAG_ENUM && (field == "has" || field == "none") {
            let id = obj.as_obj_id();
            if let crate::gc::ManagedObject::Enum(ty, variant, _) = self.heap.get(id) {
                if ty.as_str() != "Option" {
                    return Err(
                        self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                    );
                }
                let b = if field == "has" {
                    variant.as_str() == "some"
                } else {
                    variant.as_str() == "none"
                };
                Ok(Value::from_bool(b))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not an enum".into())))
            }
        } else if tag == crate::value::TAG_LIST && (field == "len" || field == "length") {
            let id = obj.as_obj_id();
            if let crate::gc::ManagedObject::List(v) = self.heap.get(id) {
                Ok(Value::from_i64(v.len() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        } else if tag == crate::value::TAG_DICT && (field == "len" || field == "length") {
            let id = obj.as_obj_id();
            if let crate::gc::ManagedObject::Dict(v) = self.heap.get(id) {
                let mut n = v.map.len();
                n += v.prop_values.len();
                for ev in &v.elements {
                    if ev.get_tag() != crate::value::TAG_NULL {
                        n += 1;
                    }
                }
                Ok(Value::from_i64(n as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())))
            }
        } else if tag == crate::value::TAG_DICT && field == "keys" {
            let id = obj.as_obj_id();
            let keys_raw: Vec<crate::Text> =
                if let crate::gc::ManagedObject::Dict(db) = self.heap.get(id) {
                    let mut out: Vec<crate::Text> =
                        Vec::with_capacity(db.map.len() + db.prop_values.len());
                    for k in db.map.keys() {
                        match k {
                            DictKey::Str(x) => out.push(x.clone()),
                            DictKey::Int(i) => out.push(i64_to_text_fast(*i)),
                        }
                    }
                    if let Some(sid) = db.shape {
                        if let crate::gc::ManagedObject::Shape(shape) = self.heap.get(sid) {
                            for k in shape.prop_map.keys() {
                                out.push(crate::Text::from_str(k.as_str()));
                            }
                        }
                    }
                    for (i, v) in db.elements.iter().enumerate() {
                        if v.get_tag() != crate::value::TAG_NULL {
                            out.push(i64_to_text_fast(i as i64));
                        }
                    }
                    out
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
                };
            let mut keys: Vec<Value> = Vec::with_capacity(keys_raw.len());
            for s in keys_raw {
                keys.push(Value::str(
                    self.heap.alloc(crate::gc::ManagedObject::Str(s)),
                ));
            }
            Ok(Value::list(
                self.heap.alloc(crate::gc::ManagedObject::List(keys)),
            ))
        } else if tag == crate::value::TAG_DICT && field == "values" {
            let id = obj.as_obj_id();
            let values: Vec<Value> = if let crate::gc::ManagedObject::Dict(db) = self.heap.get(id) {
                let mut out: Vec<Value> = Vec::with_capacity(db.map.len() + db.prop_values.len());
                out.extend(db.map.values().cloned());
                if let Some(sid) = db.shape {
                    if let crate::gc::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        for (_, off) in shape.prop_map.iter() {
                            if let Some(v) = db.prop_values.get(*off) {
                                out.push(*v);
                            }
                        }
                    }
                }
                for v in db.elements.iter() {
                    if v.get_tag() != crate::value::TAG_NULL {
                        out.push(*v);
                    }
                }
                out
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            Ok(Value::list(
                self.heap.alloc(crate::gc::ManagedObject::List(values)),
            ))
        } else if tag == crate::value::TAG_DICT && field == "items" {
            let id = obj.as_obj_id();
            let mut entries = Vec::new();
            if let crate::gc::ManagedObject::Dict(dict_b) = self.heap.get(id) {
                for (k, val) in dict_b.map.iter() {
                    let ks = match k {
                        DictKey::Str(x) => x.clone(),
                        DictKey::Int(i) => i64_to_text_fast(*i),
                    };
                    entries.push((ks, *val));
                }
                if let Some(sid) = dict_b.shape {
                    if let crate::gc::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        for (k, off) in shape.prop_map.iter() {
                            if let Some(v) = dict_b.prop_values.get(*off) {
                                entries.push((crate::Text::from_str(k.as_str()), *v));
                            }
                        }
                    }
                }
                for (i, v) in dict_b.elements.iter().enumerate() {
                    if v.get_tag() != crate::value::TAG_NULL {
                        entries.push((i64_to_text_fast(i as i64), *v));
                    }
                }
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            }

            let mut pairs: Vec<Value> = Vec::with_capacity(entries.len());
            for (ks, val) in entries {
                let mut map = crate::value::dict_with_capacity(2);
                map.map.insert(
                    DictKey::Str("key".into()),
                    Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(ks))),
                );
                map.map.insert(DictKey::Str("value".into()), val);
                pairs.push(Value::dict(
                    self.heap.alloc(crate::gc::ManagedObject::Dict(map)),
                ));
            }
            Ok(Value::list(
                self.heap.alloc(crate::gc::ManagedObject::List(pairs)),
            ))
        } else if tag == crate::value::TAG_DICT {
            let id = obj.as_obj_id();
            let key = Text::from_str(field);

            let (cur_ver, key_hash) = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id)
            {
                (me.ver, Self::hash_bytes(me.map.hasher(), key.as_bytes()))
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            self.dict_version_last = Some((id.0, cur_ver));

            // Inline Cache lookup
            if let Some(idx) = slot_idx {
                if idx < self.ic_slots.len() {
                    let c = &self.ic_slots[idx];
                    if let Some(off) = c.field_offset {
                        if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                            if let Some(sid) = me.shape {
                                if c.id == sid.0 {
                                    return Ok(me.prop_values[off]);
                                }
                            }
                        }
                    } else {
                        if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                            return Ok(c.value);
                        }
                    }
                }
            }

            if let Some(c) = self.dict_cache_last.as_ref() {
                if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash && c.key == key {
                    return Ok(c.value);
                }
            }

            let mut out_val = None;
            let mut shape_info = (id.0, None);

            if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                if let Some(sid) = me.shape {
                    if let crate::gc::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        if let Some(&off) = shape.prop_map.get(field) {
                            out_val = Some(me.prop_values[off]);
                            shape_info = (sid.0, Some(off));
                        }
                    }
                }
                if out_val.is_none() {
                    out_val = Self::dict_get_by_str_with_hash(me, field, key_hash);
                }
            }

            let v = out_val.ok_or_else(|| {
                self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
            })?;

            self.dict_cache_last = Some(crate::runtime::DictCacheLast {
                id: id.0,
                key_hash,
                ver: cur_ver,
                key,
                value: v,
            });

            // Update IC slot
            if let Some(idx) = slot_idx {
                while self.ic_slots.len() <= idx {
                    self.ic_slots.push(crate::runtime::ICSlot::default());
                }
                self.ic_slots[idx] = crate::runtime::ICSlot {
                    id: shape_info.0,
                    key_hash,
                    ver: cur_ver,
                    value: v,
                    field_offset: shape_info.1,
                    ..Default::default()
                };
            }

            Ok(v)
        } else if tag == crate::value::TAG_MODULE {
            let id = obj.as_obj_id();
            if let crate::gc::ManagedObject::Module(m) = self.heap.get(id) {
                m.exports.map.get(field).cloned().ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::UnknownMember(field.to_string()))
                })
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a module".into())))
            }
        } else if tag == crate::value::TAG_STR && (field == "len" || field == "length") {
            if let crate::gc::ManagedObject::Str(s) = self.heap.get(obj.as_obj_id()) {
                Ok(Value::from_i64(s.chars().count() as i64))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a string".into())))
            }
        } else if tag == crate::value::TAG_FILE && field == "path" {
            let id = obj.as_obj_id();
            if let crate::gc::ManagedObject::File(h) = self.heap.get(id) {
                Ok(Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(
                    h.path.clone().into(),
                ))))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a file".into())))
            }
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::InvalidMemberAccess {
                field: field.to_string(),
                ty: obj.type_name().to_string(),
            }))
        }
    }

    pub(super) fn get_index_with_ic(
        &mut self,
        obj: Value,
        index: Value,
        slot_cell: &std::cell::Cell<Option<usize>>,
    ) -> Result<Value, String> {
        let idx = if let Some(idx) = slot_cell.get() {
            Some(idx)
        } else {
            let idx = self.ic_slots.len();
            self.ic_slots.push(crate::runtime::ICSlot::default());
            slot_cell.set(Some(idx));
            Some(idx)
        };
        self.get_index_with_ic_raw(obj, index, idx)
    }

    pub(super) fn get_index_with_ic_raw(
        &mut self,
        obj: Value,
        index: Value,
        slot_idx: Option<usize>,
    ) -> Result<Value, String> {
        let tag = obj.get_tag();
        if tag == crate::value::TAG_DICT && index.get_tag() == crate::value::TAG_STR {
            let id = obj.as_obj_id();
            let key_id = index.as_obj_id();
            let key = if let crate::gc::ManagedObject::Str(s) = self.heap.get(key_id) {
                s.clone()
            } else {
                return Err("Not a string".to_string());
            };

            let (cur_ver, key_hash) = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id)
            {
                (me.ver, Self::hash_bytes(me.map.hasher(), key.as_bytes()))
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            self.dict_version_last = Some((id.0, cur_ver));

            // Inline Cache lookup
            if let Some(idx) = slot_idx {
                if idx < self.ic_slots.len() {
                    let c = &self.ic_slots[idx];
                    if let Some(off) = c.field_offset {
                        if c.ver == cur_ver && c.key_hash == key_hash {
                            if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                                if let Some(sid) = me.shape {
                                    if c.id == sid.0 {
                                        return Ok(me.prop_values[off]);
                                    }
                                }
                            }
                        }
                    } else {
                        if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                            return Ok(c.value);
                        }
                    }
                }
            }

            // Fallback to dict_cache_last
            if let Some(c) = self.dict_cache_last.as_ref() {
                if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash && c.key == key {
                    return Ok(c.value);
                }
            }

            let mut out_val = None;
            let mut shape_info = (id.0, None);
            if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                if let Some(sid) = me.shape {
                    if let crate::gc::ManagedObject::Shape(shape) = self.heap.get(sid) {
                        if let Some(&off) = shape.prop_map.get(key.as_str()) {
                            out_val = Some(me.prop_values[off]);
                            shape_info = (sid.0, Some(off));
                        }
                    }
                }
                if out_val.is_none() {
                    out_val = Self::dict_get_by_str_with_hash(me, &key, key_hash);
                }
            }
            let v = out_val.ok_or_else(|| {
                self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string()))
            })?;

            // Update dict_cache_last
            self.dict_cache_last = Some(crate::runtime::DictCacheLast {
                id: id.0,
                key_hash,
                ver: cur_ver,
                key,
                value: v,
            });

            // Update IC slot
            if let Some(idx) = slot_idx {
                while self.ic_slots.len() <= idx {
                    self.ic_slots.push(crate::runtime::ICSlot::default());
                }
                self.ic_slots[idx] = crate::runtime::ICSlot {
                    id: shape_info.0,
                    key_hash,
                    ver: cur_ver,
                    value: v,
                    field_offset: shape_info.1,
                    ..Default::default()
                };
            }

            Ok(v)
        } else if tag == crate::value::TAG_DICT && index.is_int() {
            let id = obj.as_obj_id();
            let key = index.as_i64();

            if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                if key >= 0 && key < 1024 {
                    let ui = key as usize;
                    if ui < me.elements.len() {
                        let v = me.elements[ui];
                        if v.get_tag() != crate::value::TAG_NULL {
                            return Ok(v);
                        }
                    }
                }
            }

            let (cur_ver, key_hash) = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id)
            {
                (me.ver, Self::hash_dict_key_int(me.map.hasher(), key))
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
            };
            self.dict_version_last = Some((id.0, cur_ver));

            // Inline Cache lookup
            if let Some(idx) = slot_idx {
                if idx < self.ic_slots.len() {
                    let c = &self.ic_slots[idx];
                    if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash {
                        return Ok(c.value);
                    }
                }
            }

            // Fallback to dict_cache_int_last
            if let Some(c) = self.dict_cache_int_last.as_ref() {
                if c.id == id.0 && c.ver == cur_ver && c.key == key {
                    return Ok(c.value);
                }
            }

            let v = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                me.map.get(&crate::value::DictKey::Int(key)).cloned()
            } else {
                None
            }
            .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string())))?;

            // Update dict_cache_int_last
            self.dict_cache_int_last = Some(crate::runtime::DictCacheIntLast {
                id: id.0,
                key,
                ver: cur_ver,
                value: v,
            });

            // Update IC slot
            if let Some(idx) = slot_idx {
                while self.ic_slots.len() <= idx {
                    self.ic_slots.push(crate::runtime::ICSlot::default());
                }
                self.ic_slots[idx] = crate::runtime::ICSlot {
                    id: id.0,
                    key_hash,
                    ver: cur_ver,
                    value: v,
                    ..Default::default()
                };
            }

            Ok(v)
        } else if tag == crate::value::TAG_LIST {
            let id = obj.as_obj_id();
            let i = if index.is_int() {
                index.as_i64()
            } else if index.is_f64() {
                index.as_f64() as i64
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::ListIndexRequired));
            };
            if i < 0 {
                return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
            }
            let ui = i as usize;
            if let crate::gc::ManagedObject::List(list) = self.heap.get(id) {
                list.get(ui)
                    .cloned()
                    .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::IndexOutOfRange))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a list".into())))
            }
        } else if tag == crate::value::TAG_DICT {
            let id = obj.as_obj_id();
            if index.get_tag() == crate::value::TAG_STR {
                let key_id = index.as_obj_id();
                let key = if let crate::gc::ManagedObject::Str(s) = self.heap.get(key_id) {
                    s.clone()
                } else {
                    return Err("Not a string".to_string());
                };

                let (cur_ver, key_hash) =
                    if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                        (
                            me.ver,
                            super::Runtime::hash_bytes(me.map.hasher(), key.as_bytes()),
                        )
                    } else {
                        return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
                    };
                self.dict_version_last = Some((id.0, cur_ver));
                if let Some(c) = self.dict_cache_last.as_ref() {
                    if c.id == id.0 && c.ver == cur_ver && c.key_hash == key_hash && c.key == key {
                        return Ok(c.value);
                    }
                }
                let v = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                    super::Runtime::dict_get_by_str_with_hash(me, &key, key_hash)
                } else {
                    None
                }
                .ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string()))
                })?;
                self.dict_cache_last = Some(crate::runtime::DictCacheLast {
                    id: id.0,
                    key_hash,
                    ver: cur_ver,
                    key,
                    value: v,
                });
                Ok(v)
            } else if index.is_int() {
                let i = index.as_i64();
                let cur_ver = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                    me.ver
                } else {
                    return Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a dict".into())));
                };
                self.dict_version_last = Some((id.0, cur_ver));
                if let Some(c) = self.dict_cache_int_last.as_ref() {
                    if c.id == id.0 && c.ver == cur_ver && c.key == i {
                        return Ok(c.value);
                    }
                }
                let v = if let crate::gc::ManagedObject::Dict(me) = self.heap.get(id) {
                    me.map.get(&DictKey::Int(i)).cloned()
                } else {
                    None
                }
                .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::KeyNotFound(i.to_string())))?;
                self.dict_cache_int_last = Some(crate::runtime::DictCacheIntLast {
                    id: id.0,
                    key: i,
                    ver: cur_ver,
                    value: v,
                });
                Ok(v)
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::DictKeyRequired))
            }
        } else if tag == crate::value::TAG_MODULE {
            let id = obj.as_obj_id();
            let key = if index.get_tag() == crate::value::TAG_STR {
                if let crate::gc::ManagedObject::Str(s) = self.heap.get(index.as_obj_id()) {
                    s.clone()
                } else {
                    return Err("Not a string".to_string());
                }
            } else {
                return Err(self.error(xu_syntax::DiagnosticKind::Raw(
                    "Module index key must be text".into(),
                )));
            };
            if let crate::gc::ManagedObject::Module(m) = self.heap.get(id) {
                m.exports.map.get(key.as_str()).cloned().ok_or_else(|| {
                    self.error(xu_syntax::DiagnosticKind::KeyNotFound(key.to_string()))
                })
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::Raw("Not a module".into())))
            }
        } else if tag == crate::value::TAG_STR {
            let s_id = obj.as_obj_id();
            let s = if let crate::gc::ManagedObject::Str(s) = self.heap.get(s_id) {
                s.clone()
            } else {
                return Err("Not a string".to_string());
            };
            if index.is_int() {
                let i = index.as_i64();
                if i < 0 {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let ui = i as usize;
                let ch = s
                    .chars()
                    .nth(ui)
                    .ok_or_else(|| self.error(xu_syntax::DiagnosticKind::IndexOutOfRange))?;
                Ok(Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(
                    crate::Text::from_str(ch.to_string().as_str()),
                ))))
            } else if index.get_tag() == crate::value::TAG_RANGE {
                let r_id = index.as_obj_id();
                let (start, end) =
                    if let crate::gc::ManagedObject::Range(s, e) = self.heap.get(r_id) {
                        (*s, *e)
                    } else {
                        return Err("Not a range".to_string());
                    };

                if start < 0 || end < 0 || end < start {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let start = start as usize;
                let end = end as usize;
                let len = end - start + 1;
                let total = s.chars().count();
                if end >= total {
                    return Err(self.error(xu_syntax::DiagnosticKind::IndexOutOfRange));
                }
                let sub: String = s.chars().skip(start).take(len).collect();
                Ok(Value::str(self.heap.alloc(crate::gc::ManagedObject::Str(
                    crate::Text::from_string(sub),
                ))))
            } else {
                Err(self.error(xu_syntax::DiagnosticKind::InvalidIndexAccess {
                    expected: "int or range".to_string(),
                    actual: index.type_name().to_string(),
                }))
            }
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::InvalidIndexAccess {
                expected: "list, dict, or text".to_string(),
                actual: obj.type_name().to_string(),
            }))
        }
    }

    pub(super) fn call_function(&mut self, f: Value, args: &[Value]) -> Result<Value, String> {
        if f.get_tag() == crate::value::TAG_FUNC {
            let id = f.as_obj_id();
            let func_obj = if let crate::gc::ManagedObject::Function(f) = self.heap.get(id) {
                f.clone()
            } else {
                return Err("Not a function".to_string());
            };

            match func_obj {
                Function::Builtin(fun) => fun(self, args),
                Function::User(fun) => {
                    if fun.def.name == "main" {
                        self.main_invoked = true;
                    }
                    self.call_user_function(fun, args)
                }
                Function::Bytecode(fun) => {
                    if fun.def.name == "main" {
                        self.main_invoked = true;
                    }
                    self.call_bytecode_function(fun, args)
                }
            }
        } else {
            Err(self.error(xu_syntax::DiagnosticKind::NotCallable(
                f.type_name().to_string(),
            )))
        }
    }

    pub(super) fn call_bytecode_function(
        &mut self,
        fun: Rc<BytecodeFunction>,
        args: &[Value],
    ) -> Result<Value, String> {
        self.call_stack_depth += 1;
        if self.call_stack_depth > 100 {
            self.call_stack_depth -= 1;
            return Err(self.error(xu_syntax::DiagnosticKind::RecursionLimitExceeded));
        }
        let res = self.call_bytecode_function_impl(&fun, args);
        self.call_stack_depth -= 1;
        res
    }

    fn call_bytecode_function_impl(
        &mut self,
        fun: &BytecodeFunction,
        args: &[Value],
    ) -> Result<Value, String> {
        if !fun.needs_env_frame && fun.def.params.len() == args.len() {
            if fun.def.params.iter().all(|p| p.default.is_none()) {
                let use_type_ic = fun.def.params.iter().any(|p| p.ty.is_some());
                let mut skip_type_checks = false;
                let mut type_sig = 0u64;
                if use_type_ic {
                    type_sig = 1469598103934665603u64;
                    for v in args {
                        let mut x = v.get_tag() as u64;
                        if v.get_tag() == crate::value::TAG_STRUCT {
                            let id = v.as_obj_id();
                            if let crate::gc::ManagedObject::Struct(si) = self.heap.get(id) {
                                x ^= si.ty_hash;
                            }
                        }
                        type_sig ^= x;
                        type_sig = type_sig.wrapping_mul(1099511628211);
                    }
                    skip_type_checks = fun.type_sig_ic.get() == Some(type_sig);
                }
                if !skip_type_checks {
                    for (idx, p) in fun.def.params.iter().enumerate() {
                        if let Some(ty) = &p.ty {
                            let tn = ty.name.as_str();
                            let v = args[idx];
                            if !type_matches(tn, &v, &self.heap) {
                                return Err(self.error(
                                    xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                                        name: fun.def.name.clone(),
                                        param: p.name.clone(),
                                        expected: tn.to_string(),
                                        actual: v.type_name().to_string(),
                                    },
                                ));
                            }
                        }
                    }
                    if use_type_ic {
                        fun.type_sig_ic.set(Some(type_sig));
                    }
                }
                if let Some(res) = super::ir::run_bytecode_fast_params_only(
                    self,
                    &fun.bytecode,
                    &fun.def.params,
                    args,
                ) {
                    let v = res?;
                    if let Some(ret) = &fun.def.return_ty {
                        let tn = ret.name.as_str();
                        if !type_matches(tn, &v, &self.heap) {
                            return Err(self.error(
                                xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                                    expected: tn.to_string(),
                                    actual: v.type_name().to_string(),
                                },
                            ));
                        }
                    }
                    return Ok(v);
                }
            }
        }

        let mut call_env = self.env_pool.pop().unwrap_or_else(super::Env::new);
        call_env.reset_for_call_from(&fun.env);
        let saved_env = std::mem::replace(&mut self.env, call_env);
        let mut saved_env = Some(saved_env);
        let saved_func = self.current_func.take();
        let mut saved_param_bindings = self.current_param_bindings.take();

        if fun.needs_env_frame {
            self.env.push();
        }
        self.push_locals();
        self.current_func = Some(fun.def.name.clone());
        if fun.needs_env_frame {
            if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
                self.locals.init_from_index_map(idxmap);
            }
        }
        if fun.locals_count > 0 {
            if let Some(values) = self.locals.values.last_mut() {
                if values.len() < fun.locals_count {
                    values.resize(fun.locals_count, Value::NULL);
                }
            }
        }
        let param_indices = if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
            let indices = fun
                .def
                .params
                .iter()
                .map(|p| idxmap.get(p.name.as_str()).copied())
                .collect::<Vec<_>>();
            Some(indices)
        } else {
            // For bytecode functions compiled with the new compiler,
            // parameters are always at indices 0..params.len()
            let indices = (0..fun.def.params.len()).map(Some).collect::<Vec<_>>();
            Some(indices)
        };
        if let Some(indices) = param_indices.as_ref() {
            let mut bindings: Vec<(String, usize)> = Vec::with_capacity(indices.len());
            for (i, p) in fun.def.params.iter().enumerate() {
                if let Some(Some(idx)) = indices.get(i) {
                    bindings.push((p.name.clone(), *idx));
                }
            }
            self.current_param_bindings = Some(bindings);
        } else {
            self.current_param_bindings = None;
        }

        let use_type_ic = args.len() == fun.def.params.len()
            && fun.def.params.iter().all(|p| p.default.is_none())
            && fun.def.params.iter().any(|p| p.ty.is_some());
        let mut skip_type_checks = false;
        let mut type_sig = 0u64;
        if use_type_ic {
            type_sig = 1469598103934665603u64;
            for v in args {
                let mut x = v.get_tag() as u64;
                if v.get_tag() == crate::value::TAG_STRUCT {
                    let id = v.as_obj_id();
                    if let crate::gc::ManagedObject::Struct(si) = self.heap.get(id) {
                        x ^= si.ty_hash;
                    }
                }
                type_sig ^= x;
                type_sig = type_sig.wrapping_mul(1099511628211);
            }
            skip_type_checks = fun.type_sig_ic.get() == Some(type_sig);
        }

        for (idx, p) in fun.def.params.iter().enumerate() {
            let v = if idx < args.len() {
                args[idx]
            } else if let Some(d) = &p.default {
                match self.eval_expr(d) {
                    Ok(v) => v,
                    Err(e) => {
                        self.pop_locals();
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
                        self.env_pool.push(call_env);
                        self.current_func = saved_func;
                        self.current_param_bindings = saved_param_bindings.take();
                        return Err(e);
                    }
                }
            } else {
                Value::NULL
            };
            if !skip_type_checks {
                if let Some(ty) = &p.ty {
                    let tn = ty.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
                        self.env_pool.push(call_env);
                        self.pop_locals();
                        self.current_func = saved_func;
                        self.current_param_bindings = saved_param_bindings.take();
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                            name: fun.def.name.clone(),
                            param: p.name.clone(),
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
            }
            if let Some(indices) = param_indices.as_ref() {
                if idx < indices.len() {
                    if let Some(pidx) = indices[idx] {
                        let _ = self.locals.set_by_index(pidx, v);
                        continue;
                    }
                }
            }
            self.define_local(p.name.clone(), v);
        }
        if use_type_ic && !skip_type_checks {
            fun.type_sig_ic.set(Some(type_sig));
        }

        let exec = if !fun.needs_env_frame {
            super::ir::run_bytecode_fast(self, &fun.bytecode)
                .unwrap_or_else(|| super::ir::run_bytecode(self, &fun.bytecode))
        } else {
            super::ir::run_bytecode(self, &fun.bytecode)
        };
        let flow = match exec {
            Ok(v) => v,
            Err(e) => {
                self.pop_locals();
                let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
                self.env_pool.push(call_env);
                self.current_func = saved_func;
                self.current_param_bindings = saved_param_bindings.take();
                return Err(e);
            }
        };
        self.pop_locals();
        let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
        self.env_pool.push(call_env);
        self.current_func = saved_func;
        self.current_param_bindings = saved_param_bindings.take();

        match flow {
            Flow::Return(v) => {
                if let Some(ret) = &fun.def.return_ty {
                    let tn = ret.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        return Err(self.error(xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
                Ok(v)
            }
            Flow::None => Ok(Value::NULL),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => Err(self.error(
                xu_syntax::DiagnosticKind::UnexpectedControlFlowInFunction("break or continue"),
            )),
        }
    }

    pub(super) fn call_user_function(
        &mut self,
        fun: Rc<UserFunction>,
        args: &[Value],
    ) -> Result<Value, String> {
        self.call_stack_depth += 1;
        if self.call_stack_depth > 100 {
            self.call_stack_depth -= 1;
            return Err(self.error(xu_syntax::DiagnosticKind::RecursionLimitExceeded));
        }

        let res = self.call_user_function_impl(&fun, args);
        self.call_stack_depth -= 1;
        res
    }

    fn call_user_function_impl(
        &mut self,
        fun: &UserFunction,
        args: &[Value],
    ) -> Result<Value, String> {
        let mut call_env = self.env_pool.pop().unwrap_or_else(super::Env::new);
        call_env.reset_for_call_from(&fun.env);
        let saved_env = std::mem::replace(&mut self.env, call_env);
        let mut saved_env = Some(saved_env);
        let saved_func = self.current_func.take();

        if fun.needs_env_frame {
            self.env.push();
        }
        self.push_locals();
        self.current_func = Some(fun.def.name.clone());
        if !fun.skip_local_map {
            if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
                self.locals.init_from_index_map(idxmap);
            }
        }
        if let Some(size) = fun.fast_locals_size {
            if let Some(values) = self.locals.values.last_mut() {
                if values.len() < size {
                    values.resize(size, Value::NULL);
                }
            }
        }

        let use_type_ic = args.len() == fun.def.params.len()
            && fun.def.params.iter().all(|p| p.default.is_none())
            && fun.def.params.iter().any(|p| p.ty.is_some());
        let mut skip_type_checks = false;
        let mut type_sig = 0u64;
        if use_type_ic {
            type_sig = 1469598103934665603u64;
            for v in args {
                let mut x = v.get_tag() as u64;
                if v.get_tag() == crate::value::TAG_STRUCT {
                    let id = v.as_obj_id();
                    if let crate::gc::ManagedObject::Struct(si) = self.heap.get(id) {
                        x ^= si.ty_hash;
                    }
                }
                type_sig ^= x;
                type_sig = type_sig.wrapping_mul(1099511628211);
            }
            skip_type_checks = fun.type_sig_ic.get() == Some(type_sig);
        }

        for (idx, p) in fun.def.params.iter().enumerate() {
            let v = if idx < args.len() {
                args[idx]
            } else if let Some(d) = &p.default {
                match self.eval_expr(d) {
                    Ok(v) => v,
                    Err(e) => {
                        self.pop_locals();
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
                        self.env_pool.push(call_env);
                        self.current_func = saved_func;
                        return Err(e);
                    }
                }
            } else {
                Value::NULL
            };
            if !skip_type_checks {
                if let Some(ty) = &p.ty {
                    let tn = ty.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
                        self.env_pool.push(call_env);
                        self.pop_locals();
                        self.current_func = saved_func;
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                            name: fun.def.name.clone(),
                            param: p.name.clone(),
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
            } else if let Some(ty) = &p.ty {
                let _ = ty;
            }
            if let Some(param_idxs) = fun.fast_param_indices.as_ref() {
                if idx < param_idxs.len() {
                    self.locals.set_by_index(param_idxs[idx], v);
                    continue;
                }
            }
            self.define_local(p.name.clone(), v);
        }
        if use_type_ic && !skip_type_checks {
            fun.type_sig_ic.set(Some(type_sig));
        }

        let flow = self.exec_stmts(&fun.def.body);
        self.pop_locals();
        let call_env = std::mem::replace(&mut self.env, saved_env.take().unwrap());
        self.env_pool.push(call_env);
        self.current_func = saved_func;

        match flow {
            Flow::Return(v) => {
                if let Some(ret) = &fun.def.return_ty {
                    let tn = ret.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        return Err(self.error(xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
                Ok(v)
            }
            Flow::None => Ok(Value::NULL),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => Err(self.error(
                xu_syntax::DiagnosticKind::UnexpectedControlFlowInFunction("break or continue"),
            )),
        }
    }
}
