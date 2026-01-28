use std::process::Command;

#[test]
fn cli_static_diagnostics_have_context() {
    let dir = std::env::temp_dir().join("xu_cli_static_diag_tests");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("bad.xu");

    let src = r#"
func add(a: int, b: int) -> int {
  return a + b;
}

print(undef);
print(add(1));
"#;
    std::fs::write(&path, src).unwrap();

    let exe = env!("CARGO_BIN_EXE_xu");
    let out = Command::new(exe)
        .arg("check")
        .arg(path.to_string_lossy().to_string())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Undefined identifier: undef"), "{stderr}");
    assert!(stderr.contains("Argument count mismatch"), "{stderr}");
    assert!(stderr.contains("  | "), "{stderr}");
    assert!(stderr.contains('^'), "{stderr}");
}
