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
    AssignOp, AssignStmt, BinaryOp, CallExpr, CatchClause, DeclKind, DoesBlock, EnumDef, Expr,
    ForEachStmt, FuncDef, IfStmt, IndexExpr, MemberExpr, MethodCallExpr, Module, Param, Pattern,
    Stmt, StructDef, StructField, StructInitExpr, TryStmt, TypeRef, UnaryOp, Visibility, WhenStmt,
    WhileStmt,
};

mod interp;

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
    bump: &'b bumpalo::Bump,
}

impl<'a, 'b> Parser<'a, 'b> {
    /// Create a new parser.
    pub fn new(input: &'a str, tokens: &'a [Token], bump: &'b bumpalo::Bump) -> Self {
        Self {
            input,
            tokens,
            i: 0,
            diagnostics: Vec::with_capacity(32),
            interp_cache: HashMap::with_capacity(32),
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
            match self.parse_stmt() {
                Some(stmt) => stmts.push(stmt),
                None => stmts.push(self.recover_stmt()),
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
        if self.at(TokenKind::Ident) && self.token_text(&self.tokens[self.i]) == "inner" {
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
            TokenKind::KwTry => self.parse_try().map(|x| Stmt::Try(Box::new(x))),
            TokenKind::KwReturn => self.parse_return(),
            TokenKind::KwBreak => self.parse_simple_kw_stmt(TokenKind::KwBreak, Stmt::Break),
            TokenKind::KwContinue => {
                self.parse_simple_kw_stmt(TokenKind::KwContinue, Stmt::Continue)
            }
            TokenKind::KwThrow => self.parse_throw(),
            TokenKind::KwImport => self.parse_import_stmt(),
            TokenKind::Ident => {
                let t = &self.tokens[self.i];
                let kw = self.token_text(t);
                if kw == "let" || kw == "var" {
                    return self.parse_let_var_decl();
                }
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
    fn parse_let_var_decl(&mut self) -> Option<Stmt> {
        // consume 'let' or 'var'
        let kw_token = self.expect(TokenKind::Ident)?;
        self.skip_trivia();
        // variable name
        let name = self.expect_ident()?;
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
        // if no type provided, use any to allow declaration
        if ty.is_none() {
            ty = Some(TypeRef {
                name: "any".to_string(),
                params: Box::new([]),
            });
        }
        let kw_text = self.token_text(&kw_token);
        let decl = if kw_text == "let" {
            Some(DeclKind::Let)
        } else {
            Some(DeclKind::Var)
        };
        Some(Stmt::Assign(Box::new(AssignStmt {
            target: Expr::Ident(name, Cell::new(None)),
            op: AssignOp::Set,
            value,
            ty,
            slot: None,
            decl,
        })))
    }

    fn parse_import_stmt(&mut self) -> Option<Stmt> {
        self.expect(TokenKind::KwImport)?;
        if self.at(TokenKind::Str) {
            let t = self.expect(TokenKind::Str)?;
            let raw = self.token_text(&t);
            let path = unquote(raw);
            let call = Expr::Call(Box::new(CallExpr {
                callee: Box::new(Expr::Ident(
                    "import".to_string(),
                    std::cell::Cell::new(None),
                )),
                args: vec![Expr::Str(path)].into_boxed_slice(),
            }));
            self.skip_layout();
            if self.at(TokenKind::Ident) && self.token_text(&self.tokens[self.i]) == "as" {
                self.bump();
                let alias = self.expect_ident()?;
                self.expect_stmt_terminator()?;
                return Some(Stmt::Assign(Box::new(AssignStmt {
                    target: Expr::Ident(alias, std::cell::Cell::new(None)),
                    op: AssignOp::Set,
                    value: call,
                    ty: None,
                    slot: None,
                    decl: None,
                })));
            }
            self.expect_stmt_terminator()?;
            return Some(Stmt::Expr(call));
        }
        if self.at(TokenKind::LParen) {
            let args = self.parse_args()?;
            let call = Expr::Call(Box::new(CallExpr {
                callee: Box::new(Expr::Ident(
                    "import".to_string(),
                    std::cell::Cell::new(None),
                )),
                args: args.into_boxed_slice(),
            }));
            self.expect_stmt_terminator()?;
            return Some(Stmt::Expr(call));
        }
        None
    }

    fn parse_struct_def(&mut self, vis: Visibility) -> Option<StructDef> {
        let name = self.expect_ident()?;
        self.expect(TokenKind::KwWith)?;
        self.expect(TokenKind::LBrace)?;
        let fields = self.parse_struct_fields()?;
        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::StmtEnd)?;
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
            if self.at(TokenKind::Comma) || self.at(TokenKind::Pipe) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect(close)?;
        self.expect(TokenKind::StmtEnd)?;
        Some(EnumDef {
            vis,
            name,
            variants: variants.into_boxed_slice(),
        })
    }

    fn parse_struct_fields(&mut self) -> Option<Vec<StructField>> {
        let mut fields: Vec<StructField> = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_layout();
            if self.at(TokenKind::RBrace) {
                break;
            }
            let name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_ref()?;
            fields.push(StructField { name, ty });
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        Some(fields)
    }

    fn parse_func_def(&mut self, vis: Visibility) -> Option<FuncDef> {
        self.expect(TokenKind::KwFunc)?;
        let name = if self.at(TokenKind::LParen) {
            self.bump();
            let receiver_name = self.expect_ident()?;
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
            if self.at(TokenKind::Colon) {
                self.bump();
            }
            let body = self.parse_block()?;
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
        if self.at(TokenKind::Colon) {
            self.bump();
        }
        let body = self.parse_block()?;
        Some(FuncDef {
            vis,
            name,
            params: params.into_boxed_slice(),
            return_ty,
            body,
        })
    }

    fn parse_does_block(&mut self, vis: Visibility) -> Option<DoesBlock> {
        let target = self.expect_ident()?;
        self.expect_ident_text("does")?;
        self.skip_trivia();
        self.expect(TokenKind::LBrace)?;
        let mut funcs: Vec<FuncDef> = Vec::with_capacity(4);
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_trivia();
            if self.at(TokenKind::RBrace) {
                break;
            }
            let mut fvis = Visibility::Public;
            if self.at(TokenKind::Ident) && self.token_text(&self.tokens[self.i]) == "inner" {
                self.bump();
                fvis = Visibility::Inner;
            }
            self.skip_trivia();
            funcs.push(self.parse_func_def(fvis)?);
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
        let mut else_branch: Option<Box<[Stmt]>> = None;
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_trivia();
            if self.at(TokenKind::RBrace) {
                break;
            }
            if self.at(TokenKind::KwElse) {
                self.bump();
                if self.at(TokenKind::Colon) {
                    self.bump();
                }
                else_branch = Some(self.parse_block()?);
                break;
            }
            let pat = self.parse_pattern()?;
            if self.at(TokenKind::Colon) {
                self.bump();
            }
            let body = self.parse_block()?;
            arms.push((pat, body));
            self.skip_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
            }
        }
        self.expect(TokenKind::RBrace)?;
        self.expect_stmt_terminator()?;
        Some(WhenStmt {
            expr,
            arms: arms.into_boxed_slice(),
            else_branch,
        })
    }

    fn parse_pattern(&mut self) -> Option<Pattern> {
        self.skip_trivia();
        match self.peek_kind() {
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
                Some(Pattern::Int(s.parse::<i64>().unwrap_or(0)))
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
            TokenKind::Null => {
                self.bump();
                Some(Pattern::Null)
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
        if self.peek_kind() != TokenKind::Ident || self.peek_kind_n(1) != Some(TokenKind::Ident) {
            return false;
        }
        let t = self.tokens.get(self.i + 1).unwrap();
        self.token_text(t) == "does"
    }

    fn parse_params(&mut self) -> Option<Vec<Param>> {
        let mut params: Vec<Param> = Vec::new();
        if self.at(TokenKind::RParen) {
            return Some(params);
        }
        loop {
            self.skip_layout();
            let name = self.expect_ident()?;
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
        let cond = self.parse_expr(0)?;
        if self.at(TokenKind::Colon) {
            self.bump();
        }
        let body = self.parse_block()?;
        let mut branches = vec![(cond, body)];

        let mut else_branch = None;
        loop {
            self.skip_trivia();
            if self.at(TokenKind::KwElse) {
                self.bump();
                self.skip_trivia();
                if self.at(TokenKind::KwIf) {
                    self.bump();
                    let cond = self.parse_expr(0)?;
                    if self.at(TokenKind::Colon) {
                        self.bump();
                    }
                    let body = self.parse_block()?;
                    branches.push((cond, body));
                    continue;
                }
                if self.at(TokenKind::Colon) {
                    self.bump();
                }
                else_branch = Some(self.parse_block()?);
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
        let cond = self.parse_expr(0)?;
        if self.at(TokenKind::Colon) {
            self.bump();
        }
        let body = self.parse_block()?;
        Some(WhileStmt { cond, body })
    }

    fn parse_foreach(&mut self) -> Option<ForEachStmt> {
        self.expect(TokenKind::KwFor)?;
        let var = self.expect_ident()?;
        self.expect_ident_text("in")?;
        let iter = self.parse_expr(0)?;
        if self.at(TokenKind::Colon) {
            self.bump();
        }
        let body = self.parse_block()?;
        Some(ForEachStmt { iter, var, body })
    }

    fn parse_try(&mut self) -> Option<TryStmt> {
        self.expect(TokenKind::KwTry)?;
        if self.at(TokenKind::Colon) {
            self.bump();
        }
        let body = self.parse_block()?;

        let catch = if self.at(TokenKind::KwCatch) {
            self.bump();
            let var = if self.at(TokenKind::LParen) {
                self.bump();
                let v = if self.at(TokenKind::Ident) {
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect(TokenKind::RParen)?;
                v
            } else if self.at(TokenKind::Ident) {
                Some(self.expect_ident()?)
            } else {
                None
            };
            if self.at(TokenKind::Colon) {
                self.bump();
            }
            let body = self.parse_block()?;
            Some(CatchClause { var, body })
        } else {
            None
        };

        let finally = if self.at(TokenKind::KwFinally) {
            self.bump();
            if self.at(TokenKind::Colon) {
                self.bump();
            }
            Some(self.parse_block()?)
        } else {
            None
        };

        Some(TryStmt {
            body,
            catch,
            finally,
        })
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

    fn parse_throw(&mut self) -> Option<Stmt> {
        self.expect(TokenKind::KwThrow)?;
        let expr = self.parse_expr(0)?;
        self.expect_stmt_terminator()?;
        Some(Stmt::Throw(expr))
    }

    fn parse_simple_kw_stmt(&mut self, kw: TokenKind, stmt: Stmt) -> Option<Stmt> {
        self.expect(kw)?;
        self.expect_stmt_terminator()?;
        Some(stmt)
    }

    fn parse_assign_or_expr_stmt(&mut self) -> Option<Stmt> {
        let lhs = self.parse_expr(0)?;
        self.skip_trivia();

        let mut ty = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type_ref()?);
            self.skip_trivia();
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

    fn parse_expr(&mut self, min_bp: u8) -> Option<Expr> {
        let lhs = self.parse_prefix()?;
        self.parse_expr_from_prefix(lhs, min_bp)
    }

    fn parse_expr_no_struct_init(&mut self, min_bp: u8) -> Option<Expr> {
        let lhs = self.parse_prefix_no_struct_init()?;
        self.parse_expr_from_prefix_no_struct_init(lhs, min_bp)
    }

    /// Parse infix operators using binding powers (Pratt parsing).
    fn parse_expr_from_prefix(&mut self, mut lhs: Expr, min_bp: u8) -> Option<Expr> {
        loop {
            self.skip_trivia();
            let op = match self.peek_kind() {
                TokenKind::KwOr => BinaryOp::Or,
                TokenKind::KwAnd => BinaryOp::And,
                TokenKind::KwIs => BinaryOp::Eq,
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::KwIsnt => BinaryOp::Ne,
                TokenKind::Ne => BinaryOp::Ne,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Ge => BinaryOp::Ge,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };

            let (l_bp, r_bp) = infix_binding_power(op);
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr(r_bp)?;
            lhs = Expr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Some(lhs)
    }

    fn parse_expr_from_prefix_no_struct_init(&mut self, mut lhs: Expr, min_bp: u8) -> Option<Expr> {
        loop {
            self.skip_trivia();
            let op = match self.peek_kind() {
                TokenKind::KwOr => BinaryOp::Or,
                TokenKind::KwAnd => BinaryOp::And,
                TokenKind::KwIs => BinaryOp::Eq,
                TokenKind::EqEq => BinaryOp::Eq,
                TokenKind::KwIsnt => BinaryOp::Ne,
                TokenKind::Ne => BinaryOp::Ne,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::Ge => BinaryOp::Ge,
                TokenKind::Le => BinaryOp::Le,
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => break,
            };

            let (l_bp, r_bp) = infix_binding_power(op);
            if l_bp < min_bp {
                break;
            }
            self.bump();
            let rhs = self.parse_expr_no_struct_init(r_bp)?;
            lhs = Expr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
            };
        }
        Some(lhs)
    }

    fn parse_prefix(&mut self) -> Option<Expr> {
        self.skip_trivia();
        match self.peek_kind() {
            TokenKind::KwNot | TokenKind::Bang => {
                self.bump();
                let expr = self.parse_expr(prefix_binding_power())?;
                Some(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Minus => {
                self.bump();
                let expr = self.parse_expr(prefix_binding_power())?;
                Some(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix_expr(),
        }
    }

    fn parse_prefix_no_struct_init(&mut self) -> Option<Expr> {
        self.skip_trivia();
        match self.peek_kind() {
            TokenKind::KwNot | TokenKind::Bang => {
                self.bump();
                let expr = self.parse_expr_no_struct_init(prefix_binding_power())?;
                Some(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            TokenKind::Minus => {
                self.bump();
                let expr = self.parse_expr_no_struct_init(prefix_binding_power())?;
                Some(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            _ => self.parse_postfix_expr_with_struct_init(false),
        }
    }

    /// Parse postfix expressions (member/index/call/construct).
    fn parse_postfix_expr(&mut self) -> Option<Expr> {
        self.parse_postfix_expr_with_struct_init(true)
    }

    fn parse_postfix_expr_with_struct_init(&mut self, allow_struct_init: bool) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            self.skip_trivia();
            match self.peek_kind() {
                TokenKind::Hash => {
                    if let Expr::Ident(ty, _) = expr {
                        self.bump();
                        let variant = self.expect_ident()?;
                        expr = Expr::EnumCtor {
                            ty,
                            variant,
                            args: Box::new([]),
                        };
                    } else {
                        break;
                    }
                }
                TokenKind::Dot => {
                    self.bump();
                    let field = self.expect_ident()?;
                    expr = Expr::Member(Box::new(MemberExpr {
                        object: Box::new(expr),
                        field,
                        ic_slot: std::cell::Cell::new(None),
                    }));
                }
                TokenKind::LBracket => {
                    self.bump();
                    let start = self.parse_expr(0)?;
                    self.skip_trivia();
                    let index = if self.at(TokenKind::DotDot) {
                        self.bump();
                        let end = self.parse_expr(0)?;
                        Expr::Range(Box::new(start), Box::new(end))
                    } else {
                        start
                    };
                    self.expect(TokenKind::RBracket)?;
                    expr = Expr::Index(Box::new(IndexExpr {
                        object: Box::new(expr),
                        index: Box::new(index),
                        ic_slot: std::cell::Cell::new(None),
                    }));
                }
                TokenKind::LParen => {
                    let args = self.parse_args()?;
                    if let Expr::EnumCtor { ty, variant, .. } = expr {
                        expr = Expr::EnumCtor {
                            ty,
                            variant,
                            args: args.into_boxed_slice(),
                        };
                    } else if let Expr::Member(m) = expr {
                        expr = Expr::MethodCall(Box::new(MethodCallExpr {
                            receiver: m.object,
                            method: m.field,
                            args: args.into_boxed_slice(),
                            ic_slot: std::cell::Cell::new(None),
                        }));
                    } else {
                        expr = Expr::Call(Box::new(CallExpr {
                            callee: Box::new(expr),
                            args: args.into_boxed_slice(),
                        }));
                    }
                }
                TokenKind::Ident if self.peek_kind_n(1) == Some(TokenKind::LParen) => {
                    let method = self.expect_ident()?;
                    let args = self.parse_args()?;
                    expr = Expr::MethodCall(Box::new(MethodCallExpr {
                        receiver: Box::new(expr),
                        method,
                        args: args.into_boxed_slice(),
                        ic_slot: std::cell::Cell::new(None),
                    }));
                }
                TokenKind::KwOr if self.peek_kind_n(1) == Some(TokenKind::LParen) => {
                    let t = self.bumped();
                    let method = self.token_text(&t).to_string();
                    let args = self.parse_args()?;
                    expr = Expr::MethodCall(Box::new(MethodCallExpr {
                        receiver: Box::new(expr),
                        method,
                        args: args.into_boxed_slice(),
                        ic_slot: std::cell::Cell::new(None),
                    }));
                }
                TokenKind::LBrace => {
                    if allow_struct_init {
                        if let Expr::Ident(ty, _) = expr {
                            let fields = self.parse_struct_init_fields()?;
                            expr = Expr::StructInit(Box::new(StructInitExpr {
                                ty,
                                fields: fields.into_boxed_slice(),
                            }));
                            continue;
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        self.skip_trivia();
        match self.peek_kind() {
            TokenKind::Ident => {
                let s = self.expect_ident()?;
                Some(Expr::Ident(s, std::cell::Cell::new(None)))
            }
            TokenKind::Int => {
                let t = self.bumped();
                let s = self.token_text(&t);
                let v = s.parse::<i64>().unwrap_or(0);
                Some(Expr::Int(v))
            }
            TokenKind::Float => {
                let t = self.bumped();
                let s = self.token_text(&t);
                let v = s.parse::<f64>().unwrap_or(0.0);
                Some(Expr::Float(v))
            }
            TokenKind::Str => {
                let t = self.bumped();
                let raw = self.token_text(&t).to_string();
                self.parse_interpolated_string(raw.as_str())
            }
            TokenKind::True => {
                self.bump();
                Some(Expr::Bool(true))
            }
            TokenKind::False => {
                self.bump();
                Some(Expr::Bool(false))
            }
            TokenKind::Null => {
                self.bump();
                Some(Expr::Null)
            }
            TokenKind::LParen => {
                self.bump();
                let expr = self.parse_expr(0)?;
                self.expect(TokenKind::RParen)?;
                Some(Expr::Group(Box::new(expr)))
            }
            TokenKind::LBracket => self.parse_list_or_range(),
            TokenKind::LBrace => self.parse_dict(),
            TokenKind::KwImport => {
                self.bump();
                Some(Expr::Ident(
                    "import".to_string(),
                    std::cell::Cell::new(None),
                ))
            }
            _ => {
                let span = self.cur_span();
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::ExpectedExpression,
                    Some(span),
                ));
                Some(Expr::Error(span))
            }
        }
    }

    fn parse_list_or_range(&mut self) -> Option<Expr> {
        self.expect(TokenKind::LBracket)?;
        self.skip_layout();
        if self.at(TokenKind::RBracket) {
            self.bump();
            return Some(Expr::List(vec![].into_boxed_slice()));
        }
        let first = self.parse_expr(0)?;
        self.skip_layout();
        if self.at(TokenKind::DotDot) {
            self.bump();
            let end = self.parse_expr(0)?;
            self.skip_layout();
            self.expect(TokenKind::RBracket)?;
            return Some(Expr::Range(Box::new(first), Box::new(end)));
        }
        let mut items: Vec<Expr> = Vec::with_capacity(4);
        items.push(first);
        while self.at(TokenKind::Comma) {
            self.bump();
            self.skip_layout();
            if self.at(TokenKind::RBracket) {
                break;
            }
            items.push(self.parse_expr(0)?);
            self.skip_layout();
        }
        self.expect(TokenKind::RBracket)?;
        Some(Expr::List(items.into_boxed_slice()))
    }

    fn parse_dict(&mut self) -> Option<Expr> {
        self.expect(TokenKind::LBrace)?;
        let mut entries: Vec<(String, Expr)> = Vec::with_capacity(4);
        self.skip_layout();
        if self.at(TokenKind::RBrace) {
            self.bump();
            return Some(Expr::Dict(entries.into_boxed_slice()));
        }
        loop {
            self.skip_layout();
            let key_tok = self.expect(TokenKind::Str)?;
            let key = unquote(self.token_text(&key_tok));
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr(0)?;
            entries.push((key, value));
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.skip_layout();
        self.expect(TokenKind::RBrace)?;
        Some(Expr::Dict(entries.into_boxed_slice()))
    }

    fn parse_struct_init_fields(&mut self) -> Option<Vec<(String, Expr)>> {
        self.expect(TokenKind::LBrace)?;
        let mut entries: Vec<(String, Expr)> = Vec::with_capacity(4);
        self.skip_layout();
        if self.at(TokenKind::RBrace) {
            self.bump();
            return Some(entries);
        }
        loop {
            self.skip_layout();
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr(0)?;
            entries.push((key, value));
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.skip_layout();
        self.expect(TokenKind::RBrace)?;
        Some(entries)
    }

    fn parse_args(&mut self) -> Option<Vec<Expr>> {
        self.expect(TokenKind::LParen)?;
        let mut args: Vec<Expr> = Vec::with_capacity(4);
        self.skip_layout();
        if self.at(TokenKind::RParen) {
            self.bump();
            return Some(args);
        }
        loop {
            self.skip_layout();
            if self.at(TokenKind::RParen) {
                break;
            }
            args.push(self.parse_expr(0)?);
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.skip_layout();
        self.expect(TokenKind::RParen)?;
        Some(args)
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
        self.expect(TokenKind::Indent)?;
        let mut stmts: Vec<Stmt> = Vec::with_capacity(8);
        while !self.at(TokenKind::Dedent) && !self.at(TokenKind::Eof) {
            self.skip_trivia();
            if self.at(TokenKind::Dedent) {
                break;
            }
            match self.parse_stmt() {
                Some(s) => stmts.push(s),
                None => stmts.push(self.recover_stmt()),
            }
        }
        self.expect(TokenKind::Dedent)?;
        Some(stmts.into_boxed_slice())
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
        let t = self.expect(TokenKind::Ident)?;
        Some(self.token_text(&t).to_string())
    }

    fn expect_ident_text(&mut self, expected: &str) -> Option<()> {
        let t = self.expect(TokenKind::Ident)?;
        if self.token_text(&t) == expected {
            return Some(());
        }
        let span = t.span;
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::ExpectedToken(expected.to_string()),
            Some(span),
        ));
        None
    }

    fn at_arrow(&self) -> bool {
        self.peek_kind() == TokenKind::Minus && self.peek_kind_n(1) == Some(TokenKind::Gt)
    }

    fn expect_stmt_terminator(&mut self) -> Option<()> {
        if self.at(TokenKind::StmtEnd) || self.at(TokenKind::Newline) || self.at(TokenKind::Eof) {
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
        self.peek_kind() == TokenKind::Ident && self.peek_kind_n(1) == Some(TokenKind::KwWith)
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
