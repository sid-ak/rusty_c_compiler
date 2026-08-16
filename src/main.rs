//! The `rustycc` binary: parse argv, run the compiler, map the result to an exit code.
//!
//! Everything else lives in the library so integration tests can drive the compiler in process
//! rather than through a child process.

use std::process::ExitCode;

use clap::Parser;

use rustycc::cli::Options;

/// Parse the command line and run one compilation.
fn main() -> ExitCode {
    let options = Options::parse();

    match rustycc::run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rustycc: {error}");
            ExitCode::FAILURE
        }
    }
}
