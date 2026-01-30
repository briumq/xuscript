use xu_syntax::{Diagnostic, DiagnosticKind, SourceFile, TokenKind, Span};
use xu_parser::Pattern;

pub struct Finder<'a> {
    source: &'a SourceFile,
    tokens: &'a [xu_syntax::Token],
    i: usize,
}

impl<'a> Finder<'a> {
    pub fn new(source: &'a SourceFile, tokens: &'a [xu_syntax::Token]) -> Self {
        Self {
            source,
            tokens,
            i: 0,
        }
    }

    pub fn next_significant_span(&mut self) -> Option<Span> {
        while let Some(t) = self.tokens.get(self.i) {
            self.i += 1;
            if matches!(
                t.kind,
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent
            ) {
                continue;
            }
            return Some(t.span);
        }
        None
    }

    pub fn find_name(&mut self, name: &str) -> Option<Span> {
        for idx in self.i..self.tokens.len() {
            let t = &self.tokens[idx];
            if t.kind == TokenKind::Ident && self.source.text.slice(t.span) == name {
                self.i = idx + 1;
                return Some(t.span);
            }
        }
        None
    }

    pub fn find_name_or_next(&mut self, name: &str) -> Option<Span> {
        self.find_name(name)
            .or_else(|| self.next_significant_span())
    }

    pub fn find_kw(&mut self, kind: TokenKind) -> Option<Span> {
        for idx in self.i..self.tokens.len() {
            let t = &self.tokens[idx];
            if t.kind == kind {
                self.i = idx + 1;
                return Some(t.span);
            }
        }
        None
    }

    pub fn find_kw_or_next(&mut self, kind: TokenKind) -> Option<Span> {
        self.find_kw(kind).or_else(|| self.next_significant_span())
    }
}

pub fn report_shadowing(name: &str, finder: &mut Finder<'_>, out: &mut Vec<Diagnostic>) {
    out.push(Diagnostic::error_kind(
        DiagnosticKind::Raw(format!("shadowing: {name}")),
        finder.find_name_or_next(name),
    ));
}

pub fn collect_pattern_binds(pat: &Pattern, out: &mut Vec<String>) {
    match pat {
        Pattern::Wildcard
        | Pattern::Int(_)
        | Pattern::Float(_)
        | Pattern::Str(_)
        | Pattern::Bool(_) => {}
        Pattern::Bind(name) => out.push(name.clone()),
        Pattern::Tuple(items) => {
            for p in items.iter() {
                collect_pattern_binds(p, out);
            }
        }
        Pattern::EnumVariant { args, .. } => {
            for p in args.iter() {
                collect_pattern_binds(p, out);
            }
        }
    }
}
