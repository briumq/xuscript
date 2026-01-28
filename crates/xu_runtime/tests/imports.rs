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
fn import_is_cached() {
    let dir = std::env::temp_dir().join("xu_runtime_import_tests");
    let _ = fs::create_dir_all(&dir);
    let imported = dir.join("imported.xu");

    let imported_src = r#"
count = 0;
count += 1;
println("count={count}");
"#;
    fs::write(&imported, imported_src).unwrap();

    let main_src = format!(
        r#"
use "{path}";
use "{path}";
"#,
        path = imported.to_string_lossy()
    );
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let res = rt.exec_module(&module).unwrap();
    let out = res.output;
    assert!(out.matches("count=1").count() == 1, "output was: {out}");
}

#[test]
fn import_sugar_works() {
    let dir = std::env::temp_dir().join("xu_runtime_import_sugar_tests");
    let _ = fs::create_dir_all(&dir);
    let imported = dir.join("imported_sugar.xu");

    let imported_src = r#"
println("ok");
"#;
    fs::write(&imported, imported_src).unwrap();

    let main_src = format!(
        r#"
use "{path}";
"#,
        path = imported.to_string_lossy()
    );
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let res = rt.exec_module(&module).unwrap();
    assert!(res.output.contains("ok"), "output was: {}", res.output);
}

#[test]
fn circular_import_is_reported_with_chain() {
    let dir = std::env::temp_dir().join("xu_runtime_circular_import_tests");
    let _ = fs::create_dir_all(&dir);
    let a = dir.join("a.xu");
    let b = dir.join("b.xu");

    fs::write(&a, "").unwrap();
    fs::write(&b, "").unwrap();

    let a_path = a.to_string_lossy().to_string();
    let b_path = b.to_string_lossy().to_string();

    fs::write(&a, format!("use(\"{b_path}\");")).unwrap();
    fs::write(&b, format!("use(\"{a_path}\");")).unwrap();

    let a_key = std::fs::canonicalize(&a)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let b_key = std::fs::canonicalize(&b)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let main_src = format!("use(\"{a_path}\");");
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let err = rt.exec_module(&module).unwrap_err();
    assert!(err.contains("Circular import:"), "error was: {err}");
    assert!(
        err.contains(&format!("{a_key} -> {b_key} -> {a_key}")),
        "error was: {err}"
    );
}

#[test]
fn import_merges_exports_into_env_and_returns_dict() {
    let dir = std::env::temp_dir().join("xu_runtime_import_merge_tests");
    let _ = fs::create_dir_all(&dir);
    let imported = dir.join("lib.xu");

    let imported_src = r#"
value = 7;

func add_one(n) {
  return n + 1;
}
"#;
    fs::write(&imported, imported_src).unwrap();

    let path = imported.to_string_lossy();
    let main_src = format!(
        r#"
use "{path}" as m;
println(value);
println(m.value);
println(add_one(1));
"#,
    );
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let res = rt.exec_module(&module).unwrap();
    assert_eq!(res.output.trim_end(), "7\n7\n2");
}

#[test]
fn import_stack_is_cleared_on_import_error() {
    let dir = std::env::temp_dir().join("xu_runtime_import_stack_tests");
    let _ = fs::create_dir_all(&dir);
    let bad = dir.join("bad.xu");

    fs::write(&bad, "x @ 1;").unwrap();
    let path = bad.to_string_lossy();

    let main_src = format!(r#"use "{path}";"#);
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));

    let err1 = rt.exec_module(&module).unwrap_err();
    assert!(
        err1.contains("Unexpected character") || err1.contains("Undefined identifier"),
        "unexpected error: {err1}"
    );
    assert!(
        !err1.contains("Circular import"),
        "unexpected circular import: {err1}"
    );

    let err2 = rt.exec_module(&module).unwrap_err();
    assert!(
        err2.contains("Unexpected character") || err2.contains("Undefined identifier"),
        "unexpected error: {err2}"
    );
    assert!(
        !err2.contains("Circular import"),
        "unexpected circular import: {err2}"
    );
}

#[test]
fn relative_import_resolves_against_entry_file_dir() {
    let dir = std::env::temp_dir().join("xu_runtime_import_path_entry_base_tests");
    let _ = fs::create_dir_all(&dir);
    let main_file = dir.join("main.xu");
    let dep = dir.join("dep.xu");

    fs::write(&main_file, "println(\"main\");").unwrap();
    fs::write(&dep, "value = 1;").unwrap();

    let main_src = r#"
use "dep.xu";
println(value);
"#;
    let module = parse_source(main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_entry_path(main_file.to_string_lossy().as_ref())
        .unwrap();
    let res = rt.exec_module(&module).unwrap();
    assert_eq!(res.output.trim_end(), "1");
}

#[test]
fn relative_import_resolves_against_importer_module_dir() {
    let dir = std::env::temp_dir().join("xu_runtime_import_path_importer_base_tests");
    let _ = fs::create_dir_all(&dir);
    let main_file = dir.join("main.xu");
    let sub = dir.join("sub");
    let _ = fs::create_dir_all(&sub);
    let mod_file = sub.join("mod.xu");
    let inner = sub.join("inner.xu");

    fs::write(&main_file, "println(\"main\");").unwrap();
    fs::write(
        &mod_file,
        r#"
use "inner.xu";
println(sub_value);
"#,
    )
    .unwrap();
    fs::write(&inner, "sub_value = 2;").unwrap();

    let main_src = format!(
        r#"
use "{path}";
"#,
        path = mod_file.to_string_lossy()
    );
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_entry_path(main_file.to_string_lossy().as_ref())
        .unwrap();
    let res = rt.exec_module(&module).unwrap();
    assert_eq!(res.output.trim_end(), "2");
}

#[test]
fn underscore_prefixed_names_are_not_exported() {
    let dir = std::env::temp_dir().join("xu_runtime_import_private_exports_tests");
    let _ = fs::create_dir_all(&dir);
    let imported = dir.join("lib.xu");

    fs::write(
        &imported,
        r#"
_private = 1;
public = 2;
"#,
    )
    .unwrap();

    let main_src = format!(
        r#"
use "{path}" as m;
println(m.public);
println(m._private);
"#,
        path = imported.to_string_lossy()
    );
    let module = parse_source(&main_src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let err = rt.exec_module(&module).unwrap_err();
    assert!(
        err.contains("_private") && err.contains("Unknown member"),
        "{err}"
    );
}
