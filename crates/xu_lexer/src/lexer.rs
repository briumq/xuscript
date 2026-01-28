//! Lexer implementation.
//!
//! Scans source text into tokens (keywords/idents/literals/delimiters), handles layout
//! tokens (indentation/newlines), and collects diagnostics.
//!
//! Design: single linear pass, delimiter stack + indentation stack, minimal allocations.
//!
//! Related: `LexResult`, `xu_syntax` (tokens/diagnostics).
use crate::keywords::KEYWORDS_EN;
use xu_syntax::{
    Diagnostic, DiagnosticKind, Span, Token, TokenKind, is_ident_continue, is_ident_start,
};

/// Lexing result.
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Xu lexer.
pub struct Lexer<'a> {
    input: &'a str,
    bytes: &'a [u8],
    i: usize,
    diagnostics: Vec<Diagnostic>,
    tokens: Vec<Token>,
    at_line_start: bool,
    indent_stack: Vec<u32>,
    delim_depth: u32,
    delim_stack: Vec<char>,
    last_sig_kind: Option<TokenKind>,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer.
    pub fn new(input: &'a str) -> Self {
        Self {
            input,
            bytes: input.as_bytes(),
            i: 0,
            diagnostics: Vec::new(),
            tokens: Vec::new(),
            at_line_start: true,
            indent_stack: vec![0],
            delim_depth: 0,
            delim_stack: Vec::new(),
            last_sig_kind: None,
        }
    }

    /// Run the lexer and return tokens + diagnostics.
    pub fn lex(mut self) -> LexResult {
        let approx = self.bytes.len().saturating_div(4).max(32);
        self.tokens.reserve(approx);
        self.diagnostics.reserve(32);
        while self.i < self.bytes.len() {
            let start = self.i;
            if self.at_line_start {
                if self.delim_depth == 0 {
                    self.handle_indent();
                }
                self.at_line_start = false;
                if self.i >= self.bytes.len() {
                    break;
                }
            }
            let c = self.peek_char();

            match c {
                Some('\r') => {
                    self.i += 1;
                    if self.peek_char() == Some('\n') {
                        self.i += 1;
                    }
                    if self.should_emit_newline() {
                        self.push(TokenKind::Newline, start, self.i);
                        self.at_line_start = true;
                    } else {
                        self.at_line_start = false;
                    }
                }
                Some('\n') => {
                    self.i += 1;
                    if self.should_emit_newline() {
                        self.push(TokenKind::Newline, start, self.i);
                        self.at_line_start = true;
                    } else {
                        self.at_line_start = false;
                    }
                }
                Some('\t') => {
                    self.i += 1;
                    self.diagnostics.push(Diagnostic::error_kind(
                        DiagnosticKind::TabNotAllowed,
                        Some(Span::new(start as u32, self.i as u32)),
                    ));
                }
                Some('\u{3000}') => {
                    self.i += '\u{3000}'.len_utf8();
                    self.diagnostics.push(Diagnostic::error_kind(
                        DiagnosticKind::FullWidthSpaceNotAllowed,
                        Some(Span::new(start as u32, self.i as u32)),
                    ));
                }
                Some(' ') => {
                    self.i += 1;
                }
                Some('/') => {
                    if self.peek_str("//") {
                        self.i += 2;
                        while let Some(ch) = self.peek_char() {
                            if ch == '\n' {
                                break;
                            }
                            self.i += ch.len_utf8();
                        }
                    } else if self.peek_str("/*") {
                        self.i += 2;
                        let mut terminated = false;
                        while self.i < self.bytes.len() {
                            if self.peek_str("*/") {
                                self.i += 2;
                                terminated = true;
                                break;
                            }
                            let ch = self.peek_char().unwrap();
                            if ch == '\n' {
                                let nl_start = self.i;
                                self.i += 1;
                                self.push(TokenKind::Newline, nl_start, self.i);
                                self.at_line_start = true;
                                continue;
                            }
                            self.i += ch.len_utf8();
                        }
                        if !terminated {
                            self.diagnostics.push(Diagnostic::error_kind(
                                DiagnosticKind::UnterminatedBlockComment,
                                Some(Span::new(start as u32, self.i as u32)),
                            ));
                        }
                    } else {
                        self.i += 1;
                        if self.peek_char() == Some('=') {
                            self.i += 1;
                            self.push(TokenKind::SlashEq, start, self.i);
                        } else {
                            self.push(TokenKind::Slash, start, self.i);
                        }
                    }
                }
                Some('(') => {
                    self.i += 1;
                    self.delim_depth = self.delim_depth.saturating_add(1);
                    self.delim_stack.push('(');
                    self.push(TokenKind::LParen, start, self.i);
                }
                Some(')') => {
                    self.i += 1;
                    if let Some(top) = self.delim_stack.pop() {
                        if top != '(' {
                            self.diagnostics.push(Diagnostic::error_kind(
                                DiagnosticKind::UnmatchedDelimiter(')'),
                                Some(Span::new(start as u32, self.i as u32)),
                            ));
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error_kind(
                            DiagnosticKind::UnmatchedDelimiter(')'),
                            Some(Span::new(start as u32, self.i as u32)),
                        ));
                    }
                    self.delim_depth = self.delim_depth.saturating_sub(1);
                    self.push(TokenKind::RParen, start, self.i);
                }
                Some('[') => {
                    self.i += 1;
                    self.delim_depth = self.delim_depth.saturating_add(1);
                    self.delim_stack.push('[');
                    self.push(TokenKind::LBracket, start, self.i);
                }
                Some(']') => {
                    self.i += 1;
                    if let Some(top) = self.delim_stack.pop() {
                        if top != '[' {
                            self.diagnostics.push(Diagnostic::error_kind(
                                DiagnosticKind::UnmatchedDelimiter(']'),
                                Some(Span::new(start as u32, self.i as u32)),
                            ));
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error_kind(
                            DiagnosticKind::UnmatchedDelimiter(']'),
                            Some(Span::new(start as u32, self.i as u32)),
                        ));
                    }
                    self.delim_depth = self.delim_depth.saturating_sub(1);
                    self.push(TokenKind::RBracket, start, self.i);
                }
                Some('{') => {
                    self.i += 1;
                    self.delim_depth = self.delim_depth.saturating_add(1);
                    self.delim_stack.push('{');
                    self.push(TokenKind::LBrace, start, self.i);
                }
                Some('}') => {
                    self.i += 1;
                    if let Some(top) = self.delim_stack.pop() {
                        if top != '{' {
                            self.diagnostics.push(Diagnostic::error_kind(
                                DiagnosticKind::UnmatchedDelimiter('}'),
                                Some(Span::new(start as u32, self.i as u32)),
                            ));
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error_kind(
                            DiagnosticKind::UnmatchedDelimiter('}'),
                            Some(Span::new(start as u32, self.i as u32)),
                        ));
                    }
                    self.delim_depth = self.delim_depth.saturating_sub(1);
                    self.push(TokenKind::RBrace, start, self.i);
                }
                Some('"') => {
                    self.lex_string();
                }
                Some('.') => {
                    if self.peek_str("..") {
                        self.i += 2;
                        self.push(TokenKind::DotDot, start, self.i);
                    } else {
                        self.i += 1;
                        self.push(TokenKind::Dot, start, self.i);
                    }
                }
                Some(',' | ':' | ';') => {
                    let ch = c.unwrap();
                    self.i += ch.len_utf8();
                    match ch {
                        ';' => self.push(TokenKind::StmtEnd, start, self.i),
                        ',' => self.push(TokenKind::Comma, start, self.i),
                        ':' => self.push(TokenKind::Colon, start, self.i),
                        _ => {}
                    };
                }
                Some('+') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::PlusEq, start, self.i);
                    } else {
                        self.push(TokenKind::Plus, start, self.i);
                    }
                }
                Some('-') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::MinusEq, start, self.i);
                    } else {
                        self.push(TokenKind::Minus, start, self.i);
                    }
                }
                Some('*') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::StarEq, start, self.i);
                    } else {
                        self.push(TokenKind::Star, start, self.i);
                    }
                }
                Some('%') => {
                    self.i += 1;
                    self.push(TokenKind::Percent, start, self.i);
                }
                Some('#') => {
                    self.i += 1;
                    self.push(TokenKind::Hash, start, self.i);
                }
                Some('|') => {
                    self.i += 1;
                    self.push(TokenKind::Pipe, start, self.i);
                }
                Some('>') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::Ge, start, self.i);
                    } else {
                        self.push(TokenKind::Gt, start, self.i);
                    }
                }
                Some('<') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::Le, start, self.i);
                    } else {
                        self.push(TokenKind::Lt, start, self.i);
                    }
                }
                Some('=') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::EqEq, start, self.i);
                    } else {
                        self.push(TokenKind::Eq, start, self.i);
                    }
                }
                Some('!') => {
                    self.i += 1;
                    if self.peek_char() == Some('=') {
                        self.i += 1;
                        self.push(TokenKind::Ne, start, self.i);
                    } else {
                        self.push(TokenKind::Bang, start, self.i);
                    }
                }
                Some('?') => {
                    self.i += 1;
                    self.push(TokenKind::Question, start, self.i);
                }
                Some('r') => {
                    if self.peek_str("r\"") {
                        self.lex_raw_string();
                    } else {
                        self.lex_ident_or_keyword();
                    }
                }
                Some(ch) if ch.is_ascii_digit() => {
                    self.lex_number();
                }
                Some(ch) if is_ident_start(ch) => {
                    self.lex_ident_or_keyword();
                }
                Some(other) => {
                    self.i += other.len_utf8();
                    self.diagnostics.push(Diagnostic::error_kind(
                        DiagnosticKind::UnexpectedChar(other),
                        Some(Span::new(start as u32, self.i as u32)),
                    ));
                }
                None => break,
            }
        }

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            self.tokens.push(Token {
                kind: TokenKind::Dedent,
                span: Span::new(self.i as u32, self.i as u32),
            });
        }

        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.i as u32, self.i as u32),
        });
        if !self.delim_stack.is_empty() {
            for ch in self.delim_stack.iter().rev() {
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::UnclosedDelimiter(*ch),
                    Some(Span::new(self.i as u32, self.i as u32)),
                ));
            }
        }

        LexResult {
            tokens: self.tokens,
            diagnostics: self.diagnostics,
        }
    }

    ///
    ///
    fn handle_indent(&mut self) {
        let line_start = self.i;
        let mut spaces = 0u32;
        while self.peek_char() == Some(' ') {
            self.i += 1;
            spaces += 1;
        }

        if spaces % 2 != 0 {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::InvalidIndentation,
                Some(Span::new(line_start as u32, self.i as u32)),
            ));
        }

        if matches!(self.peek_char(), Some('\n') | Some('\r')) || self.i >= self.bytes.len() {
            self.i = line_start;
            return;
        }

        if self.peek_str("//") {
            self.i = line_start;
            return;
        }

        if self.peek_str("/*") {
            self.i = line_start;
            return;
        }

        let new_indent = spaces;
        let cur_indent = *self.indent_stack.last().unwrap_or(&0);
        if new_indent == cur_indent {
            return;
        }

        if new_indent > cur_indent {
            self.indent_stack.push(new_indent);
            self.tokens.push(Token {
                kind: TokenKind::Indent,
                span: Span::new(line_start as u32, self.i as u32),
            });
            return;
        }

        if !self.indent_stack.contains(&new_indent) {
            self.diagnostics.push(Diagnostic::error_kind(
                DiagnosticKind::InconsistentDedent,
                Some(Span::new(line_start as u32, self.i as u32)),
            ));
        }

        while self.indent_stack.len() > 1 {
            let top = *self.indent_stack.last().unwrap();
            if top <= new_indent {
                break;
            }
            self.indent_stack.pop();
            self.tokens.push(Token {
                kind: TokenKind::Dedent,
                span: Span::new(line_start as u32, self.i as u32),
            });
        }
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            span: Span::new(start as u32, end as u32),
        });
        if !matches!(
            kind,
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
        ) {
            self.last_sig_kind = Some(kind);
        }
    }

    fn should_emit_newline(&self) -> bool {
        if self.delim_depth > 0 {
            return false;
        }
        if self.last_sig_kind.is_some_and(|k| {
            matches!(
                k,
                TokenKind::Dot
                    | TokenKind::Comma
                    | TokenKind::Plus
                    | TokenKind::Minus
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::Eq
                    | TokenKind::LParen
                    | TokenKind::LBracket
                    | TokenKind::LBrace
            )
        }) {
            return false;
        }

        let Some(next) = self.peek_next_line_significant_char() else {
            return true;
        };
        if matches!(next, '.' | ')' | ']' | '}') {
            return false;
        }
        if matches!(next, '+' | '-' | '*' | '/' | '%' | '=' | '>' | '<' | '!') {
            return false;
        }
        true
    }

    fn peek_next_line_significant_char(&self) -> Option<char> {
        let mut j = self.i;
        while j < self.input.len() {
            let ch = self.input[j..].chars().next()?;
            if ch == '\n' || ch == '\r' {
                return None;
            }
            if ch == ' ' || ch == '\t' {
                j += ch.len_utf8();
                continue;
            }
            if ch == '/' {
                if self.input[j..].starts_with("//") {
                    return None;
                }
                if self.input[j..].starts_with("/*") {
                    if let Some(end) = self.input[j + 2..].find("*/") {
                        j = j + 2 + end + 2;
                        continue;
                    }
                    return None;
                }
            }
            return Some(ch);
        }
        None
    }

    fn lex_string(&mut self) {
        if self.peek_str("\"\"\"") {
            self.lex_triple_string();
            return;
        }
        let start = self.i;
        self.i += 1;
        while self.i < self.bytes.len() {
            let ch = self.peek_char().unwrap();
            if ch == '\n' || ch == '\r' {
                break;
            }
            if ch == '"' {
                self.i += 1;
                self.push(TokenKind::Str, start, self.i);
                return;
            }
            if ch == '\\' {
                self.i += 1;
                if self.i >= self.bytes.len() {
                    break;
                }
                let esc = self.peek_char().unwrap();
                self.i += esc.len_utf8();
                continue;
            }
            self.i += ch.len_utf8();
        }
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::UnterminatedString,
            Some(Span::new(start as u32, self.i as u32)),
        ));
    }

    ///
    ///
    fn lex_number(&mut self) {
        let start = self.i;
        if self.peek_str("0x") || self.peek_str("0X") {
            self.i += 2;
            let mut digits = 0usize;
            while self.i < self.bytes.len() {
                let ch = self.peek_char().unwrap();
                if ch == '_' {
                    self.i += 1;
                    continue;
                }
                if ch.is_ascii_hexdigit() {
                    self.i += 1;
                    digits += 1;
                    continue;
                }
                break;
            }
            if digits == 0 {
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::UnexpectedChar('x'),
                    Some(Span::new(start as u32, self.i as u32)),
                ));
            }
            self.push(TokenKind::Int, start, self.i);
            return;
        }
        if self.peek_str("0b") || self.peek_str("0B") {
            self.i += 2;
            let mut digits = 0usize;
            while self.i < self.bytes.len() {
                let ch = self.peek_char().unwrap();
                if ch == '_' {
                    self.i += 1;
                    continue;
                }
                if ch == '0' || ch == '1' {
                    self.i += 1;
                    digits += 1;
                    continue;
                }
                break;
            }
            if digits == 0 {
                self.diagnostics.push(Diagnostic::error_kind(
                    DiagnosticKind::UnexpectedChar('b'),
                    Some(Span::new(start as u32, self.i as u32)),
                ));
            }
            self.push(TokenKind::Int, start, self.i);
            return;
        }

        while self.i < self.bytes.len() {
            let ch = self.peek_char().unwrap();
            if ch.is_ascii_digit() || ch == '_' {
                self.i += 1;
            } else {
                break;
            }
        }

        let mut kind = TokenKind::Int;
        if self.peek_char() == Some('.') && !self.peek_str("..") {
            let dot = self.i;
            self.i += 1;
            let mut digits = 0usize;
            while self.i < self.bytes.len() {
                let ch = self.peek_char().unwrap();
                if ch.is_ascii_digit() {
                    self.i += 1;
                    digits += 1;
                    continue;
                }
                if ch == '_' {
                    self.i += 1;
                    continue;
                }
                break;
            }
            if digits > 0 {
                kind = TokenKind::Float;
            } else {
                self.i = dot;
            }
        }

        if matches!(self.peek_char(), Some('e' | 'E')) {
            let exp_start = self.i;
            self.i += 1;
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.i += 1;
            }
            let mut digits = 0usize;
            while self.i < self.bytes.len() {
                let ch = self.peek_char().unwrap();
                if ch.is_ascii_digit() {
                    self.i += 1;
                    digits += 1;
                    continue;
                }
                if ch == '_' {
                    self.i += 1;
                    continue;
                }
                break;
            }
            if digits > 0 {
                kind = TokenKind::Float;
            } else {
                self.i = exp_start;
            }
        }

        self.push(kind, start, self.i);
    }

    fn lex_raw_string(&mut self) {
        let start = self.i;
        self.i += 2;
        while self.i < self.bytes.len() {
            let ch = self.peek_char().unwrap();
            if ch == '\n' || ch == '\r' {
                break;
            }
            if ch == '"' {
                self.i += 1;
                self.push(TokenKind::Str, start, self.i);
                return;
            }
            self.i += ch.len_utf8();
        }
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::UnterminatedString,
            Some(Span::new(start as u32, self.i as u32)),
        ));
    }

    fn lex_triple_string(&mut self) {
        let start = self.i;
        self.i += 3;
        while self.i < self.bytes.len() {
            if self.peek_str("\"\"\"") {
                self.i += 3;
                self.push(TokenKind::Str, start, self.i);
                return;
            }
            let ch = self.peek_char().unwrap();
            self.i += ch.len_utf8();
        }
        self.diagnostics.push(Diagnostic::error_kind(
            DiagnosticKind::UnterminatedString,
            Some(Span::new(start as u32, self.i as u32)),
        ));
    }

    ///
    ///
    fn lex_ident_or_keyword(&mut self) {
        let start = self.i;
        self.i += self.peek_char().unwrap().len_utf8();
        while self.i < self.bytes.len() {
            let ch = self.peek_char().unwrap();
            if is_ident_continue(ch) {
                self.i += ch.len_utf8();
            } else {
                break;
            }
        }

        let s = &self.input[start..self.i];
        let kind = KEYWORDS_EN.get(s).cloned().unwrap_or(TokenKind::Ident);

        self.push(kind, start, self.i);
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.i..].chars().next()
    }

    fn peek_str(&self, s: &str) -> bool {
        self.input[self.i..].starts_with(s)
    }
}
