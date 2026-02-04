use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use xu_syntax::{Diagnostic, DiagnosticKind, codes, find_best_match, DiagnosticsFormatter, BUILTIN_NAMES};
use xu_parser::{Stmt, Expr, FuncDef, Param};
use super::utils::{Finder, report_shadowing, collect_pattern_binds};
use super::expr::analyze_expr;
use super::{ImportCache, process_import, infer_module_alias};

/// 返回不应触发遮蔽警告的内置函数名集合
fn builtin_names_set() -> HashSet<&'static str> {
    BUILTIN_NAMES.iter().copied().collect()
}

/// 分析上下文，封装所有需要传递的参数
struct AnalyzeContext<'a, 'b> {
    funcs: &'a HashMap<String, (usize, usize)>,
    structs: &'a mut super::StructMap,
    scope: &'a mut Vec<HashMap<String, usize>>,
    def_spans: &'a mut Vec<HashMap<String, xu_syntax::Span>>,
    finder: &'a mut Finder<'b>,
    out: &'a mut Vec<Diagnostic>,
    base_dir: &'a Path,
    strict: bool,
    cache: Arc<RwLock<ImportCache>>,
    import_stack: &'a mut Vec<PathBuf>,
    builtins: HashSet<&'static str>,
}

impl<'a, 'b> AnalyzeContext<'a, 'b> {
    /// 分析函数参数并设置作用域
    fn analyze_func_params(&mut self, params: &mut [Param]) {
        for p in params {
            if self.scope[..self.scope.len() - 1]
                .iter()
                .any(|s| s.contains_key(p.name.as_str()))
                && !self.builtins.contains(p.name.as_str())
            {
                report_shadowing(&p.name, self.finder, self.out);
            }
            let idx = self.scope.last().expect("Scope stack underflow").len();
            self.scope.last_mut().expect("Scope stack underflow").insert(p.name.clone(), idx);
            if let Some(sp) = self.finder.find_name_or_next(&p.name) {
                self.def_spans.last_mut().expect("Def spans stack underflow").insert(p.name.clone(), sp);
            }
            if let Some(d) = &mut p.default {
                analyze_expr(d, self.funcs, self.scope, self.finder, self.out);
            }
        }
    }

    /// 分析函数定义（通用逻辑）
    fn analyze_func_def_common(&mut self, def: &mut FuncDef) {
        let idx = self.scope.last().expect("Scope stack underflow").len();
        self.scope.last_mut().expect("Scope stack underflow").insert(def.name.clone(), idx);
        self.scope.push(HashMap::new());
        self.def_spans.push(HashMap::new());
        self.analyze_func_params(&mut def.params);
        self.analyze_stmts_impl(&mut def.body);
        self.scope.pop();
        self.def_spans.pop();
    }

    /// 分析语句列表的内部实现
    fn analyze_stmts_impl(&mut self, stmts: &mut [Stmt]) -> bool {
        let mut terminated = false;
        for s in stmts {
            if terminated {
                self.out.push(
                    Diagnostic::warning_kind(
                        DiagnosticKind::UnreachableCode,
                        self.finder.next_significant_span(),
                    )
                    .with_code(codes::UNREACHABLE_CODE),
                );
            }
            match s {
                Stmt::StructDef(def) => self.analyze_struct_def(def),
                Stmt::EnumDef(_) => {}
                Stmt::FuncDef(def) => self.analyze_func_def(def),
                Stmt::DoesBlock(def) => self.analyze_does_block(def),
                Stmt::Use(u) => self.analyze_use_stmt(u),
                Stmt::If(s) => {
                    if self.analyze_if_stmt(s) {
                        terminated = true;
                    }
                }
                Stmt::Match(s) => {
                    if self.analyze_match_stmt(s) {
                        terminated = true;
                    }
                }
                Stmt::While(s) => self.analyze_while_stmt(s),
                Stmt::ForEach(s) => self.analyze_foreach_stmt(s),
                Stmt::Return(e) => {
                    if let Some(e) = e {
                        analyze_expr(e, self.funcs, self.scope, self.finder, self.out);
                    }
                    terminated = true;
                }
                Stmt::Break | Stmt::Continue => {
                    terminated = true;
                }
                Stmt::Block(stmts) => {
                    if self.analyze_block_stmt(stmts) {
                        terminated = true;
                    }
                }
                Stmt::Assign(s) => self.analyze_assign_stmt(s),
                Stmt::Expr(e) => {
                    analyze_expr(e, self.funcs, self.scope, self.finder, self.out)
                }
                Stmt::Error(_) => {}
            }
        }
        terminated
    }

    /// 分析结构体定义
    fn analyze_struct_def(&mut self, def: &mut xu_parser::StructDef) {
        for method in def.methods.iter_mut() {
            let idx = self.scope.last().expect("scope stack should not be empty").len();
            self.scope.last_mut().expect("scope stack should not be empty").insert(method.name.clone(), idx);
            self.scope.push(HashMap::new());
            self.def_spans.push(HashMap::new());
            self.analyze_func_params(&mut method.params);
            self.analyze_stmts_impl(&mut method.body);
            self.scope.pop();
            self.def_spans.pop();
        }
    }

