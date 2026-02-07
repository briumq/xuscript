use std::collections::HashMap;
use xu_syntax::{Diagnostic, DiagnosticKind, codes, Type, TypeId, TypeInterner, TokenKind, Span};
use xu_parser::{Stmt, Expr, TypeRef, UnaryOp, BinaryOp, ReceiverType};
use super::utils::Finder;
use super::{StructMap, infer_module_alias};

// ==================== 辅助函数 ====================

/// 获取类型环境的最后一个作用域
#[inline]
fn env_last(type_env: &mut [HashMap<String, TypeId>]) -> &mut HashMap<String, TypeId> {
    type_env.last_mut().expect("type_env should not be empty")
}

/// 创建类型不匹配诊断
fn make_type_mismatch_diag(
    interner: &TypeInterner,
    expected: TypeId,
    actual: TypeId,
    primary: Option<Span>,
    label_msg: Option<&str>,
) -> Diagnostic {
    let mut d = Diagnostic::error_kind(
        DiagnosticKind::TypeMismatch { expected: interner.name(expected), actual: interner.name(actual) },
        primary,
    ).with_code(codes::TYPE_MISMATCH);
    if let (Some(msg), Some(sp)) = (label_msg, primary) {
        d = d.with_label(msg, sp);
    }
    d
}

pub fn analyze_types(
    module: &xu_parser::Module,
    structs: &StructMap,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
) {
    let mut interner = TypeInterner::new();
    let mut func_sigs: HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)> = HashMap::new();

    // 收集函数签名（包括嵌套函数）
    collect_all_func_sigs(&module.stmts, &mut func_sigs, &mut interner);

    let mut type_env: Vec<HashMap<String, TypeId>> = vec![HashMap::new()];
    let fn_ty = interner.builtin_by_name("func").expect("func type should be registered");
    for builtin in xu_syntax::BUILTIN_NAMES {
        env_last(&mut type_env).insert(builtin.to_string(), fn_ty);
    }
    analyze_type_stmts(&module.stmts, &func_sigs, structs, &mut type_env, finder, None, &mut interner, out);
}

/// 递归收集所有函数签名（包括嵌套函数）
fn collect_all_func_sigs(
    stmts: &[Stmt],
    func_sigs: &mut HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    interner: &mut TypeInterner,
) {
    for s in stmts {
        match s {
            Stmt::FuncDef(def) => {
                func_sigs.insert(def.name.clone(), collect_func_sig(interner, &def.params, &def.return_ty));
                // 递归收集嵌套函数
                collect_all_func_sigs(&def.body, func_sigs, interner);
            }
            Stmt::StructDef(def) => {
                for method in def.methods.iter() {
                    func_sigs.insert(method.name.clone(), collect_func_sig(interner, &method.params, &method.return_ty));
                    // 递归收集方法中的嵌套函数
                    collect_all_func_sigs(&method.body, func_sigs, interner);
                }
            }
            Stmt::DoesBlock(def) => {
                for f in def.funcs.iter() {
                    func_sigs.insert(f.name.clone(), collect_func_sig(interner, &f.params, &f.return_ty));
                    // 递归收集 impl 方法中的嵌套函数
                    collect_all_func_sigs(&f.body, func_sigs, interner);
                }
            }
            Stmt::If(i) => {
                for (_, body) in &i.branches {
                    collect_all_func_sigs(body, func_sigs, interner);
                }
                if let Some(b) = &i.else_branch {
                    collect_all_func_sigs(b, func_sigs, interner);
                }
            }
            Stmt::While(w) => {
                collect_all_func_sigs(&w.body, func_sigs, interner);
            }
            Stmt::ForEach(fe) => {
                collect_all_func_sigs(&fe.body, func_sigs, interner);
            }
            Stmt::Match(m) => {
                for (_, body) in m.arms.iter() {
                    collect_all_func_sigs(body, func_sigs, interner);
                }
            }
            Stmt::Block(stmts) => {
                collect_all_func_sigs(stmts, func_sigs, interner);
            }
            _ => {}
        }
    }
}

