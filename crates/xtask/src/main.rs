use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

mod process;
use process::{format_output, run_args, run_owned};
mod bench;
mod perf;
mod slim;

fn main() {
    let mut args = env::args().skip(1);
    let cmd = args.next().unwrap_or_else(|| "verify".to_string());
    let next = args.next();
    let result = match cmd.as_str() {
        "verify" => verify(),
        "fmt" => fmt_check(),
        "clippy" => clippy(),
        "lint" => lint_strict(),
        "check-unused" => check_unused(),
        "test" => test_workspace(),
        "examples" => verify_examples(),
        "codegen-examples" => codegen_examples(),
        "slim-baseline" => slim::slim_baseline(),
        "perf" => perf::perf(next.as_deref()),
        "bench-report" => bench::bench_report(next.as_deref()),
        _ => usage_error(&cmd),
    };
    if let Err(msg) = result {
        eprintln!("{msg}");
        std::process::exit(1);
    }
}

fn usage_error(cmd: &str) -> Result<(), String> {
    Err(format!(
        "Unknown command: {cmd}\nUsage: cargo run -p xtask -- <verify|fmt|clippy|lint|check-unused|test|examples|codegen-examples|slim-baseline|perf [update-baseline]|bench-report [scales]>"
    ))
}

fn verify() -> Result<(), String> {
    fmt_check()?;
    lint_strict()?;
    test_workspace()?;
    verify_examples()?;
    verify_optional_projects()?;
    Ok(())
}

fn fmt_check() -> Result<(), String> {
    process::run_args("cargo", &["fmt", "--all", "--", "--check"]).map(|_| ())
}

fn clippy() -> Result<(), String> {
    process::run_args(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
    .map(|_| ())
}

fn lint_strict() -> Result<(), String> {
    process::run_args(
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
            "-W",
            "clippy::all",
            "-W",
            "clippy::perf",
            "-W",
            "clippy::nursery",
        ],
    )
    .map(|_| ())
}

fn check_unused() -> Result<(), String> {
    let o = process::run_args("cargo", &["udeps", "--workspace"])?;
    if !o.status.success() {
        return Err(format!("cargo udeps failed:\n{}", process::format_output(&o)));
    }
    Ok(())
}
fn test_workspace() -> Result<(), String> {
    process::run_args("cargo", &["test", "--workspace"]).map(|_| ())
}

fn verify_examples() -> Result<(), String> {
    let manifest = load_example_manifest()?;
    let (valid, invalid) = list_examples(&manifest, "examples")?;
    let xu_bin = build_xu_cli()?;
    for f in valid {
        if !should_check_example(&manifest, &f) {
            continue;
        }
        xu_check(&xu_bin, &f)?;
        if should_ast_example(&manifest, &f) {
            xu_ast(&xu_bin, &f)?;
        }
        if should_run_example(&manifest, &f) {
            if should_expect_fail_run(&manifest, &f) {
                xu_run_expect_fail(&xu_bin, &f)?;
            } else {
                xu_run(&xu_bin, &f)?;
            }
        }
    }
    for f in invalid {
        xu_check_expect_fail(&xu_bin, &f)?;
    }
    Ok(())
}

fn build_xu_cli() -> Result<PathBuf, String> {
    let output = run_owned("cargo", &[
        "build".to_string(),
        "-p".to_string(),
        "xu_cli".to_string(),
        "--bin".to_string(),
        "xu".to_string(),
    ])?;
    if !output.status.success() {
        return Err(format!("Failed to build xu_cli:\n{}", format_output(&output)));
    }
    get_xu_binary_path()
}

fn get_xu_binary_path() -> Result<PathBuf, String> {
    let mut p = std::env::current_dir().map_err(|e| e.to_string())?;
    p.push("target");
    p.push("debug");
    p.push("xu");
    if cfg!(windows) {
        p.set_extension("exe");
    }
    if p.exists() {
        Ok(p)
    } else {
        Err(format!("Binary not found at {}", p.display()))
    }
}

