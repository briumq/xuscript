//! Token definitions.
//!
//! Defines all tokens of the Xu language, including keywords, operators, literals,
//! delimiters, and layout-sensitive tokens (indentation/newlines).
use crate::Span;

/// Token kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Newline (used for automatic statement termination).
    Newline,
    /// Indentation increase.
    Indent,
    /// Indentation decrease.
    Dedent,
    /// Space (usually filtered out; optionally preserved).
    Space,
    /// Comment.
    Comment,

    /// Identifier.
    Ident,
    /// Integer literal.
    Int,
    /// Float literal.
    Float,
    /// String literal.
    Str,

    /// `true`
    True,
    /// `false`
    False,
    /// `null`
    Null,

    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,

    DotDot,

    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    Hash,
    Pipe,

    /// `+=`
    PlusEq,
    /// `-=`
    MinusEq,
    /// `*=`
    StarEq,
    /// `/=`
    SlashEq,

    /// `>`
    Gt,
    /// `<`
    Lt,
    /// `>=`
    Ge,
    /// `<=`
    Le,

    /// `let`
    KwLet,
    /// `is`
    KwIs,
    /// `isnt`
    KwIsnt,
    /// `with`
    KwWith,
    /// `if`
    KwIf,
    /// `else`
    KwElse,
    /// `while`
    KwWhile,
    /// `for`
    KwFor,
    /// `.`
    Dot,
    /// `func`
    KwFunc,
    /// `struct`
    KwStruct,
    /// `enum`
    KwEnum,
    /// `return`
    KwReturn,
    /// `break`
    KwBreak,
    /// `continue`
    KwContinue,
    /// `throw`
    KwThrow,
    /// `try`
    KwTry,
    /// `catch`
    KwCatch,
    /// `finally`
    KwFinally,
    /// `not`
    KwNot,
    /// `and`
    KwAnd,
    /// `or`
    KwOr,
    /// `import` / `use`
    KwImport,
    /// `match`
    KwMatch,

    /// `==`
    EqEq,
    /// `!=`
    Ne,
    /// `=`
    Eq,
    /// `!`
    Bang,
    Question,

    /// Statement terminator (`;`).
    StmtEnd,
    /// `,`
    Comma,
    /// `:`
    Colon,

    /// End of file.
    Eof,
}

/// Token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// Token kind.
    pub kind: TokenKind,
    /// Span in source text.
    pub span: Span,
}
