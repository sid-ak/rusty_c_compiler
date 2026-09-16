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
pub mod codegen;
pub mod diagnostics;
pub mod driver;
pub mod lexer;
pub mod parser;
pub mod runtime;
pub mod sema;

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
    /// The generated assembly, once the pipeline has gone that far.
    pub assembly: Option<String>,
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
    /// Something between assembly text and a finished program went wrong.
    Driver {
        /// What went wrong.
        cause: driver::DriverError,
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
            Error::Driver { cause } => write!(formatter, "{cause}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Read { cause, .. } => Some(cause),
            Error::Rejected { .. } => None,
            Error::Driver { cause } => Some(cause),
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
            ..Artifacts::default()
        });
    }

    let parsed = parser::parse(&lexed.tokens);
    if !parsed.diagnostics.is_empty() {
        return Err(parsed.diagnostics);
    }

    if options.stage() == Stage::Ast {
        return Ok(Artifacts {
            dump: Some(ast::dump(&parsed.program, ast::Spans::Hidden)),
            ..Artifacts::default()
        });
    }

    let analysis = sema::analyze(&parsed.program);
    if !analysis.diagnostics.is_empty() {
        return Err(analysis.diagnostics);
    }

    if options.stage() == Stage::Annotations {
        return Ok(Artifacts {
            dump: Some(analysis.annotations.dump()),
            ..Artifacts::default()
        });
    }

    if options.stage() == Stage::Check {
        return Ok(Artifacts::default());
    }

    let generated = codegen::generate(&parsed.program, &analysis.annotations);
    if !generated.diagnostics.is_empty() {
        return Err(generated.diagnostics);
    }

    Ok(Artifacts {
        assembly: Some(generated.assembly),
        ..Artifacts::default()
    })
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

            if let Some(assembly) = artifacts.assembly {
                emit(&assembly, options)?;
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

/// Writes `assembly` out, and builds it as far as the requested stage asks.
fn emit(assembly: &str, options: &Options) -> Result<(), Error> {
    if let Some(path) = &options.emit_asm_to {
        driver::write(path, assembly.as_bytes()).map_err(|cause| Error::Driver { cause })?;
    }

    let stage = options.stage();
    let product = match stage {
        Stage::Assembly => {
            // `-S` produces the assembly and nothing else, so it is written where the output would
            // otherwise have gone rather than through a temporary directory.
            let path = options
                .output
                .clone()
                .unwrap_or_else(|| default_output(&options.input, "s"));

            return driver::write(&path, assembly.as_bytes())
                .map_err(|cause| Error::Driver { cause });
        }
        Stage::Object => driver::Product::Object,
        _ => driver::Product::Executable,
    };

    let default_extension = if product == driver::Product::Object {
        "o"
    } else {
        ""
    };
    let output = options
        .output
        .clone()
        .unwrap_or_else(|| default_output(&options.input, default_extension));

    driver::build(assembly, &output, product, options.keep_temps)
        .map_err(|cause| Error::Driver { cause })
}

/// Where output goes when `-o` did not say: beside the input, with `extension`.
fn default_output(input: &Path, extension: &str) -> PathBuf {
    let stem = input.file_stem().unwrap_or_default();
    let mut path = PathBuf::from(stem);
    if !extension.is_empty() {
        path.set_extension(extension);
    }

    path
}

#[cfg(test)]
mod tests;
