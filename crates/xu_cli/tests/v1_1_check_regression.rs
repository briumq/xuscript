use std::process::Command;

#[test]
fn cli_check_v1_1_match_bindings_ok() {
    let exe = env!("CARGO_BIN_EXE_xu");
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");
    let candidate_paths = [
        repo_root.join("tests/specs/enums_with_data.xu"),
        repo_root.join("tests/specs/v1_1_enums_with_data.xu"),
    ];
    let path = candidate_paths
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidate_paths[0].clone());

    let out = Command::new(exe)
        .arg("check")
        .arg(path.to_string_lossy().to_string())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert!(!stderr.contains("Undefined identifier"), "{stderr}");
}
