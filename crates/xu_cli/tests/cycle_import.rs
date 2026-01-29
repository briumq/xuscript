use std::fs;
use std::process::Command;

fn run_xu(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xu"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn run_reports_circular_import_chain() {
    let dir = std::env::temp_dir().join("xu_cli_circular_import_tests");
    let _ = fs::create_dir_all(&dir);
    let main = dir.join("main.xu");
    let a = dir.join("a.xu");
    let b = dir.join("b.xu");

    fs::write(&a, "").unwrap();
    fs::write(&b, "").unwrap();

    let a_path = a.to_string_lossy().to_string();
    let b_path = b.to_string_lossy().to_string();

    fs::write(&a, format!("use \"{b_path}\";")).unwrap();
    fs::write(&b, format!("use \"{a_path}\";")).unwrap();
    fs::write(&main, format!("use \"{a_path}\";")).unwrap();

    let a_key = std::fs::canonicalize(&a)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let b_key = std::fs::canonicalize(&b)
        .unwrap()
        .to_string_lossy()
        .to_string();

    let out = run_xu(&["run", main.to_string_lossy().as_ref()]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("RuntimeError: Circular import:"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains(&format!("{a_key} -> {b_key} -> {a_key}")),
        "stderr was: {stderr}"
    );
}
