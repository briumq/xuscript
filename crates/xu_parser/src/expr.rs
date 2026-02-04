use super::Parser;
use crate::parser::{infix_binding_power, prefix_binding_power, BraceContent};

use xu_syntax::{Diagnostic, DiagnosticKind, TokenKind, unquote};

use crate::{
    BinaryOp, CallExpr, Expr, FuncDef, IndexExpr, MatchExpr, MemberExpr, MethodCallExpr, Pattern,
    RangeExpr, Stmt, StructInitExpr, StructInitItem, UnaryOp, Visibility,
};

impl<'a, 'b> Parser<'a, 'b> {
    pub(super) fn parse_expr(&mut self, min_bp: u8) -> Option<Expr> {
        let lhs = self.parse_prefix()?;
        self.parse_expr_from_prefix(lhs, min_bp)
    }

    pub(super) fn parse_expr_no_struct_init(&mut self, min_bp: u8) -> Option<Expr> {
        self.with_struct_init(false, |p| p.parse_expr(min_bp))
    }

    fn parse_expr_from_prefix(
        &mut self,
        mut lhs: Expr,
        min_bp: u8,
    ) -> Option<Expr> {
        loop {
            if self.at(TokenKind::Newline) || self.at(TokenKind::StmtEnd) || self.at(TokenKind::Eof)
            {
                break;
            }
            if self.at(TokenKind::DotDot) || self.at(TokenKind::DotDotEq) {
                let inclusive = self.at(TokenKind::DotDotEq);
                let (l_bp, r_bp) = (2, 3);
                if l_bp < min_bp {
                    break;
                }
                self.bump();
                let rhs = self.parse_expr(r_bp)?;
                lhs = Expr::Range(Box::new(RangeExpr {
                    start: Box::new(lhs),
                    end: Box::new(rhs),
                    inclusive,
                }));
                continue;
            }
            let op = match self.peek_kind() {
                TokenKind::PipePipe => BinaryOp::Or,
                TokenKind::AmpAmp => BinaryOp::And,
                TokenKind::EqEq => BinaryOp::Eq,
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

    fn parse_prefix(&mut self) -> Option<Expr> {
        self.skip_trivia();
        match self.peek_kind() {
            TokenKind::Bang => {
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

    fn parse_postfix_expr(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.at(TokenKind::Newline) || self.at(TokenKind::StmtEnd) || self.at(TokenKind::Eof)
            {
                break;
            }
            match self.peek_kind() {
                TokenKind::Hash => {
                    // Handle Type#variant - enum variant
                    match expr {
                        Expr::Ident(ty, _) => {
                            // Local enum: Type#variant
                            self.bump();
                            let variant = self.expect_ident()?;
                            expr = Expr::EnumCtor {
                                module: None,
                                ty,
                                variant,
                                args: Box::new([]),
                            };
                        }
                        Expr::Member(m) => {
                            // Cross-module enum: module.Type#variant
                            self.bump();
                            let variant = self.expect_ident()?;
                            expr = Expr::EnumCtor {
                                module: Some(m.object),
                                ty: m.field,
                                variant,
                                args: Box::new([]),
                            };
                        }
                        _ => break,
                    }
                }
                TokenKind::Dot => {
                    self.bump();
                    let field = if self.at(TokenKind::Ident) {
                        self.expect_ident()?
                    } else if self.peek_kind().is_keyword() {
                        // Allow keywords as field names (e.g., obj.has, obj.is)
                        let t = self.bumped();
                        self.token_text(&t).to_string()
                    } else if self.at(TokenKind::Int) {
                        let t = self.bumped();
                        self.token_text(&t).to_string()
                    } else {
                        let span = self.cur_span();
                        self.diagnostics.push(Diagnostic::error_kind(
                            DiagnosticKind::ExpectedToken("identifier".to_string()),
                            Some(span),
                        ));
                        return None;
                    };
                    expr = Expr::Member(Box::new(MemberExpr {
                        object: Box::new(expr),
                        field,
                        ic_slot: std::cell::Cell::new(None),
                    }));
                }
                TokenKind::LBracket => {
                    self.bump();
                    let start = self.parse_expr(3)?;
                    self.skip_trivia();
                    let index = if self.at(TokenKind::DotDot) || self.at(TokenKind::DotDotEq) {
                        self.bump();
                        let end = self.parse_expr(0)?;
                        Expr::Range(Box::new(RangeExpr {
                            start: Box::new(start),
                            end: Box::new(end),
                            inclusive: true,
                        }))
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
                    if let Expr::EnumCtor { module, ty, variant, .. } = expr {
                        expr = Expr::EnumCtor {
                            module,
                            ty,
                            variant,
                            args: args.into_boxed_slice(),
                        };
                    } else if let Expr::Member(m) = expr {
                        // Always generate MethodCall, let runtime decide if it's static or instance
                        expr = Expr::MethodCall(Box::new(MethodCallExpr {
                            receiver: m.object,
                            method: m.field,
                            args: args.into_boxed_slice(),
                            ic_slot: std::cell::Cell::new(None),
                            receiver_ty: std::cell::Cell::new(None),
                        }));
                    } else {
                        expr = Expr::Call(Box::new(CallExpr {
                            callee: Box::new(expr),
                            args: args.into_boxed_slice(),
                        }));
                    }
                }
                TokenKind::Ident if self.peek_kind_n(1) == Some(TokenKind::LParen) => {
                    let t = self.bumped();
                    let method = self.token_text(&t).to_string();
                    let args = self.parse_args()?;
                    // Always generate MethodCall, let runtime decide if it's static or instance
                    expr = Expr::MethodCall(Box::new(MethodCallExpr {
                        receiver: Box::new(expr),
                        method,
                        args: args.into_boxed_slice(),
                        ic_slot: std::cell::Cell::new(None),
                        receiver_ty: std::cell::Cell::new(None),
                    }));
                }
                TokenKind::LBrace => {
                    if self.struct_init_allowed {
                        // Special handling for set{...} syntax
                        if let Expr::Ident(ty, _) = &expr {
                            if ty == "set" {
                                let items = self.parse_set_items()?;
                                // Desugar to: __set_from_list([items...])
                                let list_expr = Expr::List(items.into_boxed_slice());
                                expr = Expr::Call(Box::new(CallExpr {
                                    callee: Box::new(Expr::Ident(
                                        "__set_from_list".to_string(),
                                        std::cell::Cell::new(None),
                                    )),
                                    args: Box::new([list_expr]),
                                }));
                                continue;
                            }
                        }
                        // Check if { } looks like struct/dict literal (not a block)
                        // Patterns: { }, { ident: }, { ... }
                        if self.looks_like_struct_init() {
                            match &expr {
                                Expr::Ident(ty, _) => {
                                    // Local struct init: Type{ ... }
                                    let fields = self.parse_struct_init_fields()?;
                                    expr = Expr::StructInit(Box::new(StructInitExpr {
                                        module: None,
                                        ty: ty.clone(),
                                        items: fields.into_boxed_slice(),
                                    }));
                                    continue;
                                }
                                Expr::Member(m) => {
                                    // Cross-module struct init: module.Type{ ... }
                                    let fields = self.parse_struct_init_fields()?;
                                    expr = Expr::StructInit(Box::new(StructInitExpr {
                                        module: Some(m.object.clone()),
                                        ty: m.field.clone(),
                                        items: fields.into_boxed_slice(),
                                    }));
                                    continue;
                                }
                                _ => {}
                            }
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
            TokenKind::Pipe => self.parse_closure_expr(),
            TokenKind::PipePipe => self.parse_empty_closure_expr(),
            TokenKind::KwSelf => {
                self.bump();
                Some(Expr::Ident("self".to_string(), std::cell::Cell::new(None)))
            }
            TokenKind::Ident => {
                let s = self.expect_ident()?;
                Some(Expr::Ident(s, std::cell::Cell::new(None)))
            }
            TokenKind::Int => {
                let t = self.bumped();
                let s = self.token_text(&t);
                let v = Self::parse_int_literal(s);
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
            TokenKind::LParen => {
                self.bump();
                self.skip_layout();
                if self.at(TokenKind::RParen) {
                    self.bump();
                    return Some(Expr::Tuple(Box::new([])));
                }
                let first = self.parse_expr(0)?;
                self.skip_layout();
                if self.at(TokenKind::Comma) {
                    self.bump();
                    let mut items: Vec<Expr> = Vec::with_capacity(4);
                    items.push(first);
                    self.skip_layout();
                    if !self.at(TokenKind::RParen) {
                        loop {
                            items.push(self.parse_expr(0)?);
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
                    Some(Expr::Tuple(items.into_boxed_slice()))
                } else {
                    self.expect(TokenKind::RParen)?;
                    Some(Expr::Group(Box::new(first)))
                }
            }
            TokenKind::LBracket => self.parse_list_or_range(),
            TokenKind::LBrace => self.parse_dict(),
            TokenKind::KwIf => self.parse_if_expr(),
            TokenKind::KwMatch => self.parse_match_expr(),
            _ => {
                let span = self.cur_span();
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::ExpectedExpression,
                    Some(span),
                ));
                // Consume the unexpected token to avoid infinite loops
                self.bump();
                Some(Expr::Error(span))
            }
        }
    }

    /// Parse empty closure expression: || expr or || { body }
    fn parse_empty_closure_expr(&mut self) -> Option<Expr> {
        self.expect(TokenKind::PipePipe)?;

        self.skip_trivia();
        let name = format!("__anon_func_{}", self.tmp_counter);
        self.tmp_counter += 1;

        // Check for { body } form
        if self.at(TokenKind::LBrace) {
            let body = self.parse_block()?;
            return Some(Expr::FuncLit(Box::new(FuncDef {
                vis: Visibility::Inner,
                name,
                params: Box::new([]),
                return_ty: None,
                body,
            })));
        }

        // Single expression form: || expr
        let expr = self.parse_expr(0)?;
        let body = vec![Stmt::Return(Some(expr))].into_boxed_slice();
        Some(Expr::FuncLit(Box::new(FuncDef {
            vis: Visibility::Inner,
            name,
            params: Box::new([]),
            return_ty: None,
            body,
        })))
    }

    /// Parse closure expression: |params| expr or |params| -> T { body }
    fn parse_closure_expr(&mut self) -> Option<Expr> {
        self.expect(TokenKind::Pipe)?;

        // Parse parameters
        let params = if self.at(TokenKind::Pipe) {
            // Empty params: ||
            vec![]
        } else {
            self.parse_closure_params()?
        };
        self.expect(TokenKind::Pipe)?;

        self.skip_trivia();
        let name = format!("__anon_func_{}", self.tmp_counter);
        self.tmp_counter += 1;

        // Check for -> T { body } form (with return type)
        if self.at_arrow() {
            self.bump();
            if self.at(TokenKind::Gt) {
                self.bump();
            }
            self.skip_trivia();

            // Try to parse return type
            let saved_i = self.i;
            let saved_diags = self.diagnostics.len();
            if let Some(ret) = self.parse_type_ref() {
                self.skip_trivia();
                if self.at(TokenKind::LBrace) {
                    let body = self.parse_block()?;
                    return Some(Expr::FuncLit(Box::new(FuncDef {
                        vis: Visibility::Inner,
                        name,
                        params: params.into_boxed_slice(),
                        return_ty: Some(ret),
                        body,
                    })));
                }
            }
            // Restore if not a valid return type + block
            self.i = saved_i;
            self.diagnostics.truncate(saved_diags);
        }

        // Check for { body } form (block without return type)
        if self.at(TokenKind::LBrace) {
            let body = self.parse_block()?;
            return Some(Expr::FuncLit(Box::new(FuncDef {
                vis: Visibility::Inner,
                name,
                params: params.into_boxed_slice(),
                return_ty: None,
                body,
            })));
        }

        // Single expression form: |x| expr
        let expr = self.parse_expr(0)?;
        let body = vec![Stmt::Return(Some(expr))].into_boxed_slice();
        Some(Expr::FuncLit(Box::new(FuncDef {
            vis: Visibility::Inner,
            name,
            params: params.into_boxed_slice(),
            return_ty: None,
            body,
        })))
    }

    /// Parse closure parameters (without surrounding pipes)
    fn parse_closure_params(&mut self) -> Option<Vec<crate::Param>> {
        let mut params: Vec<crate::Param> = Vec::with_capacity(4);
        loop {
            self.skip_trivia();
            if self.at(TokenKind::Pipe) {
                break;
            }
            let name = self.expect_ident()?;
            self.skip_trivia();
            let ty = if self.at(TokenKind::Colon) {
                self.bump();
                Some(self.parse_type_ref()?)
            } else {
                None
            };
            params.push(crate::Param {
                name,
                ty,
                default: None,
            });
            self.skip_trivia();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        Some(params)
    }

    fn parse_match_expr(&mut self) -> Option<Expr> {
        self.expect(TokenKind::KwMatch)?;
        let expr = self.parse_expr_no_struct_init(0)?;
        self.skip_trivia();
        self.expect(TokenKind::LBrace)?;
        self.skip_layout();
        let mut arms: Vec<(Pattern, Expr)> = Vec::with_capacity(4);
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let p = self.parse_pattern()?;
            let b = if self.at(TokenKind::Colon) {
                self.bump();
                self.skip_trivia();
                self.parse_expr(0)?
            } else {
                self.parse_expr_block()?
            };
            arms.push((p, b));
            self.skip_layout();
        }
        let else_expr = if let Some((Pattern::Wildcard, e)) = arms.last() {
            let e = e.clone();
            arms.pop();
            Some(Box::new(e))
        } else {
            let span = self.cur_span();
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::ExpectedToken("_".to_string()),
                Some(span),
            ));
            Some(Box::new(Expr::Error(span)))
        };
        self.expect(TokenKind::RBrace)?;
        Some(Expr::Match(Box::new(MatchExpr {
            expr: Box::new(expr),
            arms: arms.into_boxed_slice(),
            else_expr,
        })))
    }

    fn parse_if_expr(&mut self) -> Option<Expr> {
        self.expect(TokenKind::KwIf)?;
        let cond = self.parse_expr_no_struct_init(0)?;
        let then_expr = if self.at(TokenKind::Colon) {
            self.bump();
            self.skip_trivia();
            self.parse_expr(0)?
        } else {
            self.parse_expr_block()?
        };
        self.skip_trivia();
        self.expect(TokenKind::KwElse)?;
        self.skip_trivia();
        let else_expr = if self.at(TokenKind::KwIf) {
            self.parse_if_expr()?
        } else if self.at(TokenKind::Colon) {
            self.bump();
            self.skip_trivia();
            self.parse_expr(0)?
        } else {
            self.parse_expr_block()?
        };
        Some(Expr::IfExpr(Box::new(xu_ir::IfExpr {
            cond: Box::new(cond),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
        })))
    }

    fn parse_expr_block(&mut self) -> Option<Expr> {
        self.skip_trivia();
        if self.at(TokenKind::LBrace) {
            self.bump();
            self.skip_layout();
            let expr = self.parse_expr(0)?;
            self.skip_layout();
            if self.at(TokenKind::StmtEnd) || self.at(TokenKind::Newline) {
                self.bump();
            }
            self.skip_layout();
            self.expect(TokenKind::RBrace)?;
            return Some(expr);
        }
        let span = self.cur_span();
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::ExpectedToken("{ ... } block".to_string()),
            Some(span),
        ));
        None
    }

    fn parse_list_or_range(&mut self) -> Option<Expr> {
        self.expect(TokenKind::LBracket)?;
        self.skip_layout();
        if self.at(TokenKind::RBracket) {
            self.bump();
            return Some(Expr::List(vec![].into_boxed_slice()));
        }
        let first = self.parse_expr(3)?;
        self.skip_layout();
        if self.at(TokenKind::RBracket) && matches!(first, Expr::Range(_)) {
            self.bump();
            return Some(first);
        }
        if self.at(TokenKind::DotDot) || self.at(TokenKind::DotDotEq) {
            self.bump();
            let end = self.parse_expr(0)?;
            self.skip_layout();
            self.expect(TokenKind::RBracket)?;
            return Some(Expr::Range(Box::new(RangeExpr {
                start: Box::new(first),
                end: Box::new(end),
                inclusive: true,
            })));
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

    fn parse_set_items(&mut self) -> Option<Vec<Expr>> {
        self.expect(TokenKind::LBrace)?;
        let mut items: Vec<Expr> = Vec::with_capacity(4);
        self.skip_layout();
        if self.at(TokenKind::RBrace) {
            self.bump();
            return Some(items);
        }
        loop {
            self.skip_layout();
            items.push(self.parse_expr(0)?);
            self.skip_layout();
            if self.at(TokenKind::Comma) {
                self.bump();
                self.skip_layout();
                if self.at(TokenKind::RBrace) {
                    break;
                }
                continue;
            }
            break;
        }
        self.skip_layout();
        self.expect(TokenKind::RBrace)?;
        Some(items)
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
            let key = if self.at(TokenKind::Str) {
                let key_tok = self.bumped();
                unquote(self.token_text(&key_tok))
            } else if self.at(TokenKind::Ident) || self.peek_kind().is_keyword() {
                // Allow keywords as dict keys (e.g., { has: 1, is: 2 })
                let t = self.bumped();
                self.token_text(&t).to_string()
            } else {
                let span = self.cur_span();
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::ExpectedToken("string or identifier".to_string()),
                    Some(span),
                ));
                return None;
            };
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

    /// Check if the current `{` looks like struct/dict literal syntax (not a block).
    fn looks_like_struct_init(&self) -> bool {
        if !self.at(TokenKind::LBrace) {
            return false;
        }
        matches!(
            self.classify_brace_content(),
            BraceContent::Empty | BraceContent::FieldInit | BraceContent::Spread
        )
    }

    fn parse_struct_init_fields(&mut self) -> Option<Vec<StructInitItem>> {
        self.expect(TokenKind::LBrace)?;
        let mut entries: Vec<StructInitItem> = Vec::with_capacity(4);
        self.skip_layout();
        if self.at(TokenKind::RBrace) {
            self.bump();
            return Some(entries);
        }
        loop {
            self.skip_layout();
            if self.at(TokenKind::Ellipsis) {
                self.bump();
                let value = self.parse_expr(0)?;
                entries.push(StructInitItem::Spread(value));
            } else {
                let key = self.expect_field_name()?;
                self.expect(TokenKind::Colon)?;
                let value = self.parse_expr(0)?;
                entries.push(StructInitItem::Field(key, value));
            }
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
        // 在括号内允许结构体字面量，因为括号提供了明确的边界
        self.with_struct_init(true, |p| {
            loop {
                p.skip_layout();
                if p.at(TokenKind::RParen) {
                    break;
                }
                args.push(p.parse_expr(0)?);
                p.skip_layout();
                if p.at(TokenKind::Comma) {
                    p.bump();
                    continue;
                }
                break;
            }
            Some(())
        })?;
        self.skip_layout();
        self.expect(TokenKind::RParen)?;
        Some(args)
    }
}
