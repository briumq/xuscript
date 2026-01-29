//! Parser.
//!
//! Converts lexer tokens into a syntax tree (Module/Stmt/Expr) and collects diagnostics.
//! The implementation uses a recursive-descent statement parser plus Pratt parsing for
//! expressions.
use std::cell::Cell;
use std::collections::HashMap;
use xu_syntax::{
    Diagnostic, DiagnosticKind, Span, Token, TokenKind, is_ident_continue, is_ident_start, unquote,
};

use crate::{
    AssignOp, AssignStmt, BinaryOp, DeclKind, DoesBlock, EnumDef, Expr, ForEachStmt, FuncDef, IfStmt,
    MemberExpr, Module, Param, Pattern, Stmt, StructDef, StructField, TypeRef, UseStmt,
    Visibility, WhenStmt, WhileStmt,
};

mod interp;
mod expr;

/// Parse result.
pub struct ParseResult {
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

/// Xu parser.
pub struct Parser<'a, 'b> {
    input: &'a str,
    tokens: &'a [Token],
    i: usize,
    diagnostics: Vec<Diagnostic>,
    interp_cache: HashMap<String, Expr>,
    pending_stmts: Vec<Stmt>,
    tmp_counter: u32,
    allow_comma_terminator: bool,
    bump: &'b bumpalo::Bump,
}

impl<'a, 'b> Parser<'a, 'b> {
    fn parse_int_literal(s: &str) -> i64 {
        let cleaned: String = s.chars().filter(|c| *c != '_').collect();
        if let Some(hex) = cleaned.strip_prefix("0x").or_else(|| cleaned.strip_prefix("0X")) {
            i64::from_str_radix(hex, 16).unwrap_or(0)
        } else if let Some(bin) = cleaned.strip_prefix("0b").or_else(|| cleaned.strip_prefix("0B"))
        {
            i64::from_str_radix(bin, 2).unwrap_or(0)
        } else {
            cleaned.parse::<i64>().unwrap_or(0)
        }
    }

    /// Create a new parser.
    pub fn new(input: &'a str, tokens: &'a [Token], bump: &'b bumpalo::Bump) -> Self {
        Self {
            input,
            tokens,
            i: 0,
            diagnostics: Vec::with_capacity(32),
            interp_cache: HashMap::with_capacity(32),
            pending_stmts: Vec::new(),
            tmp_counter: 0,
            allow_comma_terminator: false,
            bump,
        }
    }

    /// Parse the full input and return a module plus diagnostics.
    pub fn parse(mut self) -> ParseResult {
        let mut stmts: Vec<Stmt> = Vec::with_capacity(8);
        while !self.at(TokenKind::Eof) {
            self.skip_trivia();
            if self.at(TokenKind::Eof) {
                break;
            }
            if self.at(TokenKind::Dedent) {
                self.bump();
                continue;
            }
            let stmt = match self.parse_stmt() {
                Some(stmt) => stmt,
                None => self.recover_stmt(),
            };
            stmts.push(stmt);
            if !self.pending_stmts.is_empty() {
                stmts.extend(self.pending_stmts.drain(..));
            }
        }

        ParseResult {
            module: Module {
                stmts: stmts.into_boxed_slice(),
            },
            diagnostics: self.diagnostics,
        }
    }

    /// Parse a single statement.
    fn parse_stmt(&mut self) -> Option<Stmt> {
        self.skip_trivia();
        let mut vis = Visibility::Public;
        if self.at(TokenKind::KwInner) {
            self.bump();
            vis = Visibility::Inner;
            self.skip_trivia();
        }
        match self.peek_kind() {
            TokenKind::KwFunc => self.parse_func_def(vis).map(|x| Stmt::FuncDef(Box::new(x))),
            TokenKind::KwIf => self.parse_if().map(|x| Stmt::If(Box::new(x))),
            TokenKind::KwWhile => self.parse_while().map(|x| Stmt::While(Box::new(x))),
            TokenKind::KwFor => self.parse_foreach().map(|x| Stmt::ForEach(Box::new(x))),
            TokenKind::KwMatch => self.parse_when().map(|x| Stmt::When(Box::new(x))),
            TokenKind::KwWhen => self.parse_when_bind_stmt(),
            TokenKind::KwReturn => self.parse_return(),
            TokenKind::KwBreak => self.parse_simple_kw_stmt(TokenKind::KwBreak, Stmt::Break),
            TokenKind::KwContinue => {
                self.parse_simple_kw_stmt(TokenKind::KwContinue, Stmt::Continue)
            }
            TokenKind::KwUse => self.parse_use_stmt(),
            TokenKind::KwLet | TokenKind::KwVar => self.parse_let_var_decl(vis),
            TokenKind::Ident => {
                if self.is_does_block_start() {
                    self.parse_does_block(vis)
                        .map(|x| Stmt::DoesBlock(Box::new(x)))
                } else if self.is_type_def_start() {
                    if self.peek_kind_n(2) == Some(TokenKind::LBrace)
                        && self.braced_type_def_is_struct()
                    {
                        self.parse_struct_def(vis)
                            .map(|x| Stmt::StructDef(Box::new(x)))
                    } else {
                        self.parse_enum_def(vis).map(|x| Stmt::EnumDef(Box::new(x)))
                    }
                } else {
                    self.parse_assign_or_expr_stmt()
                }
            }
            _ => self.parse_assign_or_expr_stmt(),
        }
    }

