//! Unit tests for the emitter: sections, symbols, labels, and the stability of the output.
//!
//! Nothing here runs the assembler. That the emitted text is something `clang` accepts is an
//! integration test, because it needs a real toolchain and a temporary file; what is pinned here is
//! everything decidable without one.

use super::*;

/// An emitter holding one trivial function, the shape every test below starts from.
fn main_returning_zero() -> Emitter {
    let mut emitter = Emitter::new();
    emitter.begin_function("main");
    emitter.instruction("mov w0, #0");
    emitter.instruction("ret");

    emitter
}

/// A C name becomes a Mach-O symbol by gaining a leading underscore.
#[test]
fn a_c_name_becomes_an_underscored_symbol() {
    assert_eq!(Emitter::symbol("main"), "_main");
    assert_eq!(Emitter::symbol("print_string"), "_print_string");
}

/// A function is announced, aligned, and labelled, in that order.
#[test]
fn a_function_is_declared_global_aligned_and_labelled() {
    let assembly = main_returning_zero().finish();

    let text = assembly
        .split_once(".section\t__TEXT,__text,regular,pure_instructions\n")
        .map(|(_, rest)| rest)
        .expect("the text section should be present");

    assert!(
        text.starts_with("\t.globl\t_main\n\t.p2align\t2\n_main:\n"),
        "got: {text}"
    );
}

/// Instructions land in the text section, indented, in the order they were emitted.
#[test]
fn instructions_appear_in_the_text_section_in_order() {
    let assembly = main_returning_zero().finish();

    assert!(
        assembly.contains("_main:\n\tmov w0, #0\n\tret\n"),
        "got: {assembly}"
    );
}

/// A section that nothing was emitted into does not appear at all.
///
/// An empty `__DATA,__data` assembles perfectly well and says something untrue about the program,
/// which makes a snapshot of a program with no globals harder to read than it needs to be.
#[test]
fn an_empty_section_is_left_out() {
    let assembly = main_returning_zero().finish();

    assert!(!assembly.contains("__DATA,__data"), "got: {assembly}");
    assert!(!assembly.contains("__cstring"), "got: {assembly}");
    assert!(!assembly.contains(".zerofill"), "got: {assembly}");
}

/// Sections come out in a fixed order however they were written into.
#[test]
fn sections_are_concatenated_in_a_fixed_order() {
    let mut emitter = Emitter::new();
    emitter.reserve_zeroed("blank", 4, 2);
    emitter.define_word("counter", 7, true);
    emitter.define_string("l_.str.0", b"hi");
    emitter.begin_function("main");
    emitter.instruction("ret");

    let assembly = emitter.finish();
    let position = |needle: &str| {
        assembly
            .find(needle)
            .unwrap_or_else(|| panic!("{needle} missing from:\n{assembly}"))
    };

    assert!(position("__TEXT,__text") < position("__TEXT,__cstring"));
    assert!(position("__TEXT,__cstring") < position("__DATA,__data"));
    assert!(position("__DATA,__data") < position(".zerofill"));
}

/// The file ends with the directive that lets the linker drop unreferenced symbols.
#[test]
fn the_file_ends_with_subsections_via_symbols() {
    let assembly = main_returning_zero().finish();

    assert!(
        assembly.ends_with(".subsections_via_symbols\n"),
        "got: {assembly}"
    );
}

/// Labels read as what they are for, and no two are ever the same.
///
/// The counter runs across the whole file rather than restarting per function. A label that
/// restarted would be readable and wrong: `Lif_else_1` in two functions is one name for two places,
/// and the assembler would take the first.
#[test]
fn generated_labels_are_readable_and_unique() {
    let mut emitter = Emitter::new();

    let first = emitter.new_label("if_else");
    let second = emitter.new_label("if_else");
    let third = emitter.new_label("while_body");

    assert!(first.starts_with("Lif_else_"), "got {first}");
    assert!(third.starts_with("Lwhile_body_"), "got {third}");
    assert_ne!(first, second);
}

