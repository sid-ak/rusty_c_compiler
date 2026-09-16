//! Unit tests for frame layout: where things live, how big the frame is, and what it opens with.
//!
//! Offsets and sizes are arithmetic and are checked as arithmetic here. That a frame this shape
//! actually runs is an execution test, and belongs where a program can be compiled and run.

use super::*;

use crate::sema::annotations::{Frame, FrameSlot};
use crate::sema::scope::{SlotId, SymbolKind};
use crate::sema::types::Ty;

/// A slot of `size` bytes aligned to `align`, named for readability in a failure.
fn slot(index: u32, name: &str, size: u64, align: u64, kind: SymbolKind) -> FrameSlot {
    FrameSlot {
        slot: SlotId(index),
        name: name.to_owned(),
        ty: Ty::Int,
        size,
        align,
        kind,
    }
}

/// A frame holding two parameters and three locals of mixed width.
fn mixed_frame() -> Frame {
    Frame {
        slots: vec![
            slot(0, "count", 4, 4, SymbolKind::Parameter(0)),
            slot(1, "flag", 1, 1, SymbolKind::Parameter(1)),
            slot(2, "total", 4, 4, SymbolKind::Local),
            slot(3, "initial", 1, 1, SymbolKind::Local),
            slot(4, "values", 12, 4, SymbolKind::Local),
        ],
    }
}

/// Every frame is a multiple of sixteen bytes, which AAPCS64 requires of the stack pointer.
#[test]
fn every_frame_size_is_a_multiple_of_sixteen() {
    let cases = [
        ("empty", Frame::default(), 0),
        ("mixed", mixed_frame(), 0),
        ("mixed with temporaries", mixed_frame(), 7),
    ];

    for (shape, frame, temporaries) in cases {
        let layout = FrameLayout::build(
            &frame,
            Requirements {
                temporaries,
                ..Requirements::default()
            },
        );

        assert_eq!(layout.size() % 16, 0, "{shape}: size {}", layout.size());
    }
}

/// A frame with nothing in it is still big enough for the saved frame pointer and return address.
#[test]
fn an_empty_frame_still_saves_the_frame_pointer_and_return_address() {
    let layout = FrameLayout::build(&Frame::default(), Requirements::default());

    assert_eq!(layout.size(), 16);
}

/// Slots start above the saved pair, so nothing can be written over it.
#[test]
fn slots_start_above_the_saved_registers() {
    let layout = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 2,
            ..Requirements::default()
        },
    );

    for entry in mixed_frame().slots {
        let offset = layout
            .offset_of(entry.slot)
            .unwrap_or_else(|| panic!("{} has no offset", entry.name));

        assert!(
            offset >= SAVED_REGISTERS,
            "{} sits at {offset}, over the saved registers",
            entry.name
        );
    }
}

/// Each slot is aligned as its type requires.
#[test]
fn every_slot_is_aligned_as_its_type_requires() {
    let layout = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 0,
            ..Requirements::default()
        },
    );

    for entry in mixed_frame().slots {
        let offset = layout
            .offset_of(entry.slot)
            .unwrap_or_else(|| panic!("{} has no offset", entry.name));

        assert_eq!(
            offset % entry.align,
            0,
            "{} sits at {offset}, which is not a multiple of {}",
            entry.name,
            entry.align
        );
    }
}

/// No two slots overlap, and none runs past the end of the frame.
///
/// Checked over every byte rather than by comparing starts: two slots of different sizes can have
/// distinct offsets and still share bytes, and that is the failure that would corrupt a value
/// rather than crash.
#[test]
fn no_two_slots_share_a_byte() {
    let frame = mixed_frame();
    let temporaries = 4;
    let layout = FrameLayout::build(
        &frame,
        Requirements {
            temporaries,
            ..Requirements::default()
        },
    );

    let mut occupied: Vec<(u64, u64, String)> = Vec::new();
    for entry in &frame.slots {
        let offset = layout
            .offset_of(entry.slot)
            .unwrap_or_else(|| panic!("{} has no offset", entry.name));
        occupied.push((offset, entry.size, entry.name.clone()));
    }
    for depth in 0..temporaries {
        let offset = layout
            .temporary(depth)
            .unwrap_or_else(|| panic!("temporary {depth} has no offset"));
        occupied.push((offset, TEMPORARY_BYTES, format!("temporary {depth}")));
    }

    for (offset, size, name) in &occupied {
        assert!(
            offset + size <= layout.size(),
            "{name} runs past the end of the frame"
        );

        for (other_offset, other_size, other_name) in &occupied {
            if name == other_name {
                continue;
            }

            let disjoint = offset + size <= *other_offset || other_offset + other_size <= *offset;
            assert!(disjoint, "{name} and {other_name} share a byte");
        }
    }
}

