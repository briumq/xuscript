use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use xu_syntax::{Diagnostic, DiagnosticKind, codes, find_best_match, DiagnosticsFormatter, BUILTIN_NAMES};
use xu_parser::{Stmt, Expr};
use super::utils::{Finder, report_shadowing, collect_pattern_binds};
use super::expr::analyze_expr;
use super::{ImportCache, process_import, infer_module_alias};

/// Returns a set of builtin function names that should not trigger shadowing warnings
fn builtin_names_set() -> HashSet<&'static str> {
    BUILTIN_NAMES.iter().copied().collect()
}

/// Check if an expression is a void literal (empty tuple)
fn is_void_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Tuple(items) if items.is_empty() => true,
        Expr::Group(inner) => is_void_expr(inner),
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn analyze_stmts(
    stmts: &mut [Stmt],
    funcs: &HashMap<String, (usize, usize)>,
    structs: &mut super::StructMap,
    scope: &mut Vec<HashMap<String, usize>>,
    def_spans: &mut Vec<HashMap<String, xu_syntax::Span>>,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
    base_dir: &Path,
    strict: bool,
    cache: Arc<RwLock<ImportCache>>,
    import_stack: &mut Vec<PathBuf>,
) -> bool {
    let mut terminated = false;
    let builtins = builtin_names_set();
    for s in stmts {
        if terminated {
            out.push(
                Diagnostic::warning_kind(
                    DiagnosticKind::UnreachableCode,
                    finder.next_significant_span(),
                )
                .with_code(codes::UNREACHABLE_CODE),
            );
        }
        match s {
            Stmt::StructDef(def) => {
                // Analyze methods defined in the has block
                for method in def.methods.iter_mut() {
                    let idx = scope.last().unwrap().len();
                    scope.last_mut().unwrap().insert(method.name.clone(), idx);
                    scope.push(HashMap::new());
                    def_spans.push(HashMap::new());
                    for p in &mut method.params {
                        if scope[..scope.len() - 1]
                            .iter()
                            .any(|s| s.contains_key(p.name.as_str()))
                            && !builtins.contains(p.name.as_str())
                        {
                            report_shadowing(&p.name, finder, out);
                        }
                        let idx = scope.last().unwrap().len();
                        scope.last_mut().unwrap().insert(p.name.clone(), idx);
                        if let Some(sp) = finder.find_name_or_next(&p.name) {
                            def_spans.last_mut().unwrap().insert(p.name.clone(), sp);
                        }
                        if let Some(d) = &mut p.default {
                            analyze_expr(d, funcs, scope, finder, out);
                        }
                    }
                    analyze_stmts(
                        &mut method.body,
                        funcs,
                        structs,
                        scope,
                        def_spans,
                        finder,
                        out,
                        base_dir,
                        strict,
                        cache.clone(),
                        import_stack,
                    );
                    scope.pop();
                    def_spans.pop();
                }
            }
            Stmt::EnumDef(_) => {}
            Stmt::FuncDef(def) => {
                let idx = scope.last().expect("Scope stack underflow").len();
                scope.last_mut().expect("Scope stack underflow").insert(def.name.clone(), idx);
                scope.push(HashMap::new());
                def_spans.push(HashMap::new());
                for p in &mut def.params {
                    if scope[..scope.len() - 1]
                        .iter()
                        .any(|s| s.contains_key(p.name.as_str()))
                        && !builtins.contains(p.name.as_str())
                    {
                        report_shadowing(&p.name, finder, out);
                    }
                    let idx = scope.last().expect("Scope stack underflow").len();
                    scope.last_mut().expect("Scope stack underflow").insert(p.name.clone(), idx);
                    if let Some(sp) = finder.find_name_or_next(&p.name) {
                        def_spans.last_mut().expect("Def spans stack underflow").insert(p.name.clone(), sp);
                    }
                    if let Some(d) = &mut p.default {
                        analyze_expr(d, funcs, scope, finder, out);
                    }
                }
                analyze_stmts(
                    &mut def.body,
                    funcs,
                    structs,
                    scope,
                    def_spans,
                    finder,
                    out,
                    base_dir,
                    strict,
                    cache.clone(),
                    import_stack,
                );
                scope.pop();
                def_spans.pop();
            }
            Stmt::DoesBlock(def) => {
                for def in def.funcs.iter_mut() {
                    let idx = scope.last().unwrap().len();
                    scope.last_mut().unwrap().insert(def.name.clone(), idx);
                    scope.push(HashMap::new());
                    def_spans.push(HashMap::new());
                    for p in &mut def.params {
                        if scope[..scope.len() - 1]
                            .iter()
                            .any(|s| s.contains_key(p.name.as_str()))
                            && !builtins.contains(p.name.as_str())
                        {
                            report_shadowing(&p.name, finder, out);
                        }
                        let idx = scope.last().unwrap().len();
                        scope.last_mut().unwrap().insert(p.name.clone(), idx);
                        if let Some(sp) = finder.find_name_or_next(&p.name) {
                            def_spans.last_mut().unwrap().insert(p.name.clone(), sp);
                        }
                        if let Some(d) = &mut p.default {
                            analyze_expr(d, funcs, scope, finder, out);
                        }
                    }
                    analyze_stmts(
                        &mut def.body,
                        funcs,
                        structs,
                        scope,
                        def_spans,
                        finder,
                        out,
                        base_dir,
                        strict,
                        cache.clone(),
                        import_stack,
                    );
                    scope.pop();
                    def_spans.pop();
                }
            }
            Stmt::Use(u) => {
                let (new_funcs, new_structs) = process_import(
                    &u.path,
                    base_dir,
                    cache.clone(),
                    out,
                    finder.find_name_or_next("use"),
                    import_stack,
                );
                for name in new_funcs {
                    let idx = scope.last().unwrap().len();
                    scope.last_mut().unwrap().insert(name, idx);
                }
                for (k, v) in new_structs {
                    let idx = scope.last().unwrap().len();
                    scope.last_mut().unwrap().insert(k.clone(), idx);
                    structs.insert(k, v);
                }
                let alias = u
                    .alias
                    .clone()
                    .unwrap_or_else(|| infer_module_alias(&u.path));
                if scope[..scope.len() - 1]
                    .iter()
                    .any(|s| s.contains_key(alias.as_str()))
                {
                    report_shadowing(&alias, finder, out);
                }
                let idx = scope.last().expect("Scope stack underflow").len();
                scope.last_mut().expect("Scope stack underflow").insert(alias.clone(), idx);
                if let Some(sp) = finder.find_name_or_next(&alias) {
                    def_spans.last_mut().expect("Def spans stack underflow").insert(alias, sp);
                }
            }
            Stmt::If(s) => {
                let mut all_branches_terminate = !s.branches.is_empty();
                for (cond, body) in &mut s.branches {
                    analyze_expr(cond, funcs, scope, finder, out);
                    if !analyze_stmts(
                        body,
                        funcs,
                        structs,
                        scope,
                        def_spans,
                        finder,
                        out,
                        base_dir,
                        strict,
                        cache.clone(),
                        import_stack,
                    ) {
                        all_branches_terminate = false;
                    }
                }
                if let Some(body) = &mut s.else_branch {
                    if !analyze_stmts(
                        body,
                        funcs,
                        structs,
                        scope,
                        def_spans,
                        finder,
                        out,
                        base_dir,
                        strict,
                        cache.clone(),
                        import_stack,
                    ) {
                        all_branches_terminate = false;
                    }
                } else {
                    all_branches_terminate = false;
                }
                if all_branches_terminate {
                    terminated = true;
                }
            }
            Stmt::Match(s) => {
                analyze_expr(&mut s.expr, funcs, scope, finder, out);
                let mut all_arms_terminate = !s.arms.is_empty();
                for (pat, body) in &mut s.arms {
                    scope.push(HashMap::new());
                    def_spans.push(HashMap::new());
                    let mut binds: Vec<String> = Vec::new();
                    collect_pattern_binds(pat, &mut binds);
                    for name in binds {
                        let idx = scope.last().unwrap().len();
                        scope.last_mut().unwrap().insert(name.clone(), idx);
                        if let Some(sp) = finder.find_name_or_next(&name) {
                            def_spans.last_mut().unwrap().insert(name.clone(), sp);
                        }
                    }
                    if !analyze_stmts(
                        body,
                        funcs,
                        structs,
                        scope,
                        def_spans,
                        finder,
                        out,
                        base_dir,
                        strict,
                        cache.clone(),
                        import_stack,
                    ) {
                        all_arms_terminate = false;
                    }
                    scope.pop();
                    def_spans.pop();
                }
                if let Some(body) = &mut s.else_branch {
                    if !analyze_stmts(
                        body,
                        funcs,
                        structs,
                        scope,
                        def_spans,
                        finder,
                        out,
                        base_dir,
                        strict,
                        cache.clone(),
                        import_stack,
                    ) {
                        all_arms_terminate = false;
                    }
                } else {
                    all_arms_terminate = false;
                }
                if all_arms_terminate {
                    terminated = true;
                }
            }
            Stmt::While(s) => {
                analyze_expr(&mut s.cond, funcs, scope, finder, out);
                analyze_stmts(
                    &mut s.body,
                    funcs,
                    structs,
                    scope,
                    def_spans,
                    finder,
                    out,
                    base_dir,
                    strict,
                    cache.clone(),
                    import_stack,
                );
            }
            Stmt::ForEach(s) => {
                analyze_expr(&mut s.iter, funcs, scope, finder, out);
                if scope.iter().any(|sc| sc.contains_key(s.var.as_str())) {
                    report_shadowing(&s.var, finder, out);
                }
                let idx = scope.last().expect("Scope stack underflow").len();
                scope.last_mut().expect("Scope stack underflow").insert(s.var.clone(), idx);
                if let Some(sp) = finder.find_name_or_next(&s.var) {
                    def_spans.last_mut().expect("Def spans stack underflow").insert(s.var.clone(), sp);
                }
                analyze_stmts(
                    &mut s.body,
                    funcs,
                    structs,
                    scope,
                    def_spans,
                    finder,
                    out,
                    base_dir,
                    strict,
                    cache.clone(),
                    import_stack,
                );
            }
            Stmt::Return(e) => {
                if let Some(e) = e {
                    analyze_expr(e, funcs, scope, finder, out);
                }
                terminated = true;
            }
            Stmt::Break | Stmt::Continue => {
                terminated = true;
            }
            Stmt::Block(stmts) => {
                scope.push(HashMap::new());
                def_spans.push(HashMap::new());
                let block_terminated = analyze_stmts(
                    stmts,
                    funcs,
                    structs,
                    scope,
                    def_spans,
                    finder,
                    out,
                    base_dir,
                    strict,
                    cache.clone(),
                    import_stack,
                );
                scope.pop();
                def_spans.pop();
                if block_terminated {
                    terminated = true;
                }
            }
            Stmt::Assign(s) => {
                analyze_expr(&mut s.value, funcs, scope, finder, out);

                // Check for void assignment
                if is_void_expr(&s.value) {
                    out.push(Diagnostic::error_kind(
                        DiagnosticKind::VoidAssignment,
                        finder.next_significant_span(),
                    ));
                }

                match &mut s.target {
                    Expr::Ident(name, slot) => {
                        if s.decl.is_some() && s.ty.is_none() {
                            let needs_annot = match &s.value {
                                Expr::List(items) => items.is_empty(),
                                // Empty dict {} is allowed without type annotation
                                _ => false,
                            };
                            if needs_annot {
                                out.push(Diagnostic::error(
                                    "Type annotation required for empty container literal",
                                    finder.find_name_or_next(name),
                                ));
                            }
                        }

                        let mut resolved = None;
                        for (depth, f) in scope.iter().rev().enumerate() {
                            if let Some(&idx) = f.get(name) {
                                resolved = Some((depth as u32, idx as u32));
                                break;
                            }
                        }
                        if s.decl.is_some() && resolved.is_some() {
                            report_shadowing(name, finder, out);
                        }

                        if strict && s.ty.is_none() && s.decl.is_none() {
                            if resolved.is_none() {
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

                        if let Some(r) = resolved {
                            slot.set(Some(r));
                            s.slot = Some(r);
                        } else {
                            let idx = scope.last().expect("Scope stack underflow").len();
                            scope.last_mut().expect("Scope stack underflow").insert(name.clone(), idx);
                            let r = (0, idx as u32);
                            slot.set(Some(r));
                            s.slot = Some(r);
                            if let Some(sp) = finder.find_name_or_next(&name) {
                                def_spans.last_mut().expect("Def spans stack underflow").insert(name.clone(), sp);
                            }
                        }
                    }
                    other => analyze_expr(other, funcs, scope, finder, out),
                }
            }
            Stmt::Expr(e) => {
                analyze_expr(e, funcs, scope, finder, out)
            }
            Stmt::Error(_) => {}
            // Removed Try and Throw per v1.1 alignment
        }
    }
    terminated
}

/// Helper for FuncLit in expr.rs to avoid circular imports.
/// This will be used as a shim.
pub(crate) fn analyze_local_stmts_shim(
    stmts: &mut [Stmt],
    funcs: &HashMap<String, (usize, usize)>,
    scope: &mut Vec<HashMap<String, usize>>,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
) {
    for s in stmts {
        match s {
            Stmt::If(s) => {
                for (cond, body) in s.branches.iter_mut() {
                    analyze_expr(cond, funcs, scope, finder, out);
                    analyze_local_stmts_shim(body, funcs, scope, finder, out);
                }
                if let Some(b) = &mut s.else_branch {
                    analyze_local_stmts_shim(b, funcs, scope, finder, out);
                }
            }
            Stmt::Match(s) => {
                analyze_expr(&mut s.expr, funcs, scope, finder, out);
                for (pat, body) in s.arms.iter_mut() {
                    scope.push(HashMap::new());
                    let mut binds: Vec<String> = Vec::new();
                    collect_pattern_binds(pat, &mut binds);
                    for name in binds {
                        let idx = scope.last().unwrap().len();
                        scope.last_mut().unwrap().insert(name, idx);
                    }
                    analyze_local_stmts_shim(body, funcs, scope, finder, out);
                    scope.pop();
                }
                if let Some(b) = &mut s.else_branch {
                    analyze_local_stmts_shim(b, funcs, scope, finder, out);
                }
            }
            Stmt::While(s) => {
                analyze_expr(&mut s.cond, funcs, scope, finder, out);
                analyze_local_stmts_shim(&mut s.body, funcs, scope, finder, out);
            }
            Stmt::ForEach(s) => {
                analyze_expr(&mut s.iter, funcs, scope, finder, out);
                let idx = scope.last().unwrap().len();
                scope.last_mut().unwrap().insert(s.var.clone(), idx);
                analyze_local_stmts_shim(&mut s.body, funcs, scope, finder, out);
            }
            Stmt::Return(Some(e)) => {
                analyze_expr(e, funcs, scope, finder, out);
            }
            Stmt::Return(None) => {}
            Stmt::Assign(a) => {
                analyze_expr(&mut a.value, funcs, scope, finder, out);
                if let Expr::Ident(name, slot) = &mut a.target {
                    let mut resolved = None;
                    for (depth, f) in scope.iter().rev().enumerate() {
                        if let Some(&idx) = f.get(name) {
                            resolved = Some((depth as u32, idx as u32));
                            break;
                        }
                    }
                    if a.decl.is_some() || resolved.is_none() {
                        let idx = scope.last().expect("Scope stack underflow").len();
                        scope
                            .last_mut()
                            .expect("Scope stack underflow")
                            .insert(name.clone(), idx);
                        let r = (0, idx as u32);
                        slot.set(Some(r));
                        a.slot = Some(r);
                    } else if let Some(r) = resolved {
                        slot.set(Some(r));
                        a.slot = Some(r);
                    } else {
                        out.push(
                            Diagnostic::error_kind(
                                DiagnosticKind::UndefinedIdentifier(name.clone()),
                                finder.find_name_or_next(name),
                            )
                            .with_code(codes::UNDEFINED_IDENTIFIER),
                        );
                    }
                } else {
                    analyze_expr(&mut a.target, funcs, scope, finder, out);
                }
            }
            Stmt::Expr(e) => analyze_expr(e, funcs, scope, finder, out),
            Stmt::Block(stmts) => {
                scope.push(HashMap::new());
                analyze_local_stmts_shim(stmts, funcs, scope, finder, out);
                scope.pop();
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::FuncDef(_) => {}
            Stmt::StructDef(_) => {}
            Stmt::EnumDef(_) => {}
            Stmt::DoesBlock(_) => {}
            Stmt::Use(_) => {}
            Stmt::Error(_) => {}
            // Removed Try and Throw
        }
    }
}
