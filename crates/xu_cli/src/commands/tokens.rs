use std::io::Write;

use xu_driver::Driver;
use xu_syntax::TokenKind;

use crate::args::CliArgs;

pub(crate) fn run(args: &CliArgs, driver: &Driver) {
    if args.positional.len() != 1 {
        eprintln!("Missing <file>");
        std::process::exit(2);
    }
    let path = args.positional[0].as_str();
    let lexed = match driver.lex_file(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let mut out = std::io::stdout().lock();
    for t in &lexed.tokens {
        if matches!(t.kind, TokenKind::Newline) {
            continue;
        }
        let text = lexed.source.text.slice(t.span);
        if let Err(e) = writeln!(out, "{:?}\t{:?}\t{}", t.kind, t.span, escape_visible(text)) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                return;
            }
            eprintln!("stdout error: {e}");
            std::process::exit(2);
        }
    }
}

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
