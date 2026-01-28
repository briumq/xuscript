use xu_syntax::{Diagnostic, SourceFile, SourceId, render_diagnostic};

pub(super) fn render_parse_error(path: &str, text: String, diag: &Diagnostic) -> String {
    let source = SourceFile::new(SourceId(0), path, text);
    render_diagnostic(&source, diag)
}
