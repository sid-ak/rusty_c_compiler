//! The golden-program corpus: every program compiled by `rustycc`, run, and checked.
//!
//! Each program carries the exit code and stdout it should produce in its own header. Those values
//! were recorded from `clang -O0 -std=c99`, not from this compiler, so they are an independent
//! answer rather than a note of what `rustycc` happened to do on the day. Phase 5 replaces the
//! recording with `clang` run side by side; until then the header is the oracle's answer, written
//! down.
//!
//! One `#[test]` per program, so a failure names the program that broke rather than collapsing the
//! corpus into one red line.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a corpus file that cannot be read is a broken
// checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustycc::cli::{Options, Stage};
use rustycc::runtime::SHIM_OBJECT;

/// Where the corpus lives, relative to the crate root Cargo runs tests from.
const CORPUS: &str = "tests/programs";

/// Every program in the corpus. Adding a `.c` file without adding it here fails a test below.
macro_rules! golden_programs {
    ($($name:ident),* $(,)?) => {
        /// The names listed above, for the check that the list is complete.
        const LISTED: &[&str] = &[$(stringify!($name)),*];

        $(
            #[test]
            fn $name() {
                run_golden(stringify!($name));
            }
        )*
    };
}

golden_programs!(
    arithmetic,
    arrays,
    control_flow,
    functions,
    recursion,
    sorting,
    strings,
);

/// What a program's header says it should do.
struct Expected {
    /// The exit status.
    code: i32,
    /// Everything it should write to stdout.
    stdout: String,
}

/// Reads the `expect-` header of the program at `path`.
///
/// `\n` and `\t` are written as escapes so the whole expectation fits on one comment line; they are
/// decoded here rather than compared as text, so a program emitting a literal backslash-n is told
/// apart from one emitting a newline.
fn expectations(path: &Path, source: &str) -> Expected {
    let field = |key: &str| {
        let prefix = format!("// expect-{key}: ");

        source
            .lines()
            .find_map(|line| line.strip_prefix(&prefix))
            .unwrap_or_else(|| panic!("{} has no `// expect-{key}:` line", path.display()))
            .to_owned()
    };

    let code = field("exit")
        .parse()
        .unwrap_or_else(|_| panic!("{}: the expected exit code is not a number", path.display()));

    Expected {
        code,
        stdout: decode(&field("stdout")),
    }
}

/// Turns the escapes a header uses back into the bytes they stand for.
fn decode(encoded: &str) -> String {
    let mut decoded = String::new();
    let mut characters = encoded.chars();

    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);

            continue;
        }

        match characters.next() {
            Some('n') => decoded.push('\n'),
            Some('t') => decoded.push('\t'),
            Some('\\') => decoded.push('\\'),
            Some(other) => {
                decoded.push('\\');
                decoded.push(other);
            }
            None => decoded.push('\\'),
        }
    }

    decoded
}

/// A scratch directory for one program's build.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("OUT_DIR")).join("golden").join(name);
    fs::create_dir_all(&directory).expect("could not create the scratch directory");

    directory
}

/// Compiles `name` from the corpus with `rustycc`, runs it, and checks it against its header.
fn run_golden(name: &str) {
    let path = Path::new(CORPUS).join(format!("{name}.c"));
    let source = fs::read(&path).expect("a corpus program should be readable");
    let text = String::from_utf8_lossy(&source).into_owned();
    let expected = expectations(&path, &text);

    let directory = scratch(name);
    let assembly_path = directory.join(format!("{name}.s"));
    let binary = directory.join(name);

    // Driven as a library rather than through the binary, so a failure reports a diagnostic rather
    // than an exit code, and so the assembly is in hand for the message if the program misbehaves.
    let options = Options::for_source(&path, Stage::Assembly);
    let artifacts = rustycc::compile(&source, &path, &options).unwrap_or_else(|diagnostics| {
        panic!(
            "{}: rustycc rejected a valid program: {:?}",
            path.display(),
            diagnostics
                .iter()
                .map(|diagnostic| &diagnostic.message)
                .collect::<Vec<_>>()
        )
    });
    let assembly = artifacts
        .assembly
        .unwrap_or_else(|| panic!("{}: no assembly was produced", path.display()));

    fs::write(&assembly_path, &assembly).expect("could not write the assembly");

    let built = Command::new("clang")
        .args(["-std=c99", "-O0"])
        .arg(&assembly_path)
        .arg(SHIM_OBJECT)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("could not run clang; run xcode-select --install");
    assert!(
        built.status.success(),
        "{}: the emitted assembly did not assemble or link\n{}\nassembly kept at {}",
        path.display(),
        String::from_utf8_lossy(&built.stderr),
        assembly_path.display()
    );

    let run = Command::new(&binary)
        .output()
        .expect("could not run the compiled program");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();

    assert_eq!(
        stdout,
        expected.stdout,
        "{}: stdout does not match\n  expected: {:?}\n  actual:   {:?}\nassembly kept at {}",
        path.display(),
        expected.stdout,
        stdout,
        assembly_path.display()
    );
    assert_eq!(
        run.status.code(),
        Some(expected.code),
        "{}: exit code does not match; assembly kept at {}",
        path.display(),
        assembly_path.display()
    );
}

/// Every `.c` file in the corpus has a test of its own.
///
/// Without this, adding a program and forgetting to list it above would leave it untested while the
/// suite stayed green — which is the failure a corpus exists to prevent.
#[test]
fn every_corpus_program_has_a_test() {
    let mut found: Vec<String> = fs::read_dir(CORPUS)
        .expect("the corpus directory should exist")
        .filter_map(|entry| {
            let path = entry.expect("a corpus entry should be readable").path();
            let is_program = path.extension().is_some_and(|extension| extension == "c");

            is_program.then(|| {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or_default()
                    .to_owned()
            })
        })
        .collect();
    found.sort();

    let mut listed: Vec<String> = LISTED.iter().map(|name| (*name).to_owned()).collect();
    listed.sort();

    assert_eq!(found, listed, "the corpus and the test list disagree");
}

/// Every emitted `.s` assembles on its own, with the assembler given nothing to say.
///
/// Separate from the execution tests above: those link and run, which would succeed despite a
/// warning, and a directive the assembler grumbles about is one this compiler should not emit.
#[test]
fn every_program_assembles_without_warnings() {
    for name in LISTED {
        let path = Path::new(CORPUS).join(format!("{name}.c"));
        let source = fs::read(&path).expect("a corpus program should be readable");
        let options = Options::for_source(&path, Stage::Assembly);
        let assembly = rustycc::compile(&source, &path, &options)
            .ok()
            .and_then(|artifacts| artifacts.assembly)
            .unwrap_or_else(|| panic!("{}: no assembly was produced", path.display()));

        let directory = scratch(name);
        let assembly_path = directory.join("warnings.s");
        fs::write(&assembly_path, &assembly).expect("could not write the assembly");

        let assembled = Command::new("clang")
            .args(["-c", "-Werror"])
            .arg(&assembly_path)
            .arg("-o")
            .arg(directory.join("warnings.o"))
            .output()
            .expect("could not run clang");

        assert!(
            assembled.status.success() && assembled.stderr.is_empty(),
            "{name}: the assembler had something to say:\n{}",
            String::from_utf8_lossy(&assembled.stderr)
        );
    }
}