    /// 分析函数定义
    fn analyze_func_def(&mut self, def: &mut FuncDef) {
        self.analyze_func_def_common(def);
    }

    /// 分析 does 块
    fn analyze_does_block(&mut self, def: &mut xu_parser::DoesBlock) {
        for func_def in def.funcs.iter_mut() {
            let idx = self.scope.last().expect("scope stack should not be empty").len();
            self.scope.last_mut().expect("scope stack should not be empty").insert(func_def.name.clone(), idx);
            self.scope.push(HashMap::new());
            self.def_spans.push(HashMap::new());
            self.analyze_func_params(&mut func_def.params);
            self.analyze_stmts_impl(&mut func_def.body);
            self.scope.pop();
            self.def_spans.pop();
        }
    }

    /// 分析 use 语句
    fn analyze_use_stmt(&mut self, u: &xu_parser::UseStmt) {
        let (new_funcs, new_structs) = process_import(
            &u.path,
            self.base_dir,
            self.cache.clone(),
            self.out,
            self.finder.find_name_or_next("use"),
            self.import_stack,
        );
        for name in new_funcs {
            let idx = self.scope.last().expect("scope stack should not be empty").len();
            self.scope.last_mut().expect("scope stack should not be empty").insert(name, idx);
        }
        for (k, v) in new_structs {
            let idx = self.scope.last().expect("scope stack should not be empty").len();
            self.scope.last_mut().expect("scope stack should not be empty").insert(k.clone(), idx);
            self.structs.insert(k, v);
        }
        let alias = u
            .alias
            .clone()
            .unwrap_or_else(|| infer_module_alias(&u.path));
        if self.scope[..self.scope.len() - 1]
            .iter()
            .any(|s| s.contains_key(alias.as_str()))
        {
            report_shadowing(&alias, self.finder, self.out);
        }
        let idx = self.scope.last().expect("Scope stack underflow").len();
        self.scope.last_mut().expect("Scope stack underflow").insert(alias.clone(), idx);
        if let Some(sp) = self.finder.find_name_or_next(&alias) {
            self.def_spans.last_mut().expect("Def spans stack underflow").insert(alias, sp);
        }
    }

    /// 分析 if 语句，返回是否所有分支都终止
    fn analyze_if_stmt(&mut self, s: &mut xu_parser::IfStmt) -> bool {
        let mut all_branches_terminate = !s.branches.is_empty();
        for (cond, body) in &mut s.branches {
            analyze_expr(cond, self.funcs, self.scope, self.finder, self.out);
            if !self.analyze_stmts_impl(body) {
                all_branches_terminate = false;
            }
        }
        if let Some(body) = &mut s.else_branch {
            if !self.analyze_stmts_impl(body) {
                all_branches_terminate = false;
            }
        } else {
            all_branches_terminate = false;
        }
        all_branches_terminate
    }

    /// 分析 match 语句，返回是否所有分支都终止
    fn analyze_match_stmt(&mut self, s: &mut xu_parser::MatchStmt) -> bool {
        analyze_expr(&mut s.expr, self.funcs, self.scope, self.finder, self.out);
        let mut all_arms_terminate = !s.arms.is_empty();
        for (pat, body) in &mut s.arms {
            self.scope.push(HashMap::new());
            self.def_spans.push(HashMap::new());
            let mut binds: Vec<String> = Vec::new();
            collect_pattern_binds(pat, &mut binds);
            for name in binds {
                let idx = self.scope.last().expect("scope stack should not be empty").len();
                self.scope.last_mut().expect("scope stack should not be empty").insert(name.clone(), idx);
                if let Some(sp) = self.finder.find_name_or_next(&name) {
                    self.def_spans.last_mut().expect("def_spans stack should not be empty").insert(name.clone(), sp);
                }
            }
            if !self.analyze_stmts_impl(body) {
                all_arms_terminate = false;
            }
            self.scope.pop();
            self.def_spans.pop();
        }
        if let Some(body) = &mut s.else_branch {
            if !self.analyze_stmts_impl(body) {
                all_arms_terminate = false;
            }
        } else {
            all_arms_terminate = false;
        }
        all_arms_terminate
    }

    /// 分析 while 语句
    fn analyze_while_stmt(&mut self, s: &mut xu_parser::WhileStmt) {
        analyze_expr(&mut s.cond, self.funcs, self.scope, self.finder, self.out);
        self.analyze_stmts_impl(&mut s.body);
    }

