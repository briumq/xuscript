//!
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde::Deserialize;

mod slim;

///
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
        "perf" => perf(next.as_deref()),
        "bench-report" => bench_report(next.as_deref()),
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
    run_args("cargo", &["fmt", "--all", "--", "--check"]).map(|_| ())
}

fn clippy() -> Result<(), String> {
    run_args(
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
    run_args(
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
    let o = run_args("cargo", &["udeps", "--workspace"])?;
    if !o.status.success() {
        return Err(format!("cargo udeps failed:\n{}", format_output(&o)));
    }
    Ok(())
}
fn test_workspace() -> Result<(), String> {
    run_args("cargo", &["test", "--workspace"]).map(|_| ())
}

fn verify_examples() -> Result<(), String> {
    let manifest = load_example_manifest()?;
    let (valid, invalid) = list_examples(&manifest, "examples")?;
    for f in valid {
        if !should_check_example(&manifest, &f) {
            continue;
        }
        xu_check(&f)?;
        if should_ast_example(&manifest, &f) {
            xu_ast(&f)?;
        }
        if should_run_example(&manifest, &f) {
            if should_expect_fail_run(&manifest, &f) {
                xu_run_expect_fail(&f)?;
            } else {
                xu_run(&f)?;
            }
        }
    }
    for f in invalid {
        xu_check_expect_fail(&f)?;
    }
    Ok(())
}

fn codegen_examples() -> Result<(), String> {
    let manifest = load_example_manifest()?;
    let (valid, _invalid) = list_examples(&manifest, "examples")?;
    ensure_runtime_assets()?;
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
            "run".to_string(),
            "-q".to_string(),
            "-p".to_string(),
            "xu_cli".to_string(),
            "--bin".to_string(),
            "xu".to_string(),
            "--".to_string(),
            "codegen".to_string(),
            f.display().to_string(),
            "js".to_string(),
            js_out.display().to_string(),
            "--inject-runtime".to_string(),
        ];
        let cg = run_owned("cargo", &js_args)?;
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
            "run".to_string(),
            "-q".to_string(),
            "-p".to_string(),
            "xu_cli".to_string(),
            "--bin".to_string(),
            "xu".to_string(),
            "--".to_string(),
            "codegen".to_string(),
            f.display().to_string(),
            "py".to_string(),
            py_out.display().to_string(),
            "--inject-runtime".to_string(),
        ];
        let cg_py = run_owned("cargo", &py_args)?;
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
    // copy JS runtime
    let js_src = PathBuf::from("tools").join("big_world_js_runtime.js");
    let js_dst = out_dir.join("big_world_js_runtime.js");
    if js_src.exists() {
        fs::copy(&js_src, &js_dst).map_err(|e| {
            format!(
                "copy {} -> {} failed: {e}",
                js_src.display(),
                js_dst.display()
            )
        })?;
    }
    // copy Py runtime
    let py_src = PathBuf::from("tools").join("big_world_py_runtime.py");
    let py_dst = out_dir.join("big_world_py_runtime.py");
    if py_src.exists() {
        fs::copy(&py_src, &py_dst).map_err(|e| {
            format!(
                "copy {} -> {} failed: {e}",
                py_src.display(),
                py_dst.display()
            )
        })?;
    }
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
    let entries = fs::read_dir(dir.as_ref())
        .map_err(|e| format!("Failed to read examples dir {:?}: {e}", dir.as_ref()))?;
    for e in entries {
        let e = e.map_err(|e| format!("Failed to read examples entry: {e}"))?;
        let path = e.path();
        if path.is_dir() {
            let dir_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            if manifest.skip_projects.iter().any(|n| n == dir_name) {
                continue;
            }
            let candidates = manifest
                .project_entry_overrides
                .get(dir_name)
                .map(|v| v.as_slice())
                .unwrap_or(manifest.project_entry_candidates.as_slice());
            for entry in candidates {
                let entry_path = path.join(entry);
                if entry_path.exists() {
                    valid.push(entry_path);
                    break;
                }
            }
            continue;
        }
        if path.extension() != Some(OsStr::new("xu")) {
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
    valid.sort();
    invalid.sort();
    Ok((valid, invalid))
}

#[derive(Clone, Debug, Deserialize)]
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
            skip_projects: vec!["shop".into()],
            project_entry_candidates: vec![
                "test.xu".into(),
                "main.xu".into(),
                "main_split.xu".into(),
            ],
            project_entry_overrides: std::collections::HashMap::from([(
                "xu_query".into(),
                vec![
                    "smoke.xu".into(),
                    "test.xu".into(),
                    "main.xu".into(),
                    "main_split.xu".into(),
                ],
            )]),
            check_skip_prefixes: vec!["examples/big_world/".into()],
            ast_skip_prefixes: vec!["examples/big_world/".into()],
            run_skip_prefixes: vec!["examples/big_world/".into(), "examples/xu_query/".into()],
            run_expect_fail: vec!["examples/04_exceptions.xu".into()],
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
        perf(None)?;
    }
    if env::var("XU_BENCH").ok().as_deref() == Some("1") {
        run_bench_suite()?;
    }
    Ok(())
}

