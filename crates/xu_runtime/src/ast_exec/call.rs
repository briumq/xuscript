use std::rc::Rc;

use smallvec::SmallVec;
use xu_ir::Expr;

use crate::Value;
use crate::core::value::{BytecodeFunction, Function, UserFunction};

use crate::{Flow, Runtime};
use crate::util::type_matches;
use crate::runtime::type_check::{compute_type_signature, should_use_type_ic, type_sig_matches};

impl Runtime {
    pub(crate) fn call_function(&mut self, f: Value, args: &[Value]) -> Result<Value, String> {
        if f.get_tag() != crate::core::value::TAG_FUNC {
            return Err(self.error(xu_syntax::DiagnosticKind::NotCallable(
                f.type_name().to_string(),
            )));
        }
        let id = f.as_obj_id();
        let func_obj = if let crate::core::heap::ManagedObject::Function(f) = self.heap.get(id) {
            f.clone()
        } else {
            return Err("Not a function".to_string());
        };

        match func_obj {
            Function::Builtin(fun) => fun(self, args),
            Function::User(fun) => {
                if fun.def.name == "main" {
                    self.main_invoked = true;
                }
                self.call_user_function(fun, args)
            }
            Function::Bytecode(fun) => {
                if fun.def.name == "main" {
                    self.main_invoked = true;
                }
                self.call_bytecode_function(fun, args)
            }
        }
    }

    pub(crate) fn call_bytecode_function(
        &mut self,
        fun: Rc<BytecodeFunction>,
        args: &[Value],
    ) -> Result<Value, String> {
        self.call_stack_depth += 1;
        if self.call_stack_depth > 100 {
            self.call_stack_depth -= 1;
            return Err(self.error(xu_syntax::DiagnosticKind::RecursionLimitExceeded));
        }
        let res = self.call_bytecode_function_impl(&fun, args);
        self.call_stack_depth -= 1;
        res
    }

