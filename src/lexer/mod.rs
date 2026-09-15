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
        at_line_start: true,
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
    /// Whether nothing but whitespace and comments has been scanned since the last newline, which is
    /// what makes a `#` the start of a preprocessor directive rather than a stray character.
    at_line_start: bool,
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
            self.at_line_start = false;
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

            b'#' if self.at_line_start => self.skip_directive(start),

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
                    // Only a newline in whitespace starts a line for a directive. One inside a block
                    // comment does not: C replaces the whole comment with a single space before it
                    // looks for directives (C11 5.1.1.2, phase 3).
                    if byte == b'\n' {
                        self.at_line_start = true;
                    }
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

    /// Skip a preprocessor directive whose `#`, at `start`, is already consumed, and report it once.
    ///
    /// A directive runs from a `#` that is the first token on its line to the end of that line
    /// (C11 6.10p2), and a backslash immediately before the newline splices the next line onto it
    /// (C11 5.1.1.2, phase 2). The whole directive is one unsupported construct: reporting only the
    /// `#` and lexing the rest as code would turn `#include <stdio.h>` into a cascade of complaints
    /// about `.` and `<`. The newline that ends the directive is left for trivia to consume, so the
    /// next line starts a line.
    fn skip_directive(&mut self, start: usize) -> Option<TokenKind> {
        while let Some(byte) = self.peek() {
            if byte == b'\n' && !self.is_spliced(self.offset) {
                break;
            }
            self.bump();
        }

        // Trailing whitespace, a `\r` before the newline included, is not part of the directive.
        let mut end = self.offset;
        while end > start + 1
            && self
                .byte_at(end - 1)
                .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            end -= 1;
        }

        let span = Span::new(start, end);
        self.report(
            Diagnostic::unsupported(DiagnosticKind::Lex, span, "preprocessor directives")
                .with_note(NO_PREPROCESSOR_NOTE),
        );

        None
    }

    /// Whether the newline at `newline` is spliced away by a backslash just before it, allowing for
    /// a carriage return between the two.
    fn is_spliced(&self, newline: usize) -> bool {
        let before = |distance: usize| {
            newline
                .checked_sub(distance)
                .and_then(|at| self.byte_at(at))
        };

        match before(1) {
            Some(b'\\') => true,
            Some(b'\r') => before(2) == Some(b'\\'),
            _ => false,
        }
    }

    /// A character that spells a C construct this subset leaves out entirely.
    fn unsupported_punctuation(&mut self, byte: u8, start: usize) -> Option<TokenKind> {
        let note = match byte {
            b'#' => NO_PREPROCESSOR_NOTE,
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

/// The note beneath any report of preprocessor syntax, whether a whole directive or a stray `#`.
const NO_PREPROCESSOR_NOTE: &str =
    "this subset has no preprocessor, so no directive has any meaning here";

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
mod tests;
