use std::time::Instant;
use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;

fn make_source(lines: usize) -> String {
    let mut s = String::new();
    for i in 0..lines {
        s.push_str(&format!("x{i} = {i};\n"));
    }
    s
}

#[test]
#[ignore]
fn perf_lexer_parser_large_sequential_assign() {
    let scale: usize = std::env::var("BENCH_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);
    let input = make_source(scale);
    let t0 = Instant::now();
    let normalized = normalize_source(&input);
    let t1 = Instant::now();
    let lex = Lexer::new(&normalized.text).lex();
    let t2 = Instant::now();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let t3 = Instant::now();

    println!(
        "PERF|perf_lexer_parser_large_sequential_assign|normalize_ms={}|lex_ms={}|parse_ms={}",
        (t1 - t0).as_millis(),
        (t2 - t1).as_millis(),
        (t3 - t2).as_millis()
    );

    assert!(
        parse
            .diagnostics
            .iter()
            .all(|d| !matches!(d.severity, xu_syntax::Severity::Error))
    );
}

#[test]
#[ignore]
fn perf_parse_interpolated_string_many_repeated_exprs() {
    let scale: usize = std::env::var("BENCH_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);
    let mut s = String::new();
    s.push_str("x = \"");
    for i in 0..scale {
        s.push('{');
        s.push('a');
        s.push_str(&i.to_string());
        s.push('}');
    }
    s.push_str("\";\n");
    s.push_str("println(x);\n");

    let t0 = Instant::now();
    let normalized = normalize_source(&s);
    let t1 = Instant::now();
    let lex = Lexer::new(&normalized.text).lex();
    let t2 = Instant::now();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let t3 = Instant::now();

    println!(
        "PERF|perf_parse_interpolated_string_many_repeated_exprs|normalize_ms={}|lex_ms={}|parse_ms={}",
        (t1 - t0).as_millis(),
        (t2 - t1).as_millis(),
        (t3 - t2).as_millis()
    );

    assert!(
        parse
            .diagnostics
            .iter()
            .all(|d| !matches!(d.severity, xu_syntax::Severity::Error))
    );
}
