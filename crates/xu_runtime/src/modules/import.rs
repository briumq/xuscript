use super::loader::ImportStamp;
use crate::{Env, Flow, Runtime};
use crate::core::value::Value;
use crate::core::value::{DictStr, ModuleInstance};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub(crate) struct ImportParseCacheEntry {
    stamp: ImportStamp,
    result: Result<ImportParseResult, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct ImportParseResult {
    executable: xu_ir::Executable,
}

impl Runtime {
    pub(crate) fn parse_import_cached(&mut self, key: &str) -> Result<ImportParseResult, String> {
        let (input, stamp) = self.module_loader.load_text_and_stamp(self, key)?;
        if let Some(e) = self.import_parse_cache.get(key) {
            if e.stamp == stamp {
                return e.result.clone();
            }
        }

        let result = (|| {
            let Some(frontend) = self.frontend.as_ref() else {
                return Err(self.error(xu_syntax::DiagnosticKind::ImportFailed(
                    "Runtime frontend is not configured".into(),
                )));
            };
            let compiled = frontend.compile_text_no_analyze(key, &input)?;
            let text = compiled.text;
            let diags = compiled.diagnostics;
            if let Some(err) = diags
                .iter()
                .find(|d| matches!(d.severity, xu_syntax::Severity::Error))
            {
                return Err(crate::util::render_parse_error(key, text, err));
            }

            Ok(ImportParseResult {
                executable: compiled.executable,
            })
        })();

        self.import_parse_cache.insert(
            key.to_string(),
            ImportParseCacheEntry {
                stamp,
                result: result.clone(),
            },
        );

        result
    }
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

pub(crate) fn import_path(rt: &mut Runtime, path: &str) -> Result<Value, String> {
    let key = rt.module_loader.resolve_key(rt, path)?;
    #[cfg(test)]
    let trace_import = std::env::var("XU_TRACE_IMPORT")
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
    #[cfg(not(test))]
    let trace_import = false;
    if trace_import {
        eprintln!("import: {}", key);
    }
    if let Some(v) = rt.loaded_modules.get(&key).cloned() {
        if trace_import {
            eprintln!("import_done: {}", key);
        }
        return Ok(v);
    }

    if let Some(pos) = rt.import_stack.iter().position(|p| p == &key) {
        let mut chain: Vec<String> = rt.import_stack[pos..].to_vec();
        chain.push(key.clone());
        return Err(rt.error(xu_syntax::DiagnosticKind::CircularImport(chain)));
    }

    rt.import_stack.push(key.clone());
    let result = (|| {
        let parsed = rt.parse_import_cached(&key)?;
        let (module, bytecode) = match parsed.executable {
            xu_ir::Executable::Ast(m) => (m, None),
            xu_ir::Executable::Bytecode(p) => (p.module, p.bytecode),
        };

        let saved_env = rt.env.clone();
        rt.env = Env::new();
        rt.install_builtins();
        let new_locals = Runtime::collect_func_locals(&module);
        let new_locals_idx = Runtime::index_func_locals(&new_locals);
        rt.compiled_locals.extend(new_locals);
        rt.compiled_locals_idx.extend(new_locals_idx);

        let builtins: Vec<String> = rt
            .env
            .global_frame()
            .borrow()
            .names
            .keys()
            .cloned()
            .collect();

        Runtime::precompile_module(&module)?;
        let exec_result = match bytecode.as_ref() {
            Some(bc) => match crate::vm::run_bytecode(rt, bc)? {
                Flow::None | Flow::Return(_) => Ok(()),
                Flow::Throw(v) => Err(rt.format_throw(&v)),
                Flow::Break | Flow::Continue => {
                    Err(rt.error(xu_syntax::DiagnosticKind::TopLevelBreakContinue))
                }
            },
            None => match rt.exec_stmts(&module.stmts) {
                Flow::None | Flow::Return(_) => Ok(()),
                Flow::Throw(v) => Err(rt.format_throw(&v)),
                Flow::Break | Flow::Continue => {
                    Err(rt.error(xu_syntax::DiagnosticKind::TopLevelBreakContinue))
                }
            },
        };

        let module_env = rt.env.clone();
        rt.env = saved_env;
        exec_result?;

        let mut inner_names: HashSet<String> = HashSet::new();
        for s in module.stmts.iter() {
            match s {
                xu_ir::Stmt::FuncDef(def) => {
                    if def.vis == xu_ir::Visibility::Inner {
                        inner_names.insert(def.name.clone());
                    }
                }
                xu_ir::Stmt::StructDef(def) => {
                    if def.vis == xu_ir::Visibility::Inner {
                        inner_names.insert(def.name.clone());
                    }
                }
                xu_ir::Stmt::EnumDef(def) => {
                    if def.vis == xu_ir::Visibility::Inner {
                        inner_names.insert(def.name.clone());
                    }
                }
                xu_ir::Stmt::Assign(a) => {
                    if a.decl.is_some() && a.vis == xu_ir::Visibility::Inner {
                        if let xu_ir::Expr::Ident(name, _) = &a.target {
                            inner_names.insert(name.clone());
                        }
                    }
                }
                _ => {}
            }
        }

        let mut exports: DictStr = crate::core::value::dict_str_new();
        let frame_rc = module_env.global_frame();
        let frame0 = frame_rc.borrow();
        for (k, idx) in frame0.names.iter() {
            if builtins.iter().any(|b| b == k) {
                continue;
            }
            if k.starts_with('_') {
                continue;
            }
            if inner_names.contains(k) {
                continue;
            }
            if let Some(v) = frame0.values.get(*idx) {
                exports.map.insert(k.clone(), v.clone());
            }
        }
        let module_obj = Value::module(
            rt.heap
                .alloc(crate::core::heap::ManagedObject::Module(Box::new(ModuleInstance { exports }))),
        );
        rt.loaded_modules.insert(key.clone(), module_obj.clone());
        if trace_import {
            eprintln!("import_done: {}", key);
        }
        Ok(module_obj)
    })();
    rt.import_stack.pop();
    result
}
