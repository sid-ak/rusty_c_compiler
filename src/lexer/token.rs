//! The token vocabulary: what the lexer produces and the parser consumes.
//!
//! This is the interface between the front end's first two passes, so it covers the whole grammar
//! in `docs/architecture.md` rather than only what a scanner happens to recognize today.

use std::fmt;

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
            TokenKind::CharLit(byte) => {
                formatter.write_str("'")?;
                write_escaped(formatter, *byte)?;
                formatter.write_str("'")
            }
            TokenKind::StrLit(bytes) => {
                formatter.write_str("\"")?;
                for &byte in bytes {
                    write_escaped(formatter, byte)?;
                }
                formatter.write_str("\"")
            }
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

/// Write `byte` as it would be spelled inside a literal, escaping it where C requires.
fn write_escaped(formatter: &mut fmt::Formatter<'_>, byte: u8) -> fmt::Result {
    if let Some(letter) = escape_letter(byte) {
        write!(formatter, "\\{}", char::from(letter))
    } else if byte.is_ascii_graphic() || byte == b' ' {
        write!(formatter, "{}", char::from(byte))
    } else {
        write!(formatter, "\\x{byte:02x}")
    }
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
mod tests {
    use super::*;

    /// A sample of every `TokenKind` variant.
    ///
    /// The exhaustive match below is the guard: a variant added without a sample here fails to
    /// compile, and so cannot reach the parser without a spelling.
    fn every_kind() -> Vec<TokenKind> {
        let samples = vec![
            TokenKind::Keyword(Keyword::Int),
            TokenKind::Ident("name".to_string()),
            TokenKind::IntLit(7),
            TokenKind::CharLit(b'x'),
            TokenKind::StrLit(b"hi".to_vec()),
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Assign,
            TokenKind::EqEq,
            TokenKind::BangEq,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::LtEq,
            TokenKind::GtEq,
            TokenKind::AmpAmp,
            TokenKind::PipePipe,
            TokenKind::Bang,
            TokenKind::PlusPlus,
            TokenKind::MinusMinus,
            TokenKind::LParen,
            TokenKind::RParen,
            TokenKind::LBrace,
            TokenKind::RBrace,
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Semi,
            TokenKind::Comma,
            TokenKind::Eof,
        ];

        for kind in &samples {
            match kind {
                TokenKind::Keyword(_)
                | TokenKind::Ident(_)
                | TokenKind::IntLit(_)
                | TokenKind::CharLit(_)
                | TokenKind::StrLit(_)
                | TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
                | TokenKind::Assign
                | TokenKind::EqEq
                | TokenKind::BangEq
                | TokenKind::Lt
                | TokenKind::Gt
                | TokenKind::LtEq
                | TokenKind::GtEq
                | TokenKind::AmpAmp
                | TokenKind::PipePipe
                | TokenKind::Bang
                | TokenKind::PlusPlus
                | TokenKind::MinusMinus
                | TokenKind::LParen
                | TokenKind::RParen
                | TokenKind::LBrace
                | TokenKind::RBrace
                | TokenKind::LBracket
                | TokenKind::RBracket
                | TokenKind::Semi
                | TokenKind::Comma
                | TokenKind::Eof => {}
            }
        }

        samples
    }

    /// Every token kind renders as something a diagnostic can print.
    #[test]
    fn every_kind_has_a_non_empty_spelling() {
        for kind in every_kind() {
            assert!(!kind.to_string().is_empty(), "no spelling for {kind:?}");
        }
    }

    /// No two samples are the same variant, so the coverage list above is not padded.
    #[test]
    fn every_kind_sample_is_distinct() {
        let kinds = every_kind();
        let distinct: std::collections::HashSet<_> =
            kinds.iter().map(std::mem::discriminant).collect();

        assert_eq!(distinct.len(), kinds.len());
    }

    /// Operators and punctuators spell themselves; value-carrying kinds spell their value.
    #[test]
    fn fixed_spellings_are_the_source_text() {
        assert_eq!(TokenKind::LtEq.fixed_spelling(), Some("<="));
        assert_eq!(TokenKind::Semi.fixed_spelling(), Some(";"));
        assert_eq!(TokenKind::AmpAmp.fixed_spelling(), Some("&&"));
        assert_eq!(TokenKind::Ident("a".into()).fixed_spelling(), None);
        assert_eq!(TokenKind::IntLit(1).fixed_spelling(), None);
    }

    /// The parser's expected-versus-found wording reads as source text.
    #[test]
    fn spellings_read_as_source() {
        assert_eq!(
            format!(
                "expected '{}', found '{}'",
                TokenKind::Semi,
                TokenKind::RBrace
            ),
            "expected ';', found '}'"
        );
    }

    /// Every keyword round-trips: its spelling is recognized back as itself.
    #[test]
    fn keywords_round_trip_through_the_table() {
        for &keyword in Keyword::ALL {
            assert_eq!(
                Keyword::from_identifier(keyword.spelling()),
                Some(keyword),
                "for {keyword:?}"
            );
        }
    }

    /// The subset's ten keywords are the ones the grammar names, and nothing else.
    #[test]
    fn the_keyword_set_is_the_documented_one() {
        let spellings: Vec<_> = Keyword::ALL.iter().map(|k| k.spelling()).collect();

        assert_eq!(
            spellings,
            ["int", "char", "void", "if", "else", "while", "for", "return", "break", "continue"]
        );
    }

    /// A keyword with any prefix or suffix character is an identifier, not a keyword.
    #[test]
    fn keywords_with_affixes_are_identifiers() {
        for &keyword in Keyword::ALL {
            let spelling = keyword.spelling();

            for candidate in [
                format!("{spelling}eger"),
                format!("{spelling}_"),
                format!("{spelling}1"),
                format!("x{spelling}"),
                format!("_{spelling}"),
                spelling.to_uppercase(),
            ] {
                assert_eq!(
                    Keyword::from_identifier(&candidate),
                    None,
                    "{candidate} should be an identifier"
                );
            }
        }
    }

    /// The escape table reads the same in both directions, which is what keeps the lexer's
    /// decoding and the renderer's spelling from drifting apart.
    #[test]
    fn escape_table_round_trips() {
        for (letter, byte) in ESCAPES {
            assert_eq!(escape_byte(letter), Some(byte));
            assert_eq!(escape_letter(byte), Some(letter));
        }
    }

    /// A letter outside the table is not an escape.
    #[test]
    fn unknown_escape_letters_decode_to_nothing() {
        for letter in *b"qz8x" {
            assert_eq!(escape_byte(letter), None, "for {}", char::from(letter));
        }
    }

    /// Literals spell back out in a form a C programmer would recognize.
    #[test]
    fn literals_spell_themselves_back() {
        assert_eq!(TokenKind::CharLit(b'\n').to_string(), r"'\n'");
        assert_eq!(TokenKind::CharLit(b'a').to_string(), "'a'");
        assert_eq!(
            TokenKind::StrLit(b"a\tb\0c".to_vec()).to_string(),
            r#""a\tb\0c""#
        );
        assert_eq!(TokenKind::StrLit(Vec::new()).to_string(), r#""""#);
        assert_eq!(TokenKind::IntLit(-5).to_string(), "-5");
    }

    /// A byte with no escape and no printable form still renders, rather than being dropped.
    #[test]
    fn unprintable_bytes_render_as_hex() {
        assert_eq!(TokenKind::CharLit(0x01).to_string(), r"'\x01'");
        assert_eq!(TokenKind::CharLit(0xff).to_string(), r"'\xff'");
    }

    /// A token pairs a kind with the span it was scanned from, and displays as its kind.
    #[test]
    fn token_pairs_a_kind_with_a_span() {
        let token = Token::new(TokenKind::Semi, Span::new(4, 5));

        assert_eq!(token.kind, TokenKind::Semi);
        assert_eq!(token.span, Span::new(4, 5));
        assert_eq!(token.to_string(), ";");
    }
}
