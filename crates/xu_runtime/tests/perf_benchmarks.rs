use std::collections::HashMap;
use std::fs;

use xu_ir::{Executable, Frontend};
use xu_lexer::normalize_source;
use xu_runtime::Runtime;

fn run_file(path: &std::path::Path) -> String {
    let src = fs::read_to_string(path).unwrap();
    let scale = std::env::var("BENCH_SCALE").unwrap_or_else(|_| "50000".to_string());
    let wrapped = format!(
        "{}\nBENCH_SCALE = \"{}\";\nBENCH_SMOKE = \"0\";\nmain();\n",
        src, scale
    );
    let normalized = normalize_source(&wrapped);
    let driver = xu_driver::Driver::new();
    let compiled = driver
        .compile_text_no_analyze(path.to_string_lossy().as_ref(), normalized.text.as_str())
        .unwrap();
    let mut rt = Runtime::new();

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap();
    let stdlib = root.join("stdlib");
    rt.set_stdlib_path(stdlib.to_string_lossy().to_string());

    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    match compiled.executable {
        Executable::Ast(module) => rt.exec_module(&module).unwrap().output,
        Executable::Bytecode(program) => rt.exec_program(&program).unwrap().output,
    }
}

#[test]
#[ignore]
fn perf_benchmarks_suite() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .unwrap();
    let dir = root.join("tests/benchmarks/xu");
    let output = run_file(&dir.join("full_suite.xu"));
    let scale = std::env::var("BENCH_SCALE").unwrap_or_else(|_| "50000".to_string());

    // Parse JSON output lines: {"case":"name","scale":N,"duration_ms":X.X,"rss_bytes":N}
    let mut results = HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        if line.starts_with('{') && line.ends_with('}') {
            // Simple JSON parsing for our specific format
            if let (Some(case_start), Some(duration_start)) = (line.find("\"case\":\""), line.find("\"duration_ms\":")) {
                let case_end = line[case_start + 8..].find('"').map(|i| case_start + 8 + i);
                if let Some(case_end) = case_end {
                    let case = &line[case_start + 8..case_end];
                    // Find duration value
                    let duration_str = &line[duration_start + 14..];
                    let duration_end = duration_str.find(',').or_else(|| duration_str.find('}')).unwrap_or(duration_str.len());
                    let duration: f64 = duration_str[..duration_end].parse().unwrap_or(0.0);
                    let key = format!("{}_{}", case.replace('-', "_"), scale);
                    results.insert(key, (duration.ceil() as u64).to_string());
                }
            }
        } else if let Some((k, v)) = line.split_once('=') {
            // Fallback to old key=value format
            results.insert(k.trim().to_string(), v.trim().to_string());
        }
    }

    // Construct final string dynamically from available keys
    let mut keys: Vec<String> = results.keys().cloned().collect();
    keys.sort();
    let parts: Vec<String> = keys
        .iter()
        .map(|k| format!("{}={}", k, results.get(k).unwrap_or(&"0".to_string())))
        .collect();

    println!("PERF|xu_bench|{}", parts.join("|"));
}
