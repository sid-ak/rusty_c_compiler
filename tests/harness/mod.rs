//! Building a subset-C program two ways, running it, and comparing the two runs.
//!
//! This is the plumbing under the differential suite ([`tests/differential.rs`]) and the generated
//! corpus ([`tests/generated.rs`]). It lives in one place because the two harnesses differ only in
//! where their programs come from: everything after "here is a `.c` file" — assemble, link, run
//! under a timeout, compare — is the same work, and a second copy of it would be a second place for
//! the comparison rules to drift.
//!
//! The comparison is deliberately split from the running. [`compare`] is a pure function over two
//! [`Execution`] records, which is what lets the harness's own self-tests inject a wrong answer on
//! each axis without needing a compiler that produces one.
//!
//! [`tests/differential.rs`]: ../differential/index.html
//! [`tests/generated.rs`]: ../generated/index.html

// This module is compiled into every test binary that declares it, and no single binary uses all of
// it. Without this, a helper used only by the generated corpus is dead code in the differential
// binary and vice versa, which `-D warnings` would turn into a build failure.
#![allow(dead_code)]
// Harness code is test code: a scratch directory that cannot be created or a `clang` that cannot be
// spawned is a broken checkout, not something to report a diagnostic about. `clippy.toml` exempts
// `#[test]` bodies only, and none of this is one.
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use rustycc::cli::{Options, Stage};
use rustycc::runtime::SHIM_OBJECT;

/// How long a compiled program may run before the harness kills it.
///
/// Every program in the corpus finishes in milliseconds, so this is not a performance budget — it is
/// the line between "slow" and "will never finish", which is the only thing a harness can tell
/// about a program it did not write.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// The environment variable that overrides [`DEFAULT_TIMEOUT`], in whole seconds.
const TIMEOUT_VARIABLE: &str = "RUSTYCC_DIFF_TIMEOUT_SECS";

/// The wall-clock limit for one run of a compiled program.
pub fn timeout() -> Duration {
    env::var(TIMEOUT_VARIABLE)
        .ok()
        .and_then(|value| value.parse().ok())
        .map_or(DEFAULT_TIMEOUT, Duration::from_secs)
}

/// A directory for one program's artifacts, under `tier` so two harnesses cannot collide.
///
/// Rooted in Cargo's output directory rather than the system temporary directory, so the artifacts
/// a failure message points at survive the run and are found in a predictable place, and so a
/// `cargo clean` takes them away.
pub fn scratch(tier: &str, name: &str) -> PathBuf {
    let directory = Path::new(env!("OUT_DIR")).join(tier).join(name);
    fs::create_dir_all(&directory).expect("could not create the scratch directory");

    directory
}

/// A program built and ready to run.
pub struct Built {
    /// The executable.
    pub binary: PathBuf,
    /// The assembly it was built from, when this compiler produced it.
    ///
    /// Retained so a mismatch report can name a file someone can open, rather than telling them to
    /// reproduce the failure by hand first.
    pub assembly: Option<PathBuf>,
}

/// Why a program could not be built.
#[derive(Debug)]
pub enum BuildError {
    /// The compiler rejected the program, with these messages.
    ///
    /// For `rustycc` these are diagnostics; for `clang` it is whatever it printed.
    Rejected(String),
    /// The program compiled but the assembler or linker refused it.
    ///
    /// Only `rustycc` can fail this way: it is how a bug in the emitted assembly surfaces.
    NotAssembled(String),
}

impl fmt::Display for BuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BuildError::Rejected(details) => write!(formatter, "rejected the program:\n{details}"),
            BuildError::NotAssembled(details) => {
                write!(formatter, "emitted assembly that did not build:\n{details}")
            }
        }
    }
}