fn codegen_examples() -> Result<(), String> {
    let manifest = load_example_manifest()?;
    let (valid, _invalid) = list_examples(&manifest, "examples")?;
    ensure_runtime_assets()?;
    let xu_bin = build_xu_cli()?;
    let mut ok_codegen = 0usize;
    let mut ok_run_js = 0usize;
    let mut ok_run_py = 0usize;
    let mut total = 0usize;
    for f in valid {
        total += 1;
        let src = fs::read_to_string(&f).unwrap_or_default();
        let run_friendly = is_run_friendly(&src);
        // JS codegen with runtime injection
        let js_out = temp_file_path(&f, "generated.js");
        let js_args = vec![
            "codegen".to_string(),
            f.display().to_string(),
            "js".to_string(),
            js_out.display().to_string(),
            "--inject-runtime".to_string(),
        ];
        let cg = run_owned(xu_bin.to_str().unwrap(), &js_args)?;
        if cg.status.success() {
            ok_codegen += 1;
        } else {
            eprintln!(
                "JS codegen failed for {}:\n{}",
                f.display(),
                format_output(&cg)
            );
            continue;
        }
        if run_friendly
            && should_run_example(&manifest, &f)
            && !should_expect_fail_run(&manifest, &f)
        {
            let run = run_args("node", &[js_out.to_string_lossy().as_ref()])?;
            if run.status.success() {
                ok_run_js += 1;
            } else {
                eprintln!(
                    "JS run failed for {}:\n{}",
                    f.display(),
                    format_output(&run)
                );
            }
        }
        // Python codegen with runtime injection
        let py_out = temp_file_path(&f, "generated.py");
        let py_args = vec![
            "codegen".to_string(),
            f.display().to_string(),
            "py".to_string(),
            py_out.display().to_string(),
            "--inject-runtime".to_string(),
        ];
        let cg_py = run_owned(xu_bin.to_str().unwrap(), &py_args)?;
        if !cg_py.status.success() {
            eprintln!(
                "Py codegen failed for {}:\n{}",
                f.display(),
                format_output(&cg_py)
            );
            continue;
        }
        if run_friendly
            && should_run_example(&manifest, &f)
            && !should_expect_fail_run(&manifest, &f)
        {
            let runp = run_args("python3", &[py_out.to_string_lossy().as_ref()])?;
            if runp.status.success() {
                ok_run_py += 1;
            } else {
                eprintln!(
                    "Py run failed for {}:\n{}",
                    f.display(),
                    format_output(&runp)
                );
            }
        }
    }
    eprintln!(
        "Codegen summary: total={} ok_codegen={} ok_run_js={} ok_run_py={}",
        total, ok_codegen, ok_run_js, ok_run_py
    );
    Ok(())
}

fn ensure_runtime_assets() -> Result<(), String> {
    let mut out_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    out_dir.push("target");
    out_dir.push("codegen_examples");
    out_dir.push("tools");
    let _ = fs::create_dir_all(&out_dir);
    Ok(())
}

fn temp_file_path(src: &Path, suffix: &str) -> PathBuf {
    let mut p = std::env::current_dir().unwrap();
    p.push("target");
    p.push("codegen_examples");
    let _ = fs::create_dir_all(&p);
    let name = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("example");
    p.push(format!("{}_{}", name, suffix));
    p
}

fn is_run_friendly(src: &str) -> bool {
    let bad = ["input(", "open(", "import \"std/", "import(\"std/"];
    !bad.iter().any(|p| src.contains(p))
}

fn should_run_example(manifest: &ExampleManifest, path: &Path) -> bool {
    let p = path.to_string_lossy();
    !manifest
        .run_skip_prefixes
        .iter()
        .any(|prefix| p.contains(prefix))
}

fn should_check_example(manifest: &ExampleManifest, path: &Path) -> bool {
    let p = path.to_string_lossy();
    !manifest
        .check_skip_prefixes
        .iter()
        .any(|prefix| p.contains(prefix))
}

fn should_ast_example(manifest: &ExampleManifest, path: &Path) -> bool {
    let p = path.to_string_lossy();
    !manifest
        .ast_skip_prefixes
        .iter()
        .any(|prefix| p.contains(prefix))
}

fn should_expect_fail_run(manifest: &ExampleManifest, path: &Path) -> bool {
    let p = path.to_string_lossy();
    manifest
        .run_expect_fail
        .iter()
        .any(|s| p.ends_with(s.as_str()))
}

fn list_examples(
    manifest: &ExampleManifest,
    dir: impl AsRef<Path>,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>), String> {
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    fn visit_dir(
        manifest: &ExampleManifest,
        dir: &Path,
        valid: &mut Vec<PathBuf>,
        invalid: &mut Vec<PathBuf>,
    ) -> Result<(), String> {
        let entries =
            fs::read_dir(dir).map_err(|e| format!("Failed to read examples dir {:?}: {e}", dir))?;
        for e in entries {
            let e = e.map_err(|e| format!("Failed to read examples entry: {e}"))?;
            let path = e.path();
            if path.is_dir() {
                let dir_name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if dir_name == "modules" {
                    continue;
                }
                if manifest.skip_projects.iter().any(|n| n == dir_name) {
                    continue;
                }
                visit_dir(manifest, &path, valid, invalid)?;
                continue;
            }
            if path.extension() != Some(OsStr::new("xu")) {
                continue;
            }
            if path.to_string_lossy().contains("/modules/") {
                continue;
            }
            let base = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if base.contains(manifest.invalid_filename_substring.as_str()) {
                invalid.push(path);
            } else {
                valid.push(path);
            }
        }
        Ok(())
    }

    visit_dir(manifest, dir.as_ref(), &mut valid, &mut invalid)?;
    valid.sort();
    invalid.sort();
    Ok((valid, invalid))
}

