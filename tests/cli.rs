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

/// A directory for one driver test's files, empty to begin with.
fn driver_scratch(name: &str) -> std::path::PathBuf {
    let directory = Path::new(env!("OUT_DIR")).join("driver-tests").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("could not create the scratch directory");

    directory
}

/// A small program that prints through the runtime shim.
const HELLO: &str =
    "void print_string(char s[]);\nint main(void) { print_string(\"hi\"); return 0; }\n";

/// Writes `source` into `directory` and returns its path.
fn write_program(directory: &Path, source: &str) -> std::path::PathBuf {
    let path = directory.join("program.c");
    std::fs::write(&path, source).expect("could not write the program");

    path
}

/// `rustycc program.c -o program && ./program` works, which is the contract the proposal states.
#[test]
fn compiling_and_running_a_program_works_end_to_end() {
    let directory = driver_scratch("end-to-end");
    let source = write_program(&directory, HELLO);
    let binary = directory.join("program");

    let built = Command::new(RUSTYCC)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "compiling failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(binary.exists(), "-o was not respected");

    let run = Command::new(&binary).output().unwrap();

    assert!(run.status.success());
    assert_eq!(String::from_utf8_lossy(&run.stdout), "hi");
}

/// `-S` writes assembly and produces no binary.
#[test]
fn dash_s_emits_assembly_and_no_binary() {
    let directory = driver_scratch("dash-s");
    let source = write_program(&directory, HELLO);
    let assembly = directory.join("program.s");

    let built = Command::new(RUSTYCC)
        .arg(&source)
        .arg("-S")
        .arg("-o")
        .arg(&assembly)
        .output()
        .unwrap();

    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(assembly.exists(), "the assembly was not written");

    let text = std::fs::read_to_string(&assembly).expect("could not read the assembly");
    assert!(text.contains("_main:"), "got: {text}");
    assert!(
        !directory.join("program").exists(),
        "a binary was produced anyway"
    );
}

/// `-c` produces an object file rather than an executable.
#[test]
fn dash_c_emits_an_object_file() {
    let directory = driver_scratch("dash-c");
    let source = write_program(&directory, HELLO);
    let object = directory.join("program.o");

    let built = Command::new(RUSTYCC)
        .arg(&source)
        .arg("-c")
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();

    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(object.exists(), "the object file was not written");
}

/// `--emit-asm-to` writes the assembly to the given path while still producing the program.
#[test]
fn emit_asm_to_writes_the_assembly_alongside_the_binary() {
    let directory = driver_scratch("emit-asm-to");
    let source = write_program(&directory, HELLO);
    let binary = directory.join("program");
    let assembly = directory.join("kept.s");

    let built = Command::new(RUSTYCC)
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .arg("--emit-asm-to")
        .arg(&assembly)
        .output()
        .unwrap();

    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(assembly.exists(), "the assembly was not written");
    assert!(binary.exists(), "the binary was not produced");
}

/// Intermediate files are gone once the run ends, and kept when the caller asks.
///
/// `TMPDIR` points the compiler at an empty directory of this test's own, so what is left behind
/// can be listed rather than guessed at.
#[test]
fn intermediates_are_removed_unless_they_are_asked_for() {
    for (shape, keep) in [("clean", false), ("kept", true)] {
        let directory = driver_scratch(&format!("temps-{shape}"));
        let temporary = directory.join("tmp");
        std::fs::create_dir_all(&temporary).expect("could not create the temp directory");
        let source = write_program(&directory, HELLO);

        let mut command = Command::new(RUSTYCC);
        command
            .env("TMPDIR", &temporary)
            .arg(&source)
            .arg("-o")
            .arg(directory.join("program"));
        if keep {
            command.arg("--keep-temps");
        }

        let built = command.output().unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );

        let left = std::fs::read_dir(&temporary)
            .expect("could not list the temp directory")
            .count();

        if keep {
            assert_eq!(left, 1, "--keep-temps should leave the workspace behind");
        } else {
            assert_eq!(left, 0, "intermediates were left behind: {left} entries");
        }
    }
}

/// A failing link leaves nothing behind either, and says what the toolchain said.
#[test]
fn a_failing_link_is_reported_and_cleans_up() {
    let directory = driver_scratch("link-failure");
    let temporary = directory.join("tmp");
    std::fs::create_dir_all(&temporary).expect("could not create the temp directory");
    let source = write_program(
        &directory,
        "int missing(void);\nint main(void) { return missing(); }\n",
    );

    let built = Command::new(RUSTYCC)
        .env("TMPDIR", &temporary)
        .arg(&source)
        .arg("-o")
        .arg(directory.join("program"))
        .output()
        .unwrap();

    assert!(
        !built.status.success(),
        "a program with no definition should not link"
    );

    let stderr = String::from_utf8_lossy(&built.stderr);
    assert!(stderr.contains("linking failed"), "got: {stderr}");
    assert!(
        stderr.contains("missing") || stderr.contains("Undefined"),
        "the toolchain's own words should survive: {stderr}"
    );
    assert!(!stderr.contains("panicked"), "got: {stderr}");

    let left = std::fs::read_dir(&temporary)
        .expect("could not list the temp directory")
        .count();
    assert_eq!(left, 0, "a failed run left intermediates behind");
}

/// Two compilations running at once do not write over each other's intermediates.
#[test]
fn concurrent_compilations_do_not_collide() {
    let directory = driver_scratch("concurrent");
    let temporary = directory.join("tmp");
    std::fs::create_dir_all(&temporary).expect("could not create the temp directory");

    let mut running = Vec::new();
    for index in 0..4 {
        let source = directory.join(format!("program{index}.c"));
        std::fs::write(&source, format!("int main(void) {{ return {index}; }}\n"))
            .expect("could not write the program");

        running.push((
            index,
            Command::new(RUSTYCC)
                .env("TMPDIR", &temporary)
                .arg(&source)
                .arg("-o")
                .arg(directory.join(format!("program{index}")))
                .spawn()
                .expect("could not start the compiler"),
        ));
    }

    for (index, child) in running {
        let finished = child
            .wait_with_output()
            .expect("could not wait for the compiler");
        assert!(
            finished.status.success(),
            "compilation {index} failed: {}",
            String::from_utf8_lossy(&finished.stderr)
        );

        let run = Command::new(directory.join(format!("program{index}")))
            .output()
            .expect("could not run the program");
        assert_eq!(
            run.status.code(),
            Some(index),
            "compilation {index} produced the wrong program"
        );
    }
}
