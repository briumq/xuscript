//!
//!
//!
//!

use std::collections::HashSet;
use xu_ir::{
    AssignOp, AssignStmt, BinaryOp, Bytecode, BytecodeFunction, Expr, IfStmt, Module, Op, Pattern,
    ReceiverType, Stmt, UnaryOp, infer_module_alias,
};

pub fn compile_module(module: &Module) -> Option<Bytecode> {
    let mut c = Compiler::new();
    for s in &module.stmts {
        c.compile_stmt(s)?;
    }
    c.bc.ops.push(Op::Halt);
    Some(c.bc)
}

/// Check if an expression is a to_text(expr) call and return the inner expression
fn extract_to_text_arg(expr: &Expr) -> Option<&Expr> {
    if let Expr::Call(c) = expr {
        if let Expr::Ident(name, _) = c.callee.as_ref() {
            if name == "to_text" && c.args.len() == 1 {
                return Some(&c.args[0]);
            }
        }
    }
    None
}

/// Collect all binding names from a pattern in order
fn collect_pattern_bindings(pat: &Pattern) -> Vec<String> {
    let mut bindings = Vec::new();
    collect_pattern_bindings_impl(pat, &mut bindings);
    bindings
}

fn collect_pattern_bindings_impl(pat: &Pattern, bindings: &mut Vec<String>) {
    match pat {
        Pattern::Wildcard => {}
        Pattern::Bind(name) => bindings.push(name.clone()),
        Pattern::Tuple(items) => {
            for p in items.iter() {
                collect_pattern_bindings_impl(p, bindings);
            }
        }
        Pattern::Int(_) | Pattern::Float(_) | Pattern::Str(_) | Pattern::Bool(_) => {}
        Pattern::EnumVariant { args, .. } => {
            for p in args.iter() {
                collect_pattern_bindings_impl(p, bindings);
            }
        }
    }
}

struct LoopCtx {
    break_ops: Vec<usize>,
    continue_ops: Vec<usize>,
}

struct Scope {
    locals: Vec<String>,
}

struct Compiler {
    bc: Bytecode,
    loops: Vec<LoopCtx>,
    scopes: Vec<Scope>,
    next_ic_slot: usize,
    known_types: HashSet<String>,
    in_function: bool,  // Track if we're inside a function body
}

impl Compiler {
    fn new() -> Self {
        Self {
            bc: Bytecode::default(),
            loops: Vec::new(),
            scopes: vec![Scope { locals: Vec::new() }],
            next_ic_slot: 0,
            known_types: HashSet::new(),
            in_function: false,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope { locals: Vec::new() });
    }

    // ==================== 辅助方法 ====================

    /// 修补跳转指令的目标地址
    #[inline]
    fn patch_jump(&mut self, pos: usize, target: usize) -> Option<()> {
        match &mut self.bc.ops[pos] {
            Op::Jump(to) | Op::JumpIfFalse(to) | Op::JumpIfTrue(to) => { *to = target; Some(()) }
            _ => None,
        }
    }

    /// 修补多个跳转指令到同一目标
    #[inline]
    fn patch_jumps(&mut self, positions: &[usize], target: usize) -> Option<()> {
        for &pos in positions {
            self.patch_jump(pos, target)?;
        }
        Some(())
    }

    /// 编译语句列表
    #[inline]
    fn compile_stmts(&mut self, stmts: &[Stmt]) -> Option<()> {
        for s in stmts { self.compile_stmt(s)?; }
        Some(())
    }

    /// 编译表达式列表
    #[inline]
    fn compile_exprs(&mut self, exprs: &[Expr]) -> Option<()> {
        for e in exprs { self.compile_expr(e)?; }
        Some(())
    }

    /// 发出跳转指令并返回其位置（用于后续修补）
    #[inline]
    fn emit_jump(&mut self, op: Op) -> usize {
        let pos = self.bc.ops.len();
        self.bc.ops.push(op);
        pos
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        // Only use local variables inside functions
        if !self.in_function || self.scopes.len() <= 1 {
            return None; // Top-level variables are globals
        }
        let mut offset = 0;
        for i in 1..self.scopes.len() {
            let scope = &self.scopes[i];
            if let Some(pos) = scope.locals.iter().position(|l| l == name) {
                return Some(offset + pos);
            }
            offset += scope.locals.len();
        }
        None
    }

    fn define_local(&mut self, name: &str) -> usize {
        if self.scopes.len() <= 1 {
            // This should not be called for top-level if we want globals
            // But if it is, we return a dummy index
            return 0;
        }
        if let Some(idx) = self.resolve_local(name) {
            return idx;
        }
        let scope = self.scopes.last_mut().expect("scopes should never be empty");
        let pos = scope.locals.len();
        scope.locals.push(name.to_string());
        let mut offset = 0;
        for i in 1..self.scopes.len() - 1 {
            offset += self.scopes[i].locals.len();
        }
        offset + pos
    }

    fn alloc_ic_slot(&mut self) -> usize {
        let s = self.next_ic_slot;
        self.next_ic_slot += 1;
        s
    }

    fn add_constant(&mut self, c: xu_ir::Constant) -> u32 {
        if let Some(pos) = self.bc.constants.iter().position(|x| x == &c) {
            return pos as u32;
        }
        let pos = self.bc.constants.len() as u32;
        self.bc.constants.push(c);
        pos
    }

