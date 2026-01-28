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
fn strict_vars_rejects_assignment_to_new_name() {
    let src = r#"
x = 1;
println(x);
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_strict_vars(true);
    let err = rt.exec_module(&module).unwrap_err();
    assert!(err.contains("Undefined identifier: x"), "{err}");
}

#[test]
fn strict_vars_allows_typed_declare_then_assign() {
    let src = r#"
var x: int = 1;
x += 2;
println(x);
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_strict_vars(true);
    let res = rt.exec_module(&module).unwrap();
    assert!(res.output.contains("3"), "{}", res.output);
}
