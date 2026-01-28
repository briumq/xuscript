use xu_lexer::{Lexer, normalize_source};
use xu_parser::{Expr, Parser, Stmt};
use xu_syntax::Severity;

#[test]
fn interpolation_reuses_expr_and_keeps_ast_shape() {
    let src = r#"
a = 1;
s = "x{a}y{a}z";
"#;
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
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert!(errors.is_empty(), "{errors:?}");

    let stmt = parse
        .module
        .stmts
        .iter()
        .find_map(|s| match s {
            Stmt::Assign(a) => match &a.value {
                Expr::InterpolatedString(parts) => Some(parts.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("expected interpolated string assignment");

    assert!(matches!(&stmt[0], Expr::Str(s) if s == "x"));
    assert!(matches!(&stmt[1], Expr::Ident(s, _) if s == "a"));
    assert!(matches!(&stmt[2], Expr::Str(s) if s == "y"));
    assert!(matches!(&stmt[3], Expr::Ident(s, _) if s == "a"));
    assert!(matches!(&stmt[4], Expr::Str(s) if s == "z"));
}
