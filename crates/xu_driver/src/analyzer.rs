//!
//!

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use xu_lexer::Lexer;
use xu_parser::Parser;
use xu_syntax::{
    BUILTIN_NAMES, Diagnostic, DiagnosticKind, DiagnosticsFormatter, SourceFile, TokenKind,
    builtin_return_type, codes, find_best_match,
};
use xu_syntax::{Type, TypeId, TypeInterner};

struct Finder<'a> {
    source: &'a SourceFile,
    tokens: &'a [xu_syntax::Token],
    i: usize,
}

impl<'a> Finder<'a> {
    fn new(source: &'a SourceFile, tokens: &'a [xu_syntax::Token]) -> Self {
        Self {
            source,
            tokens,
            i: 0,
        }
    }

    fn next_significant_span(&mut self) -> Option<xu_syntax::Span> {
        while let Some(t) = self.tokens.get(self.i) {
            self.i += 1;
            if matches!(
                t.kind,
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                continue;
            }
            return Some(t.span);
        }
        None
    }

    fn find_name(&mut self, name: &str) -> Option<xu_syntax::Span> {
        for idx in self.i..self.tokens.len() {
            let t = &self.tokens[idx];
            if t.kind == TokenKind::Ident && self.source.text.slice(t.span) == name {
                self.i = idx + 1;
                return Some(t.span);
            }
        }
        None
    }

    fn find_name_or_next(&mut self, name: &str) -> Option<xu_syntax::Span> {
        self.find_name(name)
            .or_else(|| self.next_significant_span())
    }

    fn find_kw(&mut self, kind: TokenKind) -> Option<xu_syntax::Span> {
        for idx in self.i..self.tokens.len() {
            let t = &self.tokens[idx];
            if t.kind == kind {
                self.i = idx + 1;
                return Some(t.span);
            }
        }
        None
    }

    fn find_kw_or_next(&mut self, kind: TokenKind) -> Option<xu_syntax::Span> {
        self.find_kw(kind).or_else(|| self.next_significant_span())
    }
}

type StructMap = HashMap<String, HashMap<String, String>>;

#[derive(Clone, Default, Debug)]
pub struct ImportCache {
    pub modules: HashMap<PathBuf, (Vec<String>, StructMap)>,
}

///
///
///
///
///
///
pub(crate) fn analyze_module(
    source: &SourceFile,
    tokens: &[xu_syntax::Token],
    module: &mut xu_parser::Module,
    strict: bool,
    cache: Arc<RwLock<ImportCache>>,
    import_stack: &mut Vec<PathBuf>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Check for circular import at the entry of module analysis
    let current_path = PathBuf::from(&source.name);
    let current_path = current_path.canonicalize().unwrap_or(current_path);

    if import_stack.contains(&current_path) {
        // This shouldn't happen if process_import handles it,
        // but for safety we check here too.
        return out;
    }

    import_stack.push(current_path.clone());

    let mut funcs: HashMap<String, (usize, usize)> = HashMap::new();
    let mut structs: StructMap = HashMap::new();

    for s in &module.stmts {
        match s {
            xu_parser::Stmt::FuncDef(def) => {
                let max = def.params.len();
                let min = def.params.iter().filter(|p| p.default.is_none()).count();
                funcs.insert(def.name.clone(), (min, max));
            }
            xu_parser::Stmt::StructDef(def) => {
                let mut fields = HashMap::new();
                for f in &def.fields {
                    fields.insert(f.name.clone(), type_to_string(&f.ty));
                }
                structs.insert(def.name.clone(), fields);
            }
            _ => {}
        }
    }

    let mut scope: Vec<HashMap<String, usize>> = vec![HashMap::new()];
    let mut def_spans: Vec<HashMap<String, xu_syntax::Span>> = vec![HashMap::new()];
    for builtin in BUILTIN_NAMES {
        let idx = scope.last().unwrap().len();
        scope.last_mut().unwrap().insert(builtin.to_string(), idx);
    }
    for name in funcs.keys() {
        let idx = scope.last().unwrap().len();
        scope.last_mut().unwrap().insert(name.clone(), idx);
    }
    for name in structs.keys() {
        let idx = scope.last().unwrap().len();
        scope.last_mut().unwrap().insert(name.clone(), idx);
    }

    let mut sem_finder = Finder::new(source, tokens);
    let base_dir = Path::new(&source.name).parent().unwrap_or(Path::new("."));
    analyze_stmts(
        &mut module.stmts,
        &funcs,
        &mut structs,
        &mut scope,
        &mut def_spans,
        &mut sem_finder,
        &mut out,
        base_dir,
        strict,
        cache,
        import_stack,
    );

    let mut type_finder = Finder::new(source, tokens);
    analyze_types(module, &structs, &mut type_finder, &mut out);

    import_stack.pop();
    out
}

