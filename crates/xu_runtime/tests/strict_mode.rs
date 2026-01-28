use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;
use xu_runtime::runtime::RuntimeConfig;

fn parse_module(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    assert!(normalized.diagnostics.is_empty());
    let lex = Lexer::new(&normalized.text).lex();
    assert!(lex.diagnostics.is_empty());
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let errors: Vec<_> = parse
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    parse.module
}

#[test]
fn top_level_define_errors_in_strict_mode() {
    let module = parse_module("x = 1;\n");
    let mut rt = Runtime::with_config(RuntimeConfig {
        strict_vars: true,
        ..Default::default()
    });
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let result = rt.exec_module(&module);
    assert!(
        result.is_err(),
        "expected strict mode error, got {:?}",
        result
    );
}

#[test]
fn top_level_define_allowed_in_non_strict_mode() {
    let module = parse_module("x = 1;\nprintln(x);\n");
    let mut rt = Runtime::with_config(RuntimeConfig {
        strict_vars: false,
        ..Default::default()
    });
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let result = rt.exec_module(&module);
    assert!(
        result.is_ok(),
        "expected non-strict success, got {:?}",
        result
    );
}
