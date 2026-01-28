use std::fs;

use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;

fn parse_source(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    let lex = Lexer::new(&normalized.text).lex();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    parse.module
}

#[test]
fn import_parse_cache_invalidates_on_file_change() {
    let dir = std::env::temp_dir().join("xu_runtime_import_parse_cache_tests");
    let _ = fs::create_dir_all(&dir);

    let dep = dir.join("dep.xu");
    fs::write(&dep, "x @ 1;").unwrap();

    let dep_path = dep.to_string_lossy();
    let main_src = format!(
        r#"
use "{dep_path}";
println(value);
"#
    );
    let module = parse_source(&main_src);

    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_entry_path(dir.to_string_lossy().as_ref()).unwrap();

    let err1 = rt.exec_module(&module).unwrap_err();
    assert!(
        err1.contains("Unexpected character") || err1.contains("Undefined identifier"),
        "unexpected error: {err1}"
    );

    fs::write(&dep, "value = 1;").unwrap();

    let res2 = rt.exec_module(&module).unwrap();
    assert_eq!(res2.output.trim_end(), "1");
}