    /// 分析 foreach 语句
    fn analyze_foreach_stmt(&mut self, s: &mut xu_parser::ForEachStmt) {
        analyze_expr(&mut s.iter, self.funcs, self.scope, self.finder, self.out);
        // 注意：这里不检查遮蔽，因为 for 循环变量语义上有自己的作用域
        // 连续的 for 循环使用相同变量名是常见模式
        let idx = if let Some(&existing_idx) = self.scope.last().expect("Scope stack underflow").get(&s.var) {
            existing_idx
        } else {
            self.scope.last().expect("Scope stack underflow").len()
        };
        self.scope.last_mut().expect("Scope stack underflow").insert(s.var.clone(), idx);
        if let Some(sp) = self.finder.find_name_or_next(&s.var) {
            self.def_spans.last_mut().expect("Def spans stack underflow").insert(s.var.clone(), sp);
        }
        self.analyze_stmts_impl(&mut s.body);
    }

    /// 分析块语句，返回是否终止
    fn analyze_block_stmt(&mut self, stmts: &mut [Stmt]) -> bool {
        self.scope.push(HashMap::new());
        self.def_spans.push(HashMap::new());
        let block_terminated = self.analyze_stmts_impl(stmts);
        self.scope.pop();
        self.def_spans.pop();
        block_terminated
    }

    /// 分析赋值语句
    fn analyze_assign_stmt(&mut self, s: &mut xu_parser::AssignStmt) {
        analyze_expr(&mut s.value, self.funcs, self.scope, self.finder, self.out);

        // 检查 unit 赋值
        if is_unit_expr(&s.value) {
            self.out.push(Diagnostic::error_kind(
                DiagnosticKind::UnitAssignment,
                self.finder.next_significant_span(),
            ));
        }

        // 处理标识符赋值
        if let Expr::Ident(name, slot) = &mut s.target {
            // 检查空容器字面量是否需要类型注解
            if s.decl.is_some() && s.ty.is_none() {
                let needs_annot = match &s.value {
                    Expr::List(items) => items.is_empty(),
                    _ => false,
                };
                if needs_annot {
                    self.out.push(Diagnostic::error(
                        "Type annotation required for empty container literal",
                        self.finder.find_name_or_next(name),
                    ));
                }
            }

            // 解析变量
            let mut resolved = None;
            for (depth, f) in self.scope.iter().rev().enumerate() {
                if let Some(&idx) = f.get(name.as_str()) {
                    resolved = Some((depth as u32, idx as u32));
                    break;
                }
            }
            if s.decl.is_some() && resolved.is_some() {
                report_shadowing(name, self.finder, self.out);
            }

            // 严格模式下检查未定义标识符
            if self.strict && s.ty.is_none() && s.decl.is_none() {
                if resolved.is_none() {
                    let mut diag = Diagnostic::error_kind(
                        DiagnosticKind::UndefinedIdentifier(name.clone()),
                        self.finder.find_name_or_next(name),
                    )
                    .with_code(codes::UNDEFINED_IDENTIFIER);

                    if let Some(suggested) = find_best_match(
                        name,
                        self.scope.iter().flat_map(|s| s.keys().map(|k| k.as_str())),
                    ) {
                        diag = diag.with_suggestion(DiagnosticsFormatter::format(
                            &DiagnosticKind::DidYouMean(suggested.to_string()),
                        ));
                    }
                    self.out.push(diag);
                }
            }

            // 设置槽位
            if let Some(r) = resolved {
                slot.set(Some(r));
                s.slot = Some(r);
            } else {
                let idx = self.scope.last().expect("Scope stack underflow").len();
                self.scope.last_mut().expect("Scope stack underflow").insert(name.clone(), idx);
                let r = (0, idx as u32);
                slot.set(Some(r));
                s.slot = Some(r);
                if let Some(sp) = self.finder.find_name_or_next(name) {
                    self.def_spans.last_mut().expect("Def spans stack underflow").insert(name.clone(), sp);
                }
            }
        } else {
            analyze_expr(&mut s.target, self.funcs, self.scope, self.finder, self.out);
        }
    }
}

/// 检查表达式是否为 unit 字面量（空元组）
fn is_unit_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Tuple(items) if items.is_empty() => true,
        Expr::Group(inner) => is_unit_expr(inner),
        _ => false,
    }
}

/// 分析语句列表的公共入口函数
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
    let mut ctx = AnalyzeContext {
        funcs,
        structs,
        scope,
        def_spans,
        finder,
        out,
        base_dir,
        strict,
        cache,
        import_stack,
        builtins: builtin_names_set(),
    };
    ctx.analyze_stmts_impl(stmts)
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
                        let idx = scope.last().expect("scope stack should not be empty").len();
                        scope.last_mut().expect("scope stack should not be empty").insert(name, idx);
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
                // Reuse existing slot if the variable name already exists in scope
                let idx = if let Some(&existing_idx) = scope.last().expect("scope stack should not be empty").get(&s.var) {
                    existing_idx
                } else {
                    scope.last().expect("scope stack should not be empty").len()
                };
                scope.last_mut().expect("scope stack should not be empty").insert(s.var.clone(), idx);
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
