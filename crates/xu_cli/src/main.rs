use std::io::Write;

use xu_driver::Driver;
use xu_runtime::Runtime;
use xu_syntax::{TokenKind, render_diagnostic};

const USAGE: &str =
    "Usage: xu <tokens|check|ast|run> [--nonstrict] [--timing] [--no-diags] <args>";

fn main() {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first().cloned() else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    argv.remove(0);
    let mut strict = true;
    let mut timing = false;
    let mut no_diags = false;
    let mut positional: Vec<String> = Vec::new();

    let mut i = 0;
    while i < argv.len() {
        let a = &argv[i];
        if a == "--strict" {
            strict = true;
        } else if a == "--nonstrict" {
            strict = false;
        } else if a == "--timing" {
            timing = true;
        } else if a == "--no-diags" {
            no_diags = true;
        } else {
            positional.push(a.clone());
        }
        i += 1;
    }

    let driver = Driver::new();

    match cmd.as_str() {
        "tokens" => {
            if positional.len() != 1 {
                eprintln!("Missing <file>");
                std::process::exit(2);
            }
            let path = positional[0].as_str();
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
                if let Err(e) =
                    writeln!(out, "{:?}\t{:?}\t{}", t.kind, t.span, escape_visible(text))
                {
                    if e.kind() == std::io::ErrorKind::BrokenPipe {
                        return;
                    }
                    eprintln!("stdout error: {e}");
                    std::process::exit(2);
                }
            }
        }
        "check" => {
            if positional.len() != 1 {
                eprintln!("Missing <file>");
                std::process::exit(2);
            }
            let path = positional[0].as_str();
            let parsed = match driver.parse_file(path, strict) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            };
            for d in &parsed.diagnostics {
                eprintln!("{}", render_diagnostic(&parsed.source, d));
            }
            if parsed
                .diagnostics
                .iter()
                .any(|d| matches!(d.severity, xu_syntax::Severity::Error))
            {
                std::process::exit(1);
            }
        }
        "ast" => {
            if positional.len() != 1 {
                eprintln!("Missing <file>");
                std::process::exit(2);
            }
            let path = positional[0].as_str();
            let (parsed, tm) = if timing {
                match driver.parse_text_timed(
                    path,
                    &std::fs::read_to_string(path).unwrap_or_default(),
                    strict,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(2);
                    }
                }
            } else {
                let parsed = match driver.parse_file(path, strict) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("{e}");
                        std::process::exit(2);
                    }
                };
                (
                    parsed,
                    xu_driver::Timings {
                        normalize_us: 0,
                        lex_us: 0,
                        parse_us: 0,
                        analyze_us: 0,
                    },
                )
            };
            for d in &parsed.diagnostics {
                eprintln!("{}", render_diagnostic(&parsed.source, d));
            }
            if parsed
                .diagnostics
                .iter()
                .any(|d| matches!(d.severity, xu_syntax::Severity::Error))
            {
                std::process::exit(1);
            }
            let mut out = std::io::stdout().lock();
            if timing {
                let _ = writeln!(
                    out,
                    "TIMING normalize={:.3}ms lex={:.3}ms parse={:.3}ms analyze={:.3}ms",
                    (tm.normalize_us as f64) / 1000.0,
                    (tm.lex_us as f64) / 1000.0,
                    (tm.parse_us as f64) / 1000.0,
                    (tm.analyze_us as f64) / 1000.0,
                );
            }
            if let Err(e) = writeln!(out, "{:#?}", parsed.module) {
                if e.kind() == std::io::ErrorKind::BrokenPipe {
                    return;
                }
                eprintln!("stdout error: {e}");
                std::process::exit(2);
            }
        }
        "run" => {
            if positional.is_empty() {
                eprintln!("Missing <file>");
                std::process::exit(2);
            }
            let path = positional[0].as_str();
            let compiled = match driver.compile_file(path, strict) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            };
            if !no_diags {
                for d in &compiled.diagnostics {
                    eprintln!("{}", render_diagnostic(&compiled.source, d));
                }
            }
            // if diagnostics
            //     .iter()
            //     .any(|d| matches!(d.severity, xu_syntax::Severity::Error))
            // {
            //     std::process::exit(1);
            // }

            let mut rt = Runtime::new();
            rt.set_strict_vars(strict);
            rt.set_frontend(Box::new(Driver::new()));
            rt.set_entry_path(path).expect("set entry path");

            // Set stdlib path
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(project_root) = exe_path
                    .parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
                {
                    let stdlib = project_root.join("stdlib");
                    if stdlib.exists() {
                        rt.set_stdlib_path(stdlib.to_string_lossy().to_string());
                    }
                }
            }
            // If current_exe logic fails, try relative to CWD
            if std::path::Path::new("stdlib").exists() {
                rt.set_stdlib_path(
                    std::env::current_dir()
                        .unwrap()
                        .join("stdlib")
                        .to_string_lossy()
                        .to_string(),
                );
            }

            rt.set_args(positional.clone());

            let result = rt.exec_executable(&compiled.executable);
            let output = match &result {
                Ok(res) => res.output.clone(),
                Err(_) => rt.take_output(),
            };

            let mut stdout = std::io::stdout().lock();
            let _ = write!(stdout, "{}", output);

            match result {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("RuntimeError: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("Unknown command: {cmd}");
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
