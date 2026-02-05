use std::rc::Rc;

use smallvec::SmallVec;
use xu_ir::Expr;

use crate::Value;
use crate::core::value::{BytecodeFunction, Function, UserFunction};

use crate::{Flow, Runtime};
use crate::util::type_matches;
use crate::runtime::type_check::{compute_type_signature, should_use_type_ic, type_sig_matches};

/// 函数调用上下文，用于保存和恢复状态
struct CallContext {
    saved_env: Option<crate::Env>,
    saved_func: Option<String>,
    saved_param_bindings: Option<Vec<(String, usize)>>,
    saved_frame_depth: usize,
}

impl CallContext {
    fn save(rt: &mut Runtime, fun_env: &crate::Env) -> Self {
        let mut call_env = rt.pools.env_pool.pop().unwrap_or_default();
        call_env.reset_for_call_from(fun_env);
        let saved_env = std::mem::replace(&mut rt.env, call_env);
        Self {
            saved_env: Some(saved_env),
            saved_func: rt.current_func.take(),
            saved_param_bindings: rt.current_param_bindings.take(),
            saved_frame_depth: rt.func_entry_frame_depth,
        }
    }

    fn restore(self, rt: &mut Runtime) {
        rt.pop_locals();
        let call_env = std::mem::replace(&mut rt.env, self.saved_env.expect("saved_env was set"));
        rt.pools.env_pool.push(call_env);
        rt.current_func = self.saved_func;
        rt.current_param_bindings = self.saved_param_bindings;
        rt.func_entry_frame_depth = self.saved_frame_depth;
    }
}

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
        // 快速路径：无需环境帧且参数完全匹配
        if !fun.needs_env_frame && fun.def.params.len() == args.len() && fun.def.params.iter().all(|p| p.default.is_none()) {
            let use_type_ic = should_use_type_ic(&fun.def.params, args.len());
            let mut skip_type_checks = false;
            let mut type_sig = 0u64;
            if use_type_ic {
                type_sig = compute_type_signature(args, &self.heap);
                skip_type_checks = type_sig_matches(fun.type_sig_ic.get(), type_sig);
            }
            if !skip_type_checks {
                self.check_param_types(&fun.def.name, &fun.def.params, args)?;
                if use_type_ic { fun.type_sig_ic.set(Some(type_sig)); }
            }
            if let Some(res) = crate::vm::run_bytecode_fast_params_only(self, &fun.bytecode, &fun.def.params, args) {
                let v = res?;
                self.check_return_type(&fun.def.return_ty, &v)?;
                return Ok(v);
            }
        }

        let ctx = CallContext::save(self, &fun.env);
        if fun.needs_env_frame { self.env.push(); }
        self.push_locals();
        self.func_entry_frame_depth = self.locals.maps.len();
        self.current_func = Some(fun.def.name.clone());
        if fun.needs_env_frame {
            if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
                self.locals.init_from_index_map(idxmap);
            }
        }
        if fun.locals_count > 0 {
            if let Some(values) = self.locals.values.last_mut() {
                if values.len() < fun.locals_count { values.resize(fun.locals_count, Value::UNIT); }
            }
        }
        let param_indices = self.get_param_indices(&fun.def.name, &fun.def.params);
        self.setup_param_bindings(&fun.def.params, &param_indices);

        let use_type_ic = should_use_type_ic(&fun.def.params, args.len());
        let mut skip_type_checks = false;
        let mut type_sig = 0u64;
        if use_type_ic {
            type_sig = compute_type_signature(args, &self.heap);
            skip_type_checks = type_sig_matches(fun.type_sig_ic.get(), type_sig);
        }

        if let Err(e) = self.bind_params(&fun.def.name, &fun.def.params, args, &param_indices, skip_type_checks) {
            ctx.restore(self);
            return Err(e);
        }
        if use_type_ic && !skip_type_checks { fun.type_sig_ic.set(Some(type_sig)); }

        let exec = if !fun.needs_env_frame {
            crate::vm::run_bytecode_fast(self, &fun.bytecode).unwrap_or_else(|| crate::vm::run_bytecode(self, &fun.bytecode))
        } else {
            crate::vm::run_bytecode(self, &fun.bytecode)
        };
        let flow = match exec {
            Ok(v) => v,
            Err(e) => { ctx.restore(self); return Err(e); }
        };
        ctx.restore(self);
        self.handle_flow(flow, &fun.def.return_ty)
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

    fn call_user_function_impl(&mut self, fun: &UserFunction, args: &[Value]) -> Result<Value, String> {
        let ctx = CallContext::save(self, &fun.env);
        if fun.needs_env_frame { self.env.push(); }
        self.push_locals();
        self.func_entry_frame_depth = self.locals.maps.len();
        self.current_func = Some(fun.def.name.clone());
        if !fun.skip_local_map {
            if let Some(idxmap) = self.compiled_locals_idx.get(&fun.def.name) {
                self.locals.init_from_index_map(idxmap);
            }
        }
        let param_indices = if let Some(indices) = fun.fast_param_indices.as_ref() {
            Some(indices.iter().copied().map(Some).collect::<Vec<_>>())
        } else {
            self.get_param_indices(&fun.def.name, &fun.def.params)
        };
        self.setup_param_bindings(&fun.def.params, &param_indices);
        if let Some(size) = fun.fast_locals_size {
            if let Some(values) = self.locals.values.last_mut() {
                if values.len() < size { values.resize(size, Value::UNIT); }
            }
        }

        let use_type_ic = should_use_type_ic(&fun.def.params, args.len());
        let mut skip_type_checks = false;
        let mut type_sig = 0u64;
        if use_type_ic {
            type_sig = compute_type_signature(args, &self.heap);
            skip_type_checks = type_sig_matches(fun.type_sig_ic.get(), type_sig);
        }

        // 绑定参数，使用快速路径
        for (idx, p) in fun.def.params.iter().enumerate() {
            let v = if idx < args.len() {
                args[idx]
            } else if let Some(d) = &p.default {
                match self.eval_expr(d) {
                    Ok(v) => v,
                    Err(e) => { ctx.restore(self); return Err(e); }
                }
            } else {
                Value::UNIT
            };
            if !skip_type_checks {
                if let Some(ty) = &p.ty {
                    let tn = ty.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        ctx.restore(self);
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                            name: fun.def.name.clone(), param: p.name.clone(),
                            expected: tn.to_string(), actual: v.type_name().to_string(),
                        }));
                    }
                }
            }
            if let Some(param_idxs) = fun.fast_param_indices.as_ref() {
                if idx < param_idxs.len() {
                    self.locals.set_by_index(param_idxs[idx], v);
                    continue;
                }
            }
            self.define_local(p.name.clone(), v);
        }
        if use_type_ic && !skip_type_checks { fun.type_sig_ic.set(Some(type_sig)); }

        let flow = self.exec_stmts(&fun.def.body);
        ctx.restore(self);
        self.handle_flow(flow, &fun.def.return_ty)
    }

    /// 检查参数类型
    fn check_param_types(&self, func_name: &str, params: &[xu_ir::Param], args: &[Value]) -> Result<(), String> {
        for (idx, p) in params.iter().enumerate() {
            if let Some(ty) = &p.ty {
                let tn = ty.name.as_str();
                let v = args[idx];
                if !type_matches(tn, &v, &self.heap) {
                    return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                        name: func_name.to_string(), param: p.name.clone(),
                        expected: tn.to_string(), actual: v.type_name().to_string(),
                    }));
                }
            }
        }
        Ok(())
    }

    /// 检查返回类型
    fn check_return_type(&self, ret_ty: &Option<xu_ir::TypeRef>, v: &Value) -> Result<(), String> {
        if let Some(ret) = ret_ty {
            let tn = ret.name.as_str();
            if !type_matches(tn, v, &self.heap) {
                return Err(self.error(xu_syntax::DiagnosticKind::ReturnTypeMismatch {
                    expected: tn.to_string(), actual: v.type_name().to_string(),
                }));
            }
        }
        Ok(())
    }

    /// 获取参数索引
    fn get_param_indices(&self, func_name: &str, params: &[xu_ir::Param]) -> Option<Vec<Option<usize>>> {
        if let Some(idxmap) = self.compiled_locals_idx.get(func_name) {
            Some(params.iter().map(|p| idxmap.get(p.name.as_str()).copied()).collect())
        } else {
            Some((0..params.len()).map(Some).collect())
        }
    }

    /// 设置参数绑定
    fn setup_param_bindings(&mut self, params: &[xu_ir::Param], param_indices: &Option<Vec<Option<usize>>>) {
        if let Some(indices) = param_indices {
            let bindings: Vec<(String, usize)> = params.iter().enumerate()
                .filter_map(|(i, p)| indices.get(i).and_then(|&idx| idx.map(|idx| (p.name.clone(), idx))))
                .collect();
            self.current_param_bindings = Some(bindings);
        } else {
            self.current_param_bindings = None;
        }
    }

    /// 绑定参数值
    fn bind_params(&mut self, func_name: &str, params: &[xu_ir::Param], args: &[Value], param_indices: &Option<Vec<Option<usize>>>, skip_type_checks: bool) -> Result<(), String> {
        for (idx, p) in params.iter().enumerate() {
            let v = if idx < args.len() {
                args[idx]
            } else if let Some(d) = &p.default {
                self.eval_expr(d)?
            } else {
                Value::UNIT
            };
            if !skip_type_checks {
                if let Some(ty) = &p.ty {
                    let tn = ty.name.as_str();
                    if !type_matches(tn, &v, &self.heap) {
                        return Err(self.error(xu_syntax::DiagnosticKind::TypeMismatchDetailed {
                            name: func_name.to_string(), param: p.name.clone(),
                            expected: tn.to_string(), actual: v.type_name().to_string(),
                        }));
                    }
                }
            }
            if let Some(indices) = param_indices {
                if idx < indices.len() {
                    if let Some(pidx) = indices[idx] {
                        let _ = self.locals.set_by_index(pidx, v);
                        continue;
                    }
                }
            }
            self.define_local(p.name.clone(), v);
        }
        Ok(())
    }

    /// 处理函数返回流程
    fn handle_flow(&self, flow: Flow, ret_ty: &Option<xu_ir::TypeRef>) -> Result<Value, String> {
        match flow {
            Flow::Return(v) => {
                self.check_return_type(ret_ty, &v)?;
                Ok(v)
            }
            Flow::None => Ok(Value::UNIT),
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
