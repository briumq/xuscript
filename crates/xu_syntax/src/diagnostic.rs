//!
//!

use crate::{DiagnosticKind, DiagnosticsFormatter, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub message: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub code: Option<&'static str>,
    pub suggestion: Option<String>,
    pub span: Option<Span>,
    pub labels: Vec<Label>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(severity: Severity, kind: DiagnosticKind, span: Option<Span>) -> Self {
        Self {
            severity,
            message: DiagnosticsFormatter::format(&kind),
            code: None,
            suggestion: None,
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn error(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            code: None,
            suggestion: None,
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn error_kind(kind: DiagnosticKind, span: Option<Span>) -> Self {
        Self::new(Severity::Error, kind, span)
    }

    pub fn warning(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            code: None,
            suggestion: None,
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn warning_kind(kind: DiagnosticKind, span: Option<Span>) -> Self {
        Self::new(Severity::Warning, kind, span)
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_label(mut self, message: impl Into<String>, span: Span) -> Self {
        self.labels.push(Label {
            message: message.into(),
            span,
        });
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

pub mod codes {
    pub const UNDEFINED_IDENTIFIER: &str = "E0001";
    pub const ARGUMENT_COUNT_MISMATCH: &str = "E0002";
    pub const TYPE_MISMATCH: &str = "E0003";
    pub const CIRCULAR_IMPORT: &str = "E0004";
    pub const UNREACHABLE_CODE: &str = "W0001";
}
