use std::fs;
use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;
use xu_syntax::Severity;

fn parse_source(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    let lex = Lexer::new(&normalized.text).lex();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    assert!(
        normalized
            .diagnostics
            .iter()
            .chain(lex.diagnostics.iter())
            .chain(parse.diagnostics.iter())
            .all(|d| !matches!(d.severity, Severity::Error)),
        "source should parse without errors"
    );
    parse.module
}

#[test]
fn exec_module_is_idempotent_across_runs() {
    let dir = std::env::temp_dir().join("xu_runtime_idempotent_exec_tests");
    let _ = fs::create_dir_all(&dir);

    let lib = dir.join("lib.xu");
    let main_file = dir.join("main.xu");

    fs::write(
        &lib,
        r#"
internal = [1];

pub func get() {
  return internal[0];
}

pub func clear() {
  internal = [];
}
"#
        .trim_start(),
    )
    .unwrap();

    let main_src = r#"
use "lib.xu" as lib;

func main() {
  println(lib.get());
  lib.clear();
}
"#;
    fs::write(&main_file, main_src.trim_start()).unwrap();
    let module = parse_source(main_src);

    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_entry_path(main_file.to_string_lossy().as_ref())
        .unwrap();

    let out1 = rt.exec_module(&module).unwrap();
    assert_eq!(out1.output.trim_end(), "1");

    let out2 = rt.exec_module(&module).unwrap();
    assert_eq!(out2.output.trim_end(), "1");
}
