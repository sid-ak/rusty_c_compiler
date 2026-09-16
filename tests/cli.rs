//! Command-line surface tests: the binary's usage and exit codes, and the fact that the compiler
//! is reachable as a library so later phases can test it without spawning a process.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the corpus helper below is test scaffolding too, and a corpus directory that cannot be read is a
// broken checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used)]

use std::path::Path;
use std::process::Command;

use rustycc::cli::{Options, Stage};

/// Path to the freshly built `rustycc` binary, provided by Cargo for integration tests.
const RUSTYCC: &str = env!("CARGO_BIN_EXE_rustycc");

/// Running `rustycc` with no arguments prints usage and exits non-zero.
#[test]
fn no_arguments_prints_usage_and_fails() {
    let output = Command::new(RUSTYCC).output().unwrap();

    assert!(!output.status.success(), "expected a non-zero exit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage") || stderr.contains("usage"),
        "expected usage text, got: {stderr}"
    );
}

/// A missing input file is reported readably rather than as a panic or a bare exit code.
#[test]
fn missing_input_file_reports_a_readable_error() {
    let output = Command::new(RUSTYCC)
        .arg("definitely-not-here.c")
        .output()
        .unwrap();

    assert!(!output.status.success(), "expected a non-zero exit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("definitely-not-here.c"),
        "expected the path in the message, got: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "expected an error, not a panic: {stderr}"
    );
}

/// The compiler is callable as a library, with no child process and no file system access.
#[test]
fn compiler_is_callable_in_process() {
    let options = Options::for_source(Path::new("in-memory.c"), Stage::Tokens);

    rustycc::compile(b"", Path::new("in-memory.c"), &options)
        .expect("an empty translation unit is valid and should produce no diagnostics");
}

/// `rustycc --check` exits 0 for every valid program in the corpus and prints nothing.
///
/// The exit code is the whole interface here: `--check` is what a build script or an editor would
/// run, and its answer has to be readable without parsing any output.
#[test]
fn check_accepts_every_valid_program() {
    for path in corpus("tests/programs") {
        let output = Command::new(RUSTYCC)
            .args(["--check"])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{} should pass --check, stderr: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stdout.is_empty(),
            "{} should print nothing on success",
            path.display()
        );
    }
}

/// `rustycc --check` exits non-zero for every program in the invalid corpus, with a rendered error.
#[test]
fn check_rejects_every_invalid_program() {
    for path in corpus("tests/programs/invalid") {
        let output = Command::new(RUSTYCC)
            .args(["--check"])
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "{} should fail --check",
            path.display()
        );

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("error:"),
            "{} should report a rendered error, got: {stderr}",
            path.display()
        );
        assert!(
            !stderr.contains("panicked"),
            "{} made the compiler panic: {stderr}",
            path.display()
        );
    }
}

/// `rustycc --dump-annotations` prints what analysis recorded, for a program that analyzes.
#[test]
fn dump_annotations_prints_the_annotation_tables() {
    let output = Command::new(RUSTYCC)
        .args(["--dump-annotations", "tests/programs/arrays.c"])
        .output()
        .unwrap();

    assert!(output.status.success(), "expected a zero exit");

    let stdout = String::from_utf8_lossy(&output.stdout);
    for table in ["types", "conversions", "bindings", "frames", "strings"] {
        assert!(
            stdout.contains(table),
            "expected a {table} table, got: {stdout}"
        );
    }
}

/// Every `.c` file directly inside `directory`, in a stable order.
fn corpus(directory: &str) -> Vec<std::path::PathBuf> {
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(directory)
        .expect("the corpus directory should exist")
        .filter_map(|entry| {
            let path = entry.expect("a corpus entry should be readable").path();
            let is_program = path.extension().is_some_and(|extension| extension == "c");

            is_program.then_some(path)
        })
        .collect();
    paths.sort();

    assert!(!paths.is_empty(), "{directory} has no programs in it");

    paths
}
