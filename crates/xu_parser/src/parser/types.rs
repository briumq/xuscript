use super::Parser;
use crate::TypeRef;
use xu_syntax::{Diagnostic, DiagnosticKind, TokenKind};

impl<'a, 'b> Parser<'a, 'b> {
    pub(super) fn parse_type_ref(&mut self) -> Option<TypeRef> {
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

    pub(super) fn is_type_def_start(&self) -> bool {
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
    pub(super) fn braced_type_def_is_struct(&self) -> bool {
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
}
