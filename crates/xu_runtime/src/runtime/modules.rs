use super::module_loader::ImportStamp;
use super::{Env, Flow, Runtime};
use crate::Value;
use crate::value::{DictStr, ModuleInstance};

#[derive(Clone, Debug)]
pub(super) struct ImportParseCacheEntry {
    stamp: ImportStamp,
    result: Result<xu_ir::Executable, String>,
}

impl Runtime {
    pub(super) fn parse_import_cached(&mut self, key: &str) -> Result<xu_ir::Executable, String> {
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
                return Err(super::diag::render_parse_error(key, text, err));
            }

            Ok(compiled.executable)
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

pub(super) fn builtin_import(rt: &mut Runtime, args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err(rt.error(xu_syntax::DiagnosticKind::ArgumentCountMismatch {
            expected_min: 1,
            expected_max: 1,
            actual: args.len(),
        }));
    }
    let path = if args[0].get_tag() == crate::value::TAG_STR {
        if let crate::gc::ManagedObject::Str(s) = rt.heap.get(args[0].as_obj_id()) {
            s.clone()
        } else {
            return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
                expected: "string".to_string(),
                actual: args[0].type_name().to_string(),
            }));
        }
    } else {
        return Err(rt.error(xu_syntax::DiagnosticKind::TypeMismatch {
            expected: "string".to_string(),
            actual: args[0].type_name().to_string(),
        }));
    };
    let key = rt.module_loader.resolve_key(rt, &path)?;
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
        merge_exports_into_env(rt, &v);
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
        let executable = rt.parse_import_cached(&key)?;
        let (module, bytecode) = match executable {
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
            Some(bc) => match super::ir::run_bytecode(rt, bc)? {
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

        let mut exports: DictStr = crate::value::dict_str_new();
        let frame_rc = module_env.global_frame();
        let frame0 = frame_rc.borrow();
        for (k, idx) in frame0.names.iter() {
            if builtins.iter().any(|b| b == k) {
                continue;
            }
            if k.starts_with('_') {
                continue;
            }
            if let Some(v) = frame0.values.get(*idx) {
                exports.map.insert(k.clone(), v.clone());
            }
        }
        let module_obj = Value::module(
            rt.heap
                .alloc(crate::gc::ManagedObject::Module(ModuleInstance { exports })),
        );
        rt.loaded_modules.insert(key.clone(), module_obj.clone());
        merge_exports_into_env(rt, &module_obj);
        if trace_import {
            eprintln!("import_done: {}", key);
        }
        Ok(module_obj)
    })();
    rt.import_stack.pop();
    result
}

fn merge_exports_into_env(rt: &mut Runtime, module_obj: &Value) {
    let exports = if module_obj.get_tag() == crate::value::TAG_MODULE {
        let id = module_obj.as_obj_id();
        if let crate::gc::ManagedObject::Module(m) = rt.heap.get(id) {
            m.exports.clone()
        } else {
            return;
        }
    } else {
        return;
    };
    let env = &mut rt.env;
    for (k, v) in exports.map.iter() {
        env.define(k.clone(), v.clone());
    }
}
