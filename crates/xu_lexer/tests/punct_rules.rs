use xu_lexer::Lexer;

#[test]
fn non_ascii_semicolon_is_error() {
    let src = "x = 1؛";
    let lex = Lexer::new(src).lex();
    assert!(
        lex.diagnostics
            .iter()
            .any(|d| d.message.contains("Unexpected character")),
        "diagnostics={:?}",
        lex.diagnostics
    );
}

#[test]
fn semicolon_stmt_end_english() {
    let src = "x = 1;";
    let lex = Lexer::new(src).lex();
    assert!(lex.diagnostics.is_empty());
    assert!(
        lex.tokens
            .iter()
            .any(|t| matches!(t.kind, xu_syntax::TokenKind::StmtEnd))
    );
}

#[test]
fn non_ascii_dot_is_error() {
    let src = "x = 1۔";
    let lex = Lexer::new(src).lex();
    assert!(
        lex.diagnostics
            .iter()
            .any(|d| d.message.contains("Unexpected character")),
        "diagnostics={:?}",
        lex.diagnostics
    );
}

#[test]
fn ascii_dot_is_access() {
    let src = "x = 1.";
    let lex = Lexer::new(src).lex();
    assert!(lex.diagnostics.is_empty());
    assert!(
        lex.tokens
            .iter()
            .any(|t| matches!(t.kind, xu_syntax::TokenKind::Dot))
    );
}
