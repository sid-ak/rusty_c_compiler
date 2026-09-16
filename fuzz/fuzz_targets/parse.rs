//! Raw bytes through the lexer and the parser.
//!
//! The property: any input produces a tree and some diagnostics, never a panic, never a hang, and
//! never a stack overflow. The parser is recursive, so the last of those is the one that needs a
//! guard rather than care — nested parentheses cost a stack frame each, and a file of ten thousand
//! of them would exhaust any stack the compiler runs on. The depth limit is what makes that a
//! diagnostic instead.
//!
//! Error recovery is the other half. On a syntax error the parser skips ahead and carries on, and
//! a recovery step that can consume nothing is an infinite loop — which shows up here as a timeout
//! rather than as a wrong tree.

#![no_main]

use libfuzzer_sys::fuzz_target;

use rustycc::ast::{self, Spans};

fuzz_target!(|data: &[u8]| {
    let lexed = rustycc::lexer::lex(data);
    let parsed = rustycc::parser::parse(&lexed.tokens);

    // Dumping walks the tree, which is a second recursion over it and the one the depth limit has
    // to bound as well: a loop in the parser can build a tree deeper than the parser ever recursed.
    let dump = ast::dump(&parsed.program, Spans::Shown);

    assert!(
        dump.starts_with("(program"),
        "a dump that is not a program"
    );
});
