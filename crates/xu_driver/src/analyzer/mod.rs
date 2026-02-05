//! Analyzer module.
//!
//! Responsible for semantic analysis, type checking, and scope management.
//! Refactored in v1.1 to enforce strict mode and remove legacy features.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use xu_lexer::Lexer;
use xu_parser::Parser;
use xu_syntax::{
    Diagnostic, DiagnosticKind, codes, SourceFile,
    BUILTIN_NAMES,
};

mod utils;
mod expr;
mod stmt;
mod types;

pub use types::type_to_string;
// Re-export StructMap for submodules
pub(crate) type StructMap = HashMap<String, HashMap<String, String>>;

use utils::Finder;
use stmt::analyze_stmts;
use types::analyze_types;

#[derive(Clone, Default, Debug)]
pub struct ImportCache {
    pub modules: HashMap<PathBuf, (Vec<String>, StructMap)>,
}

pub(crate) fn analyze_module(
    source: &SourceFile,
    tokens: &[xu_syntax::Token],
    module: &mut xu_parser::Module,
    strict: bool,
    cache: Arc<RwLock<ImportCache>>,
    import_stack: &mut Vec<PathBuf>,
    extra_predefs: &[&str],
) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    // Check for circular import at the entry of module analysis
    let current_path = PathBuf::from(&source.name);
    let current_path = current_path.canonicalize().unwrap_or(current_path);

    if import_stack.contains(&current_path) {
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
                // Also collect static fields with a special prefix
                for sf in &def.static_fields {
                    fields.insert(format!("static:{}", sf.name), type_to_string(&sf.ty));
                }
                structs.insert(def.name.clone(), fields);
                // Register methods defined in the has block
                for method in def.methods.iter() {
                    let max = method.params.len();
                    let min = method.params.iter().filter(|p| p.default.is_none()).count();
                    funcs.insert(method.name.clone(), (min, max));
                }
            }
            _ => {}
        }
    }

    let mut scope: Vec<HashMap<String, usize>> = vec![HashMap::new()];
    let mut def_spans: Vec<HashMap<String, xu_syntax::Span>> = vec![HashMap::new()];
    for builtin in BUILTIN_NAMES {
        let idx = scope.last().expect("scope stack should not be empty").len();
        scope.last_mut().expect("scope stack should not be empty").insert(builtin.to_string(), idx);
    }
    for name in extra_predefs {
        let idx = scope.last().expect("scope stack should not be empty").len();
        scope.last_mut().expect("scope stack should not be empty").insert((*name).to_string(), idx);
    }
    for name in funcs.keys() {
        let idx = scope.last().expect("scope stack should not be empty").len();
        scope.last_mut().expect("scope stack should not be empty").insert(name.clone(), idx);
    }
    for name in structs.keys() {
        let idx = scope.last().expect("scope stack should not be empty").len();
        scope.last_mut().expect("scope stack should not be empty").insert(name.clone(), idx);
    }

    let mut sem_finder = Finder::new(source, tokens);
    let base_dir = Path::new(&source.name).parent().unwrap_or(Path::new("."));

    // Use the strict parameter passed in
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

pub(crate) fn process_import(
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
                true, // strict
                cache.clone(),
                import_stack,
                &[],
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
                    // Also collect static fields with a special prefix
                    for sf in &def.static_fields {
                        fields.insert(format!("static:{}", sf.name), type_to_string(&sf.ty));
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

pub(crate) fn infer_module_alias(path: &str) -> String {
    let mut last = path;
    if let Some((_, tail)) = path.rsplit_once('/') {
        last = tail;
    } else if let Some((_, tail)) = path.rsplit_once('\\') {
        last = tail;
    }
    let last = last.trim_end_matches('/');
    let last = last.trim_end_matches('\\');
    last.strip_suffix(".xu").unwrap_or(last).to_string()
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