/// Compiles `source` with `rustycc`, assembles and links it, and returns the executable.
///
/// The compiler is driven as a library rather than through its binary so a rejection comes back as
/// diagnostics rather than as an exit code, and so the assembly is in hand for the report whether
/// or not the program goes on to misbehave.
pub fn build_with_rustycc(
    path: &Path,
    source: &[u8],
    directory: &Path,
) -> Result<Built, BuildError> {
    let options = Options::for_source(path, Stage::Assembly);
    let artifacts = rustycc::compile(source, path, &options).map_err(|diagnostics| {
        let messages = diagnostics
            .iter()
            .map(|diagnostic| format!("  {}", diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n");

        BuildError::Rejected(messages)
    })?;
    let assembly = artifacts
        .assembly
        .ok_or_else(|| BuildError::Rejected("  no assembly was produced".to_owned()))?;

    let assembly_path = directory.join("rustycc.s");
    fs::write(&assembly_path, &assembly).expect("could not write the assembly");

    let binary = directory.join("rustycc.out");
    link(&[assembly_path.as_os_str()], &binary).map_err(BuildError::NotAssembled)?;

    Ok(Built {
        binary,
        assembly: Some(assembly_path),
    })
}

/// Compiles `path` with `clang -O0 -std=c99 -Wall` and returns the executable.
///
/// These are the oracle's flags, fixed here rather than at each call site: a comparison against a
/// `clang` invoked differently in two places is a comparison against two oracles.
pub fn build_with_clang(path: &Path, directory: &Path) -> Result<Built, BuildError> {
    let binary = directory.join("clang.out");
    link(&[path.as_os_str()], &binary).map_err(BuildError::Rejected)?;

    Ok(Built {
        binary,
        assembly: None,
    })
}

/// Compiles `clang` over `inputs`, links the runtime shim in, and writes the executable to `binary`.
///
/// Both sides of a comparison go through here, so both link the identical `shim.o` and neither can
/// acquire a flag the other does not have.
fn link(inputs: &[&OsStr], binary: &Path) -> Result<(), String> {
    let built = Command::new("clang")
        .args(["-std=c99", "-O0", "-Wall"])
        .args(inputs)
        .arg(SHIM_OBJECT)
        .arg("-o")
        .arg(binary)
        .output()
        .expect("could not run clang; run xcode-select --install");

    if built.status.success() {
        return Ok(());
    }

    Err(String::from_utf8_lossy(&built.stderr).into_owned())
}

/// How a run of a compiled program ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The program returned from `main` or called `exit`, with this status.
    ///
    /// Already masked to the low eight bits, which is all a parent process can observe: `return
    /// 300` and `return 44` are the same event by the time anyone can see them.
    Code(i32),
    /// The program died on this signal.
    ///
    /// Kept apart from [`Exit::Code`] rather than folded into one number, because a program that
    /// segfaults and a program that returns 11 are not the same outcome and must not compare equal.
    Signal(i32),
    /// The program was still running when the harness's patience ran out.
    TimedOut,
}

impl fmt::Display for Exit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Exit::Code(code) => write!(formatter, "exit status {code}"),
            Exit::Signal(signal) => write!(formatter, "killed by signal {signal}"),
            Exit::TimedOut => write!(formatter, "timed out"),
        }
    }
}

/// Everything one run of a program produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    /// Every byte the program wrote to standard output.
    pub stdout: Vec<u8>,
    /// Every byte the program wrote to standard error.
    pub stderr: Vec<u8>,
    /// How it ended.
    pub exit: Exit,
}

/// Runs `binary` with no arguments and empty input, killing it after `limit`.
///
/// Output goes to files rather than pipes. A pipe whose buffer fills blocks the program writing to
/// it until the parent reads, and a parent that is waiting for the program to finish before reading
/// deadlocks — which would turn a chatty test program into a hang that looks like a compiler bug.
pub fn execute(binary: &Path, directory: &Path, limit: Duration) -> Execution {
    let name = binary
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("program");
    let stdout_path = directory.join(format!("{name}.stdout"));
    let stderr_path = directory.join(format!("{name}.stderr"));

    let stdout_file = fs::File::create(&stdout_path).expect("could not create the stdout file");
    let stderr_file = fs::File::create(&stderr_path).expect("could not create the stderr file");

    let mut child = Command::new(binary)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file))
        .spawn()
        .expect("could not run the compiled program");

    let exit = match wait_for(&mut child, limit) {
        Some(status) => match (status.code(), status.signal()) {
            // `code()` is already the low eight bits: that is what `wait(2)` reports.
            (Some(code), _) => Exit::Code(code),
            (None, Some(signal)) => Exit::Signal(signal),
            (None, None) => panic!("a finished process reported neither a status nor a signal"),
        },
        None => {
            let _ = child.kill();
            let _ = child.wait();

            Exit::TimedOut
        }
    };

    Execution {
        stdout: fs::read(&stdout_path).expect("could not read what the program printed"),
        stderr: fs::read(&stderr_path).expect("could not read what the program printed"),
        exit,
    }
}

/// Waits for `child`, giving up after `limit` and returning `None`.
///
/// Polled rather than blocked on, because the standard library has no wait-with-timeout and the
/// alternative — a watchdog thread holding the child — costs more than a five-millisecond poll on a
/// program that normally finishes in one.
fn wait_for(child: &mut std::process::Child, limit: Duration) -> Option<std::process::ExitStatus> {
    let deadline = Instant::now() + limit;

    loop {
        match child.try_wait().expect("could not wait for the program") {
            Some(status) => return Some(status),
            None if Instant::now() >= deadline => return None,
            None => thread::sleep(Duration::from_millis(5)),
        }
    }
}

