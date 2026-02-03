//! Token definitions.
//!
//! Defines all tokens of the Xu language, including keywords, operators, literals,
//! delimiters, and newlines for automatic statement termination.
use crate::Span;

/// Token kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// Newline (used for automatic statement termination).
    Newline,
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
    DotDotEq,
    Ellipsis,

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
    /// `::`
    ColonColon,
    Pipe,
    /// `&&`
    AmpAmp,
    /// `||`
    PipePipe,

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
    KwVar,
    /// `is` (reserved keyword)
    KwIs,
    /// `with`
    KwWith,
    KwHas,
    /// `if`
    KwIf,
    /// `else`
    KwElse,
    /// `while`
    KwWhile,
    /// `for`
    KwFor,
    KwIn,
    /// `.`
    Dot,
    /// `func`
    KwFunc,
    /// `return`
    KwReturn,
    /// `break`
    KwBreak,
    /// `continue`
    KwContinue,
    KwDoes,
    KwInner,
    KwStatic,
    KwSelf,
    KwUse,
    KwAs,
    /// `match`
    KwMatch,
    /// `when`
    KwWhen,
    KwCan,
    KwAsync,
    KwAwait,

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

impl TokenKind {
    /// Returns true if this token kind is a keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::True
                | TokenKind::False
                | TokenKind::KwLet
                | TokenKind::KwVar
                | TokenKind::KwIs
                | TokenKind::KwWith
                | TokenKind::KwHas
                | TokenKind::KwIf
                | TokenKind::KwElse
                | TokenKind::KwWhile
                | TokenKind::KwFor
                | TokenKind::KwIn
                | TokenKind::KwFunc
                | TokenKind::KwReturn
                | TokenKind::KwBreak
                | TokenKind::KwContinue
                | TokenKind::KwDoes
                | TokenKind::KwInner
                | TokenKind::KwStatic
                | TokenKind::KwSelf
                | TokenKind::KwUse
                | TokenKind::KwAs
                | TokenKind::KwMatch
                | TokenKind::KwWhen
                | TokenKind::KwCan
                | TokenKind::KwAsync
                | TokenKind::KwAwait
        )
    }
}
