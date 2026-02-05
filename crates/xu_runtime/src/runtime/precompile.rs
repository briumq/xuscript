//! Precompilation utilities for Runtime.

use xu_ir::{Expr, Module, Stmt};
use super::core::Runtime;

impl Runtime {
    pub(crate) fn precompile_module(module: &Module) -> Result<(), String> {
        Self::precompile_stmts(&module.stmts)
    }

    fn precompile_stmts(stmts: &[Stmt]) -> Result<(), String> {
        for s in stmts {
            match s {
                Stmt::StructDef(_) => {}
                Stmt::EnumDef(_) => {}
                Stmt::FuncDef(def) => {
                    Self::precompile_stmts(&def.body)?;
                    for p in &def.params {
                        if let Some(d) = &p.default {
                            Self::precompile_expr(d)?;
                        }
                    }
                }
                Stmt::DoesBlock(def) => {
                    for def in def.funcs.iter() {
                        Self::precompile_stmts(&def.body)?;
                        for p in &def.params {
                            if let Some(d) = &p.default {
                                Self::precompile_expr(d)?;
                            }
                        }
                    }
                }
                Stmt::Use(_) => {}
                Stmt::If(s) => {
                    for (cond, body) in &s.branches {
                        Self::precompile_expr(cond)?;
                        Self::precompile_stmts(body)?;
                    }
                    if let Some(body) = &s.else_branch {
                        Self::precompile_stmts(body)?;
                    }
                }
                Stmt::While(s) => {
                    Self::precompile_expr(&s.cond)?;
                    Self::precompile_stmts(&s.body)?;
                }
                Stmt::ForEach(s) => {
                    Self::precompile_expr(&s.iter)?;
                    Self::precompile_stmts(&s.body)?;
                }
                Stmt::Match(s) => {
                    Self::precompile_expr(&s.expr)?;
                    for (_, body) in s.arms.iter() {
                        Self::precompile_stmts(body)?;
                    }
                    if let Some(body) = &s.else_branch {
                        Self::precompile_stmts(body)?;
                    }
                }
                Stmt::Return(e) => {
                    if let Some(e) = e {
                        Self::precompile_expr(e)?;
                    }
                }
                Stmt::Assign(s) => {
                    Self::precompile_expr(&s.target)?;
                    Self::precompile_expr(&s.value)?;
                }
                Stmt::Expr(e) => Self::precompile_expr(e)?,
                Stmt::Block(stmts) => Self::precompile_stmts(stmts)?,
                Stmt::Break | Stmt::Continue => {}
                Stmt::Error(_) => {}
            }
        }
        Ok(())
    }

    fn precompile_expr(expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::Str(_) => Ok(()),
            Expr::EnumCtor { .. } => Ok(()),
            Expr::InterpolatedString(parts) => {
                for e in parts {
                    Self::precompile_expr(e)?;
                }
                Ok(())
            }
            Expr::List(items) => {
                for e in items {
                    Self::precompile_expr(e)?;
                }
                Ok(())
            }
            Expr::Range(r) => {
                Self::precompile_expr(&r.start)?;
                Self::precompile_expr(&r.end)
            }
            Expr::IfExpr(e) => {
                Self::precompile_expr(&e.cond)?;
                Self::precompile_expr(&e.then_expr)?;
                Self::precompile_expr(&e.else_expr)
            }
            Expr::Dict(entries) => {
                for (_, v) in entries {
                    Self::precompile_expr(v)?;
                }
                Ok(())
            }
            Expr::StructInit(s) => {
                if let Some(mod_expr) = &s.module {
                    Self::precompile_expr(mod_expr)?;
                }
                for item in s.items.iter() {
                    match item {
                        xu_ir::StructInitItem::Spread(e) => Self::precompile_expr(e)?,
                        xu_ir::StructInitItem::Field(_, v) => Self::precompile_expr(v)?,
                    }
                }
                Ok(())
            }
            Expr::Member(m) => Self::precompile_expr(&m.object),
            Expr::Index(m) => {
                Self::precompile_expr(&m.object)?;
                Self::precompile_expr(&m.index)
            }
            Expr::Call(c) => {
                Self::precompile_expr(&c.callee)?;
                for a in c.args.iter() {
                    Self::precompile_expr(a)?;
                }
                Ok(())
            }
            Expr::MethodCall(m) => {
                Self::precompile_expr(&m.receiver)?;
                for a in m.args.iter() {
                    Self::precompile_expr(a)?;
                }
                Ok(())
            }
            Expr::Unary { expr, .. } => Self::precompile_expr(expr),
            Expr::Binary { left, right, .. } => {
                Self::precompile_expr(left)?;
                Self::precompile_expr(right)
            }
            Expr::Group(e) => Self::precompile_expr(e),
            Expr::Ident(..) | Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) => Ok(()),
            _ => Ok(()),
        }
    }
}
