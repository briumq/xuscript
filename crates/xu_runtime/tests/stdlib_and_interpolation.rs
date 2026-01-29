use std::fs;
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
fn interpolation_is_precompiled_and_errors_even_if_unreached() {
    let src = r#"
if false {
  println("unreached");
} else {
  println("bad={1 + }");
}
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    let err = rt.exec_module(&module).unwrap_err();
    assert!(err.contains("Interpolation"), "{err}");
}

#[test]
fn stdlib_and_string_methods_work() {
    let src = r#"
println("ts={time_unix()}");
println("abs={abs(-3)}");
println("max={max(3, 5)}");
println("min={min(3, 5)}");

println("a,b,c" split(",") length());
println("a-b" replace("-", "_"));
println("  Ab " trim() to_upper());
println("AB" to_lower());
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    let res = rt.exec_module(&module).unwrap();
    let out = res.output;

    assert!(out.contains("ts="), "{out}");
    assert!(out.contains("abs=3"), "{out}");
    assert!(out.contains("max=5"), "{out}");
    assert!(out.contains("min=3"), "{out}");
    assert!(out.contains("3"), "{out}");
    assert!(out.contains("a_b"), "{out}");
    assert!(out.contains("AB"), "{out}");
    assert!(out.contains("ab"), "{out}");
}

#[test]
fn string_number_casts_and_file_read_trim_work() {
    let dir = std::env::temp_dir().join("xu_runtime_stdlib_io_tests");
    let _ = fs::create_dir_all(&dir);
    let p = dir.join("x.txt");
    fs::write(&p, "a\nb\n\n").unwrap();
    let path = p.to_string_lossy();

    let src = format!(
        r#"
println(" 12 ".to_int());
println(" 3.5 ".to_float());
let h = open("{path}");
println(h.read());
"#,
    );
    let module = parse_source(&src);
    let mut rt = Runtime::new();
    let res = rt.exec_module(&module).unwrap();
    let out = res.output;
    assert!(out.contains("12"), "{out}");
    assert!(out.contains("3.5"), "{out}");
    assert!(out.contains("a\nb"), "{out}");
    assert!(!out.contains("a\nb\n\n\n"), "{out}");
}
