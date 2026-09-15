//! The lexer: source bytes in, a token stream out.
//!
//! The scanner reads `&[u8]` rather than `&str`. A C file is not guaranteed to be valid UTF-8, and
//! this pass is a fuzz target handed arbitrary input, so malformed bytes have to become a
//! diagnostic rather than a decoding failure before the compiler starts.
//!
//! Two properties hold for every input, valid or not:
//!
//! - It terminates. Every step of the scan consumes at least one byte, so the offset strictly
//!   increases and the loop cannot spin on an unconsumable byte.
//! - It reaches the end. A malformed construct produces a diagnostic and resynchronizes rather
//!   than stopping the scan, so the stream always ends in [`TokenKind::Eof`]. A lexer that gave up
//!   at the first error would make the parser's own error recovery impossible to test, since the
//!   parser would never see the tokens past that point.

pub mod token;

use std::fmt::Write as _;

use crate::diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, SourceMap, Span};

pub use token::{Keyword, Token, TokenKind};

/// Everything one scan of a source file produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    /// The token stream. Always ends in [`TokenKind::Eof`], including after an error.
    pub tokens: Vec<Token>,
    /// The problems found, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Scan `source` into a token stream.
pub fn lex(source: &[u8]) -> Lexed {
    let mut lexer = Lexer {
        source,
        offset: 0,
        tokens: Vec::new(),
        diagnostics: DiagnosticBag::new(),
    };
    lexer.run();

    Lexed {
        tokens: lexer.tokens,
        diagnostics: lexer.diagnostics.into_sorted(),
    }
}

/// Render a token stream as one token per line, each with the source range it came from.
///
/// This is what `--dump-tokens` prints. The token kind is written in its `Debug` form rather than
/// its source spelling, because the point of the dump is to show what the scanner decided — that
/// `0xff` became `IntLit(255)` and that `++` became one token rather than two.
///
/// The two byte-valued literals are the exception: `Debug` would print their payload as numbers,
/// so they are shown re-escaped as `CharLit('\n')` and `StrLit("sum:\t")`, which is the same
/// information in the form the reader wrote it.
pub fn dump(map: &SourceMap, tokens: &[Token]) -> String {
    // The position column is measured before anything is written, rather than given a fixed width.
    // Positions grow with the file: a fixed column wide enough for `9:9-9:12` stops aligning the
    // moment a program reaches four-digit line numbers, which is exactly when a dump is long enough
    // that alignment is what makes it readable.
    let positions: Vec<String> = tokens
        .iter()
        .map(|token| {
            format!(
                "{}-{}",
                map.location(token.span.start),
                map.location(token.span.end)
            )
        })
        .collect();
    let width = positions.iter().map(String::len).max().unwrap_or(0) + 2;

    let mut rendered = String::new();
    for (token, position) in tokens.iter().zip(&positions) {
        let kind = match &token.kind {
            kind @ TokenKind::CharLit(_) => format!("CharLit({kind})"),
            kind @ TokenKind::StrLit(_) => format!("StrLit({kind})"),
            kind => format!("{kind:?}"),
        };

        // Writing to a String cannot fail, so there is no error path worth propagating.
        let _ = writeln!(rendered, "{position:<width$}{kind}");
    }

    rendered
}

/// The scanning state: the input, and how far into it we are.
struct Lexer<'source> {
    /// The bytes being scanned.
    source: &'source [u8],
    /// How many bytes have been consumed. Never decreases, which is what guarantees termination.
    offset: usize,
    /// Tokens produced so far.
    tokens: Vec<Token>,
    /// Problems found so far.
    diagnostics: DiagnosticBag,
}

