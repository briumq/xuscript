use std::time::Instant;
use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;

fn run(src: &str) -> String {
    let normalized = normalize_source(src);
    let lex = Lexer::new(&normalized.text).lex();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let mut rt = Runtime::new();
    let result = rt.exec_module(&parse.module).unwrap();
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
func main():
  count = 0;
  for _ in [1..{scale}]:
    count += 1;
  println(count);
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
func main():
  d = {{}};
  for _ in [1..{scale}]:
    k = "k" + to_text(gen_id());
    d.insert(k, 1);
  println(d.length);
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
