use std::fs;
use std::panic;

use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;

fn mutate_sources(src: &str) -> Vec<String> {
    let chars: Vec<char> = src.chars().collect();
    if chars.is_empty() {
        return vec!["".to_string()];
    }

    let mut out = Vec::new();
    out.push(src.to_string());

    let mid = chars.len() / 2;
    let mut removed = chars.clone();
    removed.remove(mid);
    out.push(removed.iter().collect());

    let mut replaced = chars.clone();
    replaced[mid] = '€';
    out.push(replaced.iter().collect());

    let mut inserted = chars.clone();
    inserted.insert(mid, '\t');
    out.push(inserted.iter().collect());

    let mut fullwidth_space = chars.clone();
    fullwidth_space.insert(mid, '\u{00A0}');
    out.push(fullwidth_space.iter().collect());

    let mut unterminated = src.to_string();
    unterminated.push('"');
    out.push(unterminated);

    out
}

#[test]
fn lexer_and_parser_do_not_panic_on_mutations() {
    let inputs_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("inputs");
    let mut entries: Vec<_> = fs::read_dir(&inputs_dir)
        .unwrap_or_else(|_| panic!("Failed to read inputs dir: {:?}", inputs_dir))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("xu"))
        .collect();
    entries.sort();

    if entries.is_empty() {
        // println!("No .xu files found in {:?}", inputs_dir);
    }

    for path in entries {
        let src = fs::read_to_string(&path).unwrap();
        for variant in mutate_sources(&src) {
            let result = panic::catch_unwind(|| {
                let normalized = normalize_source(&variant);
                let lex = Lexer::new(&normalized.text).lex();
                let bump = bumpalo::Bump::new();
                let _parse = Parser::new(&normalized.text, &lex.tokens, &bump).parse();
            });
            assert!(result.is_ok(), "panic in {:?}", path);
        }
    }
}