    fn call_bytecode_function_impl(
        &mut self,
        fun: &BytecodeFunction,
        args: &[Value],
    ) -> Result<Value, String> {
        if !fun.needs_env_frame && fun.def.params.len() == args.len() {
            if fun.def.params.iter().all(|p| p.default.is_none()) {
                let use_type_ic = should_use_type_ic(&fun.def.params, args.len());
                let mut skip_type_checks = false;
                let mut type_sig = 0u64;
                if use_type_ic {
                    type_sig = compute_type_signature(args, &self.heap);
                    skip_type_checks = type_sig_matches(fun.type_sig_ic.get(), type_sig);
                }
                if !skip_type_checks {
                    for (idx, p) in fun.def.params.iter().enumerate() {
                        if let Some(ty) = &p.ty {
                            let tn = ty.name.as_str();
                            let v = args[idx];
                            if !type_matches(tn, &v, &self.heap) {
                                return Err(self.error(
                                    xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                                        name: fun.def.name.clone(),
                                        param: p.name.clone(),
                                        expected: tn.to_string(),
                                        actual: v.type_name().to_string(),
                                    },
                                ));
                            }
                        }
                    }
                    if use_type_ic {
                        fun.type_sig_ic.set(Some(type_sig));
                    }
                }
                if let Some(res) = crate::vm::run_bytecode_fast_params_only(
                    self,
                    &fun.bytecode,
                    &fun.def.params,
                    args,
                ) {
                    let v = res?;
                    if let Some(ret) = &fun.def.return_ty {
                        let tn = ret.name.as_str();
                        if !type_matches(tn, &v, &self.heap) {
                            return Err(self.error(
                                xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                                    expected: tn.to_string(),
                                    actual: v.type_name().to_string(),
                                },
                            ));
                        }
                    }
                    return Ok(v);
                }
            }
        }

        let mut call_env = self.pools.env_pool.pop().unwrap_or_else(crate::Env::new);
        call_env.reset_for_call_from(&fun.env);
        let saved_env = std::mem::replace(&mut self.env, call_env);
        let mut saved_env = Some(saved_env);
        let saved_func = self.current_func.take();
        let mut saved_param_bindings = self.current_param_bindings.take();
        let saved_frame_depth = self.func_entry_frame_depth;

        if fun.needs_env_frame {
            self.env.push();
        }
        self.push_locals();
        // Record the frame depth after pushing the function's frame
        self.func_entry_frame_depth = self.locals.maps.len();
        self.current_func = Some(fun.def.name.clone());
        if fun.needs_env_frame {
            if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
                self.locals.init_from_index_map(idxmap);
            }
        }
        if fun.locals_count > 0 {
            if let Some(values) = self.locals.values.last_mut() {
                if values.len() < fun.locals_count {
                    values.resize(fun.locals_count, Value::VOID);
                }
            }
        }
        let param_indices = if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
            let indices = fun
                .def
                .params
                .iter()
                .map(|p| idxmap.get(p.name.as_str()).copied())
                .collect::<Vec<_>>();
            Some(indices)
        } else {
            let indices = (0..fun.def.params.len()).map(Some).collect::<Vec<_>>();
            Some(indices)
        };
        if let Some(indices) = param_indices.as_ref() {
            let mut bindings: Vec<(String, usize)> = Vec::with_capacity(indices.len());
            for (i, p) in fun.def.params.iter().enumerate() {
                if let Some(Some(idx)) = indices.get(i) {
                    bindings.push((p.name.clone(), *idx));
                }
            }
            self.current_param_bindings = Some(bindings);
        } else {
            self.current_param_bindings = None;
        }

        let use_type_ic = should_use_type_ic(&fun.def.params, args.len());
        let mut skip_type_checks = false;
        let mut type_sig = 0u64;
        if use_type_ic {
            type_sig = compute_type_signature(args, &self.heap);
            skip_type_checks = type_sig_matches(fun.type_sig_ic.get(), type_sig);
        }

        for (idx, p) in fun.def.params.iter().enumerate() {
            let v = if idx < args.len() {
                args[idx]
            } else if let Some(d) = &p.default {
                match self.eval_expr(d) {
                    Ok(v) => v,
                    Err(e) => {
                        self.pop_locals();
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
                        self.pools.env_pool.push(call_env);
                        self.current_func = saved_func;
                        self.current_param_bindings = saved_param_bindings.take();
                        return Err(e);
                    }
                }
            } else {
                Value::VOID
            };
            if !skip_type_checks {
                if let Some(ty) = &p.ty {
                    let tn = ty.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
                        self.pools.env_pool.push(call_env);
                        self.pop_locals();
                        self.current_func = saved_func;
                        self.current_param_bindings = saved_param_bindings.take();
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                            name: fun.def.name.clone(),
                            param: p.name.clone(),
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
            }
            if let Some(indices) = param_indices.as_ref() {
                if idx < indices.len() {
                    if let Some(pidx) = indices[idx] {
                        let _ = self.locals.set_by_index(pidx, v);
                        continue;
                    }
                }
            }
            self.define_local(p.name.clone(), v);
        }
        if use_type_ic && !skip_type_checks {
            fun.type_sig_ic.set(Some(type_sig));
        }

        let exec = if !fun.needs_env_frame {
            crate::vm::run_bytecode_fast(self, &fun.bytecode)
                .unwrap_or_else(|| crate::vm::run_bytecode(self, &fun.bytecode))
        } else {
            crate::vm::run_bytecode(self, &fun.bytecode)
        };
        let flow = match exec {
            Ok(v) => v,
            Err(e) => {
                self.pop_locals();
                let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
                self.pools.env_pool.push(call_env);
                self.current_func = saved_func;
                self.current_param_bindings = saved_param_bindings.take();
                self.func_entry_frame_depth = saved_frame_depth;
                return Err(e);
            }
        };
        self.pop_locals();
        let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
        self.pools.env_pool.push(call_env);
        self.current_func = saved_func;
        self.current_param_bindings = saved_param_bindings.take();
        self.func_entry_frame_depth = saved_frame_depth;

        match flow {
            Flow::Return(v) => {
                if let Some(ret) = &fun.def.return_ty {
                    let tn = ret.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        return Err(self.error(xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
                Ok(v)
            }
            Flow::None => Ok(Value::VOID),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => Err(self.error(
                xu_syntax::DiagnosticKind::UnexpectedControlFlowInFunction("break or continue"),
            )),
        }
    }

    pub(crate) fn call_user_function(
        &mut self,
        fun: Rc<UserFunction>,
        args: &[Value],
    ) -> Result<Value, String> {
        self.call_stack_depth += 1;
        if self.call_stack_depth > 100 {
            self.call_stack_depth -= 1;
            return Err(self.error(xu_syntax::DiagnosticKind::RecursionLimitExceeded));
        }

        let res = self.call_user_function_impl(&fun, args);
        self.call_stack_depth -= 1;
        res
    }

    fn call_user_function_impl(&mut self, fun: &UserFunction, args: &[Value]) -> Result<Value, String>
    {
        let mut call_env = self.pools.env_pool.pop().unwrap_or_else(crate::Env::new);
        call_env.reset_for_call_from(&fun.env);
        let saved_env = std::mem::replace(&mut self.env, call_env);
        let mut saved_env = Some(saved_env);
        let saved_func = self.current_func.take();
        let mut saved_param_bindings = self.current_param_bindings.take();
        let saved_frame_depth = self.func_entry_frame_depth;

        if fun.needs_env_frame {
            self.env.push();
        }
        self.push_locals();
        // Record the frame depth after pushing the function's frame
        self.func_entry_frame_depth = self.locals.maps.len();
        self.current_func = Some(fun.def.name.clone());
        if !fun.skip_local_map {
            if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
                self.locals.init_from_index_map(idxmap);
            }
        }
        let param_indices = if let Some(indices) = fun.fast_param_indices.as_ref() {
            Some(indices.iter().copied().map(Some).collect::<Vec<_>>())
        } else if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
            let indices = fun
                .def
                .params
                .iter()
                .map(|p| idxmap.get(p.name.as_str()).copied())
                .collect::<Vec<_>>();
            Some(indices)
        } else {
            let indices = (0..fun.def.params.len()).map(Some).collect::<Vec<_>>();
            Some(indices)
        };
        if let Some(indices) = param_indices.as_ref() {
            let mut bindings: Vec<(String, usize)> = Vec::with_capacity(indices.len());
            for (i, p) in fun.def.params.iter().enumerate() {
                if let Some(Some(idx)) = indices.get(i) {
                    bindings.push((p.name.clone(), *idx));
                }
            }
            self.current_param_bindings = Some(bindings);
        } else {
            self.current_param_bindings = None;
        }
        if let Some(size) = fun.fast_locals_size {
            if let Some(values) = self.locals.values.last_mut() {
                if values.len() < size {
                    values.resize(size, Value::VOID);
                }
            }
        }

        let use_type_ic = should_use_type_ic(&fun.def.params, args.len());
        let mut skip_type_checks = false;
        let mut type_sig = 0u64;
        if use_type_ic {
            type_sig = compute_type_signature(args, &self.heap);
            skip_type_checks = type_sig_matches(fun.type_sig_ic.get(), type_sig);
        }

        for (idx, p) in fun.def.params.iter().enumerate() {
            let v = if idx < args.len() {
                args[idx]
            } else if let Some(d) = &p.default {
                match self.eval_expr(d) {
                    Ok(v) => v,
                    Err(e) => {
                        self.pop_locals();
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
                        self.pools.env_pool.push(call_env);
                        self.current_func = saved_func;
                        self.current_param_bindings = saved_param_bindings.take();
                        return Err(e);
                    }
                }
            } else {
                Value::VOID
            };
            if !skip_type_checks {
                if let Some(ty) = &p.ty {
                    let tn = ty.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
                        self.pools.env_pool.push(call_env);
                        self.pop_locals();
                        self.current_func = saved_func;
                        self.current_param_bindings = saved_param_bindings.take();
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                            name: fun.def.name.clone(),
                            param: p.name.clone(),
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
            } else if let Some(ty) = &p.ty {
                let _ = ty;
            }
            if let Some(param_idxs) = fun.fast_param_indices.as_ref() {
                if idx < param_idxs.len() {
                    self.locals.set_by_index(param_idxs[idx], v);
                    continue;
                }
            }
            self.define_local(p.name.clone(), v);
        }
        if use_type_ic && !skip_type_checks {
            fun.type_sig_ic.set(Some(type_sig));
        }

        let flow = self.exec_stmts(&fun.def.body);
        self.pop_locals();
        let call_env = std::mem::replace(&mut self.env, saved_env.take().expect("saved_env was set at function entry"));
        self.pools.env_pool.push(call_env);
        self.current_func = saved_func;
        self.current_param_bindings = saved_param_bindings.take();
        self.func_entry_frame_depth = saved_frame_depth;

        match flow {
            Flow::Return(v) => {
                if let Some(ret) = &fun.def.return_ty {
                    let tn = ret.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        return Err(self.error(xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                            expected: tn.to_string(),
                            actual: v.type_name().to_string(),
                        }));
                    }
                }
                Ok(v)
            }
            Flow::None => Ok(Value::VOID),
            Flow::Throw(v) => Err(self.format_throw(&v)),
            Flow::Break | Flow::Continue => Err(self.error(
                xu_syntax::DiagnosticKind::UnexpectedControlFlowInFunction("break or continue"),
            )),
        }
    }

    pub(crate) fn eval_args(&mut self, args: &[Expr]) -> Result<SmallVec<[Value; 4]>, String> {
        let mut out: SmallVec<[Value; 4]> = SmallVec::with_capacity(args.len());
        let roots_base = self.gc_temp_roots.len();
        for a in args {
            let v = self.eval_expr(a)?;
            // Push to gc_temp_roots as GC root protection
            self.gc_temp_roots.push(v);
            out.push(v);
        }
        // Pop the temporary roots
        self.gc_temp_roots.truncate(roots_base);
        Ok(out)
    }
}
