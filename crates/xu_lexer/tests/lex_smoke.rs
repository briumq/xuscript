use xu_lexer::{Lexer, normalize_source};

#[test]
fn lex_smoke_chinese_punct() {
    let src = "let age: int = 25;\nif age >= 18:\n  println(\"adult\");\n";
    let normalized = normalize_source(src);
    assert!(normalized.diagnostics.is_empty());
    let result = Lexer::new(&normalized.text).lex();
    assert!(result.tokens.len() > 3);
}

#[test]
fn lex_emits_indent_dedent() {
    let src = "if true:\n  println(\"ok\");\nprintln(\"done\");\n";
    let normalized = normalize_source(src);
    let result = Lexer::new(&normalized.text).lex();
    let kinds: Vec<_> = result.tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&xu_syntax::TokenKind::Indent));
    assert!(kinds.contains(&xu_syntax::TokenKind::Dedent));
}

#[test]
fn odd_indent_is_error() {
    let src = "if true:\n   println(\"bad\");\n";
    let normalized = normalize_source(src);
    let result = Lexer::new(&normalized.text).lex();
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Indentation must be multiples of 2"))
    );
}

#[test]
fn indent_four_spaces_is_ok() {
    let src = "if true:\n    println(\"ok\");\nprintln(\"done\");\n";
    let normalized = normalize_source(src);
    let result = Lexer::new(&normalized.text).lex();
    assert!(
        result.diagnostics.is_empty(),
        "diagnostics={:?}",
        result.diagnostics
    );
    let kinds: Vec<_> = result.tokens.iter().map(|t| t.kind).collect();
    assert!(kinds.contains(&xu_syntax::TokenKind::Indent));
    assert!(kinds.contains(&xu_syntax::TokenKind::Dedent));
}

#[test]
fn inconsistent_dedent_is_error() {
    let src = "if true:\n    println(\"a\");\n  println(\"b\");\n";
    let normalized = normalize_source(src);
    let result = Lexer::new(&normalized.text).lex();
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.message.contains("Inconsistent dedent")),
        "diagnostics={:?}",
        result.diagnostics
    );
}
