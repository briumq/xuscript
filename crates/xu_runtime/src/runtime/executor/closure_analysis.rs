use std::collections::HashSet;

use xu_ir::{Expr, Stmt};

pub(super) fn needs_env_frame(stmts: &[Stmt]) -> bool {
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
            _ => {}
        }
    }
    false
}

pub(super) fn has_ident_assign(stmts: &[Stmt]) -> bool {
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
            Stmt::FuncDef(_) => {}
            _ => {}
        }
    }
    false
}

pub(super) fn params_all_slotted(stmts: &[Stmt], params: &[xu_ir::Param]) -> bool {
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
            Expr::Tuple(items) => items.iter().all(|x| check_expr(x, names)),
            Expr::Range(r) => check_expr(&r.start, names) && check_expr(&r.end, names),
            Expr::IfExpr(e) => {
                check_expr(&e.cond, names)
                    && check_expr(&e.then_expr, names)
                    && check_expr(&e.else_expr, names)
            }
            Expr::Dict(entries) => entries.iter().all(|(_, v)| check_expr(v, names)),
            Expr::StructInit(s) => s.items.iter().all(|it| match it {
                xu_ir::StructInitItem::Spread(e) => check_expr(e, names),
                xu_ir::StructInitItem::Field(_, v) => check_expr(v, names),
            }),
            Expr::Member(m) => check_expr(&m.object, names),
            Expr::Index(m) => check_expr(&m.object, names) && check_expr(&m.index, names),
            Expr::Call(c) => check_expr(&c.callee, names) && c.args.iter().all(|a| check_expr(a, names)),
            Expr::MethodCall(m) => check_expr(&m.receiver, names) && m.args.iter().all(|a| check_expr(a, names)),
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
                Stmt::Return(Some(e)) => {
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
