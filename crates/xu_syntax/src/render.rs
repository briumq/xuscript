use crate::{Diagnostic, SourceFile};

fn floor_char_boundary(text: &str, mut idx: usize) -> usize {
    idx = idx.min(text.len());
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

pub fn render_diagnostic(source: &SourceFile, diag: &Diagnostic) -> String {
    match diag.span {
        Some(span) => {
            let text = source.text.as_str();
            let start = floor_char_boundary(text, span.start.0 as usize);
            let (line, col) = source.text.line_col(start as u32);
            let line_start = text[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let line_end = text[start..]
                .find('\n')
                .map(|i| start + i)
                .unwrap_or(text.len());
            let line_text = &text[line_start..line_end];

            let mut out = String::new();
            let code_str = diag.code.map(|c| format!(" [{c}]")).unwrap_or_default();
            out.push_str(&format!(
                "{:?}{}:{}:{}: {}: {}",
                diag.severity,
                code_str,
                line + 1,
                col + 1,
                source.name,
                diag.message
            ));
            out.push('\n');
            out.push_str("  | ");
            out.push_str(line_text);
            out.push('\n');
            out.push_str("  | ");
            out.extend(std::iter::repeat_n(' ', col as usize));
            out.push('^');
            if let Some(s) = &diag.suggestion {
                out.push('\n');
                out.push_str("  = suggestion: ");
                out.push_str(s);
            }
            for label in &diag.labels {
                let lstart = floor_char_boundary(text, label.span.start.0 as usize);
                let (ll, lc) = source.text.line_col(lstart as u32);
                let lline_start = text[..lstart].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let lline_end = text[lstart..]
                    .find('\n')
                    .map(|i| lstart + i)
                    .unwrap_or(text.len());
                let lline_text = &text[lline_start..lline_end];
                out.push('\n');
                out.push_str("  = note: ");
                out.push_str(&label.message);
                out.push('\n');
                out.push_str("    | ");
                out.push_str(lline_text);
                out.push('\n');
                out.push_str("    | ");
                out.extend(std::iter::repeat_n(' ', lc as usize));
                out.push('^');
                out.push_str(&format!("  ({}:{}:{})", source.name, ll + 1, lc + 1));
            }
            if let Some(h) = &diag.help {
                out.push('\n');
                out.push_str("  = help: ");
                out.push_str(h);
            }
            out
        }
        None => {
            let code_str = diag.code.map(|c| format!(" [{c}]")).unwrap_or_default();
            let mut out = format!(
                "{:?}{}: {}: {}",
                diag.severity, code_str, source.name, diag.message
            );
            if let Some(s) = &diag.suggestion {
                out.push('\n');
                out.push_str("  = suggestion: ");
                out.push_str(s);
            }
            for label in &diag.labels {
                let text = source.text.as_str();
                let lstart = floor_char_boundary(text, label.span.start.0 as usize);
                let (ll, lc) = source.text.line_col(lstart as u32);
                let lline_start = text[..lstart].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let lline_end = text[lstart..]
                    .find('\n')
                    .map(|i| lstart + i)
                    .unwrap_or(text.len());
                let lline_text = &text[lline_start..lline_end];
                out.push('\n');
                out.push_str("  = note: ");
                out.push_str(&label.message);
                out.push('\n');
                out.push_str("    | ");
                out.push_str(lline_text);
                out.push('\n');
                out.push_str("    | ");
                out.extend(std::iter::repeat_n(' ', lc as usize));
                out.push('^');
                out.push_str(&format!("  ({}:{}:{})", source.name, ll + 1, lc + 1));
            }
            if let Some(h) = &diag.help {
                out.push('\n');
                out.push_str("  = help: ");
                out.push_str(h);
            }
            out
        }
    }
}

pub fn render_diagnostics(source: &SourceFile, diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for (idx, d) in diagnostics.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
        }
        out.push_str(&render_diagnostic(source, d));
    }
    out
}
