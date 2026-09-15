//! Snapshots of the AST for the test corpus, and the checks that keep the corpus honest.
//!
//! A unit test proves one production in isolation; a snapshot proves they compose, and turns a
//! regression anywhere in the parser into a readable diff rather than one failed assertion. The
//! corpus these run over is the same `tests/programs/` that Phase 4 executes and Phase 5 compares
//! against `clang`, so a program earns its keep more than once.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a corpus file that cannot be read is a broken
// checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rustycc::ast::{self, Spans};
use rustycc::lexer;
use rustycc::parser;

/// Where the corpus lives, relative to the crate root Cargo runs tests from.
const CORPUS: &str = "tests/programs";

/// Every `.c` file in the corpus, by file name, in a stable order.
///
/// Sorted, because a directory listing is in whatever order the file system feels like and a test
/// that depends on it fails on someone else's machine for no reason anyone can act on.
fn corpus() -> BTreeMap<String, PathBuf> {
    let entries = fs::read_dir(CORPUS).expect("the corpus directory should exist");

    entries
        .filter_map(|entry| {
            let path = entry.expect("a corpus entry should be readable").path();
            let is_program = path.extension().is_some_and(|extension| extension == "c");

            is_program.then(|| (file_name(&path), path))
        })
        .collect()
}

/// The file name of `path`, which is how a program is named in the coverage matrix.
fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

/// Parse `path`, asserting it lexes and parses cleanly, and return its AST dump.
fn dump(path: &Path) -> String {
    let source = fs::read(path).expect("a corpus program should be readable");
    let lexed = lexer::lex(&source);
    assert!(
        lexed.diagnostics.is_empty(),
        "{} should lex cleanly, got: {:?}",
        path.display(),
        lexed.diagnostics
    );

    let parsed = parser::parse(&lexed.tokens);
    assert!(
        parsed.diagnostics.is_empty(),
        "{} should parse cleanly, got: {:?}",
        path.display(),
        parsed.diagnostics
    );

    ast::dump(&parsed.program, Spans::Hidden)
}

/// Every program in the corpus has a row in the coverage matrix.
///
/// A program with no row is a program whose purpose nobody wrote down, which is how a corpus turns
/// from a record of what is covered into a pile of files.
#[test]
fn every_program_is_in_the_coverage_matrix() {
    let matrix = fs::read_to_string(Path::new(CORPUS).join("COVERAGE.md"))
        .expect("the coverage matrix should exist");

    for name in corpus().keys() {
        assert!(
            matrix.contains(name),
            "{name} has no entry in {CORPUS}/COVERAGE.md"
        );
    }
}

/// The corpus is not empty, so the checks over it are not passing by having nothing to check.
#[test]
fn the_corpus_has_programs_in_it() {
    assert!(corpus().len() >= 5, "expected a program per feature area");
}

/// Every program in the corpus parses, which is the exit criterion `--dump-ast` is held to.
#[test]
fn every_program_parses() {
    for (name, path) in corpus() {
        assert!(
            dump(&path).starts_with("(program\n"),
            "{name} should dump a program"
        );
    }
}

/// Dumping the same program twice produces the same bytes.
///
/// Snapshots are only worth having if the thing being snapshotted is a function of its input, so
/// this is checked directly rather than inferred from the snapshots happening to pass.
#[test]
fn the_dump_is_deterministic_across_runs() {
    for (name, path) in corpus() {
        assert_eq!(dump(&path), dump(&path), "{name} dumped differently twice");
    }
}

/// The whole tree for the arithmetic corpus program: every operator and how they group.
#[test]
fn arithmetic_program_tree() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("arithmetic.c")));
}

/// The whole tree for the control-flow corpus program: every branch and loop form.
#[test]
fn control_flow_program_tree() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("control_flow.c")));
}

/// The whole tree for the functions corpus program: declarations, definitions, and calls.
#[test]
fn functions_program_tree() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("functions.c")));
}

/// The whole tree for the arrays corpus program: declaration, indexing, and passing.
#[test]
fn arrays_program_tree() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("arrays.c")));
}

/// The whole tree for the strings corpus program: literals, `char` arrays, and escapes.
#[test]
fn strings_program_tree() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("strings.c")));
}

/// A dump with spans on, for one small program, so the positions the parser records are pinned
/// somewhere rather than only being asserted to exist.
#[test]
fn spans_are_recorded_for_every_node() {
    let source = b"int add(int a, int b) { return a + b; }";
    let lexed = lexer::lex(source);
    let parsed = parser::parse(&lexed.tokens);

    insta::assert_snapshot!(ast::dump(&parsed.program, Spans::Shown), @r###"
    (program
      (func-def int add@0..39
        (params
          (param int a@8..13)
          (param int b@15..20))
        (block
          (return@24..37
            (binary +@31..36
              (ident a@31..32)
              (ident b@35..36))))))
    "###);
}
