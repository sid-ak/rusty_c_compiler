//! Snapshots of the emitted assembly, and the check that an assembler accepts it.
//!
//! A snapshot proves the output is what was intended; only the assembler proves it is legal. The
//! two answer different questions and a change can break either one alone — a malformed directive
//! passes any snapshot taken after it was introduced, and a correct-but-wrong instruction assembles
//! perfectly.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a scratch directory that cannot be created is a
// broken checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustycc::codegen::emit::Emitter;

/// A scratch directory under Cargo's output directory, unique to `name`.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("OUT_DIR")).join("codegen-tests").join(name);
    fs::create_dir_all(&directory).expect("could not create the scratch directory");

    directory
}

/// Assembles `assembly` with `clang -c`, failing the test on any error or warning.
///
/// `-Werror` is the point: a directive the assembler merely grumbles about is one this compiler
/// should not be emitting, and a warning nobody reads is indistinguishable from no warning.
fn assembles_cleanly(name: &str, assembly: &str) {
    let directory = scratch(name);
    let source = directory.join("out.s");
    let object = directory.join("out.o");

    fs::write(&source, assembly).expect("could not write the assembly");

    let assembled = Command::new("clang")
        .args(["-c", "-Werror"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("could not run clang; run xcode-select --install");

    assert!(
        assembled.status.success(),
        "the emitted assembly did not assemble:\n{}\n--- assembly ---\n{assembly}",
        String::from_utf8_lossy(&assembled.stderr)
    );
    assert!(
        assembled.stderr.is_empty(),
        "the assembler had something to say:\n{}\n--- assembly ---\n{assembly}",
        String::from_utf8_lossy(&assembled.stderr)
    );
}

/// The skeleton of `int main(void) { return 0; }`, built through the emitter's public surface.
///
/// Hand-driven rather than compiled, because lowering is a later task. What it pins is the file
/// around the instructions: the section directives, the linkage, the alignment, and the trailer.
fn skeleton() -> String {
    let mut emitter = Emitter::new();
    emitter.begin_function("main");
    emitter.instruction("mov w0, #0");
    emitter.instruction("ret");

    emitter.finish()
}

/// Every section the compiler can emit, exercised together.
fn every_section() -> String {
    let mut emitter = Emitter::new();

    emitter.begin_function("main");
    let done = emitter.new_label("done");
    emitter.address_of("x0", "l_.str.0");
    emitter.call("print_string");
    emitter.branch_if_zero("w0", &done);
    emitter.address_of("x8", &Emitter::symbol("counter"));
    emitter.instruction("ldr w0, [x8]");
    emitter.place_label(&done);
    emitter.instruction("ret");

    emitter.define_string("l_.str.0", b"hi\n");
    emitter.define_word("counter", 7, true);
    emitter.define_bytes("letters", &[1, 2, 3, 0], 0, true);
    emitter.reserve_zeroed("blank", 40, 2);

    emitter.finish()
}

/// The assembly skeleton of a function that returns zero, directive for directive.
#[test]
fn empty_main_skeleton() {
    insta::assert_snapshot!(skeleton());
}

/// A file using every section, so the section order and each definition form are pinned together.
#[test]
fn every_section_together() {
    insta::assert_snapshot!(every_section());
}

/// The skeleton assembles with no errors and nothing to say.
#[test]
fn the_skeleton_assembles_cleanly() {
    assembles_cleanly("skeleton", &skeleton());
}

/// A file using every section assembles too, which a snapshot alone would not catch.
#[test]
fn every_section_assembles_cleanly() {
    assembles_cleanly("every-section", &every_section());
}