#[allow(clippy::too_many_arguments)]
fn analyze_stmts(
    stmts: &mut [xu_parser::Stmt],
    funcs: &HashMap<String, (usize, usize)>,
    structs: &mut StructMap,
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
            xu_parser::Stmt::StructDef(_) => {}
            xu_parser::Stmt::EnumDef(_) => {}
            xu_parser::Stmt::FuncDef(def) => {
                let idx = scope.last().unwrap().len();
                scope.last_mut().unwrap().insert(def.name.clone(), idx);
                scope.push(HashMap::new());
                def_spans.push(HashMap::new());
                for p in &mut def.params {
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
            xu_parser::Stmt::DoesBlock(def) => {
                for def in def.funcs.iter_mut() {
                    let idx = scope.last().unwrap().len();
                    scope.last_mut().unwrap().insert(def.name.clone(), idx);
                    scope.push(HashMap::new());
                    def_spans.push(HashMap::new());
                    for p in &mut def.params {
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
            xu_parser::Stmt::If(s) => {
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
            xu_parser::Stmt::When(s) => {
                analyze_expr(&mut s.expr, funcs, scope, finder, out);
                let mut all_arms_terminate = !s.arms.is_empty();
                for (_, body) in &mut s.arms {
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
            xu_parser::Stmt::While(s) => {
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
            xu_parser::Stmt::ForEach(s) => {
                analyze_expr(&mut s.iter, funcs, scope, finder, out);
                let idx = scope.last().unwrap().len();
                scope.last_mut().unwrap().insert(s.var.clone(), idx);
                if let Some(sp) = finder.find_name_or_next(&s.var) {
                    def_spans.last_mut().unwrap().insert(s.var.clone(), sp);
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
            xu_parser::Stmt::Try(s) => {
                let body_terminated = analyze_stmts(
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
                let mut catch_terminated = true;
                if let Some(c) = &mut s.catch {
                    scope.push(HashMap::new());
                    def_spans.push(HashMap::new());
                    if let Some(v) = &c.var {
                        let idx = scope.last().unwrap().len();
                        scope.last_mut().unwrap().insert(v.clone(), idx);
                        if let Some(sp) = finder.find_name_or_next(v) {
                            def_spans.last_mut().unwrap().insert(v.clone(), sp);
                        }
                    }
                    if !analyze_stmts(
                        &mut c.body,
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
                        catch_terminated = false;
                    }
                    scope.pop();
                    def_spans.pop();
                } else {
                    catch_terminated = false;
                }

                if let Some(f) = &mut s.finally {
                    if analyze_stmts(
                        f,
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
                        terminated = true;
                    }
                }
                if body_terminated && catch_terminated {
                    terminated = true;
                }
            }
            xu_parser::Stmt::Return(e) => {
                if let Some(e) = e {
                    analyze_expr(e, funcs, scope, finder, out);
                }
                terminated = true;
            }
            xu_parser::Stmt::Throw(e) => {
                analyze_expr(e, funcs, scope, finder, out);
                terminated = true;
            }
            xu_parser::Stmt::Break | xu_parser::Stmt::Continue => {
                terminated = true;
            }
            xu_parser::Stmt::Assign(s) => {
                analyze_expr(&mut s.value, funcs, scope, finder, out);
                match &mut s.target {
                    xu_parser::Expr::Ident(name, slot) => {
                        let mut resolved = None;
                        for (depth, f) in scope.iter().rev().enumerate() {
                            if let Some(&idx) = f.get(name) {
                                resolved = Some((depth as u32, idx as u32));
                                break;
                            }
                        }

                        if strict && s.ty.is_none() {
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
                            let idx = scope.last().unwrap().len();
                            scope.last_mut().unwrap().insert(name.clone(), idx);
                            let r = (0, idx as u32);
                            slot.set(Some(r));
                            s.slot = Some(r);
                            if let Some(sp) = finder.find_name_or_next(&name) {
                                def_spans.last_mut().unwrap().insert(name.clone(), sp);
                            }
                        }
                    }
                    other => analyze_expr(other, funcs, scope, finder, out),
                }
            }
            xu_parser::Stmt::Expr(e) => {
                if let xu_parser::Expr::Call(c) = e {
                    if let xu_parser::Expr::Ident(name, _) = c.callee.as_ref() {
                        if name == "import" {
                            if let Some(xu_parser::Expr::Str(path)) = c.args.first() {
                                let (new_funcs, new_structs) = process_import(
                                    path,
                                    base_dir,
                                    cache.clone(),
                                    out,
                                    finder.find_name_or_next(&name),
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
                            }
                        }
                    }
                }
                analyze_expr(e, funcs, scope, finder, out)
            }
            xu_parser::Stmt::Error(_) => {}
        }
    }
    terminated
}

///
///
fn process_import(
    path: &str,
    base_dir: &Path,
    cache: Arc<RwLock<ImportCache>>,
    out: &mut Vec<Diagnostic>,
    span: Option<xu_syntax::Span>,
    import_stack: &mut Vec<PathBuf>,
) -> (Vec<String>, StructMap) {
    if let Ok(abs_path) = resolve_import_path(base_dir, path) {
        // Check for circular import
        if import_stack.contains(&abs_path) {
            let mut chain: Vec<String> = import_stack
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect();
            chain.push(abs_path.to_string_lossy().into_owned());
            out.push(
                Diagnostic::error_kind(DiagnosticKind::CircularImport(chain), span)
                    .with_code(codes::CIRCULAR_IMPORT),
            );
            return (Vec::new(), HashMap::new());
        }

        if let Some(cached) = cache.read().unwrap().modules.get(&abs_path) {
            return cached.clone();
        }

        if let Ok(input) = fs::read_to_string(&abs_path) {
            let lex = Lexer::new(&input).lex();
            let bump = bumpalo::Bump::new();
            let mut parse = Parser::new(&input, &lex.tokens, &bump).parse();

            let source = SourceFile::new(
                xu_syntax::SourceId(0),
                abs_path.to_string_lossy().into_owned(),
                input.clone(),
            );

            // Recursively analyze the imported module
            let analysis = analyze_module(
                &source,
                &lex.tokens,
                &mut parse.module,
                false, // not strict for imports for now
                cache.clone(),
                import_stack,
            );

            out.extend(lex.diagnostics);
            out.extend(parse.diagnostics);
            out.extend(analysis);

            let mut func_exports = Vec::new();
            let mut struct_exports = HashMap::new();

            for s in &parse.module.stmts {
                if let xu_parser::Stmt::FuncDef(def) = s {
                    if !def.name.starts_with('_') {
                        func_exports.push(def.name.clone());
                    }
                }
                if let xu_parser::Stmt::StructDef(def) = s {
                    let mut fields = HashMap::new();
                    for f in &def.fields {
                        fields.insert(f.name.clone(), type_to_string(&f.ty));
                    }
                    struct_exports.insert(def.name.clone(), fields);
                }
            }

            let res = (func_exports, struct_exports);
            cache.write().unwrap().modules.insert(abs_path, res.clone());
            return res;
        }
    }
    (Vec::new(), HashMap::new())
}

fn resolve_import_path(base_dir: &Path, path: &str) -> Result<PathBuf, ()> {
    let p = Path::new(path);
    if p.is_absolute() {
        return Ok(p.to_path_buf());
    }

    // Standard relative import: relative to the current file's directory
    let joined = base_dir.join(p);
    if joined.exists() {
        return Ok(joined.canonicalize().unwrap_or(joined));
    }

    // Try with .xu extension
    if joined.extension().is_none() {
        let with_ext = joined.with_extension("xu");
        if with_ext.exists() {
            return Ok(with_ext.canonicalize().unwrap_or(with_ext));
        }
    }

    // Fallback: relative to CWD (only if not found in base_dir)
    if p.exists() {
        return Ok(p.canonicalize().unwrap_or(p.to_path_buf()));
    }

    if p.extension().is_none() {
        let with_ext = p.with_extension("xu");
        if with_ext.exists() {
            return Ok(with_ext.canonicalize().unwrap_or(with_ext));
        }
    }

    Err(())
}

fn analyze_expr(
    expr: &mut xu_parser::Expr,
    funcs: &HashMap<String, (usize, usize)>,
    scope: &mut Vec<HashMap<String, usize>>,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
) {
    match expr {
        xu_parser::Expr::Ident(name, slot) => {
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
        xu_parser::Expr::List(items) => {
            for e in items {
                analyze_expr(e, funcs, scope, finder, out);
            }
        }
        xu_parser::Expr::Range(a, b) => {
            analyze_expr(a, funcs, scope, finder, out);
            analyze_expr(b, funcs, scope, finder, out);
        }
        xu_parser::Expr::Dict(entries) => {
            for (_, v) in entries.iter_mut() {
                analyze_expr(v, funcs, scope, finder, out);
            }
        }
        xu_parser::Expr::StructInit(s) => {
            for (_, v) in s.fields.iter_mut() {
                analyze_expr(v, funcs, scope, finder, out);
            }
        }
        xu_parser::Expr::Member(m) => analyze_expr(&mut m.object, funcs, scope, finder, out),
        xu_parser::Expr::Index(m) => {
            analyze_expr(&mut m.object, funcs, scope, finder, out);
            analyze_expr(&mut m.index, funcs, scope, finder, out);
        }
        xu_parser::Expr::Call(c) => {
            if let xu_parser::Expr::Ident(name, _) = c.callee.as_ref() {
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
        xu_parser::Expr::MethodCall(m) => {
            analyze_expr(&mut m.receiver, funcs, scope, finder, out);
            for a in m.args.iter_mut() {
                analyze_expr(a, funcs, scope, finder, out);
            }
        }
        xu_parser::Expr::Unary { expr, .. } => analyze_expr(expr, funcs, scope, finder, out),
        xu_parser::Expr::Binary { left, right, .. } => {
            analyze_expr(left, funcs, scope, finder, out);
            analyze_expr(right, funcs, scope, finder, out);
        }
        xu_parser::Expr::Group(e) => analyze_expr(e, funcs, scope, finder, out),
        xu_parser::Expr::InterpolatedString(parts) => {
            for p in parts {
                analyze_expr(p, funcs, scope, finder, out);
            }
        }
        xu_parser::Expr::EnumCtor { args, .. } => {
            for a in args.iter_mut() {
                analyze_expr(a, funcs, scope, finder, out);
            }
        }
        xu_parser::Expr::Int(_)
        | xu_parser::Expr::Float(_)
        | xu_parser::Expr::Str(_)
        | xu_parser::Expr::Bool(_)
        | xu_parser::Expr::Null => {}
        xu_parser::Expr::Error(_) => {}
    }
}

fn analyze_types(
    module: &xu_parser::Module,
    structs: &StructMap,
    finder: &mut Finder<'_>,
    out: &mut Vec<Diagnostic>,
) {
    let mut interner = TypeInterner::new();
    let mut func_sigs: HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)> = HashMap::new();
    for s in &module.stmts {
        if let xu_parser::Stmt::FuncDef(def) = s {
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
    for builtin in BUILTIN_NAMES {
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

fn analyze_type_stmts(
    stmts: &[xu_parser::Stmt],
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
            xu_parser::Stmt::StructDef(_) => {}
            xu_parser::Stmt::EnumDef(_) => {}
            xu_parser::Stmt::FuncDef(def) => {
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
                                if type_mismatch_id(interner, expected_id, actual_id) {
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
            xu_parser::Stmt::DoesBlock(def) => {
                for def in def.funcs.iter() {
                    analyze_type_stmts(
                        &[xu_parser::Stmt::FuncDef(Box::new(def.clone()))],
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
            xu_parser::Stmt::If(s) => {
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
            xu_parser::Stmt::When(s) => {
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
            xu_parser::Stmt::While(s) => {
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
            xu_parser::Stmt::ForEach(s) => {
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
            xu_parser::Stmt::Try(s) => {
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
                if let Some(c) = &s.catch {
                    type_env.push(HashMap::new());
                    if let Some(v) = &c.var {
                        let text = interner.intern(Type::Text);
                        type_env.last_mut().unwrap().insert(v.clone(), text);
                    }
                    analyze_type_stmts(
                        &c.body,
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
                if let Some(f) = &s.finally {
                    analyze_type_stmts(
                        f,
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
            xu_parser::Stmt::Return(e) => {
                if let (Some(expected), Some(e)) = (expected_return, e) {
                    if let Some(actual) = infer_type(e, func_sigs, structs, type_env, interner) {
                        if type_mismatch_id(interner, expected, actual) {
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
            xu_parser::Stmt::Throw(e) => {
                let _ = infer_type(e, func_sigs, structs, type_env, interner);
            }
            xu_parser::Stmt::Break | xu_parser::Stmt::Continue => {}
            xu_parser::Stmt::Assign(s) => {
                if let Some(expected_id) = s.ty.as_ref().map(|t| typeref_to_typeid(interner, t)) {
                    if let Some(actual) =
                        infer_type(&s.value, func_sigs, structs, type_env, interner)
                    {
                        if type_mismatch_id(interner, expected_id, actual) {
                            let en = interner.name(expected_id);
                            let an = interner.name(actual);
                            let primary = match &s.target {
                                xu_parser::Expr::Ident(name, _) => finder.find_name_or_next(name),
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
                            if let xu_parser::Expr::Ident(_name, _) = &s.target {
                                let msg = "Variable is defined here";
                                if let Some(sp) = primary {
                                    d = d.with_label(msg, sp);
                                }
                            }
                            out.push(d);
                        }
                    }
                    if let xu_parser::Expr::Ident(name, _) = &s.target {
                        type_env
                            .last_mut()
                            .unwrap()
                            .insert(name.clone(), expected_id);
                    }
                } else if let xu_parser::Expr::Ident(name, _) = &s.target {
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
            xu_parser::Stmt::Expr(e) => {
                let _ = infer_type(e, func_sigs, structs, type_env, interner);
            }
            xu_parser::Stmt::Error(_) => {}
        }
    }
}

fn infer_type(
    expr: &xu_parser::Expr,
    func_sigs: &HashMap<String, (Vec<Option<TypeId>>, Option<TypeId>)>,
    structs: &StructMap,
    type_env: &Vec<HashMap<String, TypeId>>,
    interner: &mut TypeInterner,
) -> Option<TypeId> {
    match expr {
        xu_parser::Expr::Int(_) => Some(interner.intern(Type::Int)),
        xu_parser::Expr::Float(_) => Some(interner.intern(Type::Float)),
        xu_parser::Expr::Bool(_) => Some(interner.intern(Type::Bool)),
        xu_parser::Expr::Null => Some(interner.intern(Type::Null)),
        xu_parser::Expr::Str(_) => Some(interner.intern(Type::Text)),
        xu_parser::Expr::List(items) => {
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
        xu_parser::Expr::InterpolatedString(_) => Some(interner.intern(Type::Text)),
        xu_parser::Expr::Dict(entries) => {
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
        xu_parser::Expr::Range(_, _) => Some(interner.intern(Type::Range)),
        xu_parser::Expr::StructInit(s) => Some(interner.parse_type_str(&s.ty)),
        xu_parser::Expr::EnumCtor { ty, .. } => Some(interner.parse_type_str(ty)),
        xu_parser::Expr::Error(_) => None,
        xu_parser::Expr::Ident(name, _) => type_env.iter().rev().find_map(|m| m.get(name).cloned()),
        xu_parser::Expr::Group(e) => infer_type(e, func_sigs, structs, type_env, interner),
        xu_parser::Expr::Unary { op, expr } => match op {
            xu_parser::UnaryOp::Not => Some(interner.intern(Type::Bool)),
            xu_parser::UnaryOp::Neg => infer_type(expr, func_sigs, structs, type_env, interner),
        },
        xu_parser::Expr::Binary { op, left, right } => {
            let lt = infer_type(left, func_sigs, structs, type_env, interner);
            let rt = infer_type(right, func_sigs, structs, type_env, interner);
            match op {
                xu_parser::BinaryOp::Eq
                | xu_parser::BinaryOp::Ne
                | xu_parser::BinaryOp::And
                | xu_parser::BinaryOp::Or
                | xu_parser::BinaryOp::Gt
                | xu_parser::BinaryOp::Lt
                | xu_parser::BinaryOp::Ge
                | xu_parser::BinaryOp::Le => Some(interner.intern(Type::Bool)),
                xu_parser::BinaryOp::Add => {
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
                xu_parser::BinaryOp::Sub | xu_parser::BinaryOp::Mul | xu_parser::BinaryOp::Mod => {
                    let float = interner.intern(Type::Float);
                    let int = interner.intern(Type::Int);
                    match (lt, rt) {
                        (Some(l), _) if l == float => Some(float),
                        (_, Some(r)) if r == float => Some(float),
                        (Some(l), Some(r)) if l == int && r == int => Some(int),
                        _ => None,
                    }
                }
                xu_parser::BinaryOp::Div => Some(interner.intern(Type::Float)),
            }
        }
        xu_parser::Expr::Member(m) => {
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
        xu_parser::Expr::Index(m) => {
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
        xu_parser::Expr::Call(c) => {
            if let xu_parser::Expr::Ident(name, _) = c.callee.as_ref() {
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
                    return ret.or(Some(interner.intern(Type::Null)));
                }
                if let Some(ret_name) = builtin_return_type(name) {
                    return Some(interner.parse_type_str(ret_name));
                }
            }
            infer_type(&c.callee, func_sigs, structs, type_env, interner)
        }
        xu_parser::Expr::MethodCall(m) => {
            let ot = infer_type(&m.receiver, func_sigs, structs, type_env, interner);
            match (ot.map(|id| interner.get(id)), m.method.as_str()) {
                (Some(Type::List(_)), "contains") => Some(interner.intern(Type::Bool)),
                (Some(Type::List(_)), "add") => Some(interner.intern(Type::Null)),
                (Some(Type::Dict(_, _)), "contains") => Some(interner.intern(Type::Bool)),
                (Some(Type::Struct(s)), "read") if s == "file" => Some(interner.intern(Type::Text)),
                (Some(Type::Struct(s)), "close") if s == "file" => {
                    Some(interner.intern(Type::Null))
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

fn type_mismatch_id(interner: &TypeInterner, expected: TypeId, actual: TypeId) -> bool {
    !type_compatible_id(interner, expected, actual)
}

fn type_compatible_id(interner: &TypeInterner, expected: TypeId, actual: TypeId) -> bool {
    match (interner.get(expected), interner.get(actual)) {
        (_, Type::Null) => true,
        (Type::Any, _) => true,
        (Type::Float, Type::Int) => true,
        (Type::List(e), Type::List(a)) => type_compatible_id(interner, *e, *a),
        (Type::Dict(ek, ev), Type::Dict(ak, av)) => {
            type_compatible_id(interner, *ek, *ak) && type_compatible_id(interner, *ev, *av)
        }
        _ => expected == actual,
    }
}

fn typeref_to_typeid(interner: &mut TypeInterner, t: &xu_parser::TypeRef) -> TypeId {
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
    } else {
        interner.intern(Type::Struct(t.name.clone()))
    }
}

fn type_to_string(t: &xu_parser::TypeRef) -> String {
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
