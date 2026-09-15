//! The token vocabulary: what the lexer produces and the parser consumes.
//!
//! This is the interface between the front end's first two passes, so it covers the whole grammar
//! in `docs/architecture.md` rather than only what a scanner happens to recognize today.

use std::fmt;
use std::fmt::Write as _;

use crate::diagnostics::Span;

/// Declare a keyword set, deriving its spellings and its lookup from one list.
///
/// Recognition (text to keyword) and rendering (keyword to text) are the same knowledge read in
/// two directions, so they are written once here. Adding a keyword is a single line.
macro_rules! keywords {
    (
        $(#[$enum_meta:meta])*
        $name:ident { $($variant:ident => $spelling:literal),+ $(,)? }
    ) => {
        $(#[$enum_meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $(
                #[doc = concat!("The `", $spelling, "` keyword.")]
                $variant,
            )+
        }

        impl $name {
            /// Every keyword in the subset, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// How this keyword is spelled in source.
            pub fn spelling(self) -> &'static str {
                match self {
                    $($name::$variant => $spelling,)+
                }
            }

            /// The keyword `text` spells, if it spells one.
            ///
            /// Applied to an already-scanned identifier rather than matched against the raw input,
            /// which is what makes `integer` one identifier instead of `int` followed by `eger`.
            pub fn from_identifier(text: &str) -> Option<Self> {
                match text {
                    $($spelling => Some($name::$variant),)+
                    _ => None,
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.spelling())
            }
        }
    };
}

keywords! {
    /// A word this subset of C reserves.
    Keyword {
        Int => "int",
        Char => "char",
        Void => "void",
        If => "if",
        Else => "else",
        While => "while",
        For => "for",
        Return => "return",
        Break => "break",
        Continue => "continue",
    }
}

/// The words C reserves that this subset does not implement.
///
/// Together with [`Keyword`] this is C89's complete keyword set, split in two. The lexer treats
/// these as ordinary identifiers — they are not part of this grammar — but the parser consults the
/// list before it rejects one, so `struct point p;` is reported as a construct this compiler lacks
/// rather than as a program that is malformed. Someone writing real C deserves the first answer.
pub const UNSUPPORTED_KEYWORDS: [&str; 22] = [
    "auto", "case", "const", "default", "do", "double", "enum", "extern", "float", "goto", "long",
    "register", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union",
    "unsigned", "volatile",
];

/// The C keyword `text` spells that this subset leaves out, if it spells one.
pub fn unsupported_keyword(text: &str) -> Option<&'static str> {
    UNSUPPORTED_KEYWORDS
        .iter()
        .copied()
        .find(|&word| word == text)
}

/// The escape sequences this subset understands, as (the letter after the backslash, the byte it
/// denotes).
///
/// The lexer reads this table left to right to decode a literal, and [`TokenKind`]'s `Display`
/// reads it right to left to spell one back out, so the two can never disagree about what `\v` is.
pub const ESCAPES: [(u8, u8); 11] = [
    (b'n', b'\n'),
    (b't', b'\t'),
    (b'r', b'\r'),
    (b'0', b'\0'),
    (b'\\', b'\\'),
    (b'\'', b'\''),
    (b'"', b'"'),
    (b'a', 0x07),
    (b'b', 0x08),
    (b'f', 0x0c),
    (b'v', 0x0b),
];

/// The byte an escape letter denotes, if it denotes one.
pub fn escape_byte(letter: u8) -> Option<u8> {
    ESCAPES
        .iter()
        .find(|(escape, _)| *escape == letter)
        .map(|(_, byte)| *byte)
}

/// The escape letter that denotes `byte`, if one does.
fn escape_letter(byte: u8) -> Option<u8> {
    ESCAPES
        .iter()
        .find(|(_, escaped)| *escaped == byte)
        .map(|(letter, _)| *letter)
}

