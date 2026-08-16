//! Command-line surface tests: the binary's usage and exit codes, and the fact that the compiler
//! is reachable as a library so later phases can test it without spawning a process.

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
