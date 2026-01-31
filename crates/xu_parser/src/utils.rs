use xu_syntax::{Span, Token, TokenKind};

pub fn next_significant(tokens: &[Token], mut i: usize) -> (Option<Span>, usize) {
    while let Some(t) = tokens.get(i) {
        i += 1;
        if matches!(t.kind, TokenKind::Newline) {
            continue;
        }
        return (Some(t.span), i);
    }
    (None, i)
}

pub fn recover_until(tokens: &[Token], mut i: usize, terms: &[TokenKind]) -> usize {
    while let Some(t) = tokens.get(i) {
        if terms.contains(&t.kind) {
            break;
        }
        i += 1;
    }
    i
}