/// A slot that was never allocated has no offset, rather than a plausible wrong one.
#[test]
fn an_unknown_slot_has_no_offset() {
    let layout = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 1,
            ..Requirements::default()
        },
    );

    assert_eq!(layout.offset_of(SlotId(99)), None);
    assert_eq!(layout.temporary(1), None);
}

/// Laying out the same frame twice gives the same answer.
#[test]
fn layout_is_deterministic() {
    let first = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 3,
            ..Requirements::default()
        },
    );
    let second = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 3,
            ..Requirements::default()
        },
    );

    assert_eq!(first.size(), second.size());
    for entry in mixed_frame().slots {
        assert_eq!(first.offset_of(entry.slot), second.offset_of(entry.slot));
    }
}

/// A frame small enough for the pre-indexed form opens with the two documented instructions.
#[test]
fn a_small_frame_opens_with_the_pre_indexed_form() {
    let layout = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 2,
            ..Requirements::default()
        },
    );
    let mut emitter = Emitter::new();
    layout.emit_prologue(&mut emitter, &[]);

    let assembly = emitter.finish();
    let size = layout.size();

    assert!(
        assembly.contains(&format!("\tstp x29, x30, [sp, #-{size}]!\n\tmov x29, sp\n")),
        "got: {assembly}"
    );
}

/// A frame too large for that immediate lowers the stack pointer separately instead.
///
/// The pre-indexed form caps at 512 bytes. The alternative keeps `x29` at the bottom of the frame
/// exactly as the small form does, so one offset convention covers every frame size — two
/// conventions is how a slot ends up read from the wrong side of the frame pointer.
#[test]
fn a_large_frame_lowers_the_stack_pointer_separately() {
    let frame = Frame {
        slots: vec![slot(0, "big", 40_000, 4, SymbolKind::Local)],
    };
    let layout = FrameLayout::build(
        &frame,
        Requirements {
            temporaries: 0,
            ..Requirements::default()
        },
    );
    let mut emitter = Emitter::new();
    layout.emit_prologue(&mut emitter, &[]);

    let assembly = emitter.finish();

    assert!(assembly.contains("sub sp, sp, x9"), "got: {assembly}");
    assert!(
        assembly.contains("\tstp x29, x30, [sp, #0]\n\tadd x29, sp, #0\n"),
        "got: {assembly}"
    );
    assert!(
        !assembly.contains("[sp, #-"),
        "the pre-indexed form cannot hold this frame: {assembly}"
    );
}

/// Parameters are copied out of their argument registers into their slots before the body runs.
#[test]
fn parameters_are_spilled_into_their_slots() {
    let frame = mixed_frame();
    let layout = FrameLayout::build(
        &frame,
        Requirements {
            temporaries: 0,
            ..Requirements::default()
        },
    );
    let mut emitter = Emitter::new();
    layout.emit_prologue(&mut emitter, &frame.slots);

    let assembly = emitter.finish();
    let count = layout.offset_of(SlotId(0)).expect("count has an offset");
    let flag = layout.offset_of(SlotId(1)).expect("flag has an offset");

    assert!(
        assembly.contains(&format!("\tstr w0, [x29, #{count}]\n")),
        "got: {assembly}"
    );
    assert!(
        assembly.contains(&format!("\tstrb w1, [x29, #{flag}]\n")),
        "got: {assembly}"
    );
}

/// A local is not spilled from anywhere: it has no incoming register.
#[test]
fn locals_are_not_spilled_in_the_prologue() {
    let frame = mixed_frame();
    let layout = FrameLayout::build(
        &frame,
        Requirements {
            temporaries: 0,
            ..Requirements::default()
        },
    );
    let mut emitter = Emitter::new();
    layout.emit_prologue(&mut emitter, &frame.slots);

    let assembly = emitter.finish();

    assert_eq!(
        assembly.matches("str").count(),
        3,
        "only the two parameters and the saved pair are stored: {assembly}"
    );
}

