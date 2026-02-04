use std::time::Instant;
use xu_driver::Driver;
use xu_ir::Frontend;
use xu_runtime::Runtime;

fn run(src: &str) -> String {
    let compiled = Driver::new()
        .compile_text_no_analyze("test.xu", src)
        .unwrap();
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(Driver::new()));
    let result = rt.exec_executable(&compiled.executable).unwrap();
    result.output
}

#[test]
#[ignore]
fn perf_runtime_loop_accumulate() {
    let scale: usize = std::env::var("BENCH_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);
    let src = format!(
        r#"
func main() {{
  var count = 0
  for _ in [1..{scale}] {{
    count += 1
  }}
  println(count)
}}
"#
    );
    let t0 = Instant::now();
    let out = run(&src);
    let t1 = Instant::now();
    println!(
        "PERF|perf_runtime_loop_accumulate|exec_ms={}",
        (t1 - t0).as_millis()
    );
    assert!(out.trim_end() == scale.to_string());
}

#[test]
#[ignore]
fn perf_runtime_bulk_dict_ops() {
    let scale: usize = std::env::var("BENCH_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);
    let src = format!(
        r#"
func main() {{
  let d: {{string: int}} = {{}}
  for _ in [1..{scale}] {{
    let k = "k" + to_text(gen_id())
    d.insert(k, 1)
  }}
  println(d.length)
}}
"#
    );
    let t0 = Instant::now();
    let out = run(&src);
    let t1 = Instant::now();
    println!(
        "PERF|perf_runtime_bulk_dict_ops|exec_ms={}",
        (t1 - t0).as_millis()
    );
    assert!(out.trim_end() == scale.to_string());
}
