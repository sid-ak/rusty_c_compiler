//! A compiler for a subset of C, emitting ARM64 assembly for Apple Silicon macOS.
//!
//! The design, the accepted grammar, and the invariants every pass holds to are in
//! `docs/architecture.md`. The two entry points here mirror the split that document describes:
//!
//! - [`compile`] is the compiler proper. It takes source bytes and returns either artifacts or
//!   diagnostics, touching no files and spawning no processes, so tests can drive the whole
//!   pipeline in process.
//! - [`run`] is the command-line behavior around it: read the input, call [`compile`], render
//!   whatever came back, and produce the requested output.

#![deny(missing_docs)]

pub mod ast;
pub mod cli;
pub mod diagnostics;
pub mod lexer;
pub mod parser;
pub mod runtime;

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli::{Options, Stage};
use crate::diagnostics::{Diagnostic, SourceMap};

/// What a compilation run produced, as far as [`cli::Options::stage`] asked it to go.
///
/// Each stage of the pipeline fills in its own field, so a caller asking to stop early gets
/// exactly what that stage produced and nothing else.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Artifacts {
    /// The human-readable dump the requested `--dump-*` stage produced.
    pub dump: Option<String>,
}

/// A failure that is not a diagnostic about the user's program.
///
/// Diagnostics describe something wrong with the C being compiled and are rendered against the
/// source; these describe something wrong with the invocation itself.
#[derive(Debug)]
pub enum Error {
    /// The input file could not be read.
    Read {
        /// The path that could not be read.
        path: PathBuf,
        /// Why the read failed.
        cause: io::Error,
    },
    /// The program was rejected. The diagnostics have already been rendered to stderr.
    Rejected {
        /// How many diagnostics were reported.
        count: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Read { path, cause } => {
                write!(formatter, "cannot read '{}': {cause}", path.display())
            }
            Error::Rejected { count } => {
                let plural = if *count == 1 { "error" } else { "errors" };
                write!(formatter, "{count} {plural} generated")
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Read { cause, .. } => Some(cause),
            Error::Rejected { .. } => None,
        }
    }
}

/// Compile `source`, named `path` for the purpose of diagnostics, as far as `options` requires.
///
/// The source is bytes rather than a `&str` because a C file is not guaranteed to be valid UTF-8,
/// and the lexer is a fuzz target handed arbitrary input; malformed bytes must produce a
/// diagnostic rather than fail before the compiler starts.
///
/// Every problem found is returned, not just the first, so one run reports as much as it can.
pub fn compile(
    source: &[u8],
    path: &Path,
    options: &Options,
) -> Result<Artifacts, Vec<Diagnostic>> {
    let lexed = lexer::lex(source);
    if !lexed.diagnostics.is_empty() {
        return Err(lexed.diagnostics);
    }

    if options.stage() == Stage::Tokens {
        let source_map = SourceMap::new(path, source);

        return Ok(Artifacts {
            dump: Some(lexer::dump(&source_map, &lexed.tokens)),
        });
    }

    let parsed = parser::parse(&lexed.tokens);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }

    if options.stage() == Stage::Ast {
        return Ok(Artifacts {
            dump: Some(ast::dump(&parsed.program, ast::Spans::Hidden)),
        });
    }

    Ok(Artifacts::default())
}

/// Read `options.input`, compile it, and emit whatever the requested stage produces.
pub fn run(options: &Options) -> Result<(), Error> {
    let source = fs::read(&options.input).map_err(|cause| Error::Read {
        path: options.input.clone(),
        cause,
    })?;

    match compile(&source, &options.input, options) {
        Ok(artifacts) => {
            if let Some(dump) = artifacts.dump {
                print!("{dump}");
            }
            Ok(())
        }
        Err(diagnostics) => {
            let source_map = SourceMap::new(&options.input, &source);
            for diagnostic in &diagnostics {
                eprintln!("{}", source_map.render(diagnostic));
            }
            Err(Error::Rejected {
                count: diagnostics.len(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
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
}