/// The epilogue undoes the prologue and returns, in the form that matches the frame's size.
#[test]
fn the_epilogue_mirrors_the_prologue() {
    let small = FrameLayout::build(
        &mixed_frame(),
        Requirements {
            temporaries: 0,
            ..Requirements::default()
        },
    );
    let mut emitter = Emitter::new();
    small.emit_epilogue(&mut emitter);
    let assembly = emitter.finish();

    assert!(
        assembly.contains(&format!(
            "\tmov sp, x29\n\tldp x29, x30, [sp], #{}\n\tret\n",
            small.size()
        )),
        "got: {assembly}"
    );

    let frame = Frame {
        slots: vec![slot(0, "big", 40_000, 4, SymbolKind::Local)],
    };
    let large = FrameLayout::build(
        &frame,
        Requirements {
            temporaries: 0,
            ..Requirements::default()
        },
    );
    let mut emitter = Emitter::new();
    large.emit_epilogue(&mut emitter);
    let assembly = emitter.finish();

    assert!(
        assembly.contains("\tldp x29, x30, [sp, #0]\n"),
        "got: {assembly}"
    );
    assert!(assembly.contains("add sp, sp, x9"), "got: {assembly}");
}

/// Parses `source` and returns the body of the first function it defines.
fn body_of(source: &str) -> Block {
    let lexed = crate::lexer::lex(source.as_bytes());
    assert!(lexed.diagnostics.is_empty(), "fixture does not lex");

    let parsed = crate::parser::parse(&lexed.tokens);
    assert!(parsed.diagnostics.is_empty(), "fixture does not parse");

    let definition = parsed
        .program
        .items
        .into_iter()
        .find_map(|item| match item {
            crate::ast::Item::FuncDef(def) => Some(def),
            _ => None,
        });

    definition.expect("the fixture defines a function").body
}

/// The temporary count is the depth of the deepest expression, not the number of expressions.
#[test]
fn temporaries_are_counted_by_depth_not_by_quantity() {
    let cases = [
        ("no expressions", "int f(void) { return; }", 0),
        ("one literal", "int f(void) { return 1; }", 1),
        ("one operator", "int f(void) { return 1 + 2; }", 2),
        ("nested operators", "int f(void) { return 1 + 2 * 3; }", 3),
        (
            "two shallow expressions side by side",
            "int f(void) { int a; a = 1 + 2; a = 3 + 4; return a; }",
            3,
        ),
        (
            "depth inside a loop body",
            "int f(void) { int a; a = 0; while (a) { a = 1 + 2 * 3; } return a; }",
            4,
        ),
        (
            "depth inside a call argument",
            "int g(int x); int f(void) { return g(1 + 2 * 3); }",
            4,
        ),
    ];

    for (shape, source, expected) in cases {
        let body = body_of(source);

        assert_eq!(temporaries_needed(&body), expected, "{shape}");
    }
}

/// A function that passes arguments on the stack reserves room for them at the bottom of its frame.
///
/// `x29` then sits above that room rather than at the very bottom, so a named slot's offset is
/// unchanged and the outgoing area cannot be written over by anything the body does.
#[test]
fn an_outgoing_argument_area_sits_below_the_saved_registers() {
    let needs = Requirements {
        temporaries: 1,
        arguments: 9,
        call_depth: 1,
        outgoing: 16,
    };
    let layout = FrameLayout::build(&mixed_frame(), needs);

    assert_eq!(layout.outgoing(), 16);
    assert_eq!(layout.size() % 16, 0);

    for entry in mixed_frame().slots {
        let offset = layout
            .offset_of(entry.slot)
            .unwrap_or_else(|| panic!("{} has no offset", entry.name));

        assert!(
            offset >= SAVED_REGISTERS,
            "{} would overlap the saved registers",
            entry.name
        );
    }

    let mut emitter = Emitter::new();
    layout.emit_prologue(&mut emitter, &[]);
    let assembly = emitter.finish();

    assert!(
        assembly.contains("\tstp x29, x30, [sp, #16]\n\tadd x29, sp, #16\n"),
        "got: {assembly}"
    );
}

/// Argument slots are distinct across positions and across levels of call nesting.
#[test]
fn argument_slots_are_distinct_across_calls_and_positions() {
    let needs = Requirements {
        temporaries: 2,
        arguments: 3,
        call_depth: 2,
        outgoing: 0,
    };
    let layout = FrameLayout::build(&mixed_frame(), needs);

    let mut seen = Vec::new();
    for depth in 0..2 {
        for index in 0..3 {
            seen.push(
                layout
                    .argument(depth, index)
                    .unwrap_or_else(|| panic!("call {depth} argument {index} has no slot")),
            );
        }
    }

    let count = seen.len();
    seen.sort_unstable();
    seen.dedup();

    assert_eq!(seen.len(), count, "two argument slots share an offset");
    assert_eq!(layout.argument(2, 0), None, "there is no third call depth");
}