fn perf(mode: Option<&str>) -> Result<(), String> {
    let update_baseline = matches!(mode, Some("update-baseline"))
        || env::var("XU_PERF_UPDATE").ok().as_deref() == Some("1");

    let runs: usize = env::var("XU_PERF_RUNS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);

    let ratio: f64 = env::var("XU_PERF_MAX_RATIO")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(1.5);

    let abs_ms: u64 = env::var("XU_PERF_MAX_ABS_MS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(20);

    let baseline_path = PathBuf::from("perf").join("baseline.txt");
    let baseline = read_baseline(&baseline_path)?;

    let mut best: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for _ in 0..runs {
        let mut m = run_perf_suite()?;
        for (k, v) in m.drain() {
            best.entry(k)
                .and_modify(|cur| {
                    if v < *cur {
                        *cur = v;
                    }
                })
                .or_insert(v);
        }
    }

    let mut keys: Vec<_> = best.keys().cloned().collect();
    keys.sort();
    for k in keys {
        let v = best.get(&k).copied().unwrap_or(0);
        eprintln!("PERF_RESULT {k}={v}");
    }

    if update_baseline {
        write_baseline(&baseline_path, &best)?;
        return Ok(());
    }

    let mut failures = Vec::new();
    for (k, measured) in best.iter() {
        let Some(base) = baseline.get(k).copied() else {
            continue;
        };
        if base == 0 {
            continue;
        }
        let allowed = ((base as f64) * ratio).ceil() as u64 + abs_ms;
        if *measured > allowed {
            failures.push(format!(
                "{k}: measured={measured}ms baseline={base}ms allowed<={allowed}ms"
            ));
        }
    }
    if !failures.is_empty() {
        return Err(format!(
            "perf gate failed (set XU_PERF_UPDATE=1 to refresh baseline):\n{}",
            failures.join("\n")
        ));
    }

    if env::var("XU_BENCH_REPORT").ok().as_deref() == Some("1") {
        let scales = env::var("XU_BENCH_SCALES").ok();
        if let Some(s) = scales.as_deref() {
            bench_report(Some(s))?;
        } else {
            bench_report(None)?;
        }
    }

    Ok(())
}

fn run_perf_suite() -> Result<std::collections::HashMap<String, u64>, String> {
    let mut out = std::collections::HashMap::new();
    let o1 = run_args(
        "cargo",
        &[
            "test",
            "-q",
            "-p",
            "xu_runtime",
            "--test",
            "perf_lexer_parser",
            "--release",
            "--",
            "--ignored",
            "--nocapture",
        ],
    )?;
    if !o1.status.success() {
        return Err(format!("perf_lexer_parser failed:\n{}", format_output(&o1)));
    }
    parse_perf_output(&mut out, &o1)?;

    let o2 = run_args(
        "cargo",
        &[
            "test",
            "-q",
            "-p",
            "xu_runtime",
            "--test",
            "perf_runtime_exec",
            "--release",
            "--",
            "--ignored",
            "--nocapture",
        ],
    )?;
    if !o2.status.success() {
        return Err(format!("perf_runtime_exec failed:\n{}", format_output(&o2)));
    }
    parse_perf_output(&mut out, &o2)?;

    let o3 = run_args(
        "cargo",
        &[
            "test",
            "-q",
            "-p",
            "xu_runtime",
            "--test",
            "perf_benchmarks",
            "--release",
            "--",
            "--ignored",
            "--nocapture",
        ],
    )?;
    if !o3.status.success() {
        return Err(format!("perf_benchmarks failed:\n{}", format_output(&o3)));
    }
    parse_perf_output(&mut out, &o3)?;

    let o4 = run_args(
        "cargo",
        &[
            "test",
            "-q",
            "-p",
            "xu_runtime",
            "--test",
            "perf_vm_long_interpolation",
            "--release",
            "--",
            "--ignored",
            "--nocapture",
        ],
    )?;
    if !o4.status.success() {
        return Err(format!(
            "perf_vm_long_interpolation failed:\n{}",
            format_output(&o4)
        ));
    }
    parse_perf_output(&mut out, &o4)?;

    Ok(out)
}

fn run_bench_suite() -> Result<(), String> {
    let o1 = run_args("bash", &["scripts/run_cross_lang_bench.sh", "5000"])?;
    if !o1.status.success() {
        return Err(format!(
            "run_cross_lang_bench.sh 5000 failed:\n{}",
            format_output(&o1)
        ));
    }
    let o2 = run_args("bash", &["scripts/run_cross_lang_bench.sh", "10000"])?;
    if !o2.status.success() {
        return Err(format!(
            "run_cross_lang_bench.sh 10000 failed:\n{}",
            format_output(&o2)
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
struct BenchJsonLine {
    #[serde(rename = "case")]
    case_name: String,
    scale: usize,
    duration_ms: f64,
}

fn bench_report(scales_arg: Option<&str>) -> Result<(), String> {
    let scales = if let Some(s) = scales_arg {
        let mut out = Vec::new();
        for part in s.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let v: usize = part
                .parse()
                .map_err(|_| format!("bench-report scales must be like 5000,10000; got {s}"))?;
            out.push(v);
        }
        if out.is_empty() {
            vec![5000, 10000]
        } else {
            out
        }
    } else {
        vec![5000, 10000]
    };

    let rustc = run_args("rustc", &["-V"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    let python = run_args("python3", &["--version"]).ok().map(|o| {
        let mut s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if s.is_empty() {
            s = String::from_utf8_lossy(&o.stderr).trim().to_string();
        }
        s
    });
    let node = run_args("node", &["--version"])
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let mut all: std::collections::BTreeMap<
        usize,
        std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>,
    > = std::collections::BTreeMap::new();
    for &scale in &scales {
        let out = run_args(
            "bash",
            &["scripts/run_cross_lang_bench.sh", &scale.to_string()],
        )?;
        if !out.status.success() {
            return Err(format!(
                "run_cross_lang_bench.sh {scale} failed:\n{}",
                format_output(&out)
            ));
        }
        let parsed = parse_cross_lang_bench_output(scale, &out)?;
        all.insert(scale, parsed);
    }

    let report_path = PathBuf::from("benchmarks").join("report.md");
    let mut md = String::new();
    md.push_str("# Benchmarks Report\n\n");
    md.push_str(&format!(
        "- Generated: {}\n",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    ));
    md.push_str(&format!(
        "- Scales: {}\n",
        scales
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    md.push_str(&format!(
        "- OS: {} / {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    if let Some(s) = rustc {
        if !s.is_empty() {
            md.push_str(&format!("- Rust: {s}\n"));
        }
    }
    if let Some(s) = python {
        if !s.is_empty() {
            md.push_str(&format!("- Python: {s}\n"));
        }
    }
    if let Some(s) = node {
        if !s.is_empty() {
            md.push_str(&format!("- Node: {s}\n"));
        }
    }
    md.push('\n');

    let case_order = [
        "loop",
        "dict",
        "dict-intkey",
        "dict-hot",
        "string",
        "string-builder",
        "struct-method",
    ];
    let langs = ["Python", "Node.js", "Xu"];

    for (&scale, table) in &all {
        md.push_str(&format!("## Scale {scale}\n\n"));
        md.push_str("| case | Python (ms) | Node.js (ms) | Xu (ms) | winner |\n");
        md.push_str("|---|---:|---:|---:|---|\n");

        let mut seen_cases: std::collections::BTreeMap<String, ()> =
            std::collections::BTreeMap::new();
        for m in table.values() {
            for k in m.keys() {
                seen_cases.insert(k.clone(), ());
            }
        }
        let mut rows: Vec<String> = Vec::new();
        for c in case_order {
            if seen_cases.contains_key(c) {
                rows.push(c.to_string());
            }
        }
        for c in seen_cases.keys() {
            if !rows.iter().any(|x| x == c) {
                rows.push(c.clone());
            }
        }

        for case_name in rows {
            let mut ms = std::collections::BTreeMap::new();
            for lang in langs {
                let v = table.get(lang).and_then(|m| m.get(&case_name)).copied();
                ms.insert(lang.to_string(), v);
            }
            let mut winner = String::new();
            let mut best = None::<(String, f64)>;
            for (k, v) in &ms {
                if let Some(vv) = *v {
                    if best.as_ref().map(|(_, b)| vv < *b).unwrap_or(true) {
                        best = Some((k.clone(), vv));
                    }
                }
            }
            if let Some((k, _)) = best {
                winner = k;
            }
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                case_name,
                fmt_ms(ms.get("Python").copied().flatten()),
                fmt_ms(ms.get("Node.js").copied().flatten()),
                fmt_ms(ms.get("Xu").copied().flatten()),
                winner
            ));
        }
        md.push('\n');
    }

    fs::write(&report_path, md)
        .map_err(|e| format!("Failed to write {}: {e}", report_path.display()))?;
    eprintln!("Wrote {}", report_path.display());
    Ok(())
}

fn fmt_ms(v: Option<f64>) -> String {
    match v {
        Some(x) if x.is_finite() => {
            if x == 0.0 {
                return "<1".into();
            }
            if x > 0.0 && x < 1.0 {
                return format!("{:.2}", x);
            }
            if (x.fract() - 0.0).abs() < f64::EPSILON {
                format!("{}", x as u64)
            } else {
                format!("{:.2}", x)
            }
        }
        _ => "-".into(),
    }
}

fn parse_cross_lang_bench_output(
    scale: usize,
    output: &Output,
) -> Result<std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>>, String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut table: std::collections::BTreeMap<String, std::collections::BTreeMap<String, f64>> =
        std::collections::BTreeMap::new();
    let mut section: Option<String> = None;
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.ends_with(':') && !line.starts_with('{') {
            section = Some(line.trim_end_matches(':').to_string());
            continue;
        }
        let Some(sec) = section.clone() else {
            continue;
        };
        if sec == "Python" || sec == "Node.js" {
            let v: BenchJsonLine = serde_json::from_str(line)
                .map_err(|e| format!("Bad JSON in {sec} output: {e}: {line}"))?;
            if v.scale != scale {
                continue;
            }
            table
                .entry(sec)
                .or_default()
                .insert(v.case_name, v.duration_ms);
        } else if sec.starts_with("Xu ") {
            let ms: f64 = line
                .parse::<u64>()
                .map(|x| x as f64)
                .map_err(|_| format!("Bad Xu ms output under '{sec}': {line}"))?;
            let case_name = match sec.as_str() {
                "Xu loop" => "loop",
                "Xu dict" => "dict",
                "Xu dict-intkey" => "dict-intkey",
                "Xu dict-hot" => "dict-hot",
                "Xu string" => "string",
                "Xu string-builder" => "string-builder",
                "Xu struct-method" => "struct-method",
                _ => continue,
            };
            table
                .entry("Xu".to_string())
                .or_default()
                .insert(case_name.to_string(), ms);
        }
    }
    Ok(table)
}

fn parse_perf_output(
    out: &mut std::collections::HashMap<String, u64>,
    output: &Output,
) -> Result<(), String> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let Some(pos) = line.find("PERF|") else {
            continue;
        };
        let rest = &line[pos + "PERF|".len()..];
        let mut it = rest.split('|');
        let Some(test_id) = it.next() else {
            continue;
        };
        for part in it {
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| format!("Bad perf key/value: {line}"))?;
            let val: u64 = v
                .parse()
                .map_err(|_| format!("Bad perf value in: {line}"))?;
            out.insert(format!("{test_id}.{k}"), val);
        }
    }
    Ok(())
}

fn read_baseline(path: &Path) -> Result<std::collections::HashMap<String, u64>, String> {
    if !path.exists() {
        return Ok(std::collections::HashMap::new());
    }
    let s =
        fs::read_to_string(path).map_err(|e| format!("Failed to read baseline {path:?}: {e}"))?;
    let mut out = std::collections::HashMap::new();
    for (idx, line) in s.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .ok_or_else(|| format!("Bad baseline line {}: {line}", idx + 1))?;
        let val: u64 = v
            .parse()
            .map_err(|_| format!("Bad baseline value line {}: {line}", idx + 1))?;
        out.insert(k.to_string(), val);
    }
    Ok(out)
}

fn write_baseline(
    path: &Path,
    values: &std::collections::HashMap<String, u64>,
) -> Result<(), String> {
    let mut keys: Vec<_> = values.keys().cloned().collect();
    keys.sort();
    let mut s = String::new();
    for k in keys {
        let v = values.get(&k).copied().unwrap_or(0);
        s.push_str(&k);
        s.push('=');
        s.push_str(&v.to_string());
        s.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir {parent:?}: {e}"))?;
    }
    fs::write(path, s).map_err(|e| format!("Failed to write baseline {path:?}: {e}"))?;
    Ok(())
}

fn xu_check(path: &Path) -> Result<(), String> {
    let output = run_owned("cargo", &xu_args("check", path))?;
    if !output.status.success() {
        return Err(format!(
            "xu check failed for {}:\n{}",
            path.display(),
            format_output(&output)
        ));
    }
    Ok(())
}

fn xu_check_expect_fail(path: &Path) -> Result<(), String> {
    let output = run_owned("cargo", &xu_args("check", path))?;
    if output.status.success() {
        return Err(format!("unexpected check success: {}", path.display()));
    }
    Ok(())
}

fn xu_ast(path: &Path) -> Result<(), String> {
    let output = run_owned("cargo", &xu_args("ast", path))?;
    if !output.status.success() {
        return Err(format!(
            "xu ast failed for {}:\n{}",
            path.display(),
            format_output(&output)
        ));
    }
    Ok(())
}

fn xu_run(path: &Path) -> Result<(), String> {
    let output = run_owned("cargo", &xu_args("run", path))?;
    if !output.status.success() {
        return Err(format!(
            "xu run failed for {}:\n{}",
            path.display(),
            format_output(&output)
        ));
    }
    Ok(())
}

fn xu_run_expect_fail(path: &Path) -> Result<(), String> {
    let output = run_owned("cargo", &xu_args("run", path))?;
    if output.status.success() {
        return Err(format!("unexpected run success: {}", path.display()));
    }
    Ok(())
}

fn xu_args(subcmd: &str, file: &Path) -> Vec<String> {
    vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "xu_cli".to_string(),
        "--bin".to_string(),
        "xu".to_string(),
        "--".to_string(),
        subcmd.to_string(),
        file.display().to_string(),
    ]
}

fn run_args(cmd: &str, args: &[&str]) -> Result<Output, String> {
    eprintln!(
        "$ {} {}",
        cmd,
        args.iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {cmd}: {e}"))
}

fn run_owned(cmd: &str, args: &[String]) -> Result<Output, String> {
    eprintln!(
        "$ {} {}",
        cmd,
        args.iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run {cmd}: {e}"))
}

fn format_output(o: &Output) -> String {
    let mut s = String::new();
    if !o.stdout.is_empty() {
        s.push_str("stdout:\n");
        s.push_str(&String::from_utf8_lossy(&o.stdout));
        if !s.ends_with('\n') {
            s.push('\n');
        }
    }
    if !o.stderr.is_empty() {
        s.push_str("stderr:\n");
        s.push_str(&String::from_utf8_lossy(&o.stderr));
        if !s.ends_with('\n') {
            s.push('\n');
        }
    }
    if s.is_empty() {
        s.push_str("(no output)\n");
    }
    s
}

fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./:".contains(c))
    {
        return s.to_string();
    }
    format!("{:?}", s)
}
