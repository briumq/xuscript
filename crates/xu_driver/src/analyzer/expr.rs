use std::collections::HashMap;
use xu_syntax::{Diagnostic, DiagnosticKind, codes, DiagnosticsFormatter, find_best_match};
use xu_parser::{Expr, StructInitItem};
use super::utils::{Finder, collect_pattern_binds};

pub fn analyze_expr(
    expr: &mut Expr,
    funcs: &HashMap<String, (usize, usize)>,
    scope: &mut Vec<HashMap<String, usize>>,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Ident(name, slot) => {
            let mut resolved = None;
            for (depth, f) in scope.iter().rev().enumerate() {
                if let Some(&idx) = f.get(name) {
                    resolved = Some((depth as u32, idx as u32));
                    break;
                }
            }
            if let Some(r) = resolved {
                slot.set(Some(r));
            } else {
                let mut diag = Diagnostic::error_kind(
                    DiagnosticKind::UndefinedIdentifier(name.clone()),
                    finder.find_name_or_next(name),
                )
                .with_code(codes::UNDEFINED_IDENTIFIER);

                if let Some(suggested) = find_best_match(
                    name,
                    scope.iter().flat_map(|s| s.keys().map(|k| k.as_str())),
                ) {
                    diag = diag.with_suggestion(DiagnosticsFormatter::format(
                        &DiagnosticKind::DidYouMean(suggested.to_string()),
                    ));
                }
                out.push(diag);
            }
        }
        Expr::List(items) => {
            for e in items {
                analyze_expr(e, funcs, scope, finder, out);
            }
        }
        Expr::Tuple(items) => {
            for e in items.iter_mut() {
                analyze_expr(e, funcs, scope, finder, out);
            }
        }
        Expr::Range(r) => {
            analyze_expr(r.start.as_mut(), funcs, scope, finder, out);
            analyze_expr(r.end.as_mut(), funcs, scope, finder, out);
        }
        Expr::IfExpr(e) => {
            analyze_expr(e.cond.as_mut(), funcs, scope, finder, out);
            analyze_expr(e.then_expr.as_mut(), funcs, scope, finder, out);
            analyze_expr(e.else_expr.as_mut(), funcs, scope, finder, out);
        }
        Expr::Match(m) => {
            analyze_expr(m.expr.as_mut(), funcs, scope, finder, out);
            for (pat, e) in m.arms.iter_mut() {
                scope.push(HashMap::new());
                let mut binds: Vec<String> = Vec::new();
                collect_pattern_binds(pat, &mut binds);
                for name in binds {
                    let idx = scope.last().unwrap().len();
                    scope.last_mut().unwrap().insert(name, idx);
                }
                analyze_expr(e, funcs, scope, finder, out);
                scope.pop();
            }
            if let Some(e) = m.else_expr.as_mut() {
                analyze_expr(e.as_mut(), funcs, scope, finder, out);
            }
        }
        Expr::FuncLit(def) => {
            // Re-use logic from stmt analysis for local function bodies
            // Note: Since we are splitting files, we might need to handle this recursion
            // by passing a callback or exposing analyze_stmts.
            // For now, to avoid circular deps, we can implement a simplified local analyzer
            // or (better) move local stmt analysis to a shared helper if possible,
            // OR just recurse to the stmt analyzer.
            // However, Rust doesn't allow cyclic module deps easily if not carefully managed.
            // Given the structure, `analyze_stmts` will likely be in `stmt.rs`.
            // We can break the cycle by having a `Context` struct or similar,
            // but for a direct port, we might need to forward verify `analyze_stmts`.
            // actually `analyze_expr` is called by `analyze_stmts`.
            // `FuncLit` contains statements.
            // We will need to inject the stmt analyzer or put them in the same module loop.
            // For this specific refactor, let's assume we can call `super::stmt::analyze_stmts`
            // but that creates a cycle `stmt` -> `expr` -> `stmt`.
            // Rust handles `mod a; mod b;` with `use super::b` fine as long as they are in the same crate tree.

            // We will defer the body analysis call to be hooked up in `mod.rs` or via direct `super` call.
            // Here we'll use a placeholder implementation that we will fix when we link `stmt.rs`.

            scope.push(HashMap::new());
            for p in def.params.iter_mut() {
                let idx = scope.last().unwrap().len();
                scope.last_mut().unwrap().insert(p.name.clone(), idx);
                if let Some(d) = &mut p.default {
                    analyze_expr(d, funcs, scope, finder, out);
                }
            }

            // This call creates a circular dependency: expr -> stmt -> expr.
            // We will resolve this by importing `analyze_local_stmts` from `stmt` module
            // which we will define as pub(crate).
            super::stmt::analyze_local_stmts_shim(&mut def.body, funcs, scope, finder, out);

            scope.pop();
        }
        Expr::Dict(entries) => {
            for (_, v) in entries.iter_mut() {
                analyze_expr(v, funcs, scope, finder, out);
            }
        }
        Expr::StructInit(s) => {
            if let Some(mod_expr) = &mut s.module {
                analyze_expr(mod_expr, funcs, scope, finder, out);
            }
            for item in s.items.iter_mut() {
                match item {
                    StructInitItem::Spread(e) => {
                        analyze_expr(e, funcs, scope, finder, out);
                    }
                    StructInitItem::Field(_, v) => {
                        analyze_expr(v, funcs, scope, finder, out);
                    }
                }
            }
        }
        Expr::Member(m) => analyze_expr(&mut m.object, funcs, scope, finder, out),
        Expr::Index(m) => {
            analyze_expr(&mut m.object, funcs, scope, finder, out);
            analyze_expr(&mut m.index, funcs, scope, finder, out);
        }
        Expr::Call(c) => {
            if let Expr::Ident(name, _) = c.callee.as_ref() {
                if let Some((min, max)) = funcs.get(name) {
                    let n = c.args.len();
                    if n < *min || n > *max {
                        out.push(
                            Diagnostic::error_kind(
                                DiagnosticKind::ArgumentCountMismatch {
                                    expected_min: *min,
                                    expected_max: *max,
                                    actual: n,
                                },
                                finder.find_name_or_next(&name),
                            )
                            .with_code(codes::ARGUMENT_COUNT_MISMATCH),
                        );
                    }
                }
            }
            analyze_expr(&mut c.callee, funcs, scope, finder, out);
            for a in c.args.iter_mut() {
                analyze_expr(a, funcs, scope, finder, out);
            }
        }
        Expr::MethodCall(m) => {
            analyze_expr(&mut m.receiver, funcs, scope, finder, out);
            for a in m.args.iter_mut() {
                analyze_expr(a, funcs, scope, finder, out);
            }
        }
        Expr::Unary { expr, .. } => analyze_expr(expr, funcs, scope, finder, out),
        Expr::Binary { left, right, .. } => {
            analyze_expr(left, funcs, scope, finder, out);
            analyze_expr(right, funcs, scope, finder, out);
        }
        Expr::Group(e) => analyze_expr(e, funcs, scope, finder, out),
        Expr::InterpolatedString(parts) => {
            for p in parts {
                analyze_expr(p, funcs, scope, finder, out);
            }
        }
        Expr::EnumCtor { args, .. } => {
            for a in args.iter_mut() {
                analyze_expr(a, funcs, scope, finder, out);
            }
        }
        Expr::Int(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_) => {}
        Expr::Error(_) => {}
    }
}
