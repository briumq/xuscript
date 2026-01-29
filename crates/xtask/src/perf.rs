use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use crate::bench;
use crate::process::{format_output, run_args};

pub fn perf(mode: Option<&str>) -> Result<(), String> {
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
        bench::bench_report(scales.as_deref())?;
    }

    Ok(())
}

fn run_perf_suite() -> Result<std::collections::HashMap<String, u64>, String> {
    let mut out = std::collections::HashMap::new();
    run_perf_test(&mut out, "perf_lexer_parser")?;
    run_perf_test(&mut out, "perf_runtime_exec")?;
    run_perf_test(&mut out, "perf_benchmarks")?;
    run_perf_test(&mut out, "perf_vm_long_interpolation")?;
    Ok(out)
}

fn run_perf_test(
    out: &mut std::collections::HashMap<String, u64>,
    test_name: &str,
) -> Result<(), String> {
    let o = run_args(
        "cargo",
        &[
            "test",
            "-q",
            "-p",
            "xu_runtime",
            "--test",
            test_name,
            "--release",
            "--",
            "--ignored",
            "--nocapture",
        ],
    )?;
    if !o.status.success() {
        return Err(format!("{test_name} failed:\n{}", format_output(&o)));
    }
    parse_perf_output(out, &o)?;
    Ok(())
}

fn parse_perf_output(out: &mut std::collections::HashMap<String, u64>, output: &Output) -> Result<(), String> {
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
    let s = fs::read_to_string(path).map_err(|e| format!("Failed to read baseline {path:?}: {e}"))?;
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

fn write_baseline(path: &Path, values: &std::collections::HashMap<String, u64>) -> Result<(), String> {
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