#[derive(Clone, Debug, Deserialize)]
#[allow(dead_code)]
struct ExampleManifest {
    #[serde(default)]
    invalid_filename_substring: String,
    #[serde(default)]
    skip_projects: Vec<String>,
    #[serde(default)]
    project_entry_candidates: Vec<String>,
    #[serde(default)]
    project_entry_overrides: std::collections::HashMap<String, Vec<String>>,
    #[serde(default)]
    check_skip_prefixes: Vec<String>,
    #[serde(default)]
    ast_skip_prefixes: Vec<String>,
    #[serde(default)]
    run_skip_prefixes: Vec<String>,
    #[serde(default)]
    run_expect_fail: Vec<String>,
}

impl Default for ExampleManifest {
    fn default() -> Self {
        Self {
            invalid_filename_substring: "error".into(),
            skip_projects: vec![],
            project_entry_candidates: vec![
                "test.xu".into(),
                "main.xu".into(),
                "main_split.xu".into(),
            ],
            project_entry_overrides: std::collections::HashMap::from([]),
            check_skip_prefixes: vec![],
            ast_skip_prefixes: vec![],
            run_skip_prefixes: vec![],
            run_expect_fail: vec![],
        }
    }
}

fn load_example_manifest() -> Result<ExampleManifest, String> {
    let path = PathBuf::from("examples").join("manifest.json");
    let input = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ExampleManifest::default()),
        Err(e) => return Err(format!("Failed to read {}: {e}", path.display())),
    };
    let mut m = serde_json::from_str::<ExampleManifest>(&input)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;
    if m.invalid_filename_substring.is_empty() {
        m.invalid_filename_substring = ExampleManifest::default().invalid_filename_substring;
    }
    if m.project_entry_candidates.is_empty() {
        m.project_entry_candidates = ExampleManifest::default().project_entry_candidates;
    }
    Ok(m)
}

fn verify_optional_projects() -> Result<(), String> {
    if env::var("XU_PERF").ok().as_deref() == Some("1") {
        perf::perf(None)?;
    }
    if env::var("XU_BENCH").ok().as_deref() == Some("1") {
        bench::run_bench_suite()?;
    }
    Ok(())
}

fn xu_check(bin: &Path, path: &Path) -> Result<(), String> {
    let output = run_owned(bin.to_str().unwrap(), &["check".to_string(), path.display().to_string()])?;
    if !output.status.success() {
        return Err(format!(
            "xu check failed for {}:\n{}",
            path.display(),
            format_output(&output)
        ));
    }
    Ok(())
}

fn xu_check_expect_fail(bin: &Path, path: &Path) -> Result<(), String> {
    let output = run_owned(bin.to_str().unwrap(), &["check".to_string(), path.display().to_string()])?;
    if output.status.success() {
        return Err(format!("unexpected check success: {}", path.display()));
    }
    Ok(())
}

fn xu_ast(bin: &Path, path: &Path) -> Result<(), String> {
    let output = run_owned(bin.to_str().unwrap(), &["ast".to_string(), path.display().to_string()])?;
    if !output.status.success() {
        return Err(format!(
            "xu ast failed for {}:\n{}",
            path.display(),
            format_output(&output)
        ));
    }
    Ok(())
}

fn xu_run(bin: &Path, path: &Path) -> Result<(), String> {
    let output = run_owned(bin.to_str().unwrap(), &["run".to_string(), path.display().to_string()])?;
    if !output.status.success() {
        return Err(format!(
            "xu run failed for {}:\n{}",
            path.display(),
            format_output(&output)
        ));
    }
    Ok(())
}

fn xu_run_expect_fail(bin: &Path, path: &Path) -> Result<(), String> {
    let output = run_owned(bin.to_str().unwrap(), &["run".to_string(), path.display().to_string()])?;
    if output.status.success() {
        return Err(format!("unexpected run success: {}", path.display()));
    }
    Ok(())
}