/// What a token is.
///
/// Literals carry their decoded value rather than their source text: the lexer resolves escapes
/// and numeric bases once, in the one place that already has the source in hand, so nothing
/// downstream re-parses them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    /// A reserved word.
    Keyword(Keyword),
    /// A name: `[A-Za-z_][A-Za-z0-9_]*` that is not a keyword.
    Ident(String),

    /// An integer literal, decoded from decimal, hex, or octal into its value.
    IntLit(i32),
    /// A character literal, decoded to the byte it denotes.
    CharLit(u8),
    /// A string literal, decoded to the bytes it denotes, without a trailing NUL.
    StrLit(Vec<u8>),

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
    /// `=`
    Assign,
    /// `==`
    EqEq,
    /// `!=`
    BangEq,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    LtEq,
    /// `>=`
    GtEq,
    /// `&&`
    AmpAmp,
    /// `||`
    PipePipe,
    /// `!`
    Bang,
    /// `++`
    PlusPlus,
    /// `--`
    MinusMinus,

    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `;`
    Semi,
    /// `,`
    Comma,

    /// The end of the input. Always the last token, including after a lexing error.
    Eof,
}

impl fmt::Display for TokenKind {
    /// The source spelling, so a parser diagnostic can read `expected ';', found '}'`.
    ///
    /// A literal is spelled from its decoded value, so `0x10` comes back as `16`; the original
    /// base is not retained, and nothing downstream needs it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Ident(name) => formatter.write_str(name),
            TokenKind::IntLit(value) => write!(formatter, "{value}"),
            TokenKind::CharLit(byte) => write!(formatter, "'{}'", spell_literal(&[*byte])),
            TokenKind::StrLit(bytes) => write!(formatter, "\"{}\"", spell_literal(bytes)),
            // Everything else spells itself. The four value-carrying kinds are matched above, so
            // the fallback is unreachable; it exists because `Display` cannot fail here.
            fixed => formatter.write_str(fixed.fixed_spelling().unwrap_or("<token>")),
        }
    }
}

impl TokenKind {
    /// The spelling of a token whose text is fixed — every operator and punctuator.
    ///
    /// `None` for the variants that carry a value, whose spelling depends on that value.
    pub fn fixed_spelling(&self) -> Option<&'static str> {
        let spelling = match self {
            TokenKind::Keyword(keyword) => keyword.spelling(),
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Assign => "=",
            TokenKind::EqEq => "==",
            TokenKind::BangEq => "!=",
            TokenKind::Lt => "<",
            TokenKind::Gt => ">",
            TokenKind::LtEq => "<=",
            TokenKind::GtEq => ">=",
            TokenKind::AmpAmp => "&&",
            TokenKind::PipePipe => "||",
            TokenKind::Bang => "!",
            TokenKind::PlusPlus => "++",
            TokenKind::MinusMinus => "--",
            TokenKind::LParen => "(",
            TokenKind::RParen => ")",
            TokenKind::LBrace => "{",
            TokenKind::RBrace => "}",
            TokenKind::LBracket => "[",
            TokenKind::RBracket => "]",
            TokenKind::Semi => ";",
            TokenKind::Comma => ",",
            TokenKind::Eof => "end of file",
            TokenKind::Ident(_)
            | TokenKind::IntLit(_)
            | TokenKind::CharLit(_)
            | TokenKind::StrLit(_) => return None,
        };

        Some(spelling)
    }
}

/// Spell `bytes` as they would be written inside a literal, escaping where C requires.
///
/// The one place a decoded literal is turned back into source text, so a diagnostic, a token dump,
/// and an AST dump cannot disagree about what `\n` looks like. Callers add the surrounding quotes,
/// since the same body serves both literal forms.
pub fn spell_literal(bytes: &[u8]) -> String {
    let mut spelled = String::with_capacity(bytes.len());

    for &byte in bytes {
        if let Some(letter) = escape_letter(byte) {
            spelled.push('\\');
            spelled.push(char::from(letter));
        } else if byte.is_ascii_graphic() || byte == b' ' {
            spelled.push(char::from(byte));
        } else {
            // Writing to a String cannot fail, so there is no error path worth propagating.
            let _ = write!(spelled, "\\x{byte:02x}");
        }
    }

    spelled
}

/// A token: what it is, and where it came from.
///
/// Deliberately not `Copy`. An identifier or string literal owns its decoded payload, which is
/// what keeps escape handling in one place; the cost is that the parser should match on `&Token`
/// and clone only where it keeps the payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// The bytes of source it was scanned from.
    pub span: Span,
}

impl Token {
    /// A token of `kind` covering `span`.
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.kind)
    }
}

#[cfg(test)]
mod tests;
