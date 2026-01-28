use std::fs;
use std::path::PathBuf;

use xu_lexer::{Lexer, normalize_source};
use xu_parser::Parser;
use xu_syntax::{Severity, SourceFile, SourceId, TokenKind};

fn input_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("inputs")
        .join(name)
}

fn load_source(path: &PathBuf) -> (SourceFile, String) {
    let input = fs::read_to_string(path).unwrap();
    let normalized = normalize_source(&input);
    let source = SourceFile::new(SourceId(0), path.to_string_lossy(), normalized.text);
    (source, input)
}

fn format_tokens(source: &SourceFile, tokens: &[xu_syntax::Token]) -> String {
    fn escape_visible(s: &str) -> String {
        let mut out = String::new();
        for c in s.chars() {
            match c {
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                _ => out.push(c),
            }
        }
        out
    }
    let mut s = String::new();
    for t in tokens {
        if matches!(t.kind, TokenKind::Newline) {
            continue;
        }
        let text = source.text.slice(t.span);
        s.push_str(&format!(
            "{:?}\t{:?}\t{}\n",
            t.kind,
            t.span,
            escape_visible(text)
        ));
    }
    s
}

fn format_diagnostics(source: &SourceFile, mut diagnostics: Vec<xu_syntax::Diagnostic>) -> String {
    diagnostics.sort_by_key(|d| d.span.map(|sp| sp.start.0).unwrap_or(0));
    let mut s = String::new();
    for d in diagnostics {
        match d.span {
            Some(span) => {
                let (line, col) = source.text.line_col(span.start.0);
                s.push_str(&format!(
                    "{:?}:{}:{}: {}\n",
                    d.severity,
                    line + 1,
                    col + 1,
                    d.message
                ));
            }
            None => {
                s.push_str(&format!("{:?}: {}\n", d.severity, d.message));
            }
        }
    }
    s
}

fn golden_path(kind: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(kind)
        .join(format!("{name}.txt"))
}

fn golden_update_enabled() -> bool {
    let v = std::env::var("XU_UPDATE_GOLDEN").ok();
    if v.as_deref().is_some_and(|v| v == "1" || v == "true") {
        return true;
    }
    std::env::var("HAOSCRIPT_UPDATE_GOLDEN").is_ok_and(|v| v == "1" || v == "true")
}

fn assert_or_update(path: PathBuf, actual: &str) {
    let update = golden_update_enabled();
    if update {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, actual).unwrap();
        return;
    }
    let expected = fs::read_to_string(&path).unwrap();
    assert_eq!(actual.trim_end(), expected.trim_end());
}

fn normalize_ast_snapshot(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.lines();
    while let Some(line) = it.next() {
        if line.trim() == "Cell {" {
            let indent = line.split("Cell").next().unwrap_or("");
            while let Some(next) = it.next() {
                if next.trim() == "}," {
                    break;
                }
            }
            out.push_str(indent);
            out.push_str("None,");
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[test]
fn golden_ast_01_basics() {
    let path = input_path("01_basics.xu");
    let (source, _) = load_source(&path);
    let lex = Lexer::new(source.text.as_str()).lex();
    assert!(lex.diagnostics.is_empty());
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(source.text.as_str(), &lex.tokens, &bump).parse();
    let errors: Vec<_> = parse
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert!(errors.is_empty());
    let actual = normalize_ast_snapshot(format!("{:#?}", parse.module));
    assert_or_update(golden_path("ast", "01_basics"), &actual);
}

#[test]
fn golden_ast_02_control_flow() {
    let path = input_path("02_control_flow.xu");
    let (source, _) = load_source(&path);
    let lex = Lexer::new(source.text.as_str()).lex();
    assert!(lex.diagnostics.is_empty());
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(source.text.as_str(), &lex.tokens, &bump).parse();
    let errors: Vec<_> = parse
        .diagnostics
        .iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect();
    assert!(errors.is_empty());
    let actual = normalize_ast_snapshot(format!("{:#?}", parse.module));
    assert_or_update(golden_path("ast", "02_control_flow"), &actual);
}

#[test]
fn golden_tokens_01_basics() {
    let path = input_path("01_basics.xu");
    let (source, _) = load_source(&path);
    let lex = Lexer::new(source.text.as_str()).lex();
    let actual = format_tokens(&source, &lex.tokens);
    assert_or_update(golden_path("tokens", "01_basics"), &actual);
}

#[test]
fn golden_tokens_02_control_flow() {
    let path = input_path("02_control_flow.xu");
    let (source, _) = load_source(&path);
    let lex = Lexer::new(source.text.as_str()).lex();
    let actual = format_tokens(&source, &lex.tokens);
    assert_or_update(golden_path("tokens", "02_control_flow"), &actual);
}

#[test]
fn golden_diagnostics_08_error_mixed_punct() {
    let path = input_path("08_error_mixed_punct.xu");
    let (source, _) = load_source(&path);
    let lex = Lexer::new(source.text.as_str()).lex();
    let bump = bumpalo::Bump::new();
    let parse = Parser::new(source.text.as_str(), &lex.tokens, &bump).parse();
    let mut diags = Vec::new();
    diags.extend(lex.diagnostics);
    diags.extend(parse.diagnostics);
    let actual = format_diagnostics(&source, diags);
    assert_or_update(golden_path("diagnostics", "08_error_mixed_punct"), &actual);
}
