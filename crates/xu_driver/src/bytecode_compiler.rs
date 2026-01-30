//!
//!
//!
//!

use xu_ir::{
    AssignOp, AssignStmt, BinaryOp, Bytecode, BytecodeFunction, Expr, IfStmt, Module, Op, Pattern, Stmt,
    UnaryOp,
};

pub fn compile_module(module: &Module) -> Option<Bytecode> {
    let mut c = Compiler::new();
    for s in &module.stmts {
        c.compile_stmt(s)?;
    }
    c.bc.ops.push(Op::Halt);
    Some(c.bc)
}

fn infer_module_alias(path: &str) -> String {
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
}

impl Compiler {
    fn new() -> Self {
        Self {
            bc: Bytecode::default(),
            loops: Vec::new(),
            scopes: vec![Scope { locals: Vec::new() }],
            next_ic_slot: 0,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope { locals: Vec::new() });
    }

    fn resolve_local(&self, name: &str) -> Option<usize> {
        if self.scopes.len() <= 1 {
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
        let scope = self.scopes.last_mut().unwrap();
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

    fn compile_stmt(&mut self, stmt: &Stmt) -> Option<()> {
        match stmt {
            Stmt::Error(_) => None,
            Stmt::StructDef(def) => {
                let idx = self.add_constant(xu_ir::Constant::Struct((**def).clone()));
                self.bc.ops.push(Op::DefineStruct(idx));
                Some(())
            }
            Stmt::EnumDef(def) => {
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
            Stmt::Match(_) => None, // TODO: implement match stmt bytecode compilation
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
        let mut inner = Compiler::new();
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
        let f_idx = self.add_constant(xu_ir::Constant::Func(fun));
        self.bc.ops.push(Op::MakeFunction(f_idx));
        let n_idx = self.add_constant(xu_ir::Constant::Str(def.name.clone()));
        self.bc.ops.push(Op::StoreName(n_idx));
        Some(())
    }

    fn compile_if(&mut self, stmt: &IfStmt) -> Option<()> {
        let mut end_jumps: Vec<usize> = Vec::new();
        for (cond, body) in &stmt.branches {
            self.compile_expr(cond)?;
            let jfalse_pos = self.bc.ops.len();
            self.bc.ops.push(Op::JumpIfFalse(usize::MAX));
            for s in body {
                self.compile_stmt(s)?;
            }
            let jend_pos = self.bc.ops.len();
            self.bc.ops.push(Op::Jump(usize::MAX));
            end_jumps.push(jend_pos);
            let next_start = self.bc.ops.len();
            let Op::JumpIfFalse(to) = &mut self.bc.ops[jfalse_pos] else {
                return None;
            };
            *to = next_start;
        }
        if let Some(body) = &stmt.else_branch {
            for s in body {
                self.compile_stmt(s)?;
            }
        }
        let end = self.bc.ops.len();
        for pos in end_jumps {
            let Op::Jump(to) = &mut self.bc.ops[pos] else {
                return None;
            };
            *to = end;
        }
        Some(())
    }

    fn compile_while(&mut self, stmt: &xu_ir::WhileStmt) -> Option<()> {
        let loop_start = self.bc.ops.len();
        self.compile_expr(&stmt.cond)?;
        let jfalse_pos = self.bc.ops.len();
        self.bc.ops.push(Op::JumpIfFalse(usize::MAX));
        self.loops.push(LoopCtx {
            break_ops: Vec::new(),
            continue_ops: Vec::new(),
        });
        for s in &stmt.body {
            self.compile_stmt(s)?;
        }
        let ctx = self.loops.pop()?;
        self.bc.ops.push(Op::Jump(loop_start));
        let end = self.bc.ops.len();
        let Op::JumpIfFalse(to) = &mut self.bc.ops[jfalse_pos] else {
            return None;
        };
        *to = end;
        self.patch_loop(ctx, end, loop_start)?;
        Some(())
    }

    fn compile_foreach(&mut self, stmt: &xu_ir::ForEachStmt) -> Option<()> {
        self.compile_expr(&stmt.iter)?;
        let init_pos = self.bc.ops.len();
        let var_idx = if self.scopes.len() > 1 {
            Some(self.define_local(&stmt.var))
        } else {
            None
        };
        let n_idx = self.add_constant(xu_ir::Constant::Str(stmt.var.clone()));
        self.bc
            .ops
            .push(Op::ForEachInit(n_idx, var_idx, usize::MAX));
        self.loops.push(LoopCtx {
            break_ops: Vec::new(),
            continue_ops: Vec::new(),
        });
        let body_start = self.bc.ops.len();
        for s in &stmt.body {
            self.compile_stmt(s)?;
        }
        let next_pos = self.bc.ops.len();
        self.bc
            .ops
            .push(Op::ForEachNext(n_idx, var_idx, body_start, usize::MAX));
        let break_cleanup = self.bc.ops.len();
        self.bc.ops.push(Op::IterPop);
        let j_to_end = self.bc.ops.len();
        self.bc.ops.push(Op::Jump(usize::MAX));
        let end = self.bc.ops.len();
        let Op::ForEachInit(_, _, end_ip) = &mut self.bc.ops[init_pos] else {
            return None;
        };
        *end_ip = end;
        let Op::ForEachNext(_, _, _, end_ip) = &mut self.bc.ops[next_pos] else {
            return None;
        };
        *end_ip = end;
        let Op::Jump(to) = &mut self.bc.ops[j_to_end] else {
            return None;
        };
        *to = end;
        let ctx = self.loops.pop()?;
        self.patch_loop(ctx, break_cleanup, next_pos)?;
        Some(())
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
                    } else if self.scopes.len() > 1 {
                        let idx = self.define_local(name);
                        self.bc.ops.push(Op::StoreLocal(idx));
                    } else {
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

    ///
    ///
    fn compile_expr(&mut self, expr: &Expr) -> Option<()> {
        match expr {
            Expr::Error(_) => None,
            Expr::Ident(name, _) => {
                if let Some(idx) = self.resolve_local(name) {
                    self.bc.ops.push(Op::LoadLocal(idx));
                } else {
                    let n_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                    self.bc.ops.push(Op::LoadName(n_idx));
                }
                Some(())
            }
            Expr::Int(v) => {
                self.bc.ops.push(Op::ConstInt(*v));
                Some(())
            }
            Expr::Float(v) => {
                self.bc.ops.push(Op::ConstFloat(*v));
                Some(())
            }
            Expr::Bool(v) => {
                self.bc.ops.push(Op::ConstBool(*v));
                Some(())
            }
            Expr::Str(s) => {
                let s_idx = self.add_constant(xu_ir::Constant::Str(s.clone()));
                self.bc.ops.push(Op::Const(s_idx));
                Some(())
            }
            Expr::Unary { op, expr } => {
                if let Some(folded) = self.try_fold_unary(*op, expr) {
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
            Expr::Binary { op, left, right } => {
                if let Some(folded) = self.try_fold_binary(*op, left, right) {
                    self.bc.ops.push(folded);
                    return Some(());
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
                    BinaryOp::And => Op::And,
                    BinaryOp::Or => Op::Or,
                    BinaryOp::Gt => Op::Gt,
                    BinaryOp::Lt => Op::Lt,
                    BinaryOp::Ge => Op::Ge,
                    BinaryOp::Le => Op::Le,
                });
                Some(())
            }
            Expr::InterpolatedString(parts) => {
                fn const_part_text(e: &Expr) -> Option<String> {
                    match e {
                        Expr::Str(s) => Some(s.clone()),
                        Expr::Bool(b) => Some(if *b {
                            "true".to_string()
                        } else {
                            "false".to_string()
                        }),
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
                let mut const_cap = 0usize;
                let mut non_const = 0usize;
                let mut generic = 0usize;
                for p in parts {
                    if let Some(s) = const_part_text(p) {
                        const_cap += s.len();
                    } else {
                        non_const += 1;
                        match p {
                            Expr::Bool(_)
                            | Expr::Int(_)
                            | Expr::Float(_)
                            | Expr::Str(_) => {}
                            _ => generic += 1,
                        }
                    }
                }
                let use_builder =
                    parts.len() >= 8 || const_cap >= 128 || generic >= 3 || non_const >= 6;
                if use_builder {
                    self.bc.ops.push(Op::BuilderNewCap(const_cap));
                    for p in parts {
                        self.compile_expr(p)?;
                        self.bc.ops.push(Op::BuilderAppend);
                    }
                    self.bc.ops.push(Op::BuilderFinalize);
                    return Some(());
                }
                if parts.iter().all(|p| const_part_text(p).is_some()) {
                    let mut s = String::new();
                    for p in parts {
                        s.push_str(&const_part_text(p).unwrap());
                    }
                    let s_idx = self.add_constant(xu_ir::Constant::Str(s));
                    self.bc.ops.push(Op::Const(s_idx));
                    return Some(());
                }
                if let Some(Expr::Str(first)) = parts.first() {
                    let f_idx = self.add_constant(xu_ir::Constant::Str(first.clone()));
                    self.bc.ops.push(Op::Const(f_idx));
                    for p in &parts[1..] {
                        match p {
                            Expr::Bool(_) => {
                                self.compile_expr(p)?;
                                self.bc.ops.push(Op::StrAppendBool)
                            }
                            Expr::Int(_) => {
                                self.compile_expr(p)?;
                                self.bc.ops.push(Op::StrAppendInt)
                            }
                            Expr::Float(_) => {
                                self.compile_expr(p)?;
                                self.bc.ops.push(Op::StrAppendFloat)
                            }
                            Expr::Str(s) => {
                                let s_idx = self.add_constant(xu_ir::Constant::Str(s.clone()));
                                self.bc.ops.push(Op::Const(s_idx));
                                self.bc.ops.push(Op::StrAppendStr);
                            }
                            _ => {
                                self.compile_expr(p)?;
                                self.bc.ops.push(Op::StrAppend);
                            }
                        }
                    }
                    return Some(());
                }
                let empty_idx = self.add_constant(xu_ir::Constant::Str(String::new()));
                self.bc.ops.push(Op::Const(empty_idx));
                for p in parts {
                    match p {
                        Expr::Bool(_) => {
                            self.compile_expr(p)?;
                            self.bc.ops.push(Op::StrAppendBool)
                        }
                        Expr::Int(_) => {
                            self.compile_expr(p)?;
                            self.bc.ops.push(Op::StrAppendInt)
                        }
                        Expr::Float(_) => {
                            self.compile_expr(p)?;
                            self.bc.ops.push(Op::StrAppendFloat)
                        }
                        Expr::Str(s) => {
                            let s_idx = self.add_constant(xu_ir::Constant::Str(s.clone()));
                            self.bc.ops.push(Op::Const(s_idx));
                            self.bc.ops.push(Op::StrAppendStr);
                        }
                        _ => {
                            self.compile_expr(p)?;
                            self.bc.ops.push(Op::StrAppend);
                        }
                    }
                }
                Some(())
            }
            Expr::List(items) => {
                for e in items {
                    self.compile_expr(e)?;
                }
                self.bc.ops.push(Op::ListNew(items.len()));
                Some(())
            }
            Expr::Tuple(items) => {
                for e in items {
                    self.compile_expr(e)?;
                }
                self.bc.ops.push(Op::TupleNew(items.len()));
                Some(())
            }
            Expr::Dict(entries) => {
                for (k, v) in entries {
                    let k_idx = self.add_constant(xu_ir::Constant::Str(k.clone()));
                    self.bc.ops.push(Op::Const(k_idx));
                    self.compile_expr(v)?;
                }
                self.bc.ops.push(Op::DictNew(entries.len()));
                Some(())
            }
            Expr::Range(r) => {
                self.compile_expr(&r.start)?;
                self.compile_expr(&r.end)?;
                self.bc.ops.push(Op::MakeRange(r.inclusive));
                Some(())
            }
            Expr::IfExpr(e) => {
                self.compile_expr(&e.cond)?;
                let j_if = self.bc.ops.len();
                self.bc.ops.push(Op::JumpIfFalse(usize::MAX));
                self.compile_expr(&e.then_expr)?;
                let j_end = self.bc.ops.len();
                self.bc.ops.push(Op::Jump(usize::MAX));
                let else_ip = self.bc.ops.len();
                match self.bc.ops[j_if] {
                    Op::JumpIfFalse(ref mut to) => *to = else_ip,
                    _ => return None,
                }
                self.compile_expr(&e.else_expr)?;
                let end_ip = self.bc.ops.len();
                match self.bc.ops[j_end] {
                    Op::Jump(ref mut to) => *to = end_ip,
                    _ => return None,
                }
                Some(())
            }
            Expr::Match(m) => {
                // Compile the match expression
                self.compile_expr(&m.expr)?;

                let mut arm_end_jumps: Vec<usize> = Vec::new();

                for (i, (pat, body)) in m.arms.iter().enumerate() {
                    let is_last = i == m.arms.len() - 1 && m.else_expr.is_none();

                    // Handle pattern matching
                    if !matches!(pat, xu_ir::Pattern::Wildcard) {
                        // Add pattern to constants and emit match instruction
                        // MatchPattern peeks the value and pushes bool result
                        let pat_idx = self.add_constant(xu_ir::Constant::Pattern(pat.clone()));
                        self.bc.ops.push(Op::MatchPattern(pat_idx));

                        // Jump to next arm if pattern doesn't match
                        let jump_pos = self.bc.ops.len();
                        self.bc.ops.push(Op::JumpIfFalse(usize::MAX));

                        // Get bindings from pattern
                        let bindings = collect_pattern_bindings(pat);
                        if !bindings.is_empty() {
                            // Push env frame for bindings
                            self.bc.ops.push(Op::EnvPush);

                            // Emit MatchBindings to pop value and push binding values onto stack
                            self.bc.ops.push(Op::MatchBindings(pat_idx));

                            // Store bindings in env (in reverse order since they're on stack)
                            for name in bindings.iter().rev() {
                                let name_idx = self.add_constant(xu_ir::Constant::Str(name.clone()));
                                self.bc.ops.push(Op::StoreName(name_idx));
                            }

                            // Compile body expression
                            self.compile_expr(body)?;

                            // Pop env frame
                            self.bc.ops.push(Op::EnvPop);
                        } else {
                            // No bindings, just pop the original value
                            self.bc.ops.push(Op::Pop);

                            // Compile body expression
                            self.compile_expr(body)?;
                        }

                        // Jump to end of match
                        let end_jump = self.bc.ops.len();
                        self.bc.ops.push(Op::Jump(usize::MAX));
                        arm_end_jumps.push(end_jump);

                        // Patch the conditional jump to point here (next arm)
                        let next_arm_ip = self.bc.ops.len();
                        match self.bc.ops[jump_pos] {
                            Op::JumpIfFalse(ref mut to) => *to = next_arm_ip,
                            _ => return None,
                        }
                    } else {
                        // Wildcard pattern - always matches
                        // Pop the original value
                        self.bc.ops.push(Op::Pop);

                        // Compile body expression
                        self.compile_expr(body)?;

                        // Jump to end (unless this is the last arm)
                        if !is_last {
                            let end_jump = self.bc.ops.len();
                            self.bc.ops.push(Op::Jump(usize::MAX));
                            arm_end_jumps.push(end_jump);
                        }
                    }
                }

                // Compile else expression if present
                if let Some(else_expr) = &m.else_expr {
                    // Pop the original value
                    self.bc.ops.push(Op::Pop);
                    self.compile_expr(else_expr)?;
                }

                // Patch all end jumps to point here
                let end_ip = self.bc.ops.len();
                for jump_pos in arm_end_jumps {
                    match self.bc.ops[jump_pos] {
                        Op::Jump(ref mut to) => *to = end_ip,
                        _ => return None,
                    }
                }

                Some(())
            }
            Expr::FuncLit(def) => {
                let mut inner = Compiler::new();
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
                    def: (**def).clone(),
                    bytecode: Box::new(inner.bc),
                    locals_count,
                };
                let f_idx = self.add_constant(xu_ir::Constant::Func(fun));
                self.bc.ops.push(Op::MakeFunction(f_idx));
                Some(())
            }
            Expr::StructInit(s) => {
                let mut names: Vec<String> =
                    Vec::with_capacity(s.items.iter().filter(|x| matches!(x, xu_ir::StructInitItem::Field(_, _))).count());
                for item in s.items.iter() {
                    match item {
                        xu_ir::StructInitItem::Field(k, v) => {
                            names.push(k.clone());
                            self.compile_expr(v)?;
                        }
                        xu_ir::StructInitItem::Spread(_) => return None,
                    }
                }
                let t_idx = self.add_constant(xu_ir::Constant::Str(s.ty.clone()));
                let n_idx = self.add_constant(xu_ir::Constant::Names(names));
                self.bc.ops.push(Op::StructInit(t_idx, n_idx));
                Some(())
            }
            Expr::EnumCtor { ty, variant, args } => {
                for a in args.iter() {
                    self.compile_expr(a)?;
                }
                let t_idx = self.add_constant(xu_ir::Constant::Str(ty.clone()));
                let v_idx = self.add_constant(xu_ir::Constant::Str(variant.clone()));
                if args.is_empty() {
                    self.bc.ops.push(Op::EnumCtor(t_idx, v_idx));
                } else {
                    self.bc.ops.push(Op::EnumCtorN(t_idx, v_idx, args.len()));
                }
                Some(())
            }
            Expr::Member(m) => {
                self.compile_expr(&m.object)?;
                let slot = self.alloc_ic_slot();
                let n_idx = self.add_constant(xu_ir::Constant::Str(m.field.clone()));
                self.bc.ops.push(Op::GetMember(n_idx, Some(slot)));
                Some(())
            }
            Expr::Index(ix) => {
                self.compile_expr(&ix.object)?;
                self.compile_expr(&ix.index)?;
                let slot = self.alloc_ic_slot();
                self.bc.ops.push(Op::GetIndex(Some(slot)));
                Some(())
            }
            Expr::Call(c) => {
                self.compile_expr(&c.callee)?;
                for a in c.args.iter() {
                    self.compile_expr(a)?;
                }
                self.bc.ops.push(Op::Call(c.args.len()));
                Some(())
            }
            Expr::MethodCall(m) => {
                let mname = m.method.as_str();
                if mname == "add" {
                    self.compile_expr(&m.receiver)?;
                    for a in m.args.iter() {
                        self.compile_expr(a)?;
                    }
                    self.bc.ops.push(Op::ListAppend(m.args.len()));
                } else if mname == "merge" && m.args.len() == 1 {
                    self.compile_expr(&m.receiver)?;
                    self.compile_expr(&m.args[0])?;
                    self.bc.ops.push(Op::DictMerge);
                } else if (mname == "insert" || mname == "insert_int") && m.args.len() == 2 {
                    self.compile_expr(&m.receiver)?;
                    // Check if key is a string constant for optimized path
                    if mname == "insert" {
                        if let Expr::Str(s) = &m.args[0] {
                            // Use optimized DictInsertStrConst for string constant keys
                            self.compile_expr(&m.args[1])?;
                            let slot = self.alloc_ic_slot();
                            let s_idx = self.add_constant(xu_ir::Constant::Str(s.clone()));
                            self.bc.ops.push(Op::DictInsertStrConst(
                                s_idx,
                                xu_ir::stable_hash64(s.as_str()),
                                Some(slot),
                            ));
                        } else {
                            self.compile_expr(&m.args[0])?;
                            self.compile_expr(&m.args[1])?;
                            self.bc.ops.push(Op::DictInsert);
                        }
                    } else {
                        self.compile_expr(&m.args[0])?;
                        self.compile_expr(&m.args[1])?;
                        self.bc.ops.push(Op::DictInsert);
                    }
                } else if mname == "get" && m.args.len() == 1 {
                    self.compile_expr(&m.receiver)?;
                    if let Expr::Str(s) = &m.args[0] {
                        let slot = self.alloc_ic_slot();
                        let s_idx = self.add_constant(xu_ir::Constant::Str(s.clone()));
                        self.bc.ops.push(Op::DictGetStrConst(
                            s_idx,
                            xu_ir::stable_hash64(s.as_str()),
                            Some(slot),
                        ));
                    } else if let Expr::Int(i) = &m.args[0] {
                        let slot = self.alloc_ic_slot();
                        self.bc.ops.push(Op::DictGetIntConst(*i, Some(slot)));
                    } else {
                        self.compile_expr(&m.args[0])?;
                        let slot = self.alloc_ic_slot();
                        self.bc.ops.push(Op::GetIndex(Some(slot)));
                    }
                } else if mname == "get_int" && m.args.len() == 1 {
                    self.compile_expr(&m.receiver)?;
                    if let Expr::Int(i) = &m.args[0] {
                        let slot = self.alloc_ic_slot();
                        self.bc.ops.push(Op::DictGetIntConst(*i, Some(slot)));
                    } else {
                        self.compile_expr(&m.args[0])?;
                        let slot = self.alloc_ic_slot();
                        self.bc.ops.push(Op::GetIndex(Some(slot)));
                    }
                } else {
                    self.compile_expr(&m.receiver)?;
                    for a in m.args.iter() {
                        self.compile_expr(a)?;
                    }
                    let slot = self.alloc_ic_slot();
                    let m_idx = self.add_constant(xu_ir::Constant::Str(m.method.clone()));
                    self.bc.ops.push(Op::CallMethod(
                        m_idx,
                        xu_ir::stable_hash64(m.method.as_str()),
                        m.args.len(),
                        Some(slot),
                    ));
                }
                Some(())
            }
            Expr::Group(e) => self.compile_expr(e),
        }
    }
}
