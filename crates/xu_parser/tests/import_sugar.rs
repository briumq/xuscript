use xu_lexer::{Lexer, normalize_source};
use xu_parser::{Parser, Stmt, UseStmt};

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
        Stmt::Use(u) => {
            assert_eq!(
                u.as_ref(),
                &UseStmt {
                    path: "./a.xu".to_string(),
                    alias: None
                }
            );
        }
        other => panic!("unexpected stmt: {other:?}"),
    }
}
