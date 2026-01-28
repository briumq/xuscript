use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;

fn parse_source(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    assert!(
        normalized.diagnostics.is_empty(),
        "{:?}",
        normalized.diagnostics
    );
    let lex = Lexer::new(&normalized.text).lex();
    assert!(lex.diagnostics.is_empty(), "{:?}", lex.diagnostics);
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let errors: Vec<_> = parse
        .diagnostics
        .into_iter()
        .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    parse.module
}

#[test]
fn text_indexing_and_slicing_work() {
    let src = r#"
s = "你好世界";
println(s[0]);
println(s[1]);
println(s[1..2]);

t = "中a文";
println(t[0]);
println(t[1]);
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    let res = rt.exec_module(&module).unwrap();
    let out = res.output;

    assert!(out.contains("你"), "{out}");
    assert!(out.contains("好"), "{out}");
    assert!(out.contains("好世"), "{out}");
    assert!(out.contains("中"), "{out}");
    assert!(out.contains("a"), "{out}");
}

#[test]
fn text_indexing_out_of_range_errors() {
    let src = r#"
s = "你好";
println(s[2]);
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    let err = rt.exec_module(&module).unwrap_err();
    assert!(err.contains("Index out of range"), "{err}");
}

#[test]
fn text_slicing_invalid_range_errors() {
    let src = r#"
s = "你好世界";
println(s[2..1]);
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    let err = rt.exec_module(&module).unwrap_err();
    assert!(err.contains("Index out of range"), "{err}");
}
