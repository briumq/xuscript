use std::fs;

use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;

#[test]
fn parse_all_examples() {
    let examples_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("inputs");
    let mut entries: Vec<_> = fs::read_dir(&examples_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let ext = p.extension().and_then(|e| e.to_str());
            matches!(ext, Some("xu") | Some("hs"))
        })
        .filter(|p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            !name.contains("error")
        })
        .collect();
    entries.sort();

    for path in entries {
        let src = fs::read_to_string(&path).unwrap();
        let normalized = normalize_source(&src);
        assert!(
            normalized.diagnostics.is_empty(),
            "normalize diagnostics in {:?}: {:?}",
            path,
            normalized.diagnostics
        );
        let lex = Lexer::new(&normalized.text).lex();
        assert!(
            lex.diagnostics.is_empty(),
            "lex diagnostics in {:?}: {:?}",
            path,
            lex.diagnostics
        );
        let bump = bumpalo::Bump::new();
        let parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
        let errors: Vec<_> = parse
            .diagnostics
            .into_iter()
            .filter(|d| matches!(d.severity, xu_syntax::Severity::Error))
            .collect();
        assert!(errors.is_empty(), "parse errors in {:?}: {errors:?}", path);
    }
}
