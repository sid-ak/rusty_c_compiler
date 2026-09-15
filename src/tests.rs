//! Unit tests for the crate root: compiling source in process and reading the input file.

use super::*;

use crate::cli::Stage;

/// An empty translation unit is valid C and compiles without diagnostics.
#[test]
fn empty_source_compiles() {
    let options = Options::for_source(Path::new("empty.c"), Stage::Tokens);

    assert!(compile(b"", Path::new("empty.c"), &options).is_ok());
}

/// A missing input file is an `Error::Read` naming the path, not a panic.
#[test]
fn missing_input_is_a_read_error() {
    let options = Options::for_source(Path::new("no-such-file.c"), Stage::Tokens);

    match run(&options) {
        Err(Error::Read { path, .. }) => assert_eq!(path, Path::new("no-such-file.c")),
        other => panic!("expected a read error, got {other:?}"),
    }
}

/// The rendered form of a read failure names the path and the underlying cause.
#[test]
fn read_error_message_names_the_path() {
    let error = Error::Read {
        path: PathBuf::from("missing.c"),
        cause: io::Error::new(io::ErrorKind::NotFound, "No such file or directory"),
    };

    assert_eq!(
        error.to_string(),
        "cannot read 'missing.c': No such file or directory"
    );
}

/// The rejection summary agrees in number with the count it reports.
#[test]
fn rejection_message_agrees_in_number() {
    assert_eq!(
        Error::Rejected { count: 1 }.to_string(),
        "1 error generated"
    );
    assert_eq!(
        Error::Rejected { count: 3 }.to_string(),
        "3 errors generated"
    );
}
