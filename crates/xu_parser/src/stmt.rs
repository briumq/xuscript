use std::cell::Cell;
use crate::mangling::{method_name, static_name, METHOD_PREFIX};
use crate::parser::BraceContent;
use crate::{
    AssignOp, AssignStmt, DeclKind, DoesBlock, EnumDef, Expr, ForEachStmt, FuncDef, IfStmt,
    MemberExpr, MatchStmt, Param, Pattern, StaticField, Stmt, StructDef, StructField, TypeRef, UseStmt, Visibility,
    WhileStmt,
};
use xu_syntax::{Diagnostic, DiagnosticKind, TokenKind, unquote};
use super::Parser;

#[allow(clippy::needless_lifetimes)]
impl<'a, 'b> Parser<'a, 'b> {
    /// Parse a single statement.
    pub(super) fn parse_stmt(&mut self) -> Option<Stmt> {
        self.skip_trivia();
        let mut vis = Visibility::Inner;
        if self.at(TokenKind::KwPub) {
            self.bump();
            vis = Visibility::Public;
            self.skip_trivia();
        }
        match self.peek_kind() {
            TokenKind::KwFunc => self.parse_func_def(vis).map(|x| Stmt::FuncDef(Box::new(x))),
            TokenKind::KwIf => self.parse_if().map(|x| Stmt::If(Box::new(x))),
            TokenKind::KwWhile => self.parse_while().map(|x| Stmt::While(Box::new(x))),
            TokenKind::KwFor => self.parse_foreach().map(|x| Stmt::ForEach(Box::new(x))),
            TokenKind::KwMatch => self.parse_match_stmt().map(|x| Stmt::Match(Box::new(x))),
            TokenKind::KwWhen => self.parse_when_bind_stmt(),
            TokenKind::KwReturn => self.parse_return(),
            TokenKind::KwBreak => self.parse_simple_kw_stmt(TokenKind::KwBreak, Stmt::Break),
            TokenKind::KwContinue => {
                self.parse_simple_kw_stmt(TokenKind::KwContinue, Stmt::Continue)
            }
            TokenKind::KwUse => self.parse_use_stmt(),
            TokenKind::KwLet | TokenKind::KwVar => self.parse_let_var_decl(vis),
            TokenKind::LBrace => {
                // Check if this is a block statement or a dict literal
                if self.is_block_stmt() {
                    self.parse_block_stmt()
                } else {
                    self.parse_assign_or_expr_stmt()
                }
            }
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

    /// Check if the current `{` starts a block statement (not a dict/struct literal).
    fn is_block_stmt(&self) -> bool {
        if !self.at(TokenKind::LBrace) {
            return false;
        }
        matches!(
            self.classify_brace_content(),
            BraceContent::Block | BraceContent::Empty
        )
    }

    /// Parse a block statement: `{ stmts... }`
    fn parse_block_stmt(&mut self) -> Option<Stmt> {
        let stmts = self.parse_block()?;
        Some(Stmt::Block(stmts))
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
            // Only check for 'as' on the same line - don't skip newlines
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
        let mut static_fields: Vec<StaticField> = Vec::with_capacity(2);
        let mut methods: Vec<FuncDef> = Vec::with_capacity(4);
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            self.skip_layout();
            if self.at(TokenKind::RBrace) {
                break;
            }

            let mut item_vis = Visibility::Inner;
            if self.at(TokenKind::KwPub) {
                self.bump();
                item_vis = Visibility::Public;
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
                    f.name = static_name(&name, &f.name);
                } else if !f.name.starts_with(METHOD_PREFIX) {
                    let original = f.name.clone();
                    f.name = method_name(&name, &original);
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
                methods.push(f);
                continue;
            }

            let field_name = self.expect_field_name()?;
            self.expect(TokenKind::Colon)?;
            let field_ty = self.parse_type_ref()?;
            self.skip_layout();

            if is_static {
                // Static fields require a default value
                if !self.at(TokenKind::Eq) {
                    self.diagnostics.push(Diagnostic::error_kind(
                        DiagnosticKind::Raw("Static field requires a default value".to_string()),
                        Some(self.cur_span()),
                    ));
                    return None;
                }
                self.bump();
                let default = self.parse_expr(0)?;
                static_fields.push(StaticField {
                    name: field_name,
                    ty: field_ty,
                    default,
                });
            } else {
                // Instance fields have optional default
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
            }
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
            static_fields: static_fields.into_boxed_slice(),
            methods: methods.into_boxed_slice(),
        })
    }

