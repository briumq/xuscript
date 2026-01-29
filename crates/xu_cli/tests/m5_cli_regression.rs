use std::path::PathBuf;
use std::process::Command;

fn write_temp_xu(name: &str, content: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    let unique = format!(
        "xu_cli_test_{}_{}_{}.xu",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    path.push(unique);
    std::fs::write(&path, content).unwrap();
    path
}

fn run_xu(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xu"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn usage_without_args() {
    let out = run_xu(&[]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage: xu"));
}

#[test]
fn check_requires_file() {
    let out = run_xu(&["check"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Missing <file>"));
}

#[test]
fn check_valid_example_succeeds() {
    let path = write_temp_xu(
        "check_valid_example_succeeds",
        r#"
println("hello");
let a = 1 + 2;
println("a={a}");
"#
        .trim_start(),
    );
    let out = run_xu(&["check", path.to_string_lossy().as_ref()]);
    let _ = std::fs::remove_file(&path);
    assert!(out.status.success());
}

#[test]
fn check_invalid_example_fails_with_diagnostics() {
    let path = write_temp_xu(
        "check_invalid_example_fails_with_diagnostics",
        r#"
if true {
  println("oops")
"#
        .trim_start(),
    );
    let out = run_xu(&["check", path.to_string_lossy().as_ref()]);
    let _ = std::fs::remove_file(&path);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Error:"));
    assert!(stderr.contains("Expected"), "{stderr}");
}
