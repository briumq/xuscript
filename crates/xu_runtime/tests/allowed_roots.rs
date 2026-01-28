use std::fs;
use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;

fn parse_source(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    assert!(normalized.diagnostics.is_empty());
    let lex = Lexer::new(&normalized.text).lex();
    assert!(lex.diagnostics.is_empty());
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
fn import_denied_outside_allowed_roots() {
    let dir = std::env::temp_dir().join("xu_runtime_allowed_root_tests");
    let _ = fs::create_dir_all(&dir);
    let dep = dir.join("dep.xu");
    fs::write(&dep, "value = 1;").unwrap();

    let main_src = format!(r#"use "{path}";"#, path = dep.to_string_lossy());
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.clear_allowed_roots();
    let another = std::env::temp_dir().join("another_root");
    let _ = fs::create_dir_all(&another);
    rt.add_allowed_root(another.to_string_lossy().as_ref())
        .unwrap();
    let err = rt.exec_module(&module).unwrap_err();
    assert!(err.contains("Path is not within allowed roots"), "{err}");
}
