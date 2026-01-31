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
fn indentation_is_just_whitespace() {
    let src = "if true:\n   println(\"bad\");\n";
    let normalized = normalize_source(src);
    let result = Lexer::new(&normalized.text).lex();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}