impl Lexer<'_> {
    /// Scan the whole input, ending with [`TokenKind::Eof`].
    fn run(&mut self) {
        loop {
            self.skip_trivia();

            let start = self.offset;
            let Some(byte) = self.bump() else {
                self.tokens
                    .push(Token::new(TokenKind::Eof, Span::empty_at(start)));
                return;
            };

            if let Some(kind) = self.scan(byte, start) {
                self.tokens
                    .push(Token::new(kind, Span::new(start, self.offset)));
            }
        }
    }

    /// Scan one token, given its already-consumed first byte at `start`.
    ///
    /// `None` means the bytes were consumed but produced no token, which is what a stray character
    /// does. The first byte is consumed before this is called, so a scan always makes progress.
    fn scan(&mut self, byte: u8, start: usize) -> Option<TokenKind> {
        match byte {
            _ if is_ident_start(byte) => Some(self.scan_word(start)),
            b'0'..=b'9' => Some(self.scan_number(start)),
            b'\'' => Some(self.scan_char_literal(start)),
            b'"' => Some(self.scan_string_literal(start)),

            b'+' => Some(self.one_or_two(b'+', TokenKind::PlusPlus, TokenKind::Plus)),
            b'-' => Some(self.one_or_two(b'-', TokenKind::MinusMinus, TokenKind::Minus)),
            b'=' => Some(self.one_or_two(b'=', TokenKind::EqEq, TokenKind::Assign)),
            b'!' => Some(self.one_or_two(b'=', TokenKind::BangEq, TokenKind::Bang)),
            b'<' => Some(self.one_or_two(b'=', TokenKind::LtEq, TokenKind::Lt)),
            b'>' => Some(self.one_or_two(b'=', TokenKind::GtEq, TokenKind::Gt)),
            b'&' => self.paired_only(b'&', TokenKind::AmpAmp, start),
            b'|' => self.paired_only(b'|', TokenKind::PipePipe, start),

            // Punctuation of real C that this grammar has no token for at all. The parser can
            // never report these, because nothing reaches it to report — so the judgement is made
            // here, where the character is still in hand.
            b'#' | b'?' | b':' | b'^' | b'~' => self.unsupported_punctuation(byte, start),

            b'*' => Some(TokenKind::Star),
            b'/' => Some(TokenKind::Slash),
            b'%' => Some(TokenKind::Percent),
            b'(' => Some(TokenKind::LParen),
            b')' => Some(TokenKind::RParen),
            b'{' => Some(TokenKind::LBrace),
            b'}' => Some(TokenKind::RBrace),
            b'[' => Some(TokenKind::LBracket),
            b']' => Some(TokenKind::RBracket),
            b';' => Some(TokenKind::Semi),
            b',' => Some(TokenKind::Comma),

            _ => {
                self.error(
                    Span::new(start, self.offset),
                    format!("stray {} in program", describe_byte(byte)),
                );
                None
            }
        }
    }

    /// Skip whitespace and comments until the next thing that could start a token.
    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(byte) if byte.is_ascii_whitespace() => {
                    self.bump();
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => self.skip_line_comment(),
                Some(b'/') if self.peek_at(1) == Some(b'*') => self.skip_block_comment(),
                _ => return,
            }
        }
    }

    /// Skip `// ...` up to, but not including, the newline that ends it.
    fn skip_line_comment(&mut self) {
        while !matches!(self.peek(), None | Some(b'\n')) {
            self.bump();
        }
    }

    /// Skip `/* ... */`, which does not nest.
    ///
    /// An unterminated one resynchronizes at end of file: there is no other plausible place for it
    /// to end, and guessing would silently turn the rest of the program into comment text.
    fn skip_block_comment(&mut self) {
        let start = self.offset;
        self.bump();
        self.bump();

        while let Some(byte) = self.bump() {
            if byte == b'*' && self.peek() == Some(b'/') {
                self.bump();
                return;
            }
        }

        self.error(
            Span::new(start, self.offset),
            "unterminated block comment".to_string(),
        );
    }

    /// Scan an identifier and decide whether it spells a keyword.
    ///
    /// Keyword recognition happens here, on the whole word, rather than by matching keyword text
    /// against the raw input, which is what makes `integer` one identifier and not `int` + `eger`.
    fn scan_word(&mut self, start: usize) -> TokenKind {
        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }

        // Every byte scanned above is ASCII, so this cannot lose information.
        let text: String = self
            .slice(start, self.offset)
            .iter()
            .map(|&byte| char::from(byte))
            .collect();

        match Keyword::from_identifier(&text) {
            Some(keyword) => TokenKind::Keyword(keyword),
            None => TokenKind::Ident(text),
        }
    }

    /// Scan an integer literal in decimal, hex (`0x`), or octal (leading `0`).
    ///
    /// The whole alphanumeric run is consumed before it is validated, so `0xZZ` and `123abc` each
    /// produce one diagnostic covering the whole literal rather than a good token followed by a
    /// confusing identifier.
    fn scan_number(&mut self, start: usize) -> TokenKind {
        let (radix, digits_start) = match (self.byte_at(start), self.peek()) {
            (Some(b'0'), Some(b'x' | b'X')) => {
                self.bump();
                (16, self.offset)
            }
            (Some(b'0'), Some(b'0'..=b'9')) => (8, self.offset),
            _ => (10, start),
        };

        while self.peek().is_some_and(is_ident_continue) {
            self.bump();
        }

        let span = Span::new(start, self.offset);
        let digits = self.slice(digits_start, self.offset);

        if digits.is_empty() {
            self.error(span, "expected digits after '0x'".to_string());
            return TokenKind::IntLit(0);
        }

        let mut value: u64 = 0;
        for &byte in digits {
            let Some(digit) = char::from(byte).to_digit(radix) else {
                self.error(
                    span,
                    format!(
                        "invalid digit '{}' in {} literal",
                        char::from(byte),
                        radix_name(radix)
                    ),
                );
                return TokenKind::IntLit(0);
            };

            // Saturating, so a very long literal cannot wrap back into range mid-way.
            value = value
                .saturating_mul(u64::from(radix))
                .saturating_add(u64::from(digit));
        }

        match i32::try_from(value) {
            Ok(value) => TokenKind::IntLit(value),
            Err(_) => {
                // The literal's own text, not the accumulated value: accumulation saturates, so a
                // very long literal would otherwise be reported as some unrelated round number.
                let text = self.text(span);
                self.report(
                    Diagnostic::lex(
                        span,
                        format!("integer literal is too large for 'int': {text}"),
                    )
                    .with_note("the maximum is 2147483647; write INT_MIN as -2147483647 - 1"),
                );
                TokenKind::IntLit(0)
            }
        }
    }

    /// Scan a character literal, whose opening quote is already consumed.
    ///
    /// An unterminated literal resynchronizes at the end of the line: a quote is far more often
    /// missing than a newline is intended inside one.
    fn scan_char_literal(&mut self, start: usize) -> TokenKind {
        let mut bytes = Vec::new();
        let terminated = self.scan_literal_body(b'\'', &mut bytes);
        let span = Span::new(start, self.offset);

        if !terminated {
            self.error(span, "unterminated character literal".to_string());
        } else if bytes.is_empty() {
            self.error(span, "empty character literal".to_string());
        } else if bytes.len() > 1 {
            self.error(
                span,
                "character literal must contain exactly one character".to_string(),
            );
        }

        TokenKind::CharLit(bytes.first().copied().unwrap_or(0))
    }

    /// Scan a string literal, whose opening quote is already consumed.
    fn scan_string_literal(&mut self, start: usize) -> TokenKind {
        let mut bytes = Vec::new();

        if !self.scan_literal_body(b'"', &mut bytes) {
            self.error(
                Span::new(start, self.offset),
                "unterminated string literal".to_string(),
            );
        }

        TokenKind::StrLit(bytes)
    }

    /// Read literal bytes up to `quote`, decoding escapes into `bytes`.
    ///
    /// Returns whether the closing quote was found. Escapes are resolved here, once, so the code
    /// generator emits stored bytes rather than re-parsing `\n` out of the original source.
    fn scan_literal_body(&mut self, quote: u8, bytes: &mut Vec<u8>) -> bool {
        loop {
            match self.peek() {
                None | Some(b'\n') => return false,
                Some(byte) if byte == quote => {
                    self.bump();
                    return true;
                }
                Some(b'\\') => {
                    let escape_start = self.offset;
                    self.bump();

                    // Peeked, not consumed: a newline here ends the line, and swallowing it would
                    // move the resynchronization point onto the next line.
                    let Some(letter) = self.peek().filter(|&byte| byte != b'\n') else {
                        return false;
                    };
                    self.bump();

                    match token::escape_byte(letter) {
                        Some(decoded) => bytes.push(decoded),
                        None => {
                            self.error(
                                Span::new(escape_start, self.offset),
                                format!("unknown escape sequence '\\{}'", char::from(letter)),
                            );
                            // Keep the letter itself, which is what C compilers do, so one bad
                            // escape does not also shorten the literal.
                            bytes.push(letter);
                        }
                    }
                }
                Some(byte) => {
                    self.bump();
                    bytes.push(byte);
                }
            }
        }
    }

    /// Maximal munch for an operator that doubles: `paired` if the next byte is `expected`.
    fn one_or_two(&mut self, expected: u8, paired: TokenKind, single: TokenKind) -> TokenKind {
        if self.peek() == Some(expected) {
            self.bump();
            paired
        } else {
            single
        }
    }

    /// An operator that exists only in its doubled form, such as `&&`.
    ///
    /// The single form is a bitwise operator, which this subset does not have, so it is reported
    /// as unsupported rather than as a stray byte.
    fn paired_only(&mut self, expected: u8, paired: TokenKind, start: usize) -> Option<TokenKind> {
        if self.peek() == Some(expected) {
            self.bump();
            return Some(paired);
        }

        let spelling = char::from(expected);
        self.unsupported(start, format!("did you mean '{spelling}{spelling}'?"));

        None
    }

    /// A character that spells a C construct this subset leaves out entirely.
    fn unsupported_punctuation(&mut self, byte: u8, start: usize) -> Option<TokenKind> {
        let note = match byte {
            b'#' => "this subset has no preprocessor, so no directive has any meaning here",
            b'?' | b':' => "the conditional operator is not in this subset; use an 'if' statement",
            _ => "the bitwise operators are not in this subset",
        };
        self.unsupported(start, note);

        None
    }

    /// Report the character at `start` as real C this subset does not implement, with `note`
    /// beneath it saying what to reach for instead.
    fn unsupported(&mut self, start: usize, note: impl Into<String>) {
        let span = Span::new(start, self.offset);
        let construct = format!("'{}'", self.text(span));

        self.report(Diagnostic::unsupported(DiagnosticKind::Lex, span, construct).with_note(note));
    }

    /// Record a lexing diagnostic at `span`.
    fn error(&mut self, span: Span, message: String) {
        self.report(Diagnostic::lex(span, message));
    }

    /// Record an already-built diagnostic, for the cases that carry a note.
    fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// The byte at `offset`, if the input reaches that far.
    fn byte_at(&self, offset: usize) -> Option<u8> {
        self.source.get(offset).copied()
    }

    /// The next byte, without consuming it.
    fn peek(&self) -> Option<u8> {
        self.byte_at(self.offset)
    }

    /// The byte `ahead` positions past the next one, without consuming anything.
    fn peek_at(&self, ahead: usize) -> Option<u8> {
        self.byte_at(self.offset + ahead)
    }

    /// Consume and return the next byte.
    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;

        Some(byte)
    }

    /// The bytes in `[start, end)`, empty if that range is not in the input.
    fn slice(&self, start: usize, end: usize) -> &[u8] {
        self.source.get(start..end).unwrap_or_default()
    }

    /// The source text `span` covers, for quoting a construct back in its own diagnostic.
    fn text(&self, span: Span) -> String {
        String::from_utf8_lossy(self.slice(span.start, span.end)).into_owned()
    }
}

