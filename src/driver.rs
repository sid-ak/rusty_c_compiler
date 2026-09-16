//! The driver: assembly text becomes an object file, and an object file becomes a program.
//!
//! This compiler does not contain an assembler or a linker, and
//! [ADR 0009](../docs/decisions/0009-clang-as-assembler-and-linker.md) explains why it drives
//! `clang` instead of `as` and `ld`: `clang` already knows where the SDK is and which startup files
//! a macOS executable needs, and both it and they arrive together with the Xcode Command Line
//! Tools. Writing that out by hand would be a second thing to keep current with every OS release,
//! for no gain.
//!
//! Everything here is about being a well-behaved command-line program. Intermediate files go in a
//! directory of their own and are removed however the run ends, unless the caller asked to keep
//! them. A failure in a child process is reported with what the child said, because "clang exited
//! with 1" tells nobody anything.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::runtime::SHIM_OBJECT;

/// Something that went wrong between producing assembly and producing a program.
#[derive(Debug)]
pub enum DriverError {
    /// The C toolchain is not installed, or is not on the path.
    ToolchainMissing {
        /// Why running it failed.
        cause: io::Error,
    },
    /// A file could not be written or removed.
    File {
        /// The path involved.
        path: PathBuf,
        /// Why the operation failed.
        cause: io::Error,
    },
    /// `clang` ran and refused the work.
    Toolchain {
        /// What it was asked to do, for the message.
        step: &'static str,
        /// Whatever it printed, which is the part worth reading.
        output: String,
    },
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::ToolchainMissing { cause } => write!(
                formatter,
                "cannot run 'clang' ({cause}); install the Xcode Command Line Tools with \
                 'xcode-select --install'"
            ),
            DriverError::File { path, cause } => {
                write!(formatter, "cannot write '{}': {cause}", path.display())
            }
            DriverError::Toolchain { step, output } => {
                write!(formatter, "{step} failed:\n{}", output.trim_end())
            }
        }
    }
}

impl std::error::Error for DriverError {}

/// What the driver was asked to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    /// An object file, stopping before the link.
    Object,
    /// A linked executable.
    Executable,
}

/// Turns `assembly` into `output`, going as far as `product` asks.
///
/// `keep_temps` leaves the scratch directory in place and names it on stderr, which is the only way
/// to look at the assembly behind a link failure.
pub fn build(
    assembly: &str,
    output: &Path,
    product: Product,
    keep_temps: bool,
) -> Result<(), DriverError> {
    preflight()?;

    let workspace = Workspace::new(keep_temps)?;
    let source = workspace.path().join("program.s");
    write(&source, assembly.as_bytes())?;

    match product {
        Product::Object => assemble(&source, output)?,
        Product::Executable => {
            let object = workspace.path().join("program.o");
            assemble(&source, &object)?;
            link(&object, output)?;
        }
    }

    Ok(())
}

/// Checks that `clang` can be run at all, before anything depends on it.
///
/// A missing toolchain is the one failure with an obvious remedy, so it is worth telling apart from
/// a compilation error rather than letting it surface as one.
pub fn preflight() -> Result<(), DriverError> {
    Command::new("clang")
        .arg("--version")
        .output()
        .map(|_| ())
        .map_err(|cause| DriverError::ToolchainMissing { cause })
}

/// Assembles `source` into `object`.
fn assemble(source: &Path, object: &Path) -> Result<(), DriverError> {
    run(
        "assembling",
        Command::new("clang")
            .args(["-c"])
            .arg(source)
            .arg("-o")
            .arg(object),
    )
}

/// Links `object` with the runtime shim into `output`.
fn link(object: &Path, output: &Path) -> Result<(), DriverError> {
    run(
        "linking",
        Command::new("clang")
            .arg(object)
            .arg(SHIM_OBJECT)
            .arg("-o")
            .arg(output),
    )
}

/// Runs `command`, turning a non-zero exit into a message carrying whatever it printed.
fn run(step: &'static str, command: &mut Command) -> Result<(), DriverError> {
    let finished = command
        .output()
        .map_err(|cause| DriverError::ToolchainMissing { cause })?;

    if finished.status.success() {
        return Ok(());
    }

    let mut output = String::from_utf8_lossy(&finished.stderr).into_owned();
    if output.trim().is_empty() {
        output = String::from_utf8_lossy(&finished.stdout).into_owned();
    }

    Err(DriverError::Toolchain { step, output })
}

/// Writes `contents` to `path`.
pub fn write(path: &Path, contents: &[u8]) -> Result<(), DriverError> {
    fs::write(path, contents).map_err(|cause| DriverError::File {
        path: path.to_path_buf(),
        cause,
    })
}

/// A directory for intermediate files, removed when it goes out of scope.
///
/// Removal is in `Drop` rather than at the end of a successful run, so it happens on the failing
/// paths too — which is most of them, and the ones nobody remembers to clean up by hand.
struct Workspace {
    /// Where the files are.
    path: PathBuf,
    /// Whether to leave them behind.
    keep: bool,
}

impl Workspace {
    /// A fresh directory nothing else is using.
    ///
    /// The name carries the process id and the clock, so two compilers running at once get
    /// different directories rather than writing over each other's `program.s`.
    fn new(keep: bool) -> Result<Self, DriverError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let path = std::env::temp_dir().join(format!("rustycc-{}-{nanos}", std::process::id()));

        fs::create_dir_all(&path).map_err(|cause| DriverError::File {
            path: path.clone(),
            cause,
        })?;

        if keep {
            eprintln!("rustycc: keeping intermediates in {}", path.display());
        }

        Ok(Self { path, keep })
    }

    /// Where the intermediate files go.
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        if self.keep {
            return;
        }

        // Nothing useful can be done if this fails, and failing to tidy up is not a reason to turn
        // a successful compilation into an error.
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests;
