use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn slim_baseline() -> Result<(), String> {
    let t_build = std::time::Instant::now();
    let o1 = crate::run_args(
        "cargo",
        &["build", "-p", "xu_cli", "--release", "--bin", "xu"],
    )?;
    if !o1.status.success() {
        return Err(format!(
            "slim-baseline build failed:\n{}",
            crate::format_output(&o1)
        ));
    }
    let build_ms = t_build.elapsed().as_millis();
    let bin = xu_release_bin_path();
    let bin_bytes = fs::metadata(&bin).map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "SLIM_BASELINE build_release_ms={build_ms} bin_bytes={bin_bytes} bin={}",
        bin.display()
    );

    let t_test = std::time::Instant::now();
    let o2 = crate::run_args("cargo", &["test", "--workspace"])?;
    if !o2.status.success() {
        return Err(format!(
            "slim-baseline test failed:\n{}",
            crate::format_output(&o2)
        ));
    }
    eprintln!(
        "SLIM_BASELINE test_workspace_ms={}",
        t_test.elapsed().as_millis()
    );

    let cases = [PathBuf::from("slim_cases").join("main.xu")];
    for p in cases {
        if !p.exists() {
            return Err(format!("slim case missing: {}", p.display()));
        }
        xu_bin(&bin, "check", &p)?;
        xu_bin(&bin, "run", &p)?;
    }
    Ok(())
}

fn xu_release_bin_path() -> PathBuf {
    let mut p = PathBuf::from("target");
    p.push("release");
    if cfg!(windows) {
        p.push("xu.exe");
    } else {
        p.push("xu");
    }
    p
}

fn xu_bin(bin: &Path, subcmd: &str, file: &Path) -> Result<(), String> {
    eprintln!("$ {} {} {}", bin.display(), subcmd, file.display());
    let out = Command::new(bin)
        .arg(subcmd)
        .arg(file)
        .output()
        .map_err(|e| format!("Failed to run {}: {e}", bin.display()))?;
    if !out.status.success() {
        return Err(format!(
            "xu {} failed for {}:\n{}",
            subcmd,
            file.display(),
            crate::format_output(&out)
        ));
    }
    Ok(())
}
