//!
//!

use crate::{DiagnosticKind, DiagnosticsFormatter, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
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

    pub fn info(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            severity: Severity::Info,
            message: message.into(),
            code: None,
            suggestion: None,
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    pub fn info_kind(kind: DiagnosticKind, span: Option<Span>) -> Self {
        Self::new(Severity::Info, kind, span)
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
    // === Errors (E) ===

    // 0xxx - General / Identifiers
    pub const UNDEFINED_IDENTIFIER: &str = "E0001";

    // 1xxx - Type System
    pub const TYPE_MISMATCH: &str = "E1001";
    pub const ARGUMENT_COUNT_MISMATCH: &str = "E1002";
    pub const RETURN_TYPE_MISMATCH: &str = "E1003";
    pub const INVALID_CONDITION_TYPE: &str = "E1004";
    pub const INVALID_ITERATOR_TYPE: &str = "E1005";
    pub const INVALID_UNARY_OPERAND: &str = "E1006";

    // 2xxx - Syntax / Parsing
    pub const EXPECTED_TOKEN: &str = "E2001";
    pub const EXPECTED_EXPRESSION: &str = "E2002";
    pub const INVALID_ASSIGNMENT_TARGET: &str = "E2003";
    pub const UNTERMINATED_STRING: &str = "E2004";
    pub const UNTERMINATED_BLOCK_COMMENT: &str = "E2005";
    pub const UNEXPECTED_CHAR: &str = "E2006";
    pub const UNCLOSED_DELIMITER: &str = "E2007";
    pub const KEYWORD_AS_IDENTIFIER: &str = "E2008";

    // 3xxx - Runtime
    pub const INDEX_OUT_OF_RANGE: &str = "E3001";
    pub const DIVISION_BY_ZERO: &str = "E3002";
    pub const KEY_NOT_FOUND: &str = "E3003";
    pub const INTEGER_OVERFLOW: &str = "E3004";
    pub const RECURSION_LIMIT_EXCEEDED: &str = "E3005";
    pub const NOT_CALLABLE: &str = "E3006";

    // 4xxx - Import / Module
    pub const CIRCULAR_IMPORT: &str = "E4001";
    pub const IMPORT_FAILED: &str = "E4002";
    pub const FILE_NOT_FOUND: &str = "E4003";
    pub const PATH_NOT_ALLOWED: &str = "E4004";

    // 5xxx - Methods / Members
    pub const UNKNOWN_STRUCT: &str = "E5001";
    pub const UNKNOWN_MEMBER: &str = "E5002";
    pub const UNKNOWN_ENUM_VARIANT: &str = "E5003";
    pub const UNSUPPORTED_METHOD: &str = "E5004";
    pub const INVALID_MEMBER_ACCESS: &str = "E5005";

    // === Warnings (W) ===

    // 0xxx - Code Quality
    pub const UNREACHABLE_CODE: &str = "W0001";
    pub const SHADOWING: &str = "W0002";
    pub const VOID_ASSIGNMENT: &str = "W0003";
}