    /// Parse a `let` / `var` declaration.
    fn parse_let_var_decl(&mut self, vis: Visibility) -> Option<Stmt> {
        let kw_kind = self.peek_kind();
        if kw_kind != TokenKind::KwLet && kw_kind != TokenKind::KwVar {
            return None;
        }
        self.bump();
        self.skip_trivia();
        // variable name or tuple destructure
        let tuple_names = if self.at(TokenKind::LParen) {
            self.bump();
            let mut names: Vec<String> = Vec::with_capacity(4);
            self.skip_layout();
            if !self.at(TokenKind::RParen) {
                loop {
                    names.push(self.expect_ident()?);
                    self.skip_layout();
                    if self.at(TokenKind::Comma) {
                        self.bump();
                        self.skip_layout();
                        if self.at(TokenKind::RParen) {
                            break;
                        }
                        continue;
                    }
                    break;
                }
            }
            self.expect(TokenKind::RParen)?;
            Some(names)
        } else {
            None
        };
        let name = if tuple_names.is_none() {
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.skip_trivia();
        // optional type annotation
        let mut ty: Option<TypeRef> = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type_ref()?);
            self.skip_trivia();
        }
        // expect '='
        if self.at(TokenKind::Eq) {
            self.bump();
        } else {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::ExpectedToken("=".to_string()),
                Some(self.cur_span()),
            ));
            return None;
        }
        // initializer expression
        let value = self.parse_expr(0)?;
        self.expect_stmt_terminator()?;
        let decl = if kw_kind == TokenKind::KwLet {
            Some(DeclKind::Let)
        } else {
            Some(DeclKind::Var)
        };
        if let Some(names) = tuple_names {
            let tmp = format!("__tmp_destructure_{}", self.tmp_counter);
            self.tmp_counter += 1;
            for (i, n) in names.into_iter().enumerate() {
                if n == "_" {
                    continue;
                }
                let v = Expr::Member(Box::new(MemberExpr {
                    object: Box::new(Expr::Ident(tmp.clone(), Cell::new(None))),
                    field: i.to_string(),
                    ic_slot: Cell::new(None),
                }));
                self.pending_stmts
                    .push(Stmt::Assign(Box::new(AssignStmt {
                        vis,
                        target: Expr::Ident(n, Cell::new(None)),
                        op: AssignOp::Set,
                        value: v,
                        ty: ty.clone(),
                        slot: None,
                        decl,
                    })));
            }
            Some(Stmt::Assign(Box::new(AssignStmt {
                vis,
                target: Expr::Ident(tmp, Cell::new(None)),
                op: AssignOp::Set,
                value,
                ty,
                slot: None,
                decl: Some(DeclKind::Let),
            })))
        } else {
            if name.as_deref() == Some("_") {
                return Some(Stmt::Expr(value));
            }
            Some(Stmt::Assign(Box::new(AssignStmt {
                vis,
                target: Expr::Ident(name.unwrap(), Cell::new(None)),
                op: AssignOp::Set,
                value,
                ty,
                slot: None,
                decl,
            })))
        }
    }

    fn parse_use_stmt(&mut self) -> Option<Stmt> {
        self.expect(TokenKind::KwUse)?;
        if self.at(TokenKind::Str) {
            let t = self.expect(TokenKind::Str)?;
            let raw = self.token_text(&t);
            let path = unquote(raw);
            self.skip_layout();
            let alias = if self.at(TokenKind::KwAs) {
                self.bump();
                Some(self.expect_ident()?)
            } else {
                None
            };
            self.expect_stmt_terminator()?;
            return Some(Stmt::Use(Box::new(UseStmt { path, alias })));
        }
        None
    }

    fn parse_struct_def(&mut self, vis: Visibility) -> Option<StructDef> {
        let name = self.expect_ident()?;
        if self.at(TokenKind::KwHas) {
            self.bump();
        } else {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::ExpectedToken("has".to_string()),
                Some(self.cur_span()),
            ));
            return None;
        }
        self.expect(TokenKind::LBrace)?;

        let mut fields: Vec<StructField> = Vec::with_capacity(4);
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_layout();
            if self.at(TokenKind::RBrace) {
                break;
            }

            let mut item_vis = Visibility::Public;
            if self.at(TokenKind::KwInner) {
                self.bump();
                item_vis = Visibility::Inner;
                self.skip_trivia();
            }

            let is_static = self.at(TokenKind::KwStatic);
            if is_static {
                self.bump();
                self.skip_trivia();
            }

            if self.at(TokenKind::KwFunc) {
                let mut f = self.parse_func_def(item_vis)?;
                if is_static {
                    f.name = format!("__static__{}__{}", name, f.name);
                } else if !f.name.starts_with("__method__") {
                    let original = f.name.clone();
                    f.name = format!("__method__{}__{}", name, original);
                    let needs_self = f
                        .params
                        .first()
                        .map(|p| p.name != "self")
                        .unwrap_or(true);
                    if needs_self {
                        let mut params: Vec<Param> = Vec::with_capacity(f.params.len() + 1);
                        params.push(Param {
                            name: "self".to_string(),
                            ty: Some(TypeRef {
                                name: name.clone(),
                                params: Box::new([]),
                            }),
                            default: None,
                        });
                        params.extend(f.params.iter().cloned());
                        f.params = params.into_boxed_slice();
                    }
                }
                self.pending_stmts.push(Stmt::FuncDef(Box::new(f)));
                continue;
            }

            let field_name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let field_ty = self.parse_type_ref()?;
            self.skip_layout();
            let default = if self.at(TokenKind::Eq) {
                self.bump();
                Some(self.parse_expr(0)?)
            } else {
                None
            };
            fields.push(StructField {
                name: field_name,
                ty: field_ty,
                default,
            });
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
            }
        }

        self.expect(TokenKind::RBrace)?;
        self.expect_stmt_terminator()?;
        Some(StructDef {
            vis,
            name,
            fields: fields.into_boxed_slice(),
        })
    }

    fn parse_enum_def(&mut self, vis: Visibility) -> Option<EnumDef> {
        let name = self.expect_ident()?;
        self.expect(TokenKind::KwWith)?;
        let close = if self.at(TokenKind::LBracket) {
            self.bump();
            TokenKind::RBracket
        } else {
            self.expect(TokenKind::LBrace)?;
            TokenKind::RBrace
        };
        let mut variants: Vec<String> = Vec::new();
        loop {
            self.skip_layout();
            if self.at(close) {
                break;
            }
            variants.push(self.expect_ident()?);
            self.skip_layout();
            if self.at(TokenKind::LParen) {
                self.bump();
                let mut depth = 1usize;
                while !self.at(TokenKind::Eof) && depth > 0 {
                    match self.peek_kind() {
                        TokenKind::LParen => {
                            depth += 1;
                            self.bump();
                        }
                        TokenKind::RParen => {
                            depth = depth.saturating_sub(1);
                            self.bump();
                        }
                        _ => self.bump(),
                    }
                }
            }
            self.skip_layout();
            if self.at(TokenKind::Comma) || self.at(TokenKind::Pipe) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect(close)?;
        self.expect_stmt_terminator()?;
        Some(EnumDef {
            vis,
            name,
            variants: variants.into_boxed_slice(),
        })
    }

    fn parse_func_def(&mut self, vis: Visibility) -> Option<FuncDef> {
        self.expect(TokenKind::KwFunc)?;
        let name = if self.at(TokenKind::LParen) {
            self.bump();
            let receiver_name = if self.at(TokenKind::KwSelf) {
                self.bump();
                "self".to_string()
            } else {
                self.expect_ident()?
            };
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_ref()?;
            self.expect(TokenKind::RParen)?;
            let method = self.expect_ident()?;
            let internal = format!("__method__{}__{}", ty.name, method);
            self.expect(TokenKind::LParen)?;
            let mut params = self.parse_params()?;
            self.expect(TokenKind::RParen)?;
            params.insert(
                0,
                Param {
                    name: receiver_name,
                    ty: Some(ty),
                    default: None,
                },
            );
            let return_ty = if self.at_arrow() {
                self.bump();
                if self.at(TokenKind::Gt) {
                    self.bump();
                }
                Some(self.parse_type_ref()?)
            } else {
                None
            };
            let body = if self.at(TokenKind::Colon) {
                self.bump();
                self.parse_block_or_inline_stmt_after_colon(false)?
            } else {
                self.parse_block()?
            };
            return Some(FuncDef {
                vis,
                name: internal,
                params: params.into_boxed_slice(),
                return_ty,
                body,
            });
        } else {
            self.expect_ident()?
        };
        self.expect(TokenKind::LParen)?;
        let params = self.parse_params()?;
        self.expect(TokenKind::RParen)?;
        let return_ty = if self.at_arrow() {
            self.bump();
            if self.at(TokenKind::Gt) {
                self.bump();
            }
            Some(self.parse_type_ref()?)
        } else {
            None
        };
        let body = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_block_or_inline_stmt_after_colon(false)?
        } else {
            self.parse_block()?
        };
        Some(FuncDef {
            vis,
            name,
            params: params.into_boxed_slice(),
            return_ty,
            body,
        })
    }

    fn parse_does_block(&mut self, vis: Visibility) -> Option<DoesBlock> {
        let t = self.expect(TokenKind::Ident)?;
        let target = self.token_text(&t).to_string();
        if matches!(
            target.as_str(),
            "any"
                | "bool"
                | "int"
                | "float"
                | "text"
                | "str"
                | "string"
                | "func"
                | "range"
                | "list"
                | "dict"
        ) {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::Raw(format!("cannot extend builtin type: {}", target)),
                Some(t.span),
            ));
        }
        self.expect(TokenKind::KwDoes)?;
        self.skip_trivia();
        self.expect(TokenKind::LBrace)?;
        let mut funcs: Vec<FuncDef> = Vec::with_capacity(4);
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_trivia();
            if self.at(TokenKind::RBrace) {
                break;
            }
            let mut fvis = Visibility::Public;
            if self.at(TokenKind::KwInner) {
                self.bump();
                fvis = Visibility::Inner;
            }
            self.skip_trivia();
            let mut f = self.parse_func_def(fvis)?;
            if !f.name.starts_with("__method__") {
                let original = f.name.clone();
                f.name = format!("__method__{}__{}", target, original);
                let needs_self = f
                    .params
                    .first()
                    .map(|p| p.name != "self")
                    .unwrap_or(true);
                if needs_self {
                    let mut params: Vec<Param> = Vec::with_capacity(f.params.len() + 1);
                    params.push(Param {
                        name: "self".to_string(),
                        ty: Some(TypeRef {
                            name: target.clone(),
                            params: Box::new([]),
                        }),
                        default: None,
                    });
                    params.extend(f.params.iter().cloned());
                    f.params = params.into_boxed_slice();
                }
            }
            funcs.push(f);
        }
        self.expect(TokenKind::RBrace)?;
        self.expect_stmt_terminator()?;
        Some(DoesBlock {
            vis,
            target,
            funcs: funcs.into_boxed_slice(),
        })
    }

    fn parse_when(&mut self) -> Option<WhenStmt> {
        self.expect(TokenKind::KwMatch)?;
        let expr = self.parse_expr_no_struct_init(0)?;
        self.skip_trivia();
        self.expect(TokenKind::LBrace)?;
        let mut arms: Vec<(Pattern, Box<[Stmt]>)> = Vec::with_capacity(4);
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_trivia();
            if self.at(TokenKind::RBrace) {
                break;
            }
            let pat = self.parse_pattern()?;
            let body = if self.at(TokenKind::Colon) {
                self.bump();
                self.parse_block_or_inline_stmt_after_colon(true)?
            } else {
                self.parse_block()?
            };
            arms.push((pat, body));
            self.skip_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
            }
        }
        let else_branch = if let Some((Pattern::Wildcard, body)) = arms.last() {
            let body = body.clone();
            arms.pop();
            Some(body)
        } else {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::ExpectedToken("_".to_string()),
                Some(self.cur_span()),
            ));
            Some(Vec::new().into_boxed_slice())
        };
        self.expect(TokenKind::RBrace)?;
        self.expect_stmt_terminator()?;
        Some(WhenStmt {
            expr,
            arms: arms.into_boxed_slice(),
            else_branch,
        })
    }

    fn parse_when_bind_stmt(&mut self) -> Option<Stmt> {
        self.expect(TokenKind::KwWhen)?;
        self.skip_layout();

        let mut bindings: Vec<(String, Expr)> = Vec::with_capacity(3);
        loop {
            let name = self.expect_ident()?;
            self.skip_layout();
            self.expect(TokenKind::Eq)?;
            self.skip_layout();
            let expr = self.parse_expr(0)?;
            bindings.push((name, expr));
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.skip_layout();
                continue;
            }
            break;
        }

        let success_body = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_block_or_inline_stmt_after_colon(false)?
        } else {
            self.parse_block()?
        };

        self.skip_trivia();
        let else_body = if self.at(TokenKind::KwElse) {
            self.bump();
            if self.at(TokenKind::Colon) {
                self.bump();
                self.parse_block_or_inline_stmt_after_colon(false)?
            } else {
                self.parse_block()?
            }
        } else {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::ExpectedToken("else".to_string()),
                Some(self.cur_span()),
            ));
            Box::new([])
        };

        let mut inner_body = success_body;
        let mut outer_stmt: Option<Stmt> = None;
        for (name, expr) in bindings.into_iter().rev() {
            let pat_opt = Pattern::EnumVariant {
                ty: "Option".to_string(),
                variant: "some".to_string(),
                args: vec![Pattern::Bind(name.clone())].into_boxed_slice(),
            };
            let pat_res = Pattern::EnumVariant {
                ty: "Result".to_string(),
                variant: "ok".to_string(),
                args: vec![Pattern::Bind(name)].into_boxed_slice(),
            };
            let arms = vec![
                (pat_opt, inner_body.clone()),
                (pat_res, inner_body.clone()),
            ]
            .into_boxed_slice();
            let when_stmt = Stmt::When(Box::new(WhenStmt {
                expr,
                arms,
                else_branch: Some(else_body.clone()),
            }));
            outer_stmt = Some(when_stmt.clone());
            inner_body = vec![when_stmt].into_boxed_slice();
        }
        outer_stmt
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        self.skip_trivia();
        match self.peek_kind() {
            TokenKind::LParen => {
                self.bump();
                self.skip_trivia();
                if self.at(TokenKind::RParen) {
                    self.bump();
                    return Some(Pattern::Tuple(Box::new([])));
                }
                let first = self.parse_pattern()?;
                self.skip_trivia();
                if self.at(TokenKind::Comma) {
                    let mut items: Vec<Pattern> = Vec::with_capacity(2);
                    items.push(first);
                    while self.at(TokenKind::Comma) {
                        self.bump();
                        self.skip_trivia();
                        if self.at(TokenKind::RParen) {
                            break;
                        }
                        items.push(self.parse_pattern()?);
                        self.skip_trivia();
                    }
                    self.expect(TokenKind::RParen)?;
                    return Some(Pattern::Tuple(items.into_boxed_slice()));
                }
                self.expect(TokenKind::RParen)?;
                Some(first)
            }
            TokenKind::Ident => {
                let name = self.expect_ident()?;
                if name == "_" {
                    return Some(Pattern::Wildcard);
                }
                if self.at(TokenKind::Hash) {
                    self.bump();
                    let variant = self.expect_ident()?;
                    let mut args: Vec<Pattern> = Vec::new();
                    if self.at(TokenKind::LParen) {
                        self.bump();
                        self.skip_trivia();
                        if !self.at(TokenKind::RParen) {
                            loop {
                                args.push(self.parse_pattern()?);
                                self.skip_trivia();
                                if self.at(TokenKind::Comma) {
                                    self.bump();
                                    continue;
                                }
                                break;
                            }
                        }
                        self.skip_trivia();
                        self.expect(TokenKind::RParen)?;
                    }
                    return Some(Pattern::EnumVariant {
                        ty: name,
                        variant,
                        args: args.into_boxed_slice(),
                    });
                }
                Some(Pattern::Bind(name))
            }
            TokenKind::Int => {
                let t = self.bumped();
                let s = self.token_text(&t);
                Some(Pattern::Int(Self::parse_int_literal(&s)))
            }
            TokenKind::Float => {
                let t = self.bumped();
                let s = self.token_text(&t);
                Some(Pattern::Float(s.parse::<f64>().unwrap_or(0.0)))
            }
            TokenKind::Str => {
                let t = self.bumped();
                let raw = self.token_text(&t);
                let inner = if raw.starts_with("r\"") {
                    unquote(&raw[1..])
                } else if raw.starts_with("\"\"\"") && raw.ends_with("\"\"\"") && raw.len() >= 6 {
                    raw[3..raw.len() - 3].to_string()
                } else {
                    unquote(raw)
                };
                Some(Pattern::Str(inner))
            }
            TokenKind::True => {
                self.bump();
                Some(Pattern::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Some(Pattern::Bool(false))
            }
            _ => {
                let span = self.cur_span();
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::ExpectedToken("pattern".to_string()),
                    Some(span),
                ));
                None
            }
        }
    }

    fn is_does_block_start(&self) -> bool {
        self.peek_kind() == TokenKind::Ident && self.peek_kind_n(1) == Some(TokenKind::KwDoes)
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        let mut params: Vec<Param> = Vec::new();
        if self.at(TokenKind::RParen) {
            return Some(params);
        }
        loop {
            self.skip_layout();
            let name = if self.at(TokenKind::KwSelf) {
                self.bump();
                "self".to_string()
            } else {
                self.expect_ident()?
            };
            let ty = if self.at(TokenKind::Colon) {
                self.bump();
                Some(self.parse_type_ref()?)
            } else {
                None
            };
            let default = if self.at(TokenKind::Eq) {
                self.bump();
                Some(self.parse_expr(0)?)
            } else {
                None
            };
            params.push(Param { name, ty, default });
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        Some(params)
    }

    fn parse_if(&mut self) -> Option<IfStmt> {
        self.expect(TokenKind::KwIf)?;
        let cond = self.parse_expr_no_struct_init(0)?;
        let body = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_block_or_inline_stmt_after_colon(false)?
        } else {
            self.parse_block()?
        };
        let mut branches = vec![(cond, body)];

        let mut else_branch = None;
        loop {
            self.skip_trivia();
            if self.at(TokenKind::KwElse) {
                self.bump();
                self.skip_trivia();
                if self.at(TokenKind::KwIf) {
                    self.bump();
                    let cond = self.parse_expr_no_struct_init(0)?;
                    let body = if self.at(TokenKind::Colon) {
                        self.bump();
                        self.parse_block_or_inline_stmt_after_colon(false)?
                    } else {
                        self.parse_block()?
                    };
                    branches.push((cond, body));
                    continue;
                }
                else_branch = Some(if self.at(TokenKind::Colon) {
                    self.bump();
                    self.parse_block_or_inline_stmt_after_colon(false)?
                } else {
                    self.parse_block()?
                });
            }
            break;
        }

        Some(IfStmt {
            branches: branches.into_boxed_slice(),
            else_branch,
        })
    }

    fn parse_while(&mut self) -> Option<WhileStmt> {
        self.expect(TokenKind::KwWhile)?;
        let cond = self.parse_expr_no_struct_init(0)?;
        let body = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_block_or_inline_stmt_after_colon(false)?
        } else {
            self.parse_block()?
        };
        Some(WhileStmt { cond, body })
    }

    fn parse_foreach(&mut self) -> Option<ForEachStmt> {
        self.expect(TokenKind::KwFor)?;
        let var = self.expect_ident()?;
        self.expect(TokenKind::KwIn)?;
        let iter = self.parse_expr_no_struct_init(0)?;
        let body = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_block_or_inline_stmt_after_colon(false)?
        } else {
            self.parse_block()?
        };
        Some(ForEachStmt { iter, var, body })
    }

    fn parse_return(&mut self) -> Option<Stmt> {
        self.expect(TokenKind::KwReturn)?;
        if self.at(TokenKind::StmtEnd) {
            self.expect_stmt_terminator()?;
            return Some(Stmt::Return(None));
        }
        let expr = self.parse_expr(0)?;
        self.expect_stmt_terminator()?;
        Some(Stmt::Return(Some(expr)))
    }

    fn parse_simple_kw_stmt(&mut self, kw: TokenKind, stmt: Stmt) -> Option<Stmt> {
        self.expect(kw)?;
        self.expect_stmt_terminator()?;
        Some(stmt)
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Option<Stmt> {
        let lhs = self.parse_expr(0)?;

        let mut ty = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type_ref()?);
        }

        if let Some(op) = match self.peek_kind() {
            TokenKind::Eq => Some(AssignOp::Set),
            TokenKind::PlusEq => Some(AssignOp::Add),
            TokenKind::MinusEq => Some(AssignOp::Sub),
            TokenKind::StarEq => Some(AssignOp::Mul),
            TokenKind::SlashEq => Some(AssignOp::Div),
            _ => None,
        } {
            self.bump();
            let value = self.parse_expr(0)?;
            self.expect_stmt_terminator()?;
            return Some(Stmt::Assign(Box::new(AssignStmt {
                vis: Visibility::Public,
                target: lhs,
                op,
                value,
                ty,
                slot: None,
                decl: None,
            })));
        }

        if ty.is_some() {
            // Type annotation without an assignment is an error.
            let span = self.cur_span();
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::ExpectedToken("assignment operator".to_string()),
                Some(span),
            ));
        }

        self.expect_stmt_terminator()?;
        Some(Stmt::Expr(lhs))
    }

    fn parse_type_ref(&mut self) -> Option<TypeRef> {
        if self.at(TokenKind::LBracket) {
            self.bump();
            self.skip_layout();
            if self.at(TokenKind::RBracket) {
                self.bump();
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::ExpectedToken("type".to_string()),
                    Some(self.cur_span()),
                ));
                return Some(TypeRef {
                    name: "list".to_string(),
                    params: vec![TypeRef {
                        name: "any".to_string(),
                        params: Box::new([]),
                    }]
                    .into_boxed_slice(),
                });
            }
            let item = self.parse_type_ref()?;
            self.skip_layout();
            self.expect(TokenKind::RBracket)?;
            return Some(TypeRef {
                name: "list".to_string(),
                params: vec![item].into_boxed_slice(),
            });
        }
        if self.at(TokenKind::LBrace) {
            self.bump();
            self.skip_layout();
            if self.at(TokenKind::RBrace) {
                self.bump();
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::ExpectedToken("type".to_string()),
                    Some(self.cur_span()),
                ));
                return Some(TypeRef {
                    name: "any".to_string(),
                    params: Box::new([]),
                });
            }
            let first = self.parse_type_ref()?;
            self.skip_layout();
            if self.at(TokenKind::Colon) {
                self.bump();
                self.skip_layout();
                let second = self.parse_type_ref()?;
                self.skip_layout();
                self.expect(TokenKind::RBrace)?;
                return Some(TypeRef {
                    name: "dict".to_string(),
                    params: vec![first, second].into_boxed_slice(),
                });
            }
            self.expect(TokenKind::RBrace)?;
            return Some(TypeRef {
                name: "set".to_string(),
                params: vec![first].into_boxed_slice(),
            });
        }
        if self.at(TokenKind::LParen) {
            self.bump();
            self.skip_layout();
            if self.at(TokenKind::RParen) {
                self.bump();
                return Some(TypeRef {
                    name: "tuple".to_string(),
                    params: Box::new([]),
                });
            }
            let first = self.parse_type_ref()?;
            self.skip_layout();
            let mut items = vec![first];
            if self.at(TokenKind::Comma) {
                self.bump();
                self.skip_layout();
                if !self.at(TokenKind::RParen) {
                    loop {
                        items.push(self.parse_type_ref()?);
                        self.skip_layout();
                        if self.at(TokenKind::Comma) {
                            self.bump();
                            self.skip_layout();
                            if self.at(TokenKind::RParen) {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
            }
            self.expect(TokenKind::RParen)?;
            return Some(TypeRef {
                name: "tuple".to_string(),
                params: items.into_boxed_slice(),
            });
        }

        let name = if self.at(TokenKind::Question) {
            self.bump();
            "?".to_string()
        } else {
            self.expect_ident()?
        };
        let mut params = Vec::new();
        if self.at(TokenKind::LBracket) {
            self.bump();
            self.skip_layout();
            if !self.at(TokenKind::RBracket) {
                loop {
                    let p = self.parse_type_ref()?;
                    params.push(p);
                    self.skip_layout();
                    if self.at(TokenKind::Comma) {
                        self.bump();
                        self.skip_layout();
                        continue;
                    }
                    break;
                }
            }
            self.expect(TokenKind::RBracket)?;
        }
        Some(TypeRef {
            name,
            params: params.into_boxed_slice(),
        })
    }

    fn parse_block(&mut self) -> Option<Box<[Stmt]>> {
        while self.at(TokenKind::Newline) {
            self.bump();
        }
        if self.at(TokenKind::LBrace) {
            self.bump();
            let mut stmts: Vec<Stmt> = Vec::with_capacity(8);
            while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
                self.skip_trivia();
                if self.at(TokenKind::RBrace) {
                    break;
                }
                match self.parse_stmt() {
                    Some(s) => stmts.push(s),
                    None => stmts.push(self.recover_stmt()),
                }
            }
            self.expect(TokenKind::RBrace)?;
            return Some(stmts.into_boxed_slice());
        }
        let span = self.cur_span();
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::ExpectedToken("{ ... } block".to_string()),
            Some(span),
        ));
        None
    }

    fn parse_block_or_inline_stmt_after_colon(
        &mut self,
        allow_comma_terminator: bool,
    ) -> Option<Box<[Stmt]>> {
        self.skip_trivia();
        if self.at(TokenKind::LBrace) {
            return self.parse_block();
        }
        let prev = self.allow_comma_terminator;
        self.allow_comma_terminator = allow_comma_terminator;
        let stmt = match self.parse_stmt() {
            Some(s) => s,
            None => self.recover_stmt(),
        };
        self.allow_comma_terminator = prev;
        Some(vec![stmt].into_boxed_slice())
    }

    fn recover_stmt(&mut self) -> Stmt {
        let start_span = self.cur_span();
        while !self.at(TokenKind::StmtEnd)
            && !self.at(TokenKind::Eof)
            && !self.at(TokenKind::Dedent)
        {
            self.bump();
        }
        if self.at(TokenKind::StmtEnd) {
            self.bump();
        }
        Stmt::Error(Span::new(start_span.start.0, self.cur_span().end.0))
    }

    fn expect_ident(&mut self) -> Option<String> {
        self.skip_trivia();
        if matches!(
            self.peek_kind(),
            TokenKind::KwCan | TokenKind::KwAsync | TokenKind::KwAwait
        ) {
            let t = self.bumped();
            let kw = self.token_text(&t).to_string();
            self.diagnostics.push(Diagnostic::error(
                format!("Reserved keyword cannot be used as identifier: {}", kw),
                Some(t.span),
            ));
            return None;
        }
        let t = self.expect(TokenKind::Ident)?;
        Some(self.token_text(&t).to_string())
    }

    fn at_arrow(&self) -> bool {
        self.peek_kind() == TokenKind::Minus && self.peek_kind_n(1) == Some(TokenKind::Gt)
    }

    fn expect_stmt_terminator(&mut self) -> Option<()> {
        if self.at(TokenKind::StmtEnd)
            || self.at(TokenKind::Newline)
            || self.at(TokenKind::Eof)
            || self.at(TokenKind::RBrace)
            || self.at(TokenKind::Dedent)
            || (self.allow_comma_terminator && self.at(TokenKind::Comma))
        {
            if self.at(TokenKind::StmtEnd) {
                self.bump();
            }
            return Some(());
        }
        let span = self.cur_span();
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::ExpectedToken("statement terminator".to_string()),
            Some(span),
        ));
        None
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        self.skip_trivia();
        if self.at(kind) {
            return Some(self.bumped());
        }
        let span = self.cur_span();
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::ExpectedToken(format!("{:?}", kind)),
            Some(span),
        ));
        None
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens
            .get(self.i)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    fn peek_kind_n(&self, n: usize) -> Option<TokenKind> {
        self.tokens.get(self.i + n).map(|t| t.kind)
    }

    fn is_type_def_start(&self) -> bool {
        if self.peek_kind() != TokenKind::Ident {
            return false;
        }
        matches!(
            self.peek_kind_n(1),
            Some(TokenKind::KwWith) | Some(TokenKind::KwHas)
        )
    }

    /// Distinguish between `struct` and `enum` definitions using a braced body by scanning for
    /// a top-level `:` inside `{ ... }`.
    fn braced_type_def_is_struct(&self) -> bool {
        if self.peek_kind_n(2) != Some(TokenKind::LBrace) {
            return false;
        }
        if self.peek_kind_n(3) == Some(TokenKind::RBrace) {
            return true;
        }
        let mut j = self.i + 3;
        let mut brace_depth = 1usize;
        let mut bracket_depth = 0usize;
        let mut paren_depth = 0usize;
        while let Some(kind) = self.tokens.get(j).map(|t| t.kind) {
            match kind {
                TokenKind::LBrace => brace_depth += 1,
                TokenKind::RBrace => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0 {
                        break;
                    }
                }
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::Colon => {
                    if brace_depth == 1 && bracket_depth == 0 && paren_depth == 0 {
                        return true;
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
            j += 1;
        }
        false
    }

    fn bumped(&mut self) -> Token {
        let t = self.tokens[self.i].clone();
        self.i += 1;
        t
    }

    fn bump(&mut self) {
        self.i += 1;
    }

    fn skip_trivia(&mut self) {
        while self.at(TokenKind::Newline) {
            self.i += 1;
        }
    }

    fn skip_layout(&mut self) {
        while self.at(TokenKind::Newline)
            || self.at(TokenKind::Indent)
            || self.at(TokenKind::Dedent)
        {
            self.i += 1;
        }
    }

    fn cur_span(&self) -> Span {
        self.tokens
            .get(self.i)
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.input.len() as u32, self.input.len() as u32))
    }

    fn token_text(&self, t: &Token) -> &str {
        &self.input[t.span.start.0 as usize..t.span.end.0 as usize]
    }

    fn stmt_end_char(&self) -> char {
        ';'
    }
}

fn infix_binding_power(op: BinaryOp) -> (u8, u8) {
    match op {
        BinaryOp::Or => (1, 2),
        BinaryOp::And => (3, 4),
        BinaryOp::Eq | BinaryOp::Ne => (5, 6),
        BinaryOp::Gt | BinaryOp::Lt | BinaryOp::Ge | BinaryOp::Le => (7, 8),
        BinaryOp::Add | BinaryOp::Sub => (9, 10),
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => (11, 12),
    }
}

fn prefix_binding_power() -> u8 {
    13
}

fn fast_interpolation_expr(key: &str) -> Option<Expr> {
    if key.chars().all(|c| is_ident_continue(c)) && is_ident_start(key.chars().next()?) {
        return Some(Expr::Ident(key.to_string(), std::cell::Cell::new(None)));
    }
    None
}
