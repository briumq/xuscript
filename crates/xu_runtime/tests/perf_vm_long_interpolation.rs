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
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    let result = rt.exec_module(&parse.module).unwrap();
    result.output
}

#[test]
#[ignore]
fn perf_vm_long_interpolation() {
    let mut s = String::new();
    s.push_str("func main():\n");
    s.push_str("  println(\"");
    for _ in 0..200 {
        s.push('{');
        s.push_str("gen_id()");
        s.push('}');
    }
    s.push_str("\");\n");
    let t0 = Instant::now();
    let _ = run(&s);
    let t1 = Instant::now();
    println!(
        "PERF|perf_vm_long_interpolation|exec_ms={}",
        (t1 - t0).as_millis()
    );
}
