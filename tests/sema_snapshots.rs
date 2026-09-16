//! Snapshots of the annotation set for the test corpus, and the checks that keep it honest.
//!
//! What the analyzer records matters more than what it rejects: it is the entire interface between
//! the front end and code generation, and a gap in it becomes a missing lookup in Phase 4 rather
//! than an error message here. Snapshotting it turns any change to that interface into a readable
//! diff over real programs.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a corpus file that cannot be read is a broken
// checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rustycc::lexer;
use rustycc::parser;
use rustycc::sema;

/// Where the corpus lives, relative to the crate root Cargo runs tests from.
const CORPUS: &str = "tests/programs";

/// Every `.c` file in the corpus, by file name, in a stable order.
fn corpus() -> BTreeMap<String, PathBuf> {
    let entries = fs::read_dir(CORPUS).expect("the corpus directory should exist");

    entries
        .filter_map(|entry| {
            let path = entry.expect("a corpus entry should be readable").path();
            let is_program = path.extension().is_some_and(|extension| extension == "c");

            is_program.then(|| {
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string();

                (name, path)
            })
        })
        .collect()
}

/// Analyze `path`, asserting it gets through every pass cleanly, and return its annotation dump.
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

    let analysis = sema::analyze(&parsed.program);
    assert!(
        analysis.is_accepted(),
        "{} should analyze cleanly, got: {:?}",
        path.display(),
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.message)
            .collect::<Vec<_>>()
    );

    analysis.annotations.dump()
}

/// Every program in the corpus analyzes, which is the exit criterion `--check` is held to.
#[test]
fn every_program_analyzes() {
    for (name, path) in corpus() {
        assert!(
            dump(&path).starts_with("types\n"),
            "{name} should produce an annotation dump"
        );
    }
}

/// Every program in the corpus has a frame for each function it defines.
///
/// A missing frame is the failure mode that matters: the backend would have nowhere to put a
/// function's locals, and nothing before this point would have noticed.
#[test]
fn every_defined_function_has_a_frame() {
    for (name, path) in corpus() {
        let source = fs::read(&path).expect("a corpus program should be readable");
        let lexed = lexer::lex(&source);
        let parsed = parser::parse(&lexed.tokens);
        let analysis = sema::analyze(&parsed.program);

        for item in &parsed.program.items {
            let rustycc::ast::Item::FuncDef(def) = item else {
                continue;
            };
            let function = &def.signature.name.text;

            assert!(
                analysis.annotations.frame(function).is_some(),
                "{name}: '{function}' has no frame"
            );
        }
    }
}

/// Analyzing the same program twice produces the same annotations.
///
/// Snapshots are only worth having if the thing being snapshotted is a function of its input, so
/// this is checked directly rather than inferred from the snapshots happening to pass.
#[test]
fn the_dump_is_deterministic_across_runs() {
    for (name, path) in corpus() {
        assert_eq!(dump(&path), dump(&path), "{name} dumped differently twice");
    }
}

/// The annotations for the arithmetic corpus program: the type of every operator and operand.
#[test]
fn arithmetic_program_annotations() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("arithmetic.c")));
}

/// The annotations for the control-flow corpus program: conditions and the frames around them.
#[test]
fn control_flow_program_annotations() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("control_flow.c")));
}

/// The annotations for the functions corpus program: signatures, calls, and parameter slots.
#[test]
fn functions_program_annotations() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("functions.c")));
}

/// The annotations for the arrays corpus program: where decay is recorded, and where it is not.
#[test]
fn arrays_program_annotations() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("arrays.c")));
}

/// The annotations for the strings corpus program: interned literals and `char` promotions.
#[test]
fn strings_program_annotations() {
    insta::assert_snapshot!(dump(&Path::new(CORPUS).join("strings.c")));
}