/// Many labels, requested the way nested control flow would request them, are all distinct.
#[test]
fn labels_stay_unique_across_nested_contexts() {
    let mut emitter = Emitter::new();
    let mut seen = Vec::new();

    for _ in 0..200 {
        for purpose in ["if_else", "if_end", "while_top", "while_end", "and_short"] {
            seen.push(emitter.new_label(purpose));
        }
    }

    let count = seen.len();
    seen.sort();
    seen.dedup();

    assert_eq!(seen.len(), count, "a generated label was handed out twice");
}

/// Emitting the same program twice produces the same bytes.
///
/// Snapshots are only worth having if the output is a function of its input, so this is checked
/// directly rather than inferred from the snapshots happening to pass.
#[test]
fn the_same_input_emits_the_same_bytes() {
    assert_eq!(
        main_returning_zero().finish(),
        main_returning_zero().finish()
    );
}

/// A global word is announced, aligned, labelled, and given its value.
#[test]
fn a_global_word_is_defined_with_its_alignment_and_value() {
    let mut emitter = Emitter::new();
    emitter.define_word("counter", 7, true);

    let assembly = emitter.finish();

    assert!(
        assembly.contains("\t.globl\t_counter\n\t.p2align\t2\n_counter:\n\t.long\t7\n"),
        "got: {assembly}"
    );
}

/// A string literal is emitted null-terminated, with its bytes escaped for the assembler.
#[test]
fn a_string_literal_is_emitted_null_terminated_and_escaped() {
    let mut emitter = Emitter::new();
    emitter.define_string("l_.str.0", b"hi\n\"q\\\t");

    let assembly = emitter.finish();

    assert!(
        assembly.contains("l_.str.0:\n\t.asciz\t\"hi\\n\\\"q\\\\\\t\"\n"),
        "got: {assembly}"
    );
}

/// A byte that has no escape and does not print is emitted as an octal escape.
#[test]
fn an_unprintable_byte_is_emitted_as_an_octal_escape() {
    let mut emitter = Emitter::new();
    emitter.define_string("l_.str.0", &[1, 200]);

    let assembly = emitter.finish();

    assert!(
        assembly.contains("\t.asciz\t\"\\001\\310\"\n"),
        "got: {assembly}"
    );
}

/// An uninitialized global is reserved in `__bss` rather than written out as zeroes.
#[test]
fn an_uninitialized_global_is_reserved_in_bss() {
    let mut emitter = Emitter::new();
    emitter.reserve_zeroed("blank", 40, 2);

    let assembly = emitter.finish();

    assert!(
        assembly.contains("\t.globl\t_blank\n.zerofill __DATA,__bss,_blank,40,2\n"),
        "got: {assembly}"
    );
}

/// Taking a symbol's address is the two-instruction page-plus-offset pair ARM64 requires.
#[test]
fn a_symbol_address_is_loaded_as_a_page_and_an_offset() {
    let mut emitter = Emitter::new();
    emitter.begin_function("main");
    emitter.address_of("x0", "_counter");

    let assembly = emitter.finish();

    assert!(
        assembly.contains("\tadrp x0, _counter@PAGE\n\tadd x0, x0, _counter@PAGEOFF\n"),
        "got: {assembly}"
    );
}

/// A call and the branches read as the instructions they are.
#[test]
fn calls_and_branches_emit_their_instructions() {
    let mut emitter = Emitter::new();
    emitter.begin_function("main");
    emitter.call("helper");
    emitter.branch("Lend_1");
    emitter.branch_if_zero("w0", "Lelse_2");
    emitter.place_label("Lend_1");

    let assembly = emitter.finish();

    assert!(assembly.contains("\tbl _helper\n"), "got: {assembly}");
    assert!(assembly.contains("\tb Lend_1\n"), "got: {assembly}");
    assert!(assembly.contains("\tcbz w0, Lelse_2\n"), "got: {assembly}");
    assert!(assembly.contains("Lend_1:\n"), "got: {assembly}");
}
