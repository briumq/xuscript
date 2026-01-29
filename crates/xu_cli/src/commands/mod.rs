use serde_json::json;
use xu_syntax::{Diagnostic, RenderOptions, SourceFile, render_diagnostic_with_options};

pub(crate) mod ast;
pub(crate) mod check;
pub(crate) mod run;
pub(crate) mod tokens;
pub(crate) mod common;

pub(crate) fn emit_diagnostics(
    source: &SourceFile,
    diagnostics: &[Diagnostic],
    render_opts: RenderOptions,
    json_out: bool,
) {
    for d in diagnostics {
        if json_out {
            let span = d.span.map(|s| json!({ "start": s.start.0, "end": s.end.0 }));
            let obj = json!({
                "severity": match d.severity { xu_syntax::Severity::Error => "error", xu_syntax::Severity::Warning => "warning" },
                "code": d.code,
                "message": d.message,
                "span": span,
                "file": source.name,
            });
            println!("{}", obj);
        } else {
            eprintln!("{}", render_diagnostic_with_options(source, d, render_opts));
        }
    }
}
