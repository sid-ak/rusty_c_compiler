//! Raw bytes through the whole front end: lexer, parser, and semantic analysis.
//!
//! Semantic analysis has recursion and indexing of its own — a scope stack, a side table keyed by
//! node id, a walk over the tree that is separate from the parser's — so it needs its own target
//! rather than being assumed safe because the two passes in front of it are.
//!
//! The property is the one every pass holds to: a program is accepted or it is reported, never
//! neither and never by crashing. An input the parser rejected is still handed on, because a later
//! pass is not allowed to assume it is seeing a tree an earlier pass approved of.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let lexed = rustycc::lexer::lex(data);
    let parsed = rustycc::parser::parse(&lexed.tokens);

    // Analysis runs whether or not parsing reported anything. A tree built during error recovery is
    // the one shape this pass will never see in ordinary use and the one most likely to break it.
    let analysis = rustycc::sema::analyze(&parsed.program);

    // Every annotation is dumped, which walks the tree again and reads the side table through the
    // node ids the parser handed out. An id that collided or a node that was never annotated is a
    // wrong lookup here rather than a wrong instruction much later.
    let dump = analysis.annotations.dump();

    assert!(
        analysis.is_accepted() || !analysis.diagnostics.is_empty(),
        "a program was neither accepted nor reported on"
    );
    assert!(
        dump.is_empty() || dump.ends_with('\n'),
        "an annotation dump that does not end in a newline"
    );
});
