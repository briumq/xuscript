use xu_lexer::{Lexer, normalize_source};
use xu_parser::{Expr, Parser, Stmt};

#[test]
fn parse_import_sugar_lowering() {
    let src = "use \"./a.xu\";\n";
    let normalized = normalize_source(src);
    assert!(
        normalized.diagnostics.is_empty(),
        "{:?}",
        normalized.diagnostics
    );
    let lex = Lexer::new(&normalized.text).lex();
    assert!(lex.diagnostics.is_empty(), "{:?}", lex.diagnostics);
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
    let errors: Vec<_> = parse
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    assert_eq!(parse.module.stmts.len(), 1);
    match &parse.module.stmts[0] {
        Stmt::Expr(Expr::Call(c)) => {
            assert!(matches!(c.callee.as_ref(), Expr::Ident(name, _) if name == "import"));
            assert!(matches!(c.args.as_ref(), [Expr::Str(p)] if p == "./a.xu"));
        }
        other => panic!("unexpected stmt: {other:?}"),
    }
}
