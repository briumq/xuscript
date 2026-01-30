use std::collections::HashMap;
use xu_syntax::{Diagnostic, DiagnosticKind, codes, Type, TypeId, TypeInterner, TokenKind};
use xu_parser::{Stmt, Expr, TypeRef, UnaryOp, BinaryOp};
use super::utils::Finder;
use super::StructMap;

pub fn analyze_types(
    module: &xu_parser::Module,
    structs: &StructMap,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
) {
    let mut interner = TypeInterner::new();
    let mut func_sigs: HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)> = HashMap::new();
    for s in &module.stmts {
        if let Stmt::FuncDef(def) = s {
            let params = def
                .params
                .iter()
                .map(|p| p.ty.as_ref().map(|t| typeref_to_typeid(&mut interner, t)))
                .collect::<Vec<_>>();
            let ret = def
                .return_ty
                .as_ref()
                .map(|t| typeref_to_typeid(&mut interner, t));
            func_sigs.insert(def.name.clone(), (params, ret));
        }
    }

    let mut type_env: Vec<HashMap<String, TypeId>> = vec![HashMap::new()];
    let fn_ty = interner.builtin_by_name("func").unwrap();
    for builtin in xu_syntax::BUILTIN_NAMES {
        type_env
            .last_mut()
            .unwrap()
            .insert(builtin.to_string(), fn_ty);
    }
    analyze_type_stmts(
        &module.stmts,
        &func_sigs,
        structs,
        &mut type_env,
        finder,
        None,
        &mut interner,
        out,
    );
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
            Stmt::StructDef(_) => {}
            Stmt::EnumDef(_) => {}
            Stmt::FuncDef(def) => {
                type_env.push(HashMap::new());
                for p in &def.params {
                    if let Some(t) = &p.ty {
                        let tid = typeref_to_typeid(interner, t);
                        type_env.last_mut().unwrap().insert(p.name.clone(), tid);
                        if let Some(d) = &p.default {
                            if let Some(actual_id) =
                                infer_type(d, func_sigs, structs, type_env, interner)
                            {
                                let expected_id = tid;
                                if type_mismatch_id(interner, expected_id, actual_id)
                                    && !empty_container_literal_ok(interner, expected_id, d)
                                {
                                    let en = interner.name(expected_id);
                                    let an = interner.name(actual_id);
                                    let primary = finder.find_name_or_next(&p.name);
                                    let msg = "Variable is defined here";
                                    let mut d = Diagnostic::error_kind(
                                        DiagnosticKind::TypeMismatch {
                                            expected: en,
                                            actual: an,
                                        },
                                        primary,
                                    )
                                    .with_code(codes::TYPE_MISMATCH);
                                    if let Some(sp) = primary {
                                        d = d.with_label(msg, sp);
                                    }
                                    out.push(d);
                                }
                            }
                        }
                    }
                }
                analyze_type_stmts(
                    &def.body,
                    func_sigs,
                    structs,
                    type_env,
                    finder,
                    def.return_ty
                        .as_ref()
                        .map(|t| typeref_to_typeid(interner, t)),
                    interner,
                    out,
                );
                type_env.pop();
            }
            Stmt::DoesBlock(def) => {
                for def in def.funcs.iter() {
                    analyze_type_stmts(
                        &[Stmt::FuncDef(Box::new(def.clone()))],
                        func_sigs,
                        structs,
                        type_env,
                        finder,
                        expected_return,
                        interner,
                        out,
                    );
                }
            }
            Stmt::Use(u) => {
                if let Some(alias) = &u.alias {
                    let any = interner.builtin_by_name("any").unwrap();
                    type_env.last_mut().unwrap().insert(alias.clone(), any);
                }
            }
            Stmt::If(s) => {
                for (cond, body) in &s.branches {
                    let _ = infer_type(cond, func_sigs, structs, type_env, interner);
                    analyze_type_stmts(
                        body,
                        func_sigs,
                        structs,
                        type_env,
                        finder,
                        expected_return,
                        interner,
                        out,
                    );
                }
                if let Some(body) = &s.else_branch {
                    analyze_type_stmts(
                        body,
                        func_sigs,
                        structs,
                        type_env,
                        finder,
                        expected_return,
                        interner,
                        out,
                    );
                }
            }
            Stmt::When(s) => {
                let _ = infer_type(&s.expr, func_sigs, structs, type_env, interner);
                for (_, body) in s.arms.iter() {
                    analyze_type_stmts(
                        body,
                        func_sigs,
                        structs,
                        type_env,
                        finder,
                        expected_return,
                        interner,
                        out,
                    );
                }
                if let Some(body) = &s.else_branch {
                    analyze_type_stmts(
                        body,
                        func_sigs,
                        structs,
                        type_env,
                        finder,
                        expected_return,
                        interner,
                        out,
                    );
                }
            }
            Stmt::While(s) => {
                let _ = infer_type(&s.cond, func_sigs, structs, type_env, interner);
                analyze_type_stmts(
                    &s.body,
                    func_sigs,
                    structs,
                    type_env,
                    finder,
                    expected_return,
                    interner,
                    out,
                );
            }
            Stmt::ForEach(s) => {
                let iter_ty = infer_type(&s.iter, func_sigs, structs, type_env, interner);
                type_env.push(HashMap::new());
                let var_ty = if let Some(id) = iter_ty {
                    match interner.get(id) {
                        Type::Range => interner.intern(Type::Int),
                        Type::List(elem) => *elem,
                        _ => interner.intern(Type::Any),
                    }
                } else {
                    interner.intern(Type::Any)
                };
                type_env.last_mut().unwrap().insert(s.var.clone(), var_ty);
                analyze_type_stmts(
                    &s.body,
                    func_sigs,
                    structs,
                    type_env,
                    finder,
                    expected_return,
                    interner,
                    out,
                );
                type_env.pop();
            }
            Stmt::Return(e) => {
                if let (Some(expected), Some(e)) = (expected_return, e) {
                    if let Some(actual) = infer_type(e, func_sigs, structs, type_env, interner) {
                        if type_mismatch_id(interner, expected, actual)
                            && !empty_container_literal_ok(interner, expected, e)
                        {
                            let en = interner.name(expected);
                            let an = interner.name(actual);
                            let d = Diagnostic::error_kind(
                                DiagnosticKind::TypeMismatch {
                                    expected: en,
                                    actual: an,
                                },
                                finder.find_kw_or_next(TokenKind::KwReturn),
                            )
                            .with_code(codes::TYPE_MISMATCH)
                            .with_help("Function return type is declared at definition");
                            out.push(d);
                        }
                    }
                }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Assign(s) => {
                if let Some(expected_id) = s.ty.as_ref().map(|t| typeref_to_typeid(interner, t)) {
                    if let Some(actual) =
                        infer_type(&s.value, func_sigs, structs, type_env, interner)
                    {
                        if type_mismatch_id(interner, expected_id, actual)
                            && !empty_container_literal_ok(interner, expected_id, &s.value)
                        {
                            let en = interner.name(expected_id);
                            let an = interner.name(actual);
                            let primary = match &s.target {
                                Expr::Ident(name, _) => finder.find_name_or_next(name),
                                _ => finder.next_significant_span(),
                            };
                            let mut d = Diagnostic::error_kind(
                                DiagnosticKind::TypeMismatch {
                                    expected: en,
                                    actual: an,
                                },
                                primary,
                            )
                            .with_code(codes::TYPE_MISMATCH);
                            if let Expr::Ident(_name, _) = &s.target {
                                let msg = "Variable is defined here";
                                if let Some(sp) = primary {
                                    d = d.with_label(msg, sp);
                                }
                            }
                            out.push(d);
                        }
                    }
                    if let Expr::Ident(name, _) = &s.target {
                        type_env
                            .last_mut()
                            .unwrap()
                            .insert(name.clone(), expected_id);
                    }
                } else if s.decl.is_some() {
                    if let Expr::Ident(name, _) = &s.target {
                        let actual = infer_type(&s.value, func_sigs, structs, type_env, interner)
                            .unwrap_or(interner.intern(Type::Any));
                        type_env.last_mut().unwrap().insert(name.clone(), actual);
                    }
                } else if let Expr::Ident(name, _) = &s.target {
                    if let Some(expected) = type_env.iter().rev().find_map(|m| m.get(name).cloned())
                    {
                        if let Some(actual) =
                            infer_type(&s.value, func_sigs, structs, type_env, interner)
                        {
                            if type_mismatch_id(interner, expected, actual) {
                                let en = interner.name(expected);
                                let an = interner.name(actual);
                                let primary = finder.find_name_or_next(&name);
                                let mut d = Diagnostic::error_kind(
                                    DiagnosticKind::TypeMismatch {
                                        expected: en,
                                        actual: an,
                                    },
                                    primary,
                                )
                                .with_code(codes::TYPE_MISMATCH);
                                let msg = "Variable is defined here";
                                if let Some(sp) = primary {
                                    d = d.with_label(msg, sp);
                                }
                                out.push(d);
                            }
                        }
                    }
                }
            }
            Stmt::Expr(e) => {
                let _ = infer_type(e, func_sigs, structs, type_env, interner);
            }
            Stmt::Error(_) => {}
            // Removed Try/Throw cases
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
        Expr::FuncLit(_) => Some(interner.intern(Type::Function)),
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
                    // v1.1 prefers explicit Options, but here we might just map implicit void to...
                    // For now, if no return, it's essentially Unit/Void.
                    // The old code returned Null. Let's return None or a Unit type.
                    // If return type is None, it means Void/Unit.
                    return *ret;
                }
                if let Some(ret_name) = xu_syntax::builtin_return_type(name) {
                    return Some(interner.parse_type_str(ret_name));
                }
            }
            infer_type(&c.callee, func_sigs, structs, type_env, interner)
        }
        Expr::MethodCall(m) => {
            let ot = infer_type(&m.receiver, func_sigs, structs, type_env, interner);
            match (ot.map(|id| interner.get(id)), m.method.as_str()) {
                (Some(Type::List(_)), "contains") => Some(interner.intern(Type::Bool)),
                (Some(Type::List(_)), "add") => None, // Unit/Void
                (Some(Type::Dict(_, _)), "contains") => Some(interner.intern(Type::Bool)),
                (Some(Type::Struct(s)), "read") if s == "file" => Some(interner.intern(Type::Text)),
                (Some(Type::Struct(s)), "close") if s == "file" => None, // Unit/Void
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
            let any = interner.builtin_by_name("any").unwrap();
            interner.list(any)
        } else if t.name == "dict" {
            let text = interner.builtin_by_name("text").unwrap();
            let any = interner.builtin_by_name("any").unwrap();
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