    fn parse_enum_def(&mut self, vis: Visibility) -> Option<EnumDef> {
        let name = self.expect_ident()?;
        self.expect(TokenKind::KwWith)?;
        self.expect(TokenKind::LBracket)?;
        let mut variants: Vec<String> = Vec::new();
        loop {
            self.skip_layout();
            if self.at(TokenKind::RBracket) {
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
            if self.at(TokenKind::Pipe) {
                self.bump();
                continue;
            }
            break;
        }
        self.expect(TokenKind::RBracket)?;
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
            let internal = method_name(&ty.name, &method);
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
            let mut fvis = Visibility::Inner;
            if self.at(TokenKind::KwPub) {
                self.bump();
                fvis = Visibility::Public;
                self.skip_trivia();
            }

            let is_static = self.at(TokenKind::KwStatic);
            if is_static {
                self.bump();
                self.skip_trivia();
            }

            let mut f = self.parse_func_def(fvis)?;
            if is_static {
                f.name = static_name(&target, &f.name);
            } else if !f.name.starts_with(METHOD_PREFIX) {
                let original = f.name.clone();
                f.name = method_name(&target, &original);
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

    fn parse_match_stmt(&mut self) -> Option<MatchStmt> {
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
        Some(MatchStmt {
            expr,
            arms: arms.into_boxed_slice(),
            else_branch,
        })
    }

    fn parse_when_bind_stmt(&mut self) -> Option<Stmt> {
        self.expect(TokenKind::KwWhen)?;
        self.skip_layout();

        // Check if user is trying to use `when expr { ... }` pattern matching syntax
        // which should be `match expr { ... }` instead
        if self.at(TokenKind::Ident) {
            let next = self.peek_kind_n(1);
            if next == Some(TokenKind::LBrace) || next == Some(TokenKind::Newline) {
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::Raw(
                        "Use 'match' for pattern matching. 'when' is for optional binding: when x = expr { ... } else { ... }".to_string()
                    ),
                    Some(self.cur_span()),
                ));
                return None;
            }
        }

        let mut bindings: Vec<(String, Expr)> = Vec::with_capacity(3);
        loop {
            let name = self.expect_ident()?;
            self.skip_layout();
            if !self.at(TokenKind::Eq) {
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::Raw(
                        "Expected '=' after identifier in 'when' binding. Use 'match' for pattern matching.".to_string()
                    ),
                    Some(self.cur_span()),
                ));
                return None;
            }
            self.bump(); // consume '='
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
            // 允许省略 else，默认为空块
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
            let match_stmt = Stmt::Match(Box::new(MatchStmt {
                expr,
                arms,
                else_branch: Some(else_body.clone()),
            }));
            outer_stmt = Some(match_stmt.clone());
            inner_body = vec![match_stmt].into_boxed_slice();
        }
        outer_stmt
    }

    fn is_does_block_start(&self) -> bool {
        self.peek_kind() == TokenKind::Ident && self.peek_kind_n(1) == Some(TokenKind::KwDoes)
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
        
        // 检查是否是字典键值对循环: for (key, value) in dict
        let (var, key_value_vars) = if self.at(TokenKind::LParen) {
            self.bump();
            let key_var = self.expect_ident()?;
            self.expect(TokenKind::Comma)?;
            let value_var = self.expect_ident()?;
            self.expect(TokenKind::RParen)?;
            let tmp_var = format!("__tmp_foreach_{}", self.tmp_counter);
            self.tmp_counter += 1;
            (tmp_var, Some((key_var, value_var)))
        } else {
            // 普通循环: for var in iter
            (self.expect_ident()?, None)
        };
        
        self.expect(TokenKind::KwIn)?;
        let iter = self.parse_expr_no_struct_init(0)?;
        let body = if self.at(TokenKind::Colon) {
            self.bump();
            self.parse_block_or_inline_stmt_after_colon(false)?
        } else {
            self.parse_block()?
        };
        
        // 如果是字典键值对循环，修改body来包含键值对的解构
        let final_body = if let Some((key_var, value_var)) = key_value_vars {
            let mut new_body = Vec::new();
            
            // 创建键值对解构语句
            let key_expr = Expr::Member(Box::new(MemberExpr {
                object: Box::new(Expr::Ident(var.clone(), std::cell::Cell::new(None))),
                field: "0".to_string(),
                ic_slot: std::cell::Cell::new(None),
            }));
            let value_expr = Expr::Member(Box::new(MemberExpr {
                object: Box::new(Expr::Ident(var.clone(), std::cell::Cell::new(None))),
                field: "1".to_string(),
                ic_slot: std::cell::Cell::new(None),
            }));
            
            // 添加键的赋值语句
            new_body.push(Stmt::Assign(Box::new(AssignStmt {
                vis: Visibility::Public,
                target: Expr::Ident(key_var.clone(), std::cell::Cell::new(None)),
                op: AssignOp::Set,
                value: key_expr,
                ty: None,
                slot: None,
                decl: Some(DeclKind::Let),
            })));
            
            // 添加值的赋值语句
            new_body.push(Stmt::Assign(Box::new(AssignStmt {
                vis: Visibility::Public,
                target: Expr::Ident(value_var.clone(), std::cell::Cell::new(None)),
                op: AssignOp::Set,
                value: value_expr,
                ty: None,
                slot: None,
                decl: Some(DeclKind::Let),
            })));
            
            // 添加原有的body语句
            new_body.extend(body.iter().cloned());
            
            new_body.into_boxed_slice()
        } else {
            body
        };
        
        Some(ForEachStmt { iter, var, body: final_body })
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
}
