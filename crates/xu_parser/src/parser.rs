//! Parser.
//!
//! Converts lexer tokens into a syntax tree (Module/Stmt/Expr) and collects diagnostics.
//! The implementation uses a recursive-descent statement parser plus Pratt parsing for
//! expressions.
use std::collections::HashMap;
use xu_syntax::{
    Diagnostic, DiagnosticKind, Span, Token, TokenKind,
};

use crate::{
    Expr, Module, Pattern, Stmt,
};



/// Parse result.
pub struct ParseResult {
    pub module: Module,
    pub diagnostics: Vec<Diagnostic>,
}

/// Xu parser.
pub struct Parser<'a, 'b> {
    pub input: &'a str,
    pub tokens: &'a [Token],
    pub i: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub interp_cache: HashMap<String, Expr>,
    pub pending_stmts: Vec<Stmt>,
    pub tmp_counter: u32,
    pub allow_comma_terminator: bool,
    pub bump: &'b bumpalo::Bump,
}

impl<'a, 'b> Parser<'a, 'b> {
    pub fn parse_int_literal(s: &str) -> i64 {
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

    pub fn parse_block(&mut self) -> Option<Box<[Stmt]>> {
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
                    Some(s) => {
                        stmts.push(s);
                        if !self.pending_stmts.is_empty() {
                            stmts.extend(self.pending_stmts.drain(..));
                        }
                    }
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

    pub fn parse_block_or_inline_stmt_after_colon(
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

    pub fn recover_stmt(&mut self) -> Stmt {
        let start_span = self.cur_span();
        let mut brace_depth = 0;
        while !self.at(TokenKind::Eof) {
            if self.at(TokenKind::LBrace) {
                brace_depth += 1;
                self.bump();
                continue;
            }
            if self.at(TokenKind::RBrace) {
                if brace_depth > 0 {
                    brace_depth -= 1;
                    self.bump();
                    continue;
                } else {
                    // Don't consume unmatched RBrace
                    break;
                }
            }
            if brace_depth == 0 {
                if self.at(TokenKind::StmtEnd) || self.at(TokenKind::Newline) {
                    break;
                }
            }
            self.bump();
        }
        if self.at(TokenKind::StmtEnd) || self.at(TokenKind::Newline) {
            self.bump();
        }
        Stmt::Error(Span::new(start_span.start.0, self.cur_span().end.0))
    }

    pub fn expect_ident(&mut self) -> Option<String> {
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

    pub fn at_arrow(&self) -> bool {
        self.peek_kind() == TokenKind::Minus && self.peek_kind_n(1) == Some(TokenKind::Gt)
    }

    pub fn expect_stmt_terminator(&mut self) -> Option<()> {
        if self.at(TokenKind::StmtEnd)
            || self.at(TokenKind::Newline)
            || self.at(TokenKind::Eof)
            || self.at(TokenKind::RBrace)
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

    pub fn expect(&mut self, kind: TokenKind) -> Option<Token> {
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

    pub fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    pub fn peek_kind(&self) -> TokenKind {
        self.tokens
            .get(self.i)
            .map(|t| t.kind)
            .unwrap_or(TokenKind::Eof)
    }

    pub fn peek_kind_n(&self, n: usize) -> Option<TokenKind> {
        self.tokens.get(self.i + n).map(|t| t.kind)
    }

    /// Peek at the token kind after `{`, skipping whitespace/newlines.
    pub fn peek_kind_after_lbrace(&self) -> Option<TokenKind> {
        let mut j = self.i + 1; // skip the `{`
        while j < self.tokens.len() {
            let kind = self.tokens[j].kind;
            if kind == TokenKind::Newline {
                j += 1;
                continue;
            }
            return Some(kind);
        }
        None
    }

    /// Peek at the token kind after `{ ident`, skipping whitespace/newlines.
    pub fn peek_kind_after_lbrace_ident(&self) -> Option<TokenKind> {
        let mut j = self.i + 1; // skip the `{`
        // Skip whitespace to find the identifier
        while j < self.tokens.len() {
            let kind = self.tokens[j].kind;
            if kind == TokenKind::Newline {
                j += 1;
                continue;
            }
            break;
        }
        // Skip the identifier
        if j < self.tokens.len() && self.tokens[j].kind == TokenKind::Ident {
            j += 1;
        }
        // Skip whitespace after identifier
        while j < self.tokens.len() {
            let kind = self.tokens[j].kind;
            if kind == TokenKind::Newline {
                j += 1;
                continue;
            }
            return Some(kind);
        }
        None
    }

    /// Peek at the token kind after `{ <token>`, skipping whitespace/newlines.
    /// Used for checking what follows a string literal in `{ "key": ... }`.
    pub fn peek_kind_after_lbrace_skip_one(&self) -> Option<TokenKind> {
        let mut j = self.i + 1; // skip the `{`
        // Skip whitespace to find the first token
        while j < self.tokens.len() {
            let kind = self.tokens[j].kind;
            if kind == TokenKind::Newline {
                j += 1;
                continue;
            }
            break;
        }
        // Skip the first token (e.g., string literal)
        if j < self.tokens.len() {
            j += 1;
        }
        // Skip whitespace after the token
        while j < self.tokens.len() {
            let kind = self.tokens[j].kind;
            if kind == TokenKind::Newline {
                j += 1;
                continue;
            }
            return Some(kind);
        }
        None
    }

    pub fn bumped(&mut self) -> Token {
        let t = self.tokens[self.i].clone();
        self.i += 1;
        t
    }

    pub fn bump(&mut self) {
        self.i += 1;
    }

    pub fn skip_trivia(&mut self) {
        while self.at(TokenKind::Newline) {
            self.i += 1;
        }
    }

    pub fn skip_layout(&mut self) {
        while self.at(TokenKind::Newline) {
            self.i += 1;
        }
    }

    pub fn cur_span(&self) -> Span {
        self.tokens
            .get(self.i)
            .map(|t| t.span)
            .unwrap_or_else(|| Span::new(self.input.len() as u32, self.input.len() as u32))
    }

    pub fn token_text(&self, t: &Token) -> &str {
        &self.input[t.span.start.0 as usize..t.span.end.0 as usize]
    }

    pub fn parse_params(&mut self) -> Option<Vec<crate::Param>> {
        let mut params: Vec<crate::Param> = Vec::new();
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
            params.push(crate::Param { name, ty, default });
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        Some(params)
    }

    pub fn parse_pattern(&mut self) -> Option<Pattern> {
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
                    xu_syntax::unquote(&raw[1..])
                } else if raw.starts_with("\"\"\"") && raw.ends_with("\"\"\"") && raw.len() >= 6 {
                    raw[3..raw.len() - 3].to_string()
                } else {
                    xu_syntax::unquote(raw)
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
}

pub fn infix_binding_power(op: crate::BinaryOp) -> (u8, u8) {
    match op {
        crate::BinaryOp::Or => (1, 2),
        crate::BinaryOp::And => (3, 4),
        crate::BinaryOp::Eq | crate::BinaryOp::Ne => (5, 6),
        crate::BinaryOp::Gt | crate::BinaryOp::Lt | crate::BinaryOp::Ge | crate::BinaryOp::Le => (7, 8),
        crate::BinaryOp::Add | crate::BinaryOp::Sub => (9, 10),
        crate::BinaryOp::Mul | crate::BinaryOp::Div | crate::BinaryOp::Mod => (11, 12),
    }
}

pub fn prefix_binding_power() -> u8 {
    13
}

pub fn fast_interpolation_expr(key: &str) -> Option<Expr> {
    if key.chars().all(|c| xu_syntax::is_ident_continue(c)) && xu_syntax::is_ident_start(key.chars().next()?) {
        return Some(Expr::Ident(key.to_string(), std::cell::Cell::new(None)));
    }
    None
}
