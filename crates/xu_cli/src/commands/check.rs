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
    let parsed = match driver.parse_file(path, args.strict) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
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
}