/// One way in which two runs of the same program disagreed.
#[derive(Debug)]
pub struct Mismatch {
    /// Which of stdout, stderr, or the exit status differed.
    pub axis: &'static str,
    /// What the oracle produced, rendered for a human.
    pub oracle: String,
    /// What the compiler under test produced, rendered the same way.
    pub subject: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} differs\n     clang: {}\n   rustycc: {}",
            self.axis, self.oracle, self.subject
        )
    }
}

/// Compares two runs of the same program, reporting the first axis on which they disagree.
///
/// A pure function over two records, with no compiler and no process behind it, so the harness's
/// own tests can hand it a wrong answer directly. A comparison that only ever sees agreement has
/// never been shown to notice anything.
///
/// The axes are checked in the order a person would want them reported: what the program printed
/// first, since that is what usually says which operation went wrong, and the status last.
pub fn compare(oracle: &Execution, subject: &Execution) -> Result<(), Mismatch> {
    if oracle.stdout != subject.stdout {
        return Err(Mismatch {
            axis: "stdout",
            oracle: render(&oracle.stdout),
            subject: render(&subject.stdout),
        });
    }

    if oracle.stderr != subject.stderr {
        return Err(Mismatch {
            axis: "stderr",
            oracle: render(&oracle.stderr),
            subject: render(&subject.stderr),
        });
    }

    if oracle.exit != subject.exit {
        return Err(Mismatch {
            axis: "the exit status",
            oracle: oracle.exit.to_string(),
            subject: subject.exit.to_string(),
        });
    }

    Ok(())
}

/// Renders output bytes so escapes and trailing whitespace are visible in a failure message.
fn render(bytes: &[u8]) -> String {
    format!("{:?}", String::from_utf8_lossy(bytes))
}

/// What comparing one program under both compilers concluded.
#[derive(Debug)]
pub enum Verdict {
    /// Both compilers built the program, and it behaved identically under each.
    ///
    /// Carries the oracle's run, so a caller can go on to ask whether the program did anything at
    /// all: two compilers agreeing that a program prints nothing is agreement about nothing.
    Agreed(Execution),
    /// Both built it, and the two binaries behaved differently.
    Disagreed(Mismatch),
    /// One compiler built the program and the other would not.
    ///
    /// Kept apart from a behavioral difference: a program this compiler cannot build is a hole in
    /// the subset, and a program `clang` will not build is a broken corpus entry. Neither is a
    /// wrong answer, and reporting them as one would hide which.
    NotBuilt {
        /// Which compiler refused it.
        compiler: &'static str,
        /// What it said.
        cause: BuildError,
    },
}

/// Builds `path` under both compilers, runs both, and compares the runs.
///
/// `directory` keeps both binaries, both assemblies, and both output captures, and is named in the
/// report so a failure can be taken apart without reproducing it first.
pub fn differential(path: &Path, directory: &Path) -> Verdict {
    let source = fs::read(path).expect("a corpus program should be readable");

    let oracle = match build_with_clang(path, directory) {
        Ok(built) => built,
        Err(cause) => {
            return Verdict::NotBuilt {
                compiler: "clang",
                cause,
            }
        }
    };
    let subject = match build_with_rustycc(path, &source, directory) {
        Ok(built) => built,
        Err(cause) => {
            return Verdict::NotBuilt {
                compiler: "rustycc",
                cause,
            }
        }
    };

    let limit = timeout();
    let oracle_run = execute(&oracle.binary, directory, limit);
    let subject_run = execute(&subject.binary, directory, limit);

    match compare(&oracle_run, &subject_run) {
        Ok(()) => Verdict::Agreed(oracle_run),
        Err(mismatch) => Verdict::Disagreed(mismatch),
    }
}

/// Renders a failing verdict as the message a person reading a red test needs.
///
/// Names the program, the disagreement, and the directory holding both binaries and the emitted
/// assembly — everything needed to diagnose the failure without running anything again.
pub fn report(path: &Path, directory: &Path, verdict: &Verdict) -> Option<String> {
    let body = match verdict {
        Verdict::Agreed(_) => return None,
        Verdict::Disagreed(mismatch) => mismatch.to_string(),
        Verdict::NotBuilt { compiler, cause } => format!("{compiler} {cause}"),
    };

    Some(format!(
        "{}: {body}\n   artifacts kept in {}",
        path.display(),
        directory.display()
    ))
}
