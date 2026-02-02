use xu_lexer::{Lexer, normalize_source};
use xu_parser::{Expr, Parser, Stmt};

#[test]
fn parse_enum_ctor_with_args() {
    let src = "x = Option#some(1, 2);\n";
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
        Stmt::Assign(s) => match &s.value {
            Expr::EnumCtor { module, ty, variant, args } => {
                assert!(module.is_none());
                assert_eq!(ty, "Option");
                assert_eq!(variant, "some");
                assert!(matches!(args.as_ref(), [Expr::Int(1), Expr::Int(2)]));
            }
            other => panic!("unexpected assign value: {other:?}"),
        },
        other => panic!("unexpected stmt: {other:?}"),
    }
}
