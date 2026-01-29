use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_runtime::Runtime;
use xu_syntax::Severity;

fn parse_source(src: &str) -> xu_parser::Module {
    let normalized = normalize_source(src);
    let lex = Lexer::new(&normalized.text).lex();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let diags: Vec<_> = normalized
        .diagnostics
        .iter()
        .chain(lex.diagnostics.iter())
        .chain(parse.diagnostics.iter())
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| d.message.clone())
        .collect();
    assert!(diags.is_empty(), "source should parse without errors: {diags:?}");
    parse.module
}

#[test]
fn gc_clears_inline_caches_safely() {
    let src = r#"
func assert(x) { __builtin_assert(x) }

func main() {
  var d: {string: [int]} = {};
  d.insert("x", [1]);
  let a = d.x;
  assert(a[0] is 1);

  d = {};
  gc();

  d.insert("x", [2]);
  let b = d.x;
  assert(b[0] is 2);
}
"#;
    let module = parse_source(src);
    let mut rt = Runtime::new();
    rt.set_frontend(Box::new(xu_driver::Driver::new()));
    rt.exec_module(&module).unwrap();
}
