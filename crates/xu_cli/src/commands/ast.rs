use std::io::Write;

use xu_driver::Driver;
use xu_syntax::RenderOptions;

use crate::args::CliArgs;
use crate::commands::emit_diagnostics;

pub(crate) fn run(args: &CliArgs, driver: &Driver, render_opts: RenderOptions) {
    if args.positional.len() != 1 {
        eprintln!("Missing <file>");
        std::process::exit(2);
    }
    let path = args.positional[0].as_str();
    let (parsed, tm) = if args.timing {
        match driver.parse_text_timed(
            path,
            &std::fs::read_to_string(path).unwrap_or_default(),
            args.strict.unwrap_or(true),
        ) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(2);
            }
        }
    } else {
        let parsed = match driver.parse_file(path, args.strict.unwrap_or(true)) {
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

    emit_diagnostics(
        &parsed.source,
        &parsed.diagnostics,
        render_opts,
        args.json_out,
    );
    if parsed
        .diagnostics
        .iter()
        .any(|d| matches!(d.severity, xu_syntax::Severity::Error))
    {
        std::process::exit(1);
    }

    let mut out = std::io::stdout().lock();
    if args.timing {
        let _ = writeln!(
            out,
            "TIMING normalize={:.3}ms lex={:.3}ms parse={:.3}ms analyze={:.3}ms",
            (tm.normalize_us as f64) / 1000.0,
            (tm.lex_us as f64) / 1000.0,
            (tm.parse_us as f64) / 1000.0,
            (tm.analyze_us as f64) / 1000.0
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
