#![allow(dead_code)]

use xu_syntax::{Diagnostic, DiagnosticKind, Span};

#[derive(Clone, Copy, Debug)]
pub enum AnalyzePass {
    Symbols,
    Types,
    ControlFlow,
    ConstFold,
}

pub fn build_error_kind(kind: DiagnosticKind, span: Option<Span>) -> Diagnostic {
    Diagnostic::error_kind(kind, span)
}

pub fn build_warning_kind(kind: DiagnosticKind, span: Option<Span>) -> Diagnostic {
    Diagnostic::warning_kind(kind, span)
}
