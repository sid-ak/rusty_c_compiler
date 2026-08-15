//! The runtime shim, exercised the way a compiled program will use it.
//!
//! Each test writes a C `main`, compiles it, links it against the same `shim.o` the driver will
//! link, runs it, and asserts its stdout. That is the only way to check the shim that actually
//! proves anything: reading the source cannot tell you what `print_int(INT_MIN)` writes.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a failed `expect` here is a failed test, which is
// exactly what should happen when the toolchain is missing or the shim will not link.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use mycc::runtime::SHIM_OBJECT;

/// The shim's declarations, as a program under test must declare them for itself — there is no
/// header, because the compiled subset has no preprocessor to include one with.
const DECLARATIONS: &str = "void print_int(int n);\n\
                            void print_char(char c);\n\
                            void print_string(char *s);\n";

/// A scratch directory under Cargo's output directory, unique to `name`.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("OUT_DIR")).join("shim-tests").join(name);
    fs::create_dir_all(&directory).expect("could not create the scratch directory");

    directory
}

/// Compile `body` as the contents of `main`, link it against the shim, run it, and return stdout.
fn run_program(name: &str, body: &str) -> String {
    let directory = scratch(name);
    let source = directory.join("main.c");
    let binary = directory.join("main");

    fs::write(
        &source,
        format!("{DECLARATIONS}\nint main(void) {{\n{body}\n    return 0;\n}}\n"),
    )
    .expect("could not write the test program");

    let compile = Command::new("clang")
        .args(["-std=c99", "-O0", "-Wall", "-Wextra", "-Werror"])
        .arg(&source)
        .arg(SHIM_OBJECT)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("could not run clang");
    assert!(
        compile.status.success(),
        "linking against the shim failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let run = Command::new(&binary)
        .output()
        .expect("could not run the test program");
    assert!(
        run.status.success(),
        "the test program exited with {:?}",
        run.status.code()
    );

    String::from_utf8(run.stdout).expect("the shim writes ASCII")
}

/// `print_int` prints decimal, including at the boundaries of the range `int` can hold.
///
/// `INT_MIN` is written as `-2147483647 - 1` because negating the literal `2147483648` is how the
/// naive implementation overflows, and because that is how `limits.h` itself spells it.
#[test]
fn print_int_covers_the_whole_int_range() {
    let output = run_program(
        "print_int",
        r#"    print_int(0);
    print_char('\n');
    print_int(-1);
    print_char('\n');
    print_int(1);
    print_char('\n');
    print_int(2147483647);
    print_char('\n');
    print_int(-2147483647 - 1);
    print_char('\n');"#,
    );

    assert_eq!(output, "0\n-1\n1\n2147483647\n-2147483648\n");
}

/// Multi-digit values print their digits in the right order, not reversed.
#[test]
fn print_int_prints_digits_in_order() {
    let output = run_program(
        "digit_order",
        r#"    print_int(1024);
    print_char(' ');
    print_int(-9070);"#,
    );

    assert_eq!(output, "1024 -9070");
}

/// `print_char` writes exactly one byte, control characters included.
#[test]
fn print_char_writes_one_byte() {
    let output = run_program(
        "print_char",
        r#"    print_char('a');
    print_char('\t');
    print_char('Z');
    print_char('\n');
    print_char('0');"#,
    );

    assert_eq!(output, "a\tZ\n0");
}

/// `print_string` writes every byte up to the terminator, and nothing for an empty string.
#[test]
fn print_string_writes_up_to_the_terminator() {
    let output = run_program(
        "print_string",
        r#"    print_string("");
    print_string("hello");
    print_string("");
    print_string(" line\nnext\n");
    print_string("");"#,
    );

    assert_eq!(output, "hello line\nnext\n");
}

/// The three functions interleave in call order, because nothing is buffered.
#[test]
fn output_is_unbuffered_and_in_call_order() {
    let output = run_program(
        "ordering",
        r#"    print_string("n=");
    print_int(42);
    print_char('\n');
    print_string("m=");
    print_int(-42);
    print_char('\n');"#,
    );

    assert_eq!(output, "n=42\nm=-42\n");
}

/// The shim compiles clean under `-Wall -Wextra`, independently of how the build script is
/// configured — a warning-free build is a property of the source, not of one command line.
#[test]
fn the_shim_compiles_without_warnings() {
    let object = scratch("warnings").join("shim.o");

    let output = Command::new("clang")
        .args([
            "-std=c99",
            "-O0",
            "-Wall",
            "-Wextra",
            "-c",
            "runtime/shim.c",
            "-o",
        ])
        .arg(&object)
        .output()
        .expect("could not run clang");

    assert!(output.status.success(), "the shim should compile");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "the shim should compile without warnings"
    );
}
