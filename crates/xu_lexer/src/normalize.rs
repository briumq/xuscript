use xu_syntax::{Diagnostic, Span};

pub struct NormalizedSource {
    pub text: String,
    pub diagnostics: Vec<Diagnostic>,
}

pub fn normalize_source(input: &str) -> NormalizedSource {
    let mut diagnostics = Vec::new();

    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push('\n');
            }
            '\t' => {
                let start = out.len() as u32;
                diagnostics.push(Diagnostic::error(
                    "Tab is not allowed; use ASCII spaces",
                    Some(Span::new(start, start.saturating_add(1))),
                ));
                out.push(' ');
            }
            '\u{3000}' => {
                let start = out.len() as u32;
                diagnostics.push(Diagnostic::error(
                    "Full-width space is not allowed; use ASCII spaces",
                    Some(Span::new(start, start.saturating_add(1))),
                ));
                out.push(' ');
            }
            _ => out.push(c),
        }
    }

    NormalizedSource {
        text: out,
        diagnostics,
    }
}
