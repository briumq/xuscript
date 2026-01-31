use xu_lexer::{Lexer, normalize_source};
use xu_syntax::{
    Diagnostic, DiagnosticKind, InterpolationParser, InterpolationPiece, TokenKind, unescape,
};

use super::{Expr, Parser};
use crate::parser::fast_interpolation_expr;

impl<'a, 'b> Parser<'a, 'b> {
    pub(super) fn parse_interpolated_string(&mut self, raw: &str) -> Option<Expr> {
        if raw.starts_with("r\"") && raw.ends_with('"') && raw.len() >= 3 {
            let inner = &raw[2..raw.len() - 1];
            return Some(Expr::Str(inner.to_string()));
        }
        if raw.len() < 2 {
            return Some(Expr::Str(String::new()));
        }
        let inner = if raw.starts_with("\"\"\"") && raw.ends_with("\"\"\"") && raw.len() >= 6 {
            &raw[3..raw.len() - 3]
        } else {
            &raw[1..raw.len() - 1]
        };

        let mut parts = Vec::with_capacity(4);

        InterpolationParser::new(inner).parse(|piece| match piece {
            InterpolationPiece::Str(s) => parts.push(Expr::Str(s)),
            InterpolationPiece::Expr(expr_str) => {
                parts.push(self.parse_interpolation_expr(expr_str))
            }
        });

        if parts.is_empty() {
            return Some(Expr::Str(String::new()));
        }

        if parts.len() == 1 && matches!(parts[0], Expr::Str(_)) {
            return Some(parts.pop().unwrap());
        }

        Some(Expr::InterpolatedString(parts.into_boxed_slice()))
    }

    fn parse_interpolation_expr(&mut self, expr_str: &str) -> Expr {
        let key = expr_str.trim();
        if let Some(e) = self.interp_cache.get(key) {
            return e.clone();
        }
        let key_unescaped = unescape(key);

        let expr = if let Some(e) = fast_interpolation_expr(key) {
            e
        } else {
            let mut expr_str_with_term = String::with_capacity(key_unescaped.len() + 1);
            expr_str_with_term.push_str(key_unescaped.as_str());
            expr_str_with_term.push(';');
            let normalized = normalize_source(&expr_str_with_term);
            let lex = Lexer::new(&normalized.text).lex();
            let mut p = Parser::new(&normalized.text, &lex.tokens, self.bump);
            p.skip_trivia();
            let expr = p.parse_expr(0).unwrap_or(Expr::Tuple(Box::new([])));
            p.skip_trivia();
            if !p.at(TokenKind::StmtEnd) && !p.at(TokenKind::Eof) {
                p.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::TrailingInterpolationTokens,
                    Some(p.cur_span()),
                ));
            }

            let has_error = normalized
                .diagnostics
                .iter()
                .chain(lex.diagnostics.iter())
                .chain(p.diagnostics.iter())
                .any(|d| matches!(d.severity, xu_syntax::Severity::Error));

            if has_error { Expr::Tuple(Box::new([])) } else { expr }
        };
        self.interp_cache.insert(key.to_string(), expr.clone());
        expr
    }
}
