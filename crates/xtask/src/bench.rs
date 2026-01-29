use std::fs;
use std::path::PathBuf;
use std::process::Output;

use serde::Deserialize;

use crate::process::{format_output, run_args};

pub fn run_bench_suite() -> Result<(), String> {
    run_bench_once(5000)?;
    run_bench_once(10000)?;
    Ok(())
}

fn run_bench_once(scale: usize) -> Result<(), String> {
    let arg = scale.to_string();
    let o = run_args("bash", &["scripts/run_cross_lang_bench.sh", arg.as_str()])?;
    if o.status.success() {
        return Ok(());
    }
    Err(format!(
        "run_cross_lang_bench.sh {scale} failed:\n{}",
        format_output(&o)
    ))
}

#[derive(Clone, Debug, Deserialize)]
struct BenchJsonLine {
    #[serde(rename = "case")]
    case_name: String,
    scale: usize,
    duration_ms: f64,
}

pub fn bench_report(scales_arg: Option<&str>) -> Result<(), String> {
    let scales = parse_scales(scales_arg)?;

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
        let arg = scale.to_string();
        let out = run_args("bash", &["scripts/run_cross_lang_bench.sh", arg.as_str()])?;
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

        let mut seen_cases: std::collections::BTreeMap<String, ()> = std::collections::BTreeMap::new();
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

fn parse_scales(scales_arg: Option<&str>) -> Result<Vec<usize>, String> {
    let Some(s) = scales_arg else {
        return Ok(vec![5000, 10000]);
    };
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
        Ok(vec![5000, 10000])
    } else {
        Ok(out)
    }
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
            continue;
        }
        if sec.starts_with("Xu ") {
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

