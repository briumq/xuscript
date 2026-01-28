use std::fs;
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
    fs::write(&path, content).unwrap();
    path
}

fn run_xu(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xu"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn strict_mode_rejects_untyped_new_assignment() {
    let path = write_temp_xu(
        "strict_rejects_untyped_new_assignment",
        r#"
x = 1;
"#
        .trim_start(),
    );
    let out = run_xu(&["check", "--strict", path.to_string_lossy().as_ref()]);
    let _ = fs::remove_file(&path);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Undefined identifier: x"), "{stderr}");
}

#[test]
fn strict_mode_allows_typed_declare_then_assign() {
    let path = write_temp_xu(
        "strict_allows_typed_declare_then_assign",
        r#"
let x: int = 1;
x = 2;
"#
        .trim_start(),
    );
    let out = run_xu(&["check", "--strict", path.to_string_lossy().as_ref()]);
    let _ = fs::remove_file(&path);
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}
