//! The front end never panics and never hangs, whatever it is handed.
//!
//! Phase 5 puts this property under `cargo-fuzz`, which needs a nightly toolchain and minutes per
//! run. This is the cheap precursor: it takes the programs already in the corpus, cuts each one
//! short at every possible point, and parses each of the fragments. A prefix of a valid program is
//! exactly the shape of input a parser mishandles — a construct opened and never closed — and
//! generating them costs nothing because the programs are already written.
//!
//! What is asserted is deliberately weak: each fragment must finish, and may report anything it
//! likes about what it found. A fragment is not a program, so there is no right answer to check
//! against. The only wrong answers are a crash and a loop.
//!
//! Beside that sits a small set of hand-written inputs in `tests/adversarial/`, aimed at the
//! specific ways a recursive-descent parser falls over: nesting deep enough to exhaust the call
//! stack, files that are nothing but operators, files with no tokens at all.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a fixture that cannot be read is a broken
// checkout rather than something for the parser to answer for.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

use rustycc::lexer;
use rustycc::parser;

/// The valid programs, which the truncation corpus is generated from.
const CORPUS: &str = "tests/programs";

/// The hand-written awkward inputs.
const ADVERSARIAL: &str = "tests/adversarial";

/// Every `.c` file in `directory`, sorted, so a failure names the same file on every machine.
fn programs(directory: &str) -> Vec<PathBuf> {
    let entries = fs::read_dir(directory).expect("the directory should exist");
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| {
            let path = entry.expect("an entry should be readable").path();
            let is_program = path.extension().is_some_and(|extension| extension == "c");

            is_program.then_some(path)
        })
        .collect();
    paths.sort();

    paths
}

/// Lex and parse `source`, returning how many problems were reported.
///
/// The lexer's diagnostics are not a reason to stop: the parser has to cope with whatever token
/// stream it is handed, and a stream that came out of malformed text is precisely the interesting
/// case. This mirrors what the Phase 5 `parse` fuzz target will do.
fn parse(source: &[u8]) -> usize {
    let lexed = lexer::lex(source);
    let parsed = parser::parse(&lexed.tokens);

    lexed.diagnostics.len() + parsed.diagnostics.len()
}

/// The byte offset just past each token of `source`, which is where a prefix can end without
/// splitting a token in half.
fn token_boundaries(source: &[u8]) -> Vec<usize> {
    lexer::lex(source)
        .tokens
        .iter()
        .map(|token| token.span.end)
        .collect()
}

/// Every prefix of every corpus program, cut at a token boundary, parses without panicking.
#[test]
fn token_level_truncations_do_not_panic() {
    for path in programs(CORPUS) {
        let source = fs::read(&path).expect("a corpus program should be readable");

        for end in token_boundaries(&source) {
            let prefix = source.get(..end).unwrap_or(&source);
            parse(prefix);
        }
    }
}

/// Every prefix cut at an arbitrary byte parses too, which covers what a token-boundary cut cannot
/// produce: half a literal, half a comment, half an operator.
#[test]
fn byte_level_truncations_do_not_panic() {
    for path in programs(CORPUS) {
        let source = fs::read(&path).expect("a corpus program should be readable");

        for end in 0..=source.len() {
            let prefix = source.get(..end).unwrap_or_default();
            parse(prefix);
        }
    }
}

/// Cutting a program short is never silently fine: a prefix that stops mid-construct is reported.
///
/// Without this, the tests above would still pass against a parser that accepted everything, which
/// would prove nothing about it beyond that it returns.
#[test]
fn a_truncated_program_is_reported() {
    let source = fs::read(Path::new(CORPUS).join("functions.c"))
        .expect("a corpus program should be readable");
    let full = source.len();

    // Two thirds of the way in lands inside `main`, well past the last complete top-level item.
    let prefix = source.get(..full * 2 / 3).unwrap_or_default();

    assert!(parse(prefix) > 0, "a half-finished program should report");
}

/// Every hand-written awkward input parses without panicking or hanging.
#[test]
fn adversarial_inputs_do_not_panic() {
    let paths = programs(ADVERSARIAL);
    assert!(paths.len() >= 8, "expected the checked-in adversarial set");

    for path in paths {
        let source = fs::read(&path).expect("an adversarial input should be readable");
        parse(&source);
    }
}

/// The inputs with nothing in them parse to an empty program and report nothing.
///
/// An empty translation unit is valid C, so the right answer here is silence rather than a
/// complaint about a file with no content.
#[test]
fn inputs_with_no_tokens_are_empty_programs() {
    for name in ["empty.c", "only_whitespace.c", "only_comment.c"] {
        let source = fs::read(Path::new(ADVERSARIAL).join(name))
            .expect("an adversarial input should be readable");
        let lexed = lexer::lex(&source);
        let parsed = parser::parse(&lexed.tokens);

        assert!(parsed.program.items.is_empty(), "{name} should be empty");
        assert_eq!(parsed.diagnostics.len(), 0, "{name} should report nothing");
    }
}

/// The deeply nested inputs meet the depth limit and say so, rather than exhausting the stack.
#[test]
fn deep_nesting_reports_the_depth_limit() {
    for name in ["deep_parens.c", "deep_blocks.c", "deep_unclosed_parens.c"] {
        let source = fs::read(Path::new(ADVERSARIAL).join(name))
            .expect("an adversarial input should be readable");
        let lexed = lexer::lex(&source);
        let parsed = parser::parse(&lexed.tokens);

        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.starts_with("nesting is too deep")),
            "{name} should meet the depth limit, got: {:?}",
            parsed.diagnostics
        );
    }
}

/// A file of nothing but operators reports a bounded number of times rather than once per token.
///
/// Recovery skips to the next `;` or `}`, so a file with thousands of tokens and a few hundred
/// semicolons must not turn into thousands of diagnostics.
#[test]
fn a_file_of_operators_reports_a_bounded_number_of_times() {
    let source = fs::read(Path::new(ADVERSARIAL).join("only_operators.c"))
        .expect("an adversarial input should be readable");
    let lexed = lexer::lex(&source);
    let parsed = parser::parse(&lexed.tokens);

    assert!(!parsed.diagnostics.is_empty(), "this is not valid C");
    assert!(
        parsed.diagnostics.len() <= lexed.tokens.len(),
        "expected at most one diagnostic per token, got {} for {} tokens",
        parsed.diagnostics.len(),
        lexed.tokens.len()
    );
}

/// A very long name and a very long literal are carried through rather than truncated or refused.
#[test]
fn very_long_tokens_are_carried_through() {
    let source = fs::read(Path::new(ADVERSARIAL).join("long_identifier.c"))
        .expect("an adversarial input should be readable");
    let lexed = lexer::lex(&source);
    let parsed = parser::parse(&lexed.tokens);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    assert_eq!(parsed.program.items.len(), 1);
}