    fn is_const_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) => true,
            Expr::Unary { expr, .. } => self.is_const_expr(expr),
            Expr::Binary { left, right, .. } => {
                self.is_const_expr(left) && self.is_const_expr(right)
            }
            Expr::Group(e) => self.is_const_expr(e),
            _ => false,
        }
    }

    /// Compile a single part of an interpolated string and emit the appropriate StrAppend op.
    fn compile_interp_part(&mut self, expr: &Expr) -> Option<()> {
        match expr {
            Expr::Str(s) => {
                let s_idx = self.add_constant(xu_ir::Constant::Str(s.clone()));
                self.bc.ops.push(Op::Const(s_idx));
            }
            _ => {
                self.compile_expr(expr)?;
            }
        }
        self.bc.ops.push(Op::StrAppend);
        Some(())
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> Option<()> {
        match stmt {
            Stmt::Error(_) => None,
            Stmt::StructDef(def) => {
                self.known_types.insert(def.name.clone());
                let idx = self.add_constant(xu_ir::Constant::Struct((**def).clone()));
                self.bc.ops.push(Op::DefineStruct(idx));

                // Compile static field initializations
                let type_name = &def.name;
                for sf in def.static_fields.iter() {
                    self.compile_expr(&sf.default)?;
                    let t_idx = self.add_constant(xu_ir::Constant::Str(type_name.clone()));
                    let f_idx = self.add_constant(xu_ir::Constant::Str(sf.name.clone()));
                    self.bc.ops.push(Op::InitStaticField(t_idx, f_idx));
                }

                // Compile all methods defined in the has block
                for f in def.methods.iter() {
                    self.compile_func_def(f)?;
                }

                Some(())
            }
            Stmt::EnumDef(def) => {
                self.known_types.insert(def.name.clone());
                let idx = self.add_constant(xu_ir::Constant::Enum((**def).clone()));
                self.bc.ops.push(Op::DefineEnum(idx));
                Some(())
            }
            Stmt::FuncDef(def) => self.compile_func_def(def),
            Stmt::DoesBlock(def) => {
                for f in def.funcs.iter() {
                    self.compile_func_def(f)?;
                }
                Some(())
            }
            Stmt::If(s) => self.compile_if(s),
            Stmt::While(s) => self.compile_while(s),
            Stmt::ForEach(s) => self.compile_foreach(s),
            Stmt::Use(u) => {
                let path_idx = self.add_constant(xu_ir::Constant::Str(u.path.clone()));
                let alias = u
                    .alias
                    .clone()
                    .unwrap_or_else(|| infer_module_alias(&u.path));
                let alias_idx = self.add_constant(xu_ir::Constant::Str(alias));
                self.bc.ops.push(Op::Use(path_idx, alias_idx));
                Some(())
            }
            Stmt::Match(s) => self.compile_match_stmt(s),
            Stmt::Block(stmts) => {
                // For block scope, we don't need to push/pop runtime frames
                // because variables are already scoped by the compiler's scope tracking.
                // We only need EnvPush/EnvPop for top-level blocks to properly release
                // references for GC.
                if !self.in_function {
                    self.bc.ops.push(Op::EnvPush);
                }
                self.push_scope();
                for stmt in stmts.iter() {
                    self.compile_stmt(stmt)?;
                }
                self.scopes.pop();
                if !self.in_function {
                    self.bc.ops.push(Op::EnvPop);
                }
                Some(())
            }
            Stmt::Return(v) => self.compile_return(v.as_ref()),
            Stmt::Break => self.compile_break(),
            Stmt::Continue => self.compile_continue(),
            Stmt::Assign(s) => self.compile_assign(s),
            Stmt::Expr(e) => {
                if self.is_const_expr(e) {
                    return Some(());
                }
                self.compile_expr(e)?;
                self.bc.ops.push(Op::Pop);
                Some(())
            }
        }
    }

    fn compile_break(&mut self) -> Option<()> {
        let Some(ctx) = self.loops.last_mut() else {
            return None;
        };
        let pos = self.bc.ops.len();
        self.bc.ops.push(Op::Break(usize::MAX));
        ctx.break_ops.push(pos);
        Some(())
    }

    fn compile_continue(&mut self) -> Option<()> {
        let Some(ctx) = self.loops.last_mut() else {
            return None;
        };
        let pos = self.bc.ops.len();
        self.bc.ops.push(Op::Continue(usize::MAX));
        ctx.continue_ops.push(pos);
        Some(())
    }

    fn patch_loop(&mut self, ctx: LoopCtx, break_to: usize, cont_to: usize) -> Option<()> {
        for pos in ctx.break_ops {
            let Op::Break(to) = &mut self.bc.ops[pos] else {
                return None;
            };
            *to = break_to;
        }
        for pos in ctx.continue_ops {
            let Op::Continue(to) = &mut self.bc.ops[pos] else {
                return None;
            };
            *to = cont_to;
        }
        Some(())
    }

    fn compile_func_def(&mut self, def: &xu_ir::FuncDef) -> Option<()> {
        let f_idx = self.compile_func_body(def)?;
        self.bc.ops.push(Op::MakeFunction(f_idx));
        let n_idx = self.add_constant(xu_ir::Constant::Str(def.name.clone()));
        self.bc.ops.push(Op::StoreName(n_idx));
        Some(())
    }

    /// Compile function body and return constant index
    fn compile_func_body(&mut self, def: &xu_ir::FuncDef) -> Option<u32> {
        let mut inner = Compiler::new();
        inner.in_function = true;  // Mark that we're inside a function
        inner.push_scope();
        for p in &def.params {
            inner.define_local(&p.name);
        }
        for s in &def.body {
            inner.compile_stmt(s)?;
        }
        inner.bc.ops.push(Op::ConstNull);
        inner.bc.ops.push(Op::Return);
        let locals_count = inner.scopes.iter().map(|s| s.locals.len()).sum();
        let fun = BytecodeFunction {
            def: def.clone(),
            bytecode: Box::new(inner.bc),
            locals_count,
        };
        Some(self.add_constant(xu_ir::Constant::Func(fun)))
    }

    fn compile_if(&mut self, stmt: &IfStmt) -> Option<()> {
        let mut end_jumps: Vec<usize> = Vec::new();
        for (cond, body) in &stmt.branches {
            self.compile_expr(cond)?;
            let jfalse_pos = self.emit_jump(Op::JumpIfFalse(usize::MAX));
            self.compile_stmts(body)?;
            end_jumps.push(self.emit_jump(Op::Jump(usize::MAX)));
            self.patch_jump(jfalse_pos, self.bc.ops.len())?;
        }
        if let Some(body) = &stmt.else_branch {
            self.compile_stmts(body)?;
        }
        self.patch_jumps(&end_jumps, self.bc.ops.len())
    }

    fn compile_match_stmt(&mut self, stmt: &xu_ir::MatchStmt) -> Option<()> {
        self.compile_expr(&stmt.expr)?;
        let mut arm_end_jumps: Vec<usize> = Vec::new();

        for (i, (pat, body)) in stmt.arms.iter().enumerate() {
            let is_last = i == stmt.arms.len() - 1 && stmt.else_branch.is_none();
            let jump_pos = self.compile_match_pattern_check(pat)?;

            if let Some(jpos) = jump_pos {
                let bindings = collect_pattern_bindings(pat);
                if !bindings.is_empty() {
                    self.bc.ops.push(Op::EnvPush);
                    let pat_idx = self.add_constant(xu_ir::Constant::Pattern(pat.clone()));
                    self.bc.ops.push(Op::MatchBindings(pat_idx));
                    for name in bindings.iter().rev() {
                        let name_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                        self.bc.ops.push(Op::StoreName(name_idx));
                    }
                    self.compile_stmts(body)?;
                    self.bc.ops.push(Op::EnvPop);
                } else {
                    self.bc.ops.push(Op::Pop);
                    self.compile_stmts(body)?;
                }
                arm_end_jumps.push(self.emit_jump(Op::Jump(usize::MAX)));
                self.patch_jump(jpos, self.bc.ops.len())?;
            } else {
                self.bc.ops.push(Op::Pop);
                self.compile_stmts(body)?;
                if !is_last {
                    arm_end_jumps.push(self.emit_jump(Op::Jump(usize::MAX)));
                }
            }
        }

        if let Some(else_body) = &stmt.else_branch {
            self.bc.ops.push(Op::Pop);
            self.compile_stmts(else_body)?;
        }
        self.patch_jumps(&arm_end_jumps, self.bc.ops.len())
    }

    /// 编译模式检查，返回需要修补的跳转位置（通配符返回 None）
    fn compile_match_pattern_check(&mut self, pat: &Pattern) -> Option<Option<usize>> {
        if matches!(pat, Pattern::Wildcard) {
            return Some(None);
        }
        let pat_idx = self.add_constant(xu_ir::Constant::Pattern(pat.clone()));
        self.bc.ops.push(Op::MatchPattern(pat_idx));
        Some(Some(self.emit_jump(Op::JumpIfFalse(usize::MAX))))
    }

    fn compile_while(&mut self, stmt: &xu_ir::WhileStmt) -> Option<()> {
        let loop_start = self.bc.ops.len();
        self.compile_expr(&stmt.cond)?;
        let jfalse_pos = self.emit_jump(Op::JumpIfFalse(usize::MAX));
        self.loops.push(LoopCtx { break_ops: Vec::new(), continue_ops: Vec::new() });
        self.compile_stmts(&stmt.body)?;
        let ctx = self.loops.pop()?;
        self.bc.ops.push(Op::Jump(loop_start));
        let end = self.bc.ops.len();
        self.patch_jump(jfalse_pos, end)?;
        self.patch_loop(ctx, end, loop_start)
    }

    fn compile_foreach(&mut self, stmt: &xu_ir::ForEachStmt) -> Option<()> {
        self.compile_expr(&stmt.iter)?;
        // Only use local variables inside functions, not in top-level blocks
        let var_idx = if self.in_function && self.scopes.len() > 1 {
            Some(self.define_local(&stmt.var))
        } else {
            None
        };
        let n_idx = self.add_constant(xu_ir::Constant::Str(stmt.var.clone()));
        let init_pos = self.emit_jump(Op::ForEachInit(n_idx, var_idx, usize::MAX));
        self.loops.push(LoopCtx { break_ops: Vec::new(), continue_ops: Vec::new() });
        let body_start = self.bc.ops.len();
        self.compile_stmts(&stmt.body)?;
        let next_pos = self.emit_jump(Op::ForEachNext(n_idx, var_idx, body_start, usize::MAX));
        let break_cleanup = self.bc.ops.len();
        self.bc.ops.push(Op::IterPop);
        let j_to_end = self.emit_jump(Op::Jump(usize::MAX));
        let end = self.bc.ops.len();
        // 修补 ForEachInit 和 ForEachNext 的结束地址
        match &mut self.bc.ops[init_pos] {
            Op::ForEachInit(_, _, end_ip) => *end_ip = end,
            _ => return None,
        }
        match &mut self.bc.ops[next_pos] {
            Op::ForEachNext(_, _, _, end_ip) => *end_ip = end,
            _ => return None,
        }
        self.patch_jump(j_to_end, end)?;
        let ctx = self.loops.pop()?;
        self.patch_loop(ctx, break_cleanup, next_pos)
    }

    fn compile_return(&mut self, v: Option<&Expr>) -> Option<()> {
        if let Some(e) = v {
            self.compile_expr(e)?;
        } else {
            self.bc.ops.push(Op::ConstNull);
        }
        self.bc.ops.push(Op::Return);
        Some(())
    }

    fn compile_assign(&mut self, stmt: &AssignStmt) -> Option<()> {
        match &stmt.target {
            Expr::Ident(name, _) => match stmt.op {
                AssignOp::Set => {
                    self.compile_expr(&stmt.value)?;
                    if let Some(ty) = &stmt.ty {
                        let n_idx = self.add_constant(xu_ir::Constant::Str(ty.name.clone()));
                        self.bc.ops.push(Op::AssertType(n_idx));
                    }
                    if let Some(idx) = self.resolve_local(name) {
                        self.bc.ops.push(Op::StoreLocal(idx));
                    } else if self.in_function && stmt.decl.is_some() {
                        // Only create new local for declarations (let/var) inside functions
                        let idx = self.define_local(name);
                        self.bc.ops.push(Op::StoreLocal(idx));
                    } else {
                        // For assignments without decl, or top-level, use StoreName
                        // This allows closures to modify captured variables
                        let n_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                        self.bc.ops.push(Op::StoreName(n_idx));
                    }
                    Some(())
                }
                AssignOp::Add | AssignOp::Sub | AssignOp::Mul | AssignOp::Div => {
                    match stmt.op {
                        AssignOp::Add => {
                            if let Some(idx) = self.resolve_local(name) {
                                if let Expr::Int(v) = &stmt.value {
                                    if *v == 1 {
                                        self.bc.ops.push(Op::IncLocal(idx));
                                        return Some(());
                                    }
                                    self.bc.ops.push(Op::ConstInt(*v));
                                } else {
                                    self.compile_expr(&stmt.value)?;
                                }
                                self.bc.ops.push(Op::AddAssignLocal(idx));
                            } else {
                                if let Expr::Int(v) = &stmt.value {
                                    self.bc.ops.push(Op::ConstInt(*v));
                                } else {
                                    self.compile_expr(&stmt.value)?;
                                }
                                let n_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                                self.bc.ops.push(Op::AddAssignName(n_idx));
                            }
                        }
                        AssignOp::Sub | AssignOp::Mul | AssignOp::Div => {
                            if let Some(idx) = self.resolve_local(name) {
                                self.bc.ops.push(Op::LoadLocal(idx));
                            } else {
                                let n_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                                self.bc.ops.push(Op::LoadName(n_idx));
                            }
                            self.compile_expr(&stmt.value)?;
                            self.bc.ops.push(match stmt.op {
                                AssignOp::Sub => Op::Sub,
                                AssignOp::Mul => Op::Mul,
                                AssignOp::Div => Op::Div,
                                _ => unreachable!(),
                            });
                            if let Some(idx) = self.resolve_local(name) {
                                self.bc.ops.push(Op::StoreLocal(idx));
                            } else {
                                let n_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                                self.bc.ops.push(Op::StoreName(n_idx));
                            }
                        }
                        AssignOp::Set => unreachable!(),
                    }
                    Some(())
                }
            },
            Expr::Member(m) => {
                // Check if this is a static field assignment (Type.field = value)
                if let Expr::Ident(name, _) = m.object.as_ref() {
                    // Check if this identifier is a known type name
                    if self.known_types.contains(name) {
                        // Static field assignment
                        if stmt.op == AssignOp::Set {
                            self.compile_expr(&stmt.value)?;
                            let t_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                            let f_idx = self.add_constant(xu_ir::Constant::Str(m.field.clone()));
                            self.bc.ops.push(Op::SetStaticField(t_idx, f_idx));
                        } else {
                            // Compound assignment: Type.field += value
                            // Load current value
                            let t_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                            let f_idx = self.add_constant(xu_ir::Constant::Str(m.field.clone()));
                            self.bc.ops.push(Op::GetStaticField(t_idx, f_idx));
                            // Compile the RHS
                            self.compile_expr(&stmt.value)?;
                            // Apply the operation
                            self.bc.ops.push(match stmt.op {
                                AssignOp::Add => Op::Add,
                                AssignOp::Sub => Op::Sub,
                                AssignOp::Mul => Op::Mul,
                                AssignOp::Div => Op::Div,
                                AssignOp::Set => unreachable!(),
                            });
                            // Store back
                            self.bc.ops.push(Op::SetStaticField(t_idx, f_idx));
                        }
                        return Some(());
                    }
                }
                self.compile_expr(&stmt.value)?;
                self.compile_expr(&m.object)?;
                let n_idx = self.add_constant(xu_ir::Constant::Str(m.field.clone()));
                self.bc.ops.push(Op::AssignMember(n_idx, stmt.op));
                Some(())
            }
            Expr::Index(ix) => {
                self.compile_expr(&stmt.value)?;
                self.compile_expr(&ix.object)?;
                self.compile_expr(&ix.index)?;
                self.bc.ops.push(Op::AssignIndex(stmt.op));
                Some(())
            }
            _ => None,
        }
    }

    fn try_fold_unary(&self, op: UnaryOp, expr: &Expr) -> Option<Op> {
        match (op, expr) {
            (UnaryOp::Not, Expr::Bool(v)) => Some(Op::ConstBool(!*v)),
            (UnaryOp::Neg, Expr::Int(v)) => Some(Op::ConstInt(-*v)),
            (UnaryOp::Neg, Expr::Float(v)) => Some(Op::ConstFloat(-*v)),
            _ => None,
        }
    }

    fn try_fold_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Option<Op> {
        match (op, left, right) {
            // Arithmetic
            (BinaryOp::Add, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstInt(a + b)),
            (BinaryOp::Sub, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstInt(a - b)),
            (BinaryOp::Mul, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstInt(a * b)),
            (BinaryOp::Div, Expr::Int(a), Expr::Int(b)) if *b != 0 => Some(Op::ConstInt(a / b)),
            (BinaryOp::Mod, Expr::Int(a), Expr::Int(b)) if *b != 0 => Some(Op::ConstInt(a % b)),

            (BinaryOp::Add, Expr::Float(a), Expr::Float(b)) => Some(Op::ConstFloat(a + b)),
            (BinaryOp::Sub, Expr::Float(a), Expr::Float(b)) => Some(Op::ConstFloat(a - b)),
            (BinaryOp::Mul, Expr::Float(a), Expr::Float(b)) => Some(Op::ConstFloat(a * b)),
            (BinaryOp::Div, Expr::Float(a), Expr::Float(b)) => Some(Op::ConstFloat(a / b)),

            // String concatenation (simple case)
            (BinaryOp::Add, Expr::Str(a), Expr::Str(b)) => {
                let idx = self.add_constant(xu_ir::Constant::Str(format!("{}{}", a, b)));
                Some(Op::Const(idx))
            }

            // Comparison
            (BinaryOp::Eq, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstBool(a == b)),
            (BinaryOp::Ne, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstBool(a != b)),
            (BinaryOp::Gt, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstBool(a > b)),
            (BinaryOp::Lt, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstBool(a < b)),
            (BinaryOp::Ge, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstBool(a >= b)),
            (BinaryOp::Le, Expr::Int(a), Expr::Int(b)) => Some(Op::ConstBool(a <= b)),

            (BinaryOp::Eq, Expr::Bool(a), Expr::Bool(b)) => Some(Op::ConstBool(a == b)),
            (BinaryOp::Ne, Expr::Bool(a), Expr::Bool(b)) => Some(Op::ConstBool(a != b)),

            // Logical
            (BinaryOp::And, Expr::Bool(a), Expr::Bool(b)) => Some(Op::ConstBool(*a && *b)),
            (BinaryOp::Or, Expr::Bool(a), Expr::Bool(b)) => Some(Op::ConstBool(*a || *b)),

            _ => None,
        }
    }

    /// 编译表达式的主入口函数
    fn compile_expr(&mut self, expr: &Expr) -> Option<()> {
        match expr {
            Expr::Error(_) => None,
            // 字面量表达式
            Expr::Ident(name, _) => self.compile_expr_ident(name),
            Expr::Int(v) => self.compile_expr_int(*v),
            Expr::Float(v) => self.compile_expr_float(*v),
            Expr::Bool(v) => self.compile_expr_bool(*v),
            Expr::Str(s) => self.compile_expr_str(s),
            // 一元和二元运算
            Expr::Unary { op, expr } => self.compile_expr_unary(*op, expr),
            Expr::Binary { op, left, right } => self.compile_expr_binary(*op, left, right),
            // 字符串插值
            Expr::InterpolatedString(parts) => self.compile_expr_interpolated_string(parts),
            // 集合类型
            Expr::List(items) => self.compile_expr_list(items),
            Expr::Tuple(items) => self.compile_expr_tuple(items),
            Expr::Dict(entries) => self.compile_expr_dict(entries),
            Expr::Range(r) => self.compile_expr_range(r),
            // 控制流表达式
            Expr::IfExpr(e) => self.compile_expr_if(e),
            Expr::Match(m) => self.compile_expr_match(m),
            // 函数相关
            Expr::FuncLit(def) => self.compile_expr_func_lit(def),
            Expr::Call(c) => self.compile_expr_call(c),
            Expr::MethodCall(m) => self.compile_expr_method_call(m),
            // 访问表达式
            Expr::StructInit(s) => self.compile_expr_struct_init(s),
            Expr::EnumCtor { module, ty, variant, args } => {
                self.compile_expr_enum_ctor(module.as_deref(), ty, variant, args)
            }
            Expr::Member(m) => self.compile_expr_member(m),
            Expr::Index(ix) => self.compile_expr_index(ix),
            Expr::Group(e) => self.compile_expr(e),
        }
    }

    // ==================== 字面量表达式编译 ====================

    /// 编译标识符表达式
    #[inline]
    fn compile_expr_ident(&mut self, name: &str) -> Option<()> {
        if let Some(idx) = self.resolve_local(name) {
            self.bc.ops.push(Op::LoadLocal(idx));
        } else {
            let n_idx = self.add_constant(xu_ir::Constant::Str(name.to_string()));
            self.bc.ops.push(Op::LoadName(n_idx));
        }
        Some(())
    }

    /// 编译整数字面量
    #[inline]
    fn compile_expr_int(&mut self, v: i64) -> Option<()> {
        self.bc.ops.push(Op::ConstInt(v));
        Some(())
    }

    /// 编译浮点数字面量
    #[inline]
    fn compile_expr_float(&mut self, v: f64) -> Option<()> {
        self.bc.ops.push(Op::ConstFloat(v));
        Some(())
    }

    /// 编译布尔字面量
    #[inline]
    fn compile_expr_bool(&mut self, v: bool) -> Option<()> {
        self.bc.ops.push(Op::ConstBool(v));
        Some(())
    }

    /// 编译字符串字面量
    #[inline]
    fn compile_expr_str(&mut self, s: &str) -> Option<()> {
        let s_idx = self.add_constant(xu_ir::Constant::Str(s.to_string()));
        self.bc.ops.push(Op::Const(s_idx));
        Some(())
    }

    // ==================== 一元和二元运算编译 ====================

    /// 编译一元运算表达式
    fn compile_expr_unary(&mut self, op: UnaryOp, expr: &Expr) -> Option<()> {
        if let Some(folded) = self.try_fold_unary(op, expr) {
            self.bc.ops.push(folded);
            return Some(());
        }
        match op {
            UnaryOp::Not => {
                self.compile_expr(expr)?;
                self.bc.ops.push(Op::Not);
                Some(())
            }
            UnaryOp::Neg => {
                self.bc.ops.push(Op::ConstInt(0));
                self.compile_expr(expr)?;
                self.bc.ops.push(Op::Sub);
                Some(())
            }
        }
    }

    /// 编译二元运算表达式
    fn compile_expr_binary(&mut self, op: BinaryOp, left: &Expr, right: &Expr) -> Option<()> {
        if let Some(folded) = self.try_fold_binary(op, left, right) {
            self.bc.ops.push(folded);
            return Some(());
        }
        // 优化 "string_literal" + to_text(var) 模式
        if op == BinaryOp::Add {
            if let Expr::Str(_) = left {
                if let Some(inner) = extract_to_text_arg(right) {
                    self.compile_expr(left)?;
                    self.compile_expr(inner)?;
                    self.bc.ops.push(Op::StrAppend);
                    return Some(());
                }
            }
        }
        // 短路求值 && 和 ||
        if op == BinaryOp::And {
            return self.compile_short_circuit_and(left, right);
        }
        if op == BinaryOp::Or {
            return self.compile_short_circuit_or(left, right);
        }
        self.compile_expr(left)?;
        self.compile_expr(right)?;
        self.bc.ops.push(match op {
            BinaryOp::Add => Op::Add,
            BinaryOp::Sub => Op::Sub,
            BinaryOp::Mul => Op::Mul,
            BinaryOp::Div => Op::Div,
            BinaryOp::Mod => Op::Mod,
            BinaryOp::Eq => Op::Eq,
            BinaryOp::Ne => Op::Ne,
            BinaryOp::And => Op::And,  // 由于上面的短路求值，不会到达这里
            BinaryOp::Or => Op::Or,    // 由于上面的短路求值，不会到达这里
            BinaryOp::Gt => Op::Gt,
            BinaryOp::Lt => Op::Lt,
            BinaryOp::Ge => Op::Ge,
            BinaryOp::Le => Op::Le,
        });
        Some(())
    }

    /// 编译短路与运算 (&&)
    fn compile_short_circuit_and(&mut self, left: &Expr, right: &Expr) -> Option<()> {
        self.compile_expr(left)?;
        self.bc.ops.push(Op::Dup);
        let jump_idx = self.emit_jump(Op::JumpIfFalse(0));
        self.bc.ops.push(Op::Pop);
        self.compile_expr(right)?;
        self.patch_jump(jump_idx, self.bc.ops.len())
    }

    /// 编译短路或运算 (||)
    fn compile_short_circuit_or(&mut self, left: &Expr, right: &Expr) -> Option<()> {
        self.compile_expr(left)?;
        self.bc.ops.push(Op::Dup);
        let jump_idx = self.emit_jump(Op::JumpIfTrue(0));
        self.bc.ops.push(Op::Pop);
        self.compile_expr(right)?;
        self.patch_jump(jump_idx, self.bc.ops.len())
    }

    // ==================== 字符串插值编译 ====================

    /// 获取常量部分的文本表示
    fn const_part_text(e: &Expr) -> Option<String> {
        match e {
            Expr::Str(s) => Some(s.clone()),
            Expr::Bool(b) => Some(if *b { "true".to_string() } else { "false".to_string() }),
            Expr::Int(i) => Some((*i).to_string()),
            Expr::Float(f) => {
                if f.fract() == 0.0 {
                    Some((*f as i64).to_string())
                } else {
                    Some(f.to_string())
                }
            }
            _ => None,
        }
    }

    /// 编译字符串插值表达式
    fn compile_expr_interpolated_string(&mut self, parts: &[Expr]) -> Option<()> {
        let mut const_cap = 0usize;
        let mut non_const = 0usize;
        let mut generic = 0usize;
        for p in parts {
            if let Some(s) = Self::const_part_text(p) {
                const_cap += s.len();
            } else {
                non_const += 1;
                match p {
                    Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Str(_) => {}
                    _ => generic += 1,
                }
            }
        }
        let use_builder = parts.len() >= 8 || const_cap >= 128 || generic >= 3 || non_const >= 6;
        if use_builder {
            self.bc.ops.push(Op::BuilderNewCap(const_cap));
            for p in parts {
                self.compile_expr(p)?;
                self.bc.ops.push(Op::BuilderAppend);
            }
            self.bc.ops.push(Op::BuilderFinalize);
            return Some(());
        }
        if parts.iter().all(|p| Self::const_part_text(p).is_some()) {
            let mut s = String::new();
            for p in parts {
                // SAFETY: We just checked all parts have const text above
                if let Some(text) = Self::const_part_text(p) {
                    s.push_str(&text);
                }
            }
            let s_idx = self.add_constant(xu_ir::Constant::Str(s));
            self.bc.ops.push(Op::Const(s_idx));
            return Some(());
        }
        if let Some(Expr::Str(first)) = parts.first() {
            let f_idx = self.add_constant(xu_ir::Constant::Str(first.clone()));
            self.bc.ops.push(Op::Const(f_idx));
            for p in &parts[1..] {
                self.compile_interp_part(p)?;
            }
            return Some(());
        }
        let empty_idx = self.add_constant(xu_ir::Constant::Str(String::new()));
        self.bc.ops.push(Op::Const(empty_idx));
        for p in parts {
            self.compile_interp_part(p)?;
        }
        Some(())
    }

    // ==================== 集合类型编译 ====================

    /// 编译列表表达式
    fn compile_expr_list(&mut self, items: &[Expr]) -> Option<()> {
        self.compile_exprs(items)?;
        self.bc.ops.push(Op::ListNew(items.len()));
        Some(())
    }

    /// 编译元组表达式
    fn compile_expr_tuple(&mut self, items: &[Expr]) -> Option<()> {
        self.compile_exprs(items)?;
        self.bc.ops.push(Op::TupleNew(items.len()));
        Some(())
    }

    /// 编译字典表达式
    fn compile_expr_dict(&mut self, entries: &[(String, Expr)]) -> Option<()> {
        for (k, v) in entries {
            let k_idx = self.add_constant(xu_ir::Constant::Str(k.clone()));
            self.bc.ops.push(Op::Const(k_idx));
            self.compile_expr(v)?;
        }
        self.bc.ops.push(Op::DictNew(entries.len()));
        Some(())
    }

    /// 编译范围表达式
    fn compile_expr_range(&mut self, r: &xu_ir::RangeExpr) -> Option<()> {
        self.compile_expr(&r.start)?;
        self.compile_expr(&r.end)?;
        self.bc.ops.push(Op::MakeRange(r.inclusive));
        Some(())
    }

    // ==================== 控制流表达式编译 ====================

    /// 编译 if 表达式
    fn compile_expr_if(&mut self, e: &xu_ir::IfExpr) -> Option<()> {
        self.compile_expr(&e.cond)?;
        let j_if = self.emit_jump(Op::JumpIfFalse(usize::MAX));
        self.compile_expr(&e.then_expr)?;
        let j_end = self.emit_jump(Op::Jump(usize::MAX));
        self.patch_jump(j_if, self.bc.ops.len())?;
        self.compile_expr(&e.else_expr)?;
        self.patch_jump(j_end, self.bc.ops.len())
    }

    /// 编译 match 表达式
    fn compile_expr_match(&mut self, m: &xu_ir::MatchExpr) -> Option<()> {
        self.compile_expr(&m.expr)?;
        let mut arm_end_jumps: Vec<usize> = Vec::new();

        for (i, (pat, body)) in m.arms.iter().enumerate() {
            let is_last = i == m.arms.len() - 1 && m.else_expr.is_none();
            let jump_pos = self.compile_match_pattern_check(pat)?;

            if let Some(jpos) = jump_pos {
                let bindings = collect_pattern_bindings(pat);
                if !bindings.is_empty() {
                    self.bc.ops.push(Op::EnvPush);
                    let pat_idx = self.add_constant(xu_ir::Constant::Pattern(pat.clone()));
                    self.bc.ops.push(Op::MatchBindings(pat_idx));
                    for name in bindings.iter().rev() {
                        let name_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                        self.bc.ops.push(Op::StoreName(name_idx));
                    }
                    self.compile_expr(body)?;
                    self.bc.ops.push(Op::EnvPop);
                } else {
                    self.bc.ops.push(Op::Pop);
                    self.compile_expr(body)?;
                }
                arm_end_jumps.push(self.emit_jump(Op::Jump(usize::MAX)));
                self.patch_jump(jpos, self.bc.ops.len())?;
            } else {
                self.bc.ops.push(Op::Pop);
                self.compile_expr(body)?;
                if !is_last {
                    arm_end_jumps.push(self.emit_jump(Op::Jump(usize::MAX)));
                }
            }
        }

        if let Some(else_expr) = &m.else_expr {
            self.bc.ops.push(Op::Pop);
            self.compile_expr(else_expr)?;
        }
        self.patch_jumps(&arm_end_jumps, self.bc.ops.len())
    }

    // ==================== 函数相关编译 ====================

    /// 编译函数字面量表达式
    fn compile_expr_func_lit(&mut self, def: &xu_ir::FuncDef) -> Option<()> {
        let f_idx = self.compile_func_body(def)?;
        self.bc.ops.push(Op::MakeFunction(f_idx));
        Some(())
    }

    /// 编译函数调用表达式
    fn compile_expr_call(&mut self, c: &xu_ir::CallExpr) -> Option<()> {
        // 特殊情况: builder_push(b, x) -> BuilderAppend
        if let Expr::Ident(name, _) = c.callee.as_ref() {
            if name == "builder_push" && c.args.len() == 2 {
                self.compile_exprs(&c.args)?;
                self.bc.ops.push(Op::BuilderAppend);
                self.bc.ops.push(Op::ConstNull);
                return Some(());
            }
        }
        self.compile_expr(&c.callee)?;
        self.compile_exprs(&c.args)?;
        self.bc.ops.push(Op::Call(c.args.len()));
        Some(())
    }

    /// 编译方法调用表达式
    fn compile_expr_method_call(&mut self, m: &xu_ir::MethodCallExpr) -> Option<()> {
        let mname = m.method.as_str();
        let recv_ty = m.receiver_ty.get();

        // 只有当接收者确认为列表时才生成 ListAppend
        if mname == "add" && recv_ty == Some(ReceiverType::List) {
            self.compile_expr(&m.receiver)?;
            self.compile_exprs(&m.args)?;
            self.bc.ops.push(Op::ListAppend(m.args.len()));
            return Some(());
        }
        // 只有当接收者确认为字典时才生成 DictMerge
        if mname == "merge" && m.args.len() == 1 && recv_ty == Some(ReceiverType::Dict) {
            self.compile_expr(&m.receiver)?;
            self.compile_expr(&m.args[0])?;
            self.bc.ops.push(Op::DictMerge);
            return Some(());
        }
        // 只有当接收者确认为字典时才生成 DictInsert
        if mname == "insert_int" && m.args.len() == 2 && recv_ty == Some(ReceiverType::Dict) {
            self.compile_expr(&m.receiver)?;
            self.compile_exprs(&m.args)?;
            self.bc.ops.push(Op::DictInsert);
            return Some(());
        }
        // 所有其他情况：生成通用的 CallMethod 或 CallStaticOrMethod
        self.compile_method_call_generic(m)
    }

    /// 编译通用方法调用
    fn compile_method_call_generic(&mut self, m: &xu_ir::MethodCallExpr) -> Option<()> {
        let slot = self.alloc_ic_slot();
        let m_idx = self.add_constant(xu_ir::Constant::Str(m.method.clone()));
        let method_hash = xu_ir::stable_hash64(m.method.as_str());

        // 检查接收者是否为标识符 - 可能是静态方法调用
        if let Expr::Ident(name, _) = m.receiver.as_ref() {
            if self.resolve_local(name).is_some() {
                // 它是局部变量 - 使用常规 CallMethod
                self.compile_expr(&m.receiver)?;
                self.compile_exprs(&m.args)?;
                self.bc.ops.push(Op::CallMethod(m_idx, method_hash, m.args.len(), Some(slot)));
            } else {
                // 不是局部变量 - 可能是全局变量或类型名，生成 CallStaticOrMethod
                self.compile_exprs(&m.args)?;
                let type_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                self.bc.ops.push(Op::CallStaticOrMethod(type_idx, m_idx, method_hash, m.args.len(), Some(slot)));
            }
        } else {
            self.compile_expr(&m.receiver)?;
            self.compile_exprs(&m.args)?;
            self.bc.ops.push(Op::CallMethod(m_idx, method_hash, m.args.len(), Some(slot)));
        }
        Some(())
    }

    // ==================== 访问表达式编译 ====================

    /// 编译结构体初始化表达式
    fn compile_expr_struct_init(&mut self, s: &xu_ir::StructInitExpr) -> Option<()> {
        // 跨模块结构体初始化：编译模块表达式并丢弃（类型是全局注册的）
        if let Some(mod_expr) = &s.module {
            self.compile_expr(mod_expr)?;
            self.bc.ops.push(Op::Pop);
        }
        // 检查是否有展开项
        let spread_expr = s.items.iter().find_map(|item| {
            if let xu_ir::StructInitItem::Spread(e) = item {
                Some(e)
            } else {
                None
            }
        });

        if let Some(spread_e) = spread_expr {
            // 首先编译展开源（将在栈底）
            self.compile_expr(spread_e)?;
            // 然后编译显式字段值
            let mut names: Vec<String> = Vec::new();
            for item in s.items.iter() {
                if let xu_ir::StructInitItem::Field(k, v) = item {
                    names.push(k.clone());
                    self.compile_expr(v)?;
                }
            }
            let t_idx = self.add_constant(xu_ir::Constant::Str(s.ty.clone()));
            let n_idx = self.add_constant(xu_ir::Constant::Names(names));
            self.bc.ops.push(Op::StructInitSpread(t_idx, n_idx));
        } else {
            // 没有展开 - 使用常规 StructInit
            let mut names: Vec<String> = Vec::with_capacity(s.items.len());
            for item in s.items.iter() {
                if let xu_ir::StructInitItem::Field(k, v) = item {
                    names.push(k.clone());
                    self.compile_expr(v)?;
                }
            }
            let t_idx = self.add_constant(xu_ir::Constant::Str(s.ty.clone()));
            let n_idx = self.add_constant(xu_ir::Constant::Names(names));
            self.bc.ops.push(Op::StructInit(t_idx, n_idx));
        }
        Some(())
    }

    /// 编译枚举构造器表达式
    fn compile_expr_enum_ctor(
        &mut self,
        module: Option<&Expr>,
        ty: &str,
        variant: &str,
        args: &[Expr],
    ) -> Option<()> {
        if let Some(mod_expr) = module {
            self.compile_expr(mod_expr)?;
            self.bc.ops.push(Op::Pop);
        }
        self.compile_exprs(args)?;
        let t_idx = self.add_constant(xu_ir::Constant::Str(ty.to_string()));
        let v_idx = self.add_constant(xu_ir::Constant::Str(variant.to_string()));
        self.bc.ops.push(if args.is_empty() {
            Op::EnumCtor(t_idx, v_idx)
        } else {
            Op::EnumCtorN(t_idx, v_idx, args.len())
        });
        Some(())
    }

    /// 编译成员访问表达式
    fn compile_expr_member(&mut self, m: &xu_ir::MemberExpr) -> Option<()> {
        // 只有当标识符是已知类型名时才使用静态字段访问
        if let Expr::Ident(name, _) = m.object.as_ref() {
            // 检查是否是已知的类型名（结构体或枚举）
            if self.known_types.contains(name) {
                let t_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                let f_idx = self.add_constant(xu_ir::Constant::Str(m.field.clone()));
                self.bc.ops.push(Op::GetStaticField(t_idx, f_idx));
                return Some(());
            }
        }
        // 所有其他情况：编译对象表达式并使用 GetMember
        self.compile_expr(&m.object)?;
        let slot = self.alloc_ic_slot();
        let n_idx = self.add_constant(xu_ir::Constant::Str(m.field.clone()));
        self.bc.ops.push(Op::GetMember(n_idx, Some(slot)));
        Some(())
    }

    /// 编译索引访问表达式
    fn compile_expr_index(&mut self, ix: &xu_ir::IndexExpr) -> Option<()> {
        self.compile_expr(&ix.object)?;
        self.compile_expr(&ix.index)?;
        let slot = self.alloc_ic_slot();
        self.bc.ops.push(Op::GetIndex(Some(slot)));
        Some(())
    }
}
