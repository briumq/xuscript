use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;

struct MockClock;
impl xu_runtime::runtime::Clock for MockClock {
    fn unix_secs(&self) -> i64 {
        1234567890
    }
    fn unix_millis(&self) -> i64 {
        1234567890123
    }
    fn mono_micros(&self) -> i64 {
        42
    }
    fn mono_nanos(&self) -> i64 {
        42000
    }
}

struct MockRng;
impl xu_runtime::runtime::RngAlgorithm for MockRng {
    fn next_u64(&self, state: &mut u64) -> u64 {
        *state = state.wrapping_add(1);
        *state
    }
}

fn parse(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    assert!(normalized.diagnostics.is_empty());
    let lex = Lexer::new(&normalized.text).lex();
    assert!(lex.diagnostics.is_empty());
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let errors: Vec<_> = parse
        .diagnostics
        .into_iter()
        .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");
    parse.module
}

#[test]
fn mock_clock_and_rng_are_used() {
    let module = parse("println(time_unix());\nprintln(time_millis());\nprintln(rand(10));\n");
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.set_clock(Box::new(MockClock));
    rt.set_rng_algorithm(Box::new(MockRng));
    rt.set_rng_seed(0);
    let res = rt.exec_module(&module).unwrap();
    let out = res.output.trim_end();
    assert_eq!(out, "1234567890\n1234567890123\n1");
}
