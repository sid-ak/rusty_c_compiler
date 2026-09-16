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

use rustycc::codegen::emit::{Emitter, Width};
use rustycc::codegen::frame::{FrameLayout, Requirements};
use rustycc::sema::annotations::{Frame, FrameSlot};
use rustycc::sema::scope::{SlotId, SymbolKind};
use rustycc::sema::types::Ty;

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

/// Assembles `assembly`, links it with `driver`, runs the result, and returns its stdout.
///
/// The only way to find out whether a frame is laid out correctly is to run a function that uses
/// it. A snapshot says what was emitted and the assembler says it is legal; neither says the value
/// came back.
fn run_with_driver(name: &str, assembly: &str, driver: &str) -> String {
    let directory = scratch(name);
    let source = directory.join("out.s");
    let main = directory.join("main.c");
    let binary = directory.join("program");

    fs::write(&source, assembly).expect("could not write the assembly");
    fs::write(&main, driver).expect("could not write the driver");

    let built = Command::new("clang")
        .args(["-std=c99", "-O0"])
        .arg(&source)
        .arg(&main)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("could not run clang; run xcode-select --install");
    assert!(
        built.status.success(),
        "linking failed:\n{}\n--- assembly ---\n{assembly}",
        String::from_utf8_lossy(&built.stderr)
    );

    let run = Command::new(&binary)
        .output()
        .expect("could not run the program");
    assert!(
        run.status.success(),
        "the program exited with {:?}",
        run.status.code()
    );

    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// One local of `size` bytes, so a frame can be made as large as a test needs.
fn local(index: u32, name: &str, size: u64, align: u64) -> FrameSlot {
    FrameSlot {
        slot: SlotId(index),
        name: name.to_owned(),
        ty: Ty::Int,
        size,
        align,
        kind: SymbolKind::Local,
    }
}

/// A function whose frame is far larger than any load or store immediate can reach.
///
/// It takes one `int`, writes it into the highest slot in the frame, reads it back, and returns it.
/// Every access to that slot has to go through the materialization path, so a truncated or
/// mis-scaled offset shows up as a wrong answer rather than as anything subtler.
fn far_slot_program() -> String {
    let frame = Frame {
        slots: vec![local(0, "padding", 40_000, 4), local(1, "far", 4, 4)],
    };
    let layout = FrameLayout::build(&frame, Requirements::default());
    let far = layout
        .offset_of(SlotId(1))
        .expect("the far slot is laid out");

    let mut emitter = Emitter::new();
    emitter.begin_function("far_slot");
    layout.emit_prologue(&mut emitter, &[]);
    emitter.store_to_frame("w0", Width::Word, far);
    emitter.instruction("mov w0, #0");
    emitter.load_from_frame("w0", Width::Word, far);
    layout.emit_epilogue(&mut emitter);

    emitter.finish()
}

/// A frame too large for any immediate offset still stores and loads the right value.
#[test]
fn a_far_slot_survives_a_round_trip() {
    let assembly = far_slot_program();

    assert!(
        assembly.contains("add x9, x29, x9"),
        "this test is only meaningful on the materialization path:\n{assembly}"
    );

    let output = run_with_driver(
        "far-slot",
        &assembly,
        "#include <stdio.h>\nint far_slot(int n);\nint main(void) { printf(\"%d\\n\", far_slot(1234)); return 0; }\n",
    );

    assert_eq!(output, "1234\n");
}

/// A `char` slot round-trips through the byte instructions, sign-extending on the way back.
#[test]
fn a_char_slot_sign_extends_when_it_is_read_back() {
    let frame = Frame {
        slots: vec![local(0, "letter", 1, 1)],
    };
    let layout = FrameLayout::build(&frame, Requirements::default());
    let letter = layout.offset_of(SlotId(0)).expect("the slot is laid out");

    let mut emitter = Emitter::new();
    emitter.begin_function("round_trip");
    layout.emit_prologue(&mut emitter, &[]);
    emitter.store_to_frame("w0", Width::Byte, letter);
    emitter.load_from_frame("w0", Width::Byte, letter);
    layout.emit_epilogue(&mut emitter);

    let output = run_with_driver(
        "char-slot",
        &emitter.finish(),
        "#include <stdio.h>\nint round_trip(int n);\nint main(void) { printf(\"%d %d\\n\", round_trip(65), round_trip(200)); return 0; }\n",
    );

    // 200 does not fit a signed byte; reading it back sign-extends to -56, which is what C says a
    // `char` holding that value is worth.
    assert_eq!(output, "65 -56\n");
}

/// Parameters arrive in registers and are readable from their slots once the prologue has run.
#[test]
fn parameters_are_readable_from_their_slots_after_the_prologue() {
    let frame = Frame {
        slots: vec![
            FrameSlot {
                slot: SlotId(0),
                name: "a".to_owned(),
                ty: Ty::Int,
                size: 4,
                align: 4,
                kind: SymbolKind::Parameter(0),
            },
            FrameSlot {
                slot: SlotId(1),
                name: "b".to_owned(),
                ty: Ty::Int,
                size: 4,
                align: 4,
                kind: SymbolKind::Parameter(1),
            },
        ],
    };
    let layout = FrameLayout::build(&frame, Requirements::default());
    let a = layout.offset_of(SlotId(0)).expect("a is laid out");
    let b = layout.offset_of(SlotId(1)).expect("b is laid out");

    let mut emitter = Emitter::new();
    emitter.begin_function("difference");
    layout.emit_prologue(&mut emitter, &frame.slots);
    emitter.load_from_frame("w0", Width::Word, a);
    emitter.load_from_frame("w1", Width::Word, b);
    emitter.instruction("sub w0, w0, w1");
    layout.emit_epilogue(&mut emitter);

    let output = run_with_driver(
        "parameters",
        &emitter.finish(),
        "#include <stdio.h>\nint difference(int a, int b);\nint main(void) { printf(\"%d\\n\", difference(10, 3)); return 0; }\n",
    );

    // Asymmetric on purpose: a transposed load would give -7 and pass on any symmetric pair.
    assert_eq!(output, "7\n");
}
