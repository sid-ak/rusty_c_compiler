//! Raw bytes into the lexer.
//!
//! The property: any sequence of bytes at all produces a token stream and some diagnostics, and
//! never a panic and never a hang. A C file is not guaranteed to be valid UTF-8 and is not
//! guaranteed to be C, and the lexer is the first thing that sees it, so it is the pass with the
//! least reason to assume anything about its input.
//!
//! Termination is checked as well as absence of panics: the scanner advances by hand, and a
//! resynchronization path that forgets to consume a byte is an infinite loop rather than a crash.
//! `libfuzzer` reports an input that runs too long as a timeout, which is a finding here.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let lexed = rustycc::lexer::lex(data);

    // The stream always ends in `Eof`, whatever went wrong on the way there, because every pass
    // after this one reads until it sees one. A stream without it is a hang waiting to happen.
    assert!(
        matches!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(rustycc::lexer::token::TokenKind::Eof)
        ),
        "the token stream does not end in Eof"
    );

    // Every span points inside the input, which is what the renderer assumes when it goes looking
    // for the line to print underneath a diagnostic.
    for token in &lexed.tokens {
        assert!(
            token.span.start <= data.len() && token.span.end <= data.len(),
            "a token's span leaves the input"
        );
    }
});
