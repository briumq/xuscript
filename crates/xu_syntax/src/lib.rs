//!
//!
mod builtins;
mod diagnostic;
mod loc;
mod render;
mod source;
mod span;
mod str_util;
mod token;
mod types;
mod util;

pub use builtins::{BUILTIN_NAMES, builtin_return_type};
pub use diagnostic::{Diagnostic, Severity, codes};
pub use loc::{DiagnosticKind, DiagnosticsFormatter};
pub use render::{render_diagnostic, render_diagnostics};
pub use source::{SourceFile, SourceId, SourceText};
pub use span::{ByteIndex, Span};
pub use str_util::{InterpolationParser, InterpolationPiece, unescape, unquote};
pub use token::{Token, TokenKind};
pub use types::{Type, TypeId, TypeInterner};
pub use util::{find_best_match, is_cjk, is_ident_continue, is_ident_start, levenshtein_distance};