/// Whether `byte` can start an identifier.
fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Whether `byte` can continue an identifier.
fn is_ident_continue(byte: u8) -> bool {
    is_ident_start(byte) || byte.is_ascii_digit()
}

/// The name of a numeric base, for a diagnostic about a bad digit.
fn radix_name(radix: u32) -> &'static str {
    match radix {
        8 => "octal",
        16 => "hexadecimal",
        _ => "decimal",
    }
}

/// Describe an unexpected byte in a way that reads well in a message.
///
/// A printable byte is quoted as itself; anything else — a control byte, or a fragment of UTF-8
/// text that is not valid C — is named by its value, since printing it would corrupt the message.
fn describe_byte(byte: u8) -> String {
    if byte.is_ascii_graphic() {
        format!("'{}'", char::from(byte))
    } else {
        format!("byte 0x{byte:02x}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use crate::diagnostics::DiagnosticKind;

    /// Lex `source`, asserting it produced no diagnostics, and return its kinds without the `Eof`.
    fn kinds(source: &str) -> Vec<TokenKind> {
        let lexed = lex(source.as_bytes());
        assert!(
            lexed.diagnostics.is_empty(),
            "expected {source:?} to lex cleanly, got: {:?}",
            lexed.diagnostics
        );
        assert_eq!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(&TokenKind::Eof),
            "the stream must end in Eof"
        );

        lexed
            .tokens
            .into_iter()
            .map(|token| token.kind)
            .filter(|kind| *kind != TokenKind::Eof)
            .collect()
    }

    /// The single token `source` lexes to.
    fn single(source: &str) -> TokenKind {
        let mut kinds = kinds(source);
        assert_eq!(kinds.len(), 1, "expected one token from {source:?}");

        kinds.remove(0)
    }

    /// Every keyword lexes as itself.
    #[test]
    fn keywords_lex_as_keywords() {
        for &keyword in Keyword::ALL {
            assert_eq!(single(keyword.spelling()), TokenKind::Keyword(keyword));
        }
    }

    /// A keyword with anything attached is one identifier, not a keyword plus leftovers.
    #[test]
    fn keyword_prefixes_lex_as_identifiers() {
        assert_eq!(single("integer"), TokenKind::Ident("integer".into()));
        assert_eq!(single("if_"), TokenKind::Ident("if_".into()));
        assert_eq!(single("_int"), TokenKind::Ident("_int".into()));
        assert_eq!(single("returns"), TokenKind::Ident("returns".into()));
    }

    /// Identifiers may start with a letter or underscore and continue with digits.
    #[test]
    fn identifiers_lex_as_identifiers() {
        for name in ["x", "_", "_x9", "camelCase", "SHOUT", "a1b2"] {
            assert_eq!(single(name), TokenKind::Ident(name.into()));
        }
    }

    /// Decimal, hex, and octal literals decode to the same value the base implies.
    #[test]
    fn integer_literals_decode_by_base() {
        let cases = [
            ("0", 0),
            ("7", 7),
            ("42", 42),
            ("0x0", 0),
            ("0xff", 255),
            ("0XFF", 255),
            ("0755", 493),
            ("010", 8),
            ("2147483647", i32::MAX),
        ];

        for (source, expected) in cases {
            assert_eq!(single(source), TokenKind::IntLit(expected), "for {source}");
        }
    }

    /// A character literal decodes to the one byte it denotes, escape or not.
    #[test]
    fn character_literals_decode_to_one_byte() {
        assert_eq!(single("'a'"), TokenKind::CharLit(b'a'));
        assert_eq!(single("' '"), TokenKind::CharLit(b' '));
        assert_eq!(single(r"'\n'"), TokenKind::CharLit(b'\n'));
        assert_eq!(single(r"'\0'"), TokenKind::CharLit(0));
        assert_eq!(single(r"'\\'"), TokenKind::CharLit(b'\\'));
        assert_eq!(single(r"'\''"), TokenKind::CharLit(b'\''));
        assert_eq!(single(r#"'"'"#), TokenKind::CharLit(b'"'));
    }

    /// Every escape in the table decodes inside a character literal.
    #[test]
    fn every_escape_decodes() {
        for (letter, byte) in token::ESCAPES {
            let source = format!("'\\{}'", char::from(letter));

            assert_eq!(single(&source), TokenKind::CharLit(byte), "for {source}");
        }
    }

    /// A string literal stores decoded bytes, so nothing downstream re-parses escapes.
    #[test]
    fn string_literals_store_decoded_bytes() {
        assert_eq!(single(r#""""#), TokenKind::StrLit(Vec::new()));
        assert_eq!(single(r#""hi""#), TokenKind::StrLit(b"hi".to_vec()));
        assert_eq!(
            single(r#""a\tb\0c""#),
            TokenKind::StrLit(vec![b'a', b'\t', b'b', 0, b'c'])
        );
        assert_eq!(
            single(r#""quote:\" done""#),
            TokenKind::StrLit(b"quote:\" done".to_vec())
        );
    }

    /// A string literal may contain an unescaped single quote, and vice versa.
    #[test]
    fn quotes_nest_inside_the_other_literal_form() {
        assert_eq!(single(r#""it's""#), TokenKind::StrLit(b"it's".to_vec()));
    }

    /// Every operator and punctuator lexes as its own token.
    #[test]
    fn operators_and_punctuators_lex_as_themselves() {
        let cases = [
            ("+", TokenKind::Plus),
            ("-", TokenKind::Minus),
            ("*", TokenKind::Star),
            ("/", TokenKind::Slash),
            ("%", TokenKind::Percent),
            ("=", TokenKind::Assign),
            ("==", TokenKind::EqEq),
            ("!=", TokenKind::BangEq),
            ("<", TokenKind::Lt),
            (">", TokenKind::Gt),
            ("<=", TokenKind::LtEq),
            (">=", TokenKind::GtEq),
            ("&&", TokenKind::AmpAmp),
            ("||", TokenKind::PipePipe),
            ("!", TokenKind::Bang),
            ("++", TokenKind::PlusPlus),
            ("--", TokenKind::MinusMinus),
            ("(", TokenKind::LParen),
            (")", TokenKind::RParen),
            ("{", TokenKind::LBrace),
            ("}", TokenKind::RBrace),
            ("[", TokenKind::LBracket),
            ("]", TokenKind::RBracket),
            (";", TokenKind::Semi),
            (",", TokenKind::Comma),
        ];

        for (source, expected) in cases {
            assert_eq!(single(source), expected, "for {source}");
        }
    }

    /// Multi-character operators win over their prefixes, and a longer run splits greedily.
    #[test]
    fn maximal_munch_splits_operators_the_documented_way() {
        let a = || TokenKind::Ident("a".into());
        let b = || TokenKind::Ident("b".into());

        assert_eq!(kinds("a<=b"), [a(), TokenKind::LtEq, b()]);
        assert_eq!(kinds("a<-b"), [a(), TokenKind::Lt, TokenKind::Minus, b()]);
        assert_eq!(
            kinds("a++ +b"),
            [a(), TokenKind::PlusPlus, TokenKind::Plus, b()]
        );
        assert_eq!(
            kinds("a+++b"),
            [a(), TokenKind::PlusPlus, TokenKind::Plus, b()]
        );
        assert_eq!(
            kinds("a---b"),
            [a(), TokenKind::MinusMinus, TokenKind::Minus, b()]
        );
        assert_eq!(kinds("a==b"), [a(), TokenKind::EqEq, b()]);
        assert_eq!(
            kinds("a= =b"),
            [a(), TokenKind::Assign, TokenKind::Assign, b()]
        );
    }

    /// An empty input is just the end of the input.
    #[test]
    fn empty_input_is_only_eof() {
        assert_eq!(kinds(""), []);
    }

    /// Whitespace separates tokens without becoming one.
    #[test]
    fn whitespace_is_not_a_token() {
        assert_eq!(kinds(" \t\r\n  ;  \n"), [TokenKind::Semi]);
    }

    /// Both comment forms are skipped, including between a token and its operator.
    #[test]
    fn comments_are_skipped_wherever_they_appear() {
        assert_eq!(kinds("// nothing here\n;"), [TokenKind::Semi]);
        assert_eq!(kinds("/* nothing */ ;"), [TokenKind::Semi]);
        assert_eq!(
            kinds("a /* between */ + /* and */ b"),
            [
                TokenKind::Ident("a".into()),
                TokenKind::Plus,
                TokenKind::Ident("b".into())
            ]
        );
        assert_eq!(
            kinds("a/**/+/**/b"),
            [
                TokenKind::Ident("a".into()),
                TokenKind::Plus,
                TokenKind::Ident("b".into())
            ]
        );
    }

    /// A comment running to the end of a file with no trailing newline still ends cleanly.
    #[test]
    fn line_comment_at_eof_without_a_newline() {
        assert_eq!(kinds(";// trailing"), [TokenKind::Semi]);
        assert_eq!(kinds("// only a comment"), []);
    }

    /// A block comment does not nest: the first `*/` closes it.
    #[test]
    fn block_comments_do_not_nest() {
        assert_eq!(
            kinds("/* outer /* inner */ ;"),
            [TokenKind::Semi],
            "the first */ should close the comment"
        );
    }

    /// A `/` that does not begin a comment is division.
    #[test]
    fn a_lone_slash_is_division() {
        assert_eq!(
            kinds("a / b"),
            [
                TokenKind::Ident("a".into()),
                TokenKind::Slash,
                TokenKind::Ident("b".into())
            ]
        );
    }

    /// The dump of a multi-line fixture, which pins the line and column of every token.
    #[test]
    fn every_token_reports_its_line_and_column() {
        let source = b"int x;\nif (x)\n\treturn 0;\n";
        let lexed = lex(source);
        let map = SourceMap::new(Path::new("t.c"), source);

        assert_eq!(
            dump(&map, &lexed.tokens),
            concat!(
                "1:1-1:4    Keyword(Int)\n",
                "1:5-1:6    Ident(\"x\")\n",
                "1:6-1:7    Semi\n",
                "2:1-2:3    Keyword(If)\n",
                "2:4-2:5    LParen\n",
                "2:5-2:6    Ident(\"x\")\n",
                "2:6-2:7    RParen\n",
                "3:2-3:8    Keyword(Return)\n",
                "3:9-3:10   IntLit(0)\n",
                "3:10-3:11  Semi\n",
                "4:1-4:1    Eof\n",
            )
        );
    }

    /// The position column widens with the file, so a dump of a long program still lines up.
    ///
    /// A fixed-width column stops aligning once line numbers outgrow it, and that is precisely the
    /// size of file where a dump is long enough for alignment to be what makes it readable.
    #[test]
    fn dump_column_widens_for_large_line_numbers() {
        let mut source = vec![b'\n'; 10_000];
        source.extend_from_slice(b"int x;\n");
        let lexed = lex(&source);
        let map = SourceMap::new(Path::new("t.c"), &source);

        let dumped = dump(&map, &lexed.tokens);

        // The kind begins after the padding, so every line should start it at the same column.
        let kind_starts: std::collections::HashSet<usize> = dumped
            .lines()
            .map(|line| line.rfind("  ").map(|index| index + 2).unwrap_or(0))
            .collect();
        assert_eq!(kind_starts.len(), 1, "columns not aligned:\n{dumped}");

        let column = kind_starts.into_iter().next().unwrap_or(0);
        assert!(
            column > 16,
            "expected five-digit lines to need more than the old fixed width, got {column}"
        );
    }

    /// A tab advances the column by one, so the token after it starts one column further on.
    #[test]
    fn a_tab_advances_the_column_by_one() {
        let source = b"\t\t;";
        let lexed = lex(source);
        let map = SourceMap::new(Path::new("t.c"), source);
        let semi = lexed.tokens.first().expect("expected a token");

        assert_eq!(map.location(semi.span.start).column, 3);
    }

    /// CRLF input produces the same tokens, and the same line numbers, as LF input.
    #[test]
    fn crlf_lexes_the_same_as_lf() {
        let crlf = lex(b"int x;\r\nint y;\r\n");
        let lf = lex(b"int x;\nint y;\n");

        let crlf_kinds: Vec<_> = crlf.tokens.iter().map(|token| &token.kind).collect();
        let lf_kinds: Vec<_> = lf.tokens.iter().map(|token| &token.kind).collect();
        assert_eq!(crlf_kinds, lf_kinds);

        let map = SourceMap::new(Path::new("t.c"), b"int x;\r\nint y;\r\n");
        let second_line_start = crlf.tokens.get(3).expect("expected a fourth token");
        assert_eq!(map.location(second_line_start.span.start).line, 2);
    }

    /// Invalid UTF-8 is a diagnostic, not a panic, and the scan still reaches the end.
    #[test]
    fn invalid_utf8_is_a_diagnostic_not_a_panic() {
        let lexed = lex(b"int \xff x;");

        assert_eq!(lexed.diagnostics.len(), 1);
        assert!(
            lexed
                .diagnostics
                .first()
                .is_some_and(|diagnostic| diagnostic.message.contains("byte 0xff")),
            "got: {:?}",
            lexed.diagnostics
        );
        assert_eq!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(&TokenKind::Eof)
        );
    }

    /// A file of arbitrary bytes still terminates and still ends in `Eof`.
    #[test]
    fn arbitrary_bytes_terminate() {
        let source: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
        let lexed = lex(&source);

        assert_eq!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(&TokenKind::Eof)
        );
    }

    /// Assert `source` produces exactly one lexing diagnostic reading `message`, whose span covers
    /// `offending` — the whole malformed construct, not just the byte that gave it away — and that
    /// the scan still reached `Eof`.
    fn assert_one_error(source: &str, message: &str, offending: &str) {
        let lexed = lex(source.as_bytes());

        assert_eq!(
            lexed.diagnostics.len(),
            1,
            "for {source:?}, got: {:?}",
            lexed.diagnostics
        );
        let diagnostic = lexed
            .diagnostics
            .first()
            .expect("the length was just asserted");

        assert_eq!(diagnostic.kind, DiagnosticKind::Lex, "for {source:?}");
        assert_eq!(diagnostic.message, message, "for {source:?}");
        assert_eq!(
            source
                .as_bytes()
                .get(diagnostic.span.start..diagnostic.span.end),
            Some(offending.as_bytes()),
            "for {source:?}, the span should cover the whole offending construct"
        );
        assert_eq!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(&TokenKind::Eof),
            "for {source:?}, scanning must still reach the end"
        );
    }

    /// Every malformed-input path reports its own diagnostic, spanning the whole construct.
    #[test]
    fn each_malformed_construct_has_its_own_diagnostic() {
        let cases = [
            ("\"abc", "unterminated string literal", "\"abc"),
            ("\"abc\ndone", "unterminated string literal", "\"abc"),
            ("'a", "unterminated character literal", "'a"),
            ("''", "empty character literal", "''"),
            (
                "'ab'",
                "character literal must contain exactly one character",
                "'ab'",
            ),
            (r"'\q'", r"unknown escape sequence '\q'", r"\q"),
            (r#""a\qb""#, r"unknown escape sequence '\q'", r"\q"),
            ("/* open", "unterminated block comment", "/* open"),
            ("0x", "expected digits after '0x'", "0x"),
            ("0xzz", "invalid digit 'z' in hexadecimal literal", "0xzz"),
            ("08", "invalid digit '8' in octal literal", "08"),
            ("12ab", "invalid digit 'a' in decimal literal", "12ab"),
            (
                "2147483648",
                "integer literal is too large for 'int': 2147483648",
                "2147483648",
            ),
            (
                "99999999999999999999999",
                "integer literal is too large for 'int': 99999999999999999999999",
                "99999999999999999999999",
            ),
            ("@", "stray '@' in program", "@"),
            ("$", "stray '$' in program", "$"),
            ("`", "stray '`' in program", "`"),
        ];

        for (source, message, offending) in cases {
            assert_one_error(source, message, offending);
        }
    }

    /// Punctuation of real C that this grammar has no token for is named as unsupported rather
    /// than as a stray byte, in the same words the parser uses for the constructs it catches.
    ///
    /// The parser cannot report these: there is no token for `?` or `#`, so nothing about them
    /// ever reaches it.
    #[test]
    fn unsupported_punctuation_is_named_not_called_stray() {
        for character in ["&", "|", "#", "?", ":", "^", "~"] {
            assert_one_error(
                character,
                &format!("unsupported in this C subset: '{character}'"),
                character,
            );
        }
    }

    /// Each unsupported character says what to reach for instead.
    #[test]
    fn unsupported_punctuation_says_what_to_do_instead() {
        let cases = [
            ("a & b", "did you mean '&&'?"),
            ("a | b", "did you mean '||'?"),
            (
                "#include <stdio.h>",
                "this subset has no preprocessor, so no directive has any meaning here",
            ),
            (
                "a ? b : c",
                "the conditional operator is not in this subset; use an 'if' statement",
            ),
            ("a ^ b", "the bitwise operators are not in this subset"),
        ];

        for (source, note) in cases {
            let lexed = lex(source.as_bytes());
            let diagnostic = lexed
                .diagnostics
                .first()
                .unwrap_or_else(|| panic!("expected a diagnostic for {source:?}"));

            assert_eq!(diagnostic.notes, [note], "for {source:?}");
        }
    }

    /// An over-large literal says what the limit is and how to write `INT_MIN` within it.
    #[test]
    fn integer_overflow_explains_the_limit() {
        let lexed = lex(b"2147483648");
        let diagnostic = lexed.diagnostics.first().expect("expected a diagnostic");

        assert_eq!(
            diagnostic.notes,
            ["the maximum is 2147483647; write INT_MIN as -2147483647 - 1"]
        );
    }

    /// One bad construct produces one diagnostic, not a cascade.
    #[test]
    fn a_single_error_does_not_cascade() {
        for source in [r"'\q'", "0x", "'ab'", "\"unterminated"] {
            assert_eq!(
                lex(source.as_bytes()).diagnostics.len(),
                1,
                "for {source:?}"
            );
        }
    }

    /// An unterminated literal resynchronizes at the end of its line, so the next line still lexes.
    #[test]
    fn scanning_resumes_on_the_line_after_an_unterminated_literal() {
        let lexed = lex(b"\"oops\nint x;\n");

        assert_eq!(lexed.diagnostics.len(), 1);
        let kinds: Vec<_> = lexed.tokens.iter().map(|token| &token.kind).collect();
        assert_eq!(
            kinds,
            [
                &TokenKind::StrLit(b"oops".to_vec()),
                &TokenKind::Keyword(Keyword::Int),
                &TokenKind::Ident("x".into()),
                &TokenKind::Semi,
                &TokenKind::Eof,
            ]
        );
    }

    /// An unterminated block comment resynchronizes at end of file, swallowing the rest.
    #[test]
    fn an_unterminated_block_comment_runs_to_the_end_of_file() {
        let lexed = lex(b"int x;\n/* oops\nint y;\n");

        assert_eq!(lexed.diagnostics.len(), 1);
        let kinds: Vec<_> = lexed.tokens.iter().map(|token| &token.kind).collect();
        assert_eq!(
            kinds,
            [
                &TokenKind::Keyword(Keyword::Int),
                &TokenKind::Ident("x".into()),
                &TokenKind::Semi,
                &TokenKind::Eof,
            ]
        );
    }

    /// A file with several mistakes reports all of them, in source order.
    #[test]
    fn several_errors_are_reported_in_source_order() {
        let source = b"int a = 0x;\nchar c = '';\nint b = @;\nchar *s = \"open\n";
        let lexed = lex(source);

        let messages: Vec<_> = lexed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert_eq!(
            messages,
            [
                "expected digits after '0x'",
                "empty character literal",
                "stray '@' in program",
                "unterminated string literal",
            ]
        );

        let offsets: Vec<_> = lexed
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.span.start)
            .collect();
        let mut sorted = offsets.clone();
        sorted.sort_unstable();
        assert_eq!(offsets, sorted, "diagnostics must come out in source order");
    }

    /// A file of nothing but stray characters terminates, reporting one error per character.
    #[test]
    fn a_file_of_stray_characters_terminates() {
        let lexed = lex(b"@$#`@$#`");

        assert_eq!(lexed.diagnostics.len(), 8);
        assert_eq!(
            lexed
                .tokens
                .iter()
                .map(|token| &token.kind)
                .collect::<Vec<_>>(),
            [&TokenKind::Eof],
            "stray characters produce no tokens"
        );
    }

    /// An error path cannot loop: repeated unterminated constructs still reach the end.
    #[test]
    fn repeated_errors_still_terminate() {
        let lexed = lex(b"'\n'\n'\n'\n\"\n\"\n\"\n");

        assert!(!lexed.diagnostics.is_empty());
        assert_eq!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(&TokenKind::Eof)
        );
    }

    /// A backslash at the very end of a file is an unterminated literal, not an overrun.
    #[test]
    fn a_trailing_backslash_does_not_overrun() {
        for source in [r"'\", r#""\"#, r"'", r#"""#] {
            let lexed = lex(source.as_bytes());

            assert_eq!(lexed.diagnostics.len(), 1, "for {source:?}");
            assert_eq!(
                lexed.tokens.last().map(|token| &token.kind),
                Some(&TokenKind::Eof),
                "for {source:?}"
            );
        }
    }

    /// Spans are byte ranges over the original source, so slicing one back out gives the token.
    #[test]
    fn spans_slice_back_to_the_source_text() {
        let source = b"int total = 0xff;";
        let lexed = lex(source);

        for token in &lexed.tokens {
            if token.kind == TokenKind::Eof {
                continue;
            }
            assert!(
                source.get(token.span.start..token.span.end).is_some(),
                "span {:?} is not a valid range",
                token.span
            );
        }

        let literal = lexed.tokens.get(3).expect("expected the literal token");
        assert_eq!(literal.kind, TokenKind::IntLit(255));
        assert_eq!(
            source.get(literal.span.start..literal.span.end),
            Some(&b"0xff"[..])
        );
    }
}
