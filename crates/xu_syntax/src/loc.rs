pub enum DiagnosticKind {
    // Lexer
    TabNotAllowed,
    FullWidthSpaceNotAllowed,
    UnterminatedBlockComment,
    UnmatchedDelimiter(char),
    DotNotTerminator,
    UnterminatedString,
    UnexpectedChar(char),
    UnclosedDelimiter(char),

    // Parser
    ExpectedToken(String),
    ExpectedExpression,
    InvalidAssignmentTarget,
    ExpectedImportPath,
    TrailingInterpolationTokens,
    KeywordAsIdentifier(String),

    // Analyzer - Errors
    UnreachableCode,
    VoidAssignment,

    // Analyzer - Warnings
    Shadowing(String),
    TypeMismatch {
        expected: String,
        actual: String,
    },
    ArgumentCountMismatch {
        expected_min: usize,
        expected_max: usize,
        actual: usize,
    },
    UndefinedIdentifier(String),
    DidYouMean(String),

    // Runtime
    IndexOutOfRange,
    KeyNotFound(String),
    FileNotOpen,
    CircularImport(Vec<String>),
    TopLevelBreakContinue,
    DivisionByZero,
    IntegerOverflow,
    NotCallable(String),
    UnknownMember(String),
    UnknownStruct(String),
    UnknownEnumVariant(String, String),
    ImportFailed(String),
    FileNotFound(String),
    PathNotAllowed,
    RecursionLimitExceeded,
    InvalidConditionType(String),
    InvalidIteratorType {
        expected: String,
        actual: String,
        iter_desc: String,
    },
    InvalidUnaryOperand {
        op: char,
        expected: String,
    },
    TypeMismatchDetailed {
        name: String,
        param: String,
        expected: String,
        actual: String,
    },
    ReturnTypeMismatch {
        expected: String,
        actual: String,
    },
    UnexpectedControlFlowInFunction(&'static str),
    InvalidMemberAccess {
        field: String,
        ty: String,
    },
    InvalidIndexAccess {
        expected: String,
        actual: String,
    },
    ListIndexRequired,
    DictKeyRequired,
    InsertKeyRequired,
    GetKeyRequired,
    FormatDictRequired,
    SplitParamRequired,
    ReplaceParamRequired,
    JoinParamRequired,
    UnsupportedMethod {
        method: String,
        ty: String,
    },
    UnknownListMethod(String),
    UnknownDictMethod(String),
    UnknownFileMethod(String),
    UnknownStrMethod(String),
    ParseIntError(String),
    ParseFloatError(String),
    FileClosed,
    UnsupportedReceiver(String),

    // Custom
    Raw(String),
}

pub struct DiagnosticsFormatter;

impl DiagnosticsFormatter {
    fn format_en(kind: &DiagnosticKind) -> String {
        match kind {
            DiagnosticKind::TabNotAllowed => "Tab is not allowed; use ASCII spaces".into(),
            DiagnosticKind::FullWidthSpaceNotAllowed => {
                "Full-width space is not allowed; use ASCII spaces".into()
            }
            DiagnosticKind::UnterminatedBlockComment => "Unterminated block comment".into(),
            DiagnosticKind::UnmatchedDelimiter(c) => format!("Unmatched '{}'", c),
            DiagnosticKind::DotNotTerminator => "Dot is not a statement terminator".into(),
            DiagnosticKind::UnterminatedString => "Unterminated string literal".into(),
            DiagnosticKind::UnexpectedChar(c) => format!("Unexpected character: {}", c),
            DiagnosticKind::UnclosedDelimiter(c) => format!("Unclosed '{}'", c),

            DiagnosticKind::UnreachableCode => "Unreachable code".into(),
            DiagnosticKind::VoidAssignment => "Cannot assign void to a variable".into(),
            DiagnosticKind::Shadowing(name) => format!("Variable '{}' shadows an existing binding", name),
            DiagnosticKind::DidYouMean(s) => format!("Did you mean '{}'?", s),

            DiagnosticKind::ExpectedToken(s) => format!("Expected {}", s),
            DiagnosticKind::ExpectedExpression => "Expected expression".into(),
            DiagnosticKind::InvalidAssignmentTarget => "Invalid assignment target".into(),
            DiagnosticKind::ExpectedImportPath => {
                "Expected string literal or argument list after import".into()
            }
            DiagnosticKind::TrailingInterpolationTokens => {
                "Interpolation expression has trailing tokens".into()
            }
            DiagnosticKind::KeywordAsIdentifier(kw) => {
                format!("Keyword '{}' cannot be used as an identifier", kw)
            }

            DiagnosticKind::UndefinedIdentifier(name) => format!("Undefined identifier: {}", name),
            DiagnosticKind::TypeMismatch { expected, actual } => {
                format!("Type mismatch: expected {} but got {}", expected, actual)
            }
            DiagnosticKind::ArgumentCountMismatch {
                expected_min,
                expected_max,
                actual,
            } => {
                if expected_min == expected_max {
                    format!(
                        "Argument count mismatch: expected {} but got {}",
                        expected_min, actual
                    )
                } else {
                    format!(
                        "Argument count mismatch: expected {}..{} but got {}",
                        expected_min, expected_max, actual
                    )
                }
            }
            DiagnosticKind::IndexOutOfRange => "Index out of range".into(),
            DiagnosticKind::KeyNotFound(key) => format!("Key not found: {}", key),
            DiagnosticKind::FileNotOpen => "File is not open".into(),
            DiagnosticKind::CircularImport(chain) => {
                format!("Circular import: {}", chain.join(" -> "))
            }
            DiagnosticKind::TopLevelBreakContinue => {
                "Break or continue is not allowed at top level".into()
            }
            DiagnosticKind::DivisionByZero => "Division by zero".into(),
            DiagnosticKind::IntegerOverflow => "Integer overflow".into(),
            DiagnosticKind::NotCallable(name) => format!("'{}' is not callable", name),
            DiagnosticKind::UnknownMember(name) => format!("Unknown member: {}", name),
            DiagnosticKind::UnknownStruct(name) => format!("Unknown struct type: {}", name),
            DiagnosticKind::UnknownEnumVariant(ty, var) => {
                format!("Unknown enum variant: {}#{}", ty, var)
            }
            DiagnosticKind::ImportFailed(msg) => format!("Import failed: {}", msg),
            DiagnosticKind::FileNotFound(path) => format!("File not found: {}", path),
            DiagnosticKind::PathNotAllowed => "Path is not within allowed roots".into(),
            DiagnosticKind::RecursionLimitExceeded => "Recursion limit exceeded".into(),
            DiagnosticKind::InvalidConditionType(actual) => {
                format!("Condition must be of type ?, but got {}", actual)
            }
            DiagnosticKind::InvalidIteratorType {
                expected,
                actual,
                iter_desc,
            } => format!(
                "Iteration requires {} or {} type, but got {} (iter={})",
                expected, "Range", actual, iter_desc
            ),
            DiagnosticKind::InvalidUnaryOperand { op, expected } => {
                format!("Unary operator '{}' expects {} type", op, expected)
            }
            DiagnosticKind::TypeMismatchDetailed {
                name,
                param,
                expected,
                actual,
            } => format!(
                "Type mismatch for parameter '{}' of function {}: expected {} but got {}",
                param, name, expected, actual
            ),
            DiagnosticKind::ReturnTypeMismatch { expected, actual } => format!(
                "Type mismatch for return: expected {} but got {}",
                expected, actual
            ),
            DiagnosticKind::UnexpectedControlFlowInFunction(op) => {
                format!("Unexpected {} in function", op)
            }
            DiagnosticKind::InvalidMemberAccess { field, ty } => {
                format!("Unsupported member access: {} on type {}", field, ty)
            }
            DiagnosticKind::InvalidIndexAccess { expected, actual } => format!(
                "Index access requires {} type, but got {}",
                expected, actual
            ),
            DiagnosticKind::ListIndexRequired => "List index must be a number".into(),
            DiagnosticKind::DictKeyRequired => "Dict key must be of type Str".into(),
            DiagnosticKind::InsertKeyRequired => "Insert requires key of type Str".into(),
            DiagnosticKind::GetKeyRequired => "Get requires key of type Str".into(),
            DiagnosticKind::FormatDictRequired => "Format requires Dict argument".into(),
            DiagnosticKind::SplitParamRequired => "Split requires Str argument".into(),
            DiagnosticKind::ReplaceParamRequired => "Replace requires Str arguments".into(),
            DiagnosticKind::JoinParamRequired => "Join requires Str argument".into(),
            DiagnosticKind::UnsupportedMethod { method, ty } => {
                format!("Unsupported method: {} for type {}", method, ty)
            }
            DiagnosticKind::UnknownListMethod(name) => format!("Unknown list method: {}", name),
            DiagnosticKind::UnknownDictMethod(name) => format!("Unknown dict method: {}", name),
            DiagnosticKind::UnknownFileMethod(name) => format!("Unknown file method: {}", name),
            DiagnosticKind::UnknownStrMethod(name) => format!("Unknown text method: {}", name),
            DiagnosticKind::ParseIntError(s) => format!("Failed to parse integer: {}", s),
            DiagnosticKind::ParseFloatError(s) => format!("Failed to parse float: {}", s),
            DiagnosticKind::FileClosed => "File is closed".into(),
            DiagnosticKind::UnsupportedReceiver(ty) => {
                format!("Unsupported method receiver: {}", ty)
            }

            DiagnosticKind::Raw(s) => s.clone(),
        }
    }

    pub fn format(kind: &DiagnosticKind) -> String {
        Self::format_en(kind)
    }

    // Removed Chinese formatter; always format in English.
}