/// 收集函数签名
fn collect_func_sig(interner: &mut TypeInterner, params: &[xu_parser::Param], return_ty: &Option<TypeRef>) -> (Vec<Option<TypeId>>, Option<TypeId>) {
    let params = params.iter().map(|p| p.ty.as_ref().map(|t| typeref_to_typeid(interner, t))).collect();
    let ret = return_ty.as_ref().map(|t| typeref_to_typeid(interner, t));
    (params, ret)
}

#[allow(clippy::too_many_arguments)]
fn analyze_type_stmts(
    stmts: &[Stmt],
    func_sigs: &HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    structs: &StructMap,
    type_env: &mut Vec<HashMap<String, TypeId>>,
    finder: &mut Finder<'_>,
    expected_return: Option<TypeId>,
    interner: &mut TypeInterner,
    out: &mut Vec<Diagnostic>,
) {
    for s in stmts {
        match s {
            Stmt::StructDef(def) => {
                for method in def.methods.iter() {
                    analyze_type_stmts(&[Stmt::FuncDef(Box::new(method.clone()))], func_sigs, structs, type_env, finder, None, interner, out);
                }
            }
            Stmt::EnumDef(_) => {}
            Stmt::FuncDef(def) => {
                type_env.push(HashMap::new());
                for p in &def.params {
                    if let Some(t) = &p.ty {
                        let tid = typeref_to_typeid(interner, t);
                        env_last(type_env).insert(p.name.clone(), tid);
                        if let Some(d) = &p.default {
                            check_type_match(d, tid, func_sigs, structs, type_env, finder, interner, out, Some("Variable is defined here"));
                        }
                    }
                }
                let expected_ret = def.return_ty.as_ref().map(|t| typeref_to_typeid(interner, t));
                analyze_type_stmts(&def.body, func_sigs, structs, type_env, finder, expected_ret, interner, out);
                type_env.pop();
            }
            Stmt::DoesBlock(def) => {
                for f in def.funcs.iter() {
                    analyze_type_stmts(&[Stmt::FuncDef(Box::new(f.clone()))], func_sigs, structs, type_env, finder, None, interner, out);
                }
            }
            Stmt::Use(u) => {
                let alias = u.alias.clone().unwrap_or_else(|| infer_module_alias(&u.path));
                let any = interner.builtin_by_name("any").expect("any type should be registered");
                env_last(type_env).insert(alias, any);
            }
            Stmt::If(s) => {
                for (cond, body) in &s.branches {
                    let _ = infer_type(cond, func_sigs, structs, type_env, interner);
                    analyze_type_stmts(body, func_sigs, structs, type_env, finder, expected_return, interner, out);
                }
                if let Some(body) = &s.else_branch {
                    analyze_type_stmts(body, func_sigs, structs, type_env, finder, expected_return, interner, out);
                }
            }
            Stmt::Match(s) => {
                let _ = infer_type(&s.expr, func_sigs, structs, type_env, interner);
                for (_, body) in s.arms.iter() {
                    analyze_type_stmts(body, func_sigs, structs, type_env, finder, expected_return, interner, out);
                }
                if let Some(body) = &s.else_branch {
                    analyze_type_stmts(body, func_sigs, structs, type_env, finder, expected_return, interner, out);
                }
            }
            Stmt::While(s) => {
                let _ = infer_type(&s.cond, func_sigs, structs, type_env, interner);
                analyze_type_stmts(&s.body, func_sigs, structs, type_env, finder, expected_return, interner, out);
            }
            Stmt::ForEach(s) => {
                let iter_ty = infer_type(&s.iter, func_sigs, structs, type_env, interner);
                type_env.push(HashMap::new());
                let var_ty = iter_ty.map(|id| match interner.get(id) {
                    Type::Range => interner.intern(Type::Int),
                    Type::List(elem) => *elem,
                    _ => interner.intern(Type::Any),
                }).unwrap_or_else(|| interner.intern(Type::Any));
                env_last(type_env).insert(s.var.clone(), var_ty);
                analyze_type_stmts(&s.body, func_sigs, structs, type_env, finder, expected_return, interner, out);
                type_env.pop();
            }
            Stmt::Return(e) => {
                if let (Some(expected), Some(e)) = (expected_return, e) {
                    if let Some(actual) = infer_type(e, func_sigs, structs, type_env, interner) {
                        let any_id = interner.intern(Type::Any);
                        if actual != any_id && type_mismatch_id(interner, expected, actual) && !empty_container_literal_ok(interner, expected, e) {
                            out.push(make_type_mismatch_diag(interner, expected, actual, finder.find_kw_or_next(TokenKind::KwReturn), None)
                                .with_help("Function return type is declared at definition"));
                        }
                    }
                }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Block(stmts) => {
                type_env.push(HashMap::new());
                analyze_type_stmts(stmts, func_sigs, structs, type_env, finder, expected_return, interner, out);
                type_env.pop();
            }
            Stmt::Assign(s) => analyze_assign_stmt(s, func_sigs, structs, type_env, finder, interner, out),
            Stmt::Expr(e) => {
                let _ = infer_type(e, func_sigs, structs, type_env, interner);
                check_closure_call_args(e, func_sigs, structs, type_env, finder, interner, out);
            }
            Stmt::Error(_) => {}
        }
    }
}

/// 检查类型匹配
#[allow(clippy::too_many_arguments)]
fn check_type_match(
    expr: &Expr,
    expected: TypeId,
    func_sigs: &HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    structs: &StructMap,
    type_env: &Vec<HashMap<String, TypeId>>,
    finder: &mut Finder<'_>,
    interner: &mut TypeInterner,
    out: &mut Vec<Diagnostic>,
    label_msg: Option<&str>,
) {
    if let Some(actual) = infer_type(expr, func_sigs, structs, type_env, interner) {
        if type_mismatch_id(interner, expected, actual) && !empty_container_literal_ok(interner, expected, expr) {
            let primary = finder.next_significant_span();
            out.push(make_type_mismatch_diag(interner, expected, actual, primary, label_msg));
        }
    }
}

/// 分析赋值语句
#[allow(clippy::too_many_arguments)]
fn analyze_assign_stmt(
    s: &xu_parser::AssignStmt,
    func_sigs: &HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    structs: &StructMap,
    type_env: &mut Vec<HashMap<String, TypeId>>,
    finder: &mut Finder<'_>,
    interner: &mut TypeInterner,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(expected_id) = s.ty.as_ref().map(|t| typeref_to_typeid(interner, t)) {
        if let Some(actual) = infer_type(&s.value, func_sigs, structs, type_env, interner) {
            if type_mismatch_id(interner, expected_id, actual) && !empty_container_literal_ok(interner, expected_id, &s.value) {
                let primary = match &s.target { Expr::Ident(name, _) => finder.find_name_or_next(name), _ => finder.next_significant_span() };
                out.push(make_type_mismatch_diag(interner, expected_id, actual, primary, Some("Variable is defined here")));
            }
        }
        if let Expr::Ident(name, _) = &s.target {
            env_last(type_env).insert(name.clone(), expected_id);
        }
    } else if s.decl.is_some() {
        if let Expr::Ident(name, _) = &s.target {
            let actual = infer_type(&s.value, func_sigs, structs, type_env, interner).unwrap_or(interner.intern(Type::Any));
            env_last(type_env).insert(name.clone(), actual);
        }
    } else if let Expr::Ident(name, _) = &s.target {
        if let Some(expected) = type_env.iter().rev().find_map(|m| m.get(name).cloned()) {
            if let Some(actual) = infer_type(&s.value, func_sigs, structs, type_env, interner) {
                // Skip type check if actual is any (e.g., from lambda return)
                let any_id = interner.intern(Type::Any);
                if actual != any_id && type_mismatch_id(interner, expected, actual) {
                    let primary = finder.find_name_or_next(name);
                    out.push(make_type_mismatch_diag(interner, expected, actual, primary, Some("Variable is defined here")));
                }
            }
        }
    }
}

pub fn infer_type(
    expr: &Expr,
    func_sigs: &HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    structs: &StructMap,
    type_env: &Vec<HashMap<String, TypeId>>,
    interner: &mut TypeInterner,
) -> Option<TypeId> {
    match expr {
        Expr::Int(_) => Some(interner.intern(Type::Int)),
        Expr::Float(_) => Some(interner.intern(Type::Float)),
        Expr::Bool(_) => Some(interner.intern(Type::Bool)),
        Expr::Str(_) => Some(interner.intern(Type::Text)),
        Expr::List(items) => {
            if items.is_empty() {
                let any = interner.intern(Type::Any);
                Some(interner.list(any))
            } else {
                let mut ty = infer_type(&items[0], func_sigs, structs, type_env, interner)
                    .unwrap_or(interner.intern(Type::Any));
                for e in &items[1..] {
                    let ety = infer_type(e, func_sigs, structs, type_env, interner)
                        .unwrap_or(interner.intern(Type::Any));
                    ty = unify_types_id(interner, ty, ety);
                }
                Some(interner.list(ty))
            }
        }
        Expr::Tuple(_) => Some(interner.intern(Type::Any)),
        Expr::InterpolatedString(_) => Some(interner.intern(Type::Text)),
        Expr::IfExpr(e) => {
            let tt = infer_type(&e.then_expr, func_sigs, structs, type_env, interner);
            let et = infer_type(&e.else_expr, func_sigs, structs, type_env, interner);
            match (tt, et) {
                (Some(t), Some(e)) => Some(unify_types_id(interner, t, e)),
                (Some(t), None) => Some(t),
                (None, Some(e)) => Some(e),
                (None, None) => None,
            }
        }
        Expr::Match(m) => {
            let mut out: Option<TypeId> = None;
            for (_, e) in m.arms.iter() {
                if let Some(t) = infer_type(e, func_sigs, structs, type_env, interner) {
                    out = Some(if let Some(cur) = out {
                        unify_types_id(interner, cur, t)
                    } else {
                        t
                    });
                }
            }
            if let Some(e) = m.else_expr.as_ref() {
                if let Some(t) = infer_type(e, func_sigs, structs, type_env, interner) {
                    out = Some(if let Some(cur) = out {
                        unify_types_id(interner, cur, t)
                    } else {
                        t
                    });
                }
            }
            out
        }
        Expr::FuncLit(f) => {
            let param_tys: Box<[TypeId]> = f
                .params
                .iter()
                .map(|p| {
                    p.ty.as_ref()
                        .map(|t| typeref_to_typeid(interner, t))
                        .unwrap_or_else(|| interner.intern(Type::Any))
                })
                .collect();
            let ret_ty = f.return_ty.as_ref().map(|t| typeref_to_typeid(interner, t));
            Some(interner.intern(Type::Function(param_tys, ret_ty)))
        }
        Expr::Dict(entries) => {
            if entries.is_empty() {
                let text = interner.intern(Type::Text);
                let any = interner.intern(Type::Any);
                Some(interner.dict(text, any))
            } else {
                let mut ty = infer_type(&entries[0].1, func_sigs, structs, type_env, interner)
                    .unwrap_or(interner.intern(Type::Any));
                for (_, v) in &entries[1..] {
                    let ety = infer_type(v, func_sigs, structs, type_env, interner)
                        .unwrap_or(interner.intern(Type::Any));
                    ty = unify_types_id(interner, ty, ety);
                }
                let text = interner.intern(Type::Text);
                Some(interner.dict(text, ty))
            }
        }
        Expr::Range(_) => Some(interner.intern(Type::Range)),
        Expr::StructInit(s) => Some(interner.parse_type_str(&s.ty)),
        Expr::EnumCtor { ty, .. } => Some(interner.parse_type_str(ty)),
        Expr::Error(_) => None,
        Expr::Ident(name, _) => type_env.iter().rev().find_map(|m| m.get(name).cloned()),
        Expr::Group(e) => infer_type(e, func_sigs, structs, type_env, interner),
        Expr::Unary { op, expr } => match op {
            UnaryOp::Not => Some(interner.intern(Type::Bool)),
            UnaryOp::Neg => infer_type(expr, func_sigs, structs, type_env, interner),
        },
        Expr::Binary { op, left, right } => {
            let lt = infer_type(left, func_sigs, structs, type_env, interner);
            let rt = infer_type(right, func_sigs, structs, type_env, interner);
            match op {
                BinaryOp::Eq
                | BinaryOp::Ne
                | BinaryOp::And
                | BinaryOp::Or
                | BinaryOp::Gt
                | BinaryOp::Lt
                | BinaryOp::Ge
                | BinaryOp::Le => Some(interner.intern(Type::Bool)),
                BinaryOp::Add => {
                    let text = interner.intern(Type::Text);
                    let float = interner.intern(Type::Float);
                    let int = interner.intern(Type::Int);
                    match (lt, rt) {
                        (Some(l), Some(r)) if l == text && r == text => Some(text),
                        (Some(l), _) if l == float => Some(float),
                        (_, Some(r)) if r == float => Some(float),
                        (Some(l), Some(r)) if l == int && r == int => Some(int),
                        _ => None,
                    }
                }
                BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Mod => {
                    let float = interner.intern(Type::Float);
                    let int = interner.intern(Type::Int);
                    match (lt, rt) {
                        (Some(l), _) if l == float => Some(float),
                        (_, Some(r)) if r == float => Some(float),
                        (Some(l), Some(r)) if l == int && r == int => Some(int),
                        _ => None,
                    }
                }
                BinaryOp::Div => Some(interner.intern(Type::Float)),
            }
        }
        Expr::Member(m) => {
            // Check if this is a static field access (Type.field)
            if let Expr::Ident(type_name, _) = m.object.as_ref() {
                // Check if it's a known struct type
                if let Some(fields) = structs.get(type_name) {
                    // Check for static field with "static:" prefix
                    let static_key = format!("static:{}", m.field);
                    if let Some(field_ty) = fields.get(&static_key) {
                        return Some(interner.parse_type_str(field_ty));
                    }
                }
            }
            let ot = infer_type(&m.object, func_sigs, structs, type_env, interner);
            if let Some(tid) = ot {
                let ty_name = interner.name(tid);
                if let Some(fields) = structs.get(&ty_name) {
                    if let Some(field_ty) = fields.get(&m.field) {
                        return Some(interner.parse_type_str(field_ty));
                    }
                }
                match (interner.get(tid), m.field.as_str()) {
                    (Type::List(_) | Type::Dict(_, _) | Type::Text, "length") => {
                        return Some(interner.intern(Type::Int));
                    }
                    (Type::Dict(_, _), "keys") => {
                        let text = interner.intern(Type::Text);
                        return Some(interner.list(text));
                    }
                    (Type::Dict(_, vid), "values") => {
                        return Some(interner.list(*vid));
                    }
                    (Type::Dict(_, _), "items") => {
                        let any = interner.intern(Type::Any);
                        return Some(interner.list(any));
                    }
                    _ => {}
                }
            }
            None
        }
        Expr::Index(m) => {
            let ot = infer_type(&m.object, func_sigs, structs, type_env, interner);
            match ot.map(|id| interner.get(id)) {
                Some(Type::Text) => {
                    let it = infer_type(&m.index, func_sigs, structs, type_env, interner);
                    match it.map(|id| interner.get(id)) {
                        Some(Type::Int) | Some(Type::Range) => Some(interner.intern(Type::Text)),
                        _ => None,
                    }
                }
                Some(Type::List(elem)) => {
                    let elem = *elem;
                    let it = infer_type(&m.index, func_sigs, structs, type_env, interner);
                    match it.map(|id| interner.get(id)) {
                        Some(Type::Int) => Some(elem),
                        Some(Type::Range) => ot,
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        Expr::Call(c) => {
            if let Expr::Ident(name, _) = c.callee.as_ref() {
                // Skip type checking for cross-module static method calls
                // These have the form __static__ModuleName__TypeName__method
                if name.starts_with("__static__") && name.matches("__").count() >= 3 {
                    return None;
                }
                if let Some((params, ret)) = func_sigs.get(name) {
                    for (idx, a) in c.args.iter().enumerate() {
                        if idx >= params.len() {
                            break;
                        }
                        if let Some(expected) = params[idx] {
                            if let Some(actual) =
                                infer_type(a, func_sigs, structs, type_env, interner)
                            {
                                if type_mismatch_id(interner, expected, actual) {
                                    return *ret;
                                }
                            }
                        }
                    }
                    // Return None instead of Null if return type is not explicit?
                    // v1.1 prefers explicit Options, but here we might just map implicit unit to...
                    // For now, if no return, it's essentially Unit.
                    // The old code returned Null. Let's return None or a Unit type.
                    // If return type is None, it means Unit.
                    return *ret;
                }
                if let Some(ret_name) = xu_syntax::builtin_return_type(name) {
                    return Some(interner.parse_type_str(ret_name));
                }
            }
            // Handle cross-module static method calls: module.__static__TypeName__method(...)
            if let Expr::Member(m) = c.callee.as_ref() {
                if m.field.starts_with("__static__") {
                    // Skip type checking for cross-module static method calls
                    return None;
                }
            }
            // If callee is a function type (closure variable), use its return type if available
            let callee_ty = infer_type(&c.callee, func_sigs, structs, type_env, interner);
            if let Some(tid) = callee_ty {
                if let Type::Function(_, ret_ty) = interner.get(tid) {
                    // Return the closure's return type, or None if not specified
                    return *ret_ty;
                }
            }
            callee_ty
        }
        Expr::MethodCall(m) => {
            let ot = infer_type(&m.receiver, func_sigs, structs, type_env, interner);

            // Set receiver type hint for the compiler
            if let Some(tid) = ot {
                let recv_ty = match interner.get(tid) {
                    Type::List(_) => ReceiverType::List,
                    Type::Dict(_, _) => ReceiverType::Dict,
                    Type::Struct(_) => ReceiverType::Struct,
                    _ => ReceiverType::Other,
                };
                m.receiver_ty.set(Some(recv_ty));
            }

            match (ot.map(|id| interner.get(id)), m.method.as_str()) {
                (Some(Type::List(_)), "contains") => Some(interner.intern(Type::Bool)),
                (Some(Type::List(_)), "add") => None, // Unit
                (Some(Type::Dict(_, _)), "contains") => Some(interner.intern(Type::Bool)),
                (Some(Type::Dict(_, _)), "get") => {
                    // dict.get() returns Option[V], but we simplify to Option (struct type)
                    // since we don't have a proper Option[T] generic type in the type system
                    Some(interner.intern(Type::Struct("Option".to_string())))
                }
                (Some(Type::Struct(s)), "read") if s == "file" => Some(interner.intern(Type::Text)),
                (Some(Type::Struct(s)), "close") if s == "file" => None, // Unit
                (Some(Type::Text), "split") => {
                    let text = interner.intern(Type::Text);
                    Some(interner.list(text))
                }
                _ => None,
            }
        }
    }
}

fn unify_types_id(interner: &mut TypeInterner, a: TypeId, b: TypeId) -> TypeId {
    if a == b {
        return a;
    }
    let float = interner.intern(Type::Float);
    let int = interner.intern(Type::Int);
    if (a == float && b == int) || (a == int && b == float) {
        return float;
    }
    interner.intern(Type::Any)
}

fn empty_container_literal_ok(interner: &TypeInterner, expected: TypeId, expr: &Expr) -> bool {
    match interner.get(expected) {
        Type::List(_) => matches!(expr, Expr::List(items) if items.is_empty()),
        Type::Dict(_, _) => matches!(expr, Expr::Dict(entries) if entries.is_empty()),
        _ => false,
    }
}

fn type_mismatch_id(interner: &TypeInterner, expected: TypeId, actual: TypeId) -> bool {
    !type_compatible_id(interner, expected, actual)
}

fn type_compatible_id(interner: &TypeInterner, expected: TypeId, actual: TypeId) -> bool {
    match (interner.get(expected), interner.get(actual)) {
        (Type::Any, _) => true,
        (Type::Float, Type::Int) => true,
        (Type::List(e), Type::List(a)) => type_compatible_id(interner, *e, *a),
        (Type::Dict(ek, ev), Type::Dict(ak, av)) => {
            type_compatible_id(interner, *ek, *ak) && type_compatible_id(interner, *ev, *av)
        }
        _ => expected == actual,
    }
}

fn typeref_to_typeid(interner: &mut TypeInterner, t: &TypeRef) -> TypeId {
    if t.params.is_empty() {
        if let Some(id) = interner.builtin_by_name(&t.name) {
            id
        } else if t.name == "list" {
            let any = interner.builtin_by_name("any").expect("any type should be registered");
            interner.list(any)
        } else if t.name == "dict" {
            let text = interner.builtin_by_name("text").expect("text type should be registered");
            let any = interner.builtin_by_name("any").expect("any type should be registered");
            interner.dict(text, any)
        } else if t.name == "tuple" {
            interner.intern(Type::Any)
        } else {
            interner.intern(Type::Struct(t.name.clone()))
        }
    } else if t.name == "list" {
        let elem = typeref_to_typeid(interner, &t.params[0]);
        interner.list(elem)
    } else if t.name == "dict" {
        let k = typeref_to_typeid(interner, &t.params[0]);
        let v = typeref_to_typeid(interner, &t.params[1]);
        interner.dict(k, v)
    } else if t.name == "tuple" {
        interner.intern(Type::Any)
    } else {
        interner.intern(Type::Struct(t.name.clone()))
    }
}

/// Check closure call argument types and report mismatches.
#[allow(clippy::too_many_arguments)]
fn check_closure_call_args(
    expr: &Expr,
    func_sigs: &HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    structs: &StructMap,
    type_env: &Vec<HashMap<String, TypeId>>,
    finder: &mut Finder<'_>,
    interner: &mut TypeInterner,
    out: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Call(c) => {
            // First check nested expressions
            check_closure_call_args(&c.callee, func_sigs, structs, type_env, finder, interner, out);
            for arg in c.args.iter() {
                check_closure_call_args(arg, func_sigs, structs, type_env, finder, interner, out);
            }

            // Check if callee is a closure variable
            if let Some(callee_tid) = infer_type(&c.callee, func_sigs, structs, type_env, interner) {
                if let Type::Function(param_tys, _) = interner.get(callee_tid).clone() {
                    // Check each argument against expected parameter type
                    for (idx, arg) in c.args.iter().enumerate() {
                        if idx >= param_tys.len() {
                            break;
                        }
                        let expected = param_tys[idx];
                        // Skip if expected is Any (untyped parameter)
                        if matches!(interner.get(expected), Type::Any) {
                            continue;
                        }
                        if let Some(actual) = infer_type(arg, func_sigs, structs, type_env, interner) {
                            if type_mismatch_id(interner, expected, actual) {
                                let en = interner.name(expected);
                                let an = interner.name(actual);
                                let d = Diagnostic::error_kind(
                                    DiagnosticKind::TypeMismatch {
                                        expected: en,
                                        actual: an,
                                    },
                                    finder.next_significant_span(),
                                )
                                .with_code(codes::TYPE_MISMATCH)
                                .with_help(format!("Argument {} has wrong type", idx + 1));
                                out.push(d);
                            }
                        }
                    }
                }
            }
        }
        // Recursively check other expression types
        Expr::Binary { left, right, .. } => {
            check_closure_call_args(left, func_sigs, structs, type_env, finder, interner, out);
            check_closure_call_args(right, func_sigs, structs, type_env, finder, interner, out);
        }
        Expr::Unary { expr, .. } => {
            check_closure_call_args(expr, func_sigs, structs, type_env, finder, interner, out);
        }
        Expr::Member(m) => {
            check_closure_call_args(&m.object, func_sigs, structs, type_env, finder, interner, out);
        }
        Expr::Index(i) => {
            check_closure_call_args(&i.object, func_sigs, structs, type_env, finder, interner, out);
            check_closure_call_args(&i.index, func_sigs, structs, type_env, finder, interner, out);
        }
        Expr::MethodCall(m) => {
            check_closure_call_args(&m.receiver, func_sigs, structs, type_env, finder, interner, out);
            for arg in m.args.iter() {
                check_closure_call_args(arg, func_sigs, structs, type_env, finder, interner, out);
            }
        }
        Expr::List(items) => {
            for item in items.iter() {
                check_closure_call_args(item, func_sigs, structs, type_env, finder, interner, out);
            }
        }
        Expr::Dict(entries) => {
            for (_, v) in entries.iter() {
                check_closure_call_args(v, func_sigs, structs, type_env, finder, interner, out);
            }
        }
        Expr::IfExpr(i) => {
            check_closure_call_args(&i.cond, func_sigs, structs, type_env, finder, interner, out);
            check_closure_call_args(&i.then_expr, func_sigs, structs, type_env, finder, interner, out);
            check_closure_call_args(&i.else_expr, func_sigs, structs, type_env, finder, interner, out);
        }
        _ => {}
    }
}

pub fn type_to_string(t: &TypeRef) -> String {
    if t.params.is_empty() {
        t.name.clone()
    } else {
        let inner = t
            .params
            .iter()
            .map(type_to_string)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}[{}]", t.name, inner)
    }
}
