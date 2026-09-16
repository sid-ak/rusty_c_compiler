# Unit Test Report — Code Generation: The Emitter

## Unit

Source under test: `src/codegen/emit.rs`, with `tests/codegen_snapshots.rs` at the integration level.

This unit is the part of the code generator that knows about *assembly as a document* rather than
about C. It owns:

- Sections. A finished assembly file is divided into regions: executable instructions in one,
  read-only data such as string literals in another, initialized globals in a third, and
  zero-initialized globals in a fourth. Instructions arrive in the order they were generated, but
  sections have to come out grouped.
- Symbol naming. A C function called `main` is a symbol called `_main` in a Mach-O object file.
  That leading underscore is a platform convention, and it lives here so that nothing else has to
  remember it.
- Labels. Every branch needs a destination, and every destination needs a name nothing else
  uses. The emitter hands those out.
- Alignment and directives. The assembler needs to be told how to align each section and what
  kind of thing each symbol is.

Nothing in this unit runs an assembler. Whether the text is something `clang` accepts is an
integration question that needs a real toolchain and a temporary file; what is pinned at the unit
level is everything decidable without one.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

Approach: golden-string assertions on the emitted text, uniqueness properties for the generated
names, and a separate integration test that hands the output to a real assembler.

1. The exact text, not a substring of it. The emitter's whole job is to produce a specific
   document, so tests assert the whole document for a small input rather than checking that it
   contains particular lines. A "contains" assertion on generated code is satisfied by output that
   also contains something disastrous.

2. Uniqueness as a property. Labels must never repeat within a function — two branches sharing a
   destination is a program that jumps to the wrong place — so the test generates many and asserts
   they are pairwise distinct, rather than generating two and comparing them.

3. Section grouping tested by interleaving. The tests deliberately emit into several sections in
   an order that is not the order they must appear in the output. An implementation that simply
   appended everything would pass a test that wrote the sections in their final order anyway.

4. The assembler as the judge, once. `tests/codegen_snapshots.rs` takes emitted output and runs
   the real assembler over it with warnings treated as errors. This is the test that would catch a
   directive that is well-formed as text and meaningless as assembly — something no amount of string
   comparison can notice.

### Why this test methodology?

Assembly text is a case where exact comparison is both possible and necessary. It is deterministic,
it is small for small inputs, and almost every mistake in it is a mistake of *detail* — a missing
underscore, a wrong alignment, a section header in the wrong place. Those are exactly the mistakes
that survive a loose assertion and exactly the ones a whole-output comparison cannot miss.

The division of labor matters too: the unit tests answer "is this the text we meant to write", and
the integration test answers "is this text meaningful". Neither question subsumes the other, and
answering only the second would make every failure a hunt through an assembler's error message.

## Test Coverage

Covered: every section, alone and interleaved; symbol naming for functions and for data; label
generation and uniqueness; the file's overall structure for a minimal program; and the assembler
accepting the result without a word of complaint.

Not covered here: whether the *instructions* are correct, which is not this unit's concern — it
writes down whatever it is given.

## Automated Test Code

<!-- inventory: src/codegen/emit/tests.rs, tests/codegen_snapshots.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `a_c_name_becomes_an_underscored_symbol` | A C name becomes a Mach-O symbol by gaining a leading underscore. |
| 2 | `a_function_is_declared_global_aligned_and_labelled` | A function is announced, aligned, and labelled, in that order. |
| 3 | `instructions_appear_in_the_text_section_in_order` | Instructions land in the text section, indented, in the order they were emitted. |
| 4 | `an_empty_section_is_left_out` | A section that nothing was emitted into does not appear at all. |
| 5 | `sections_are_concatenated_in_a_fixed_order` | Sections come out in a fixed order however they were written into. |
| 6 | `the_file_ends_with_subsections_via_symbols` | The file ends with the directive that lets the linker drop unreferenced symbols. |
| 7 | `generated_labels_are_readable_and_unique` | Labels read as what they are for, and no two are ever the same. |
| 8 | `labels_stay_unique_across_nested_contexts` | Many labels, requested the way nested control flow would request them, are all distinct. |
| 9 | `the_same_input_emits_the_same_bytes` | Emitting the same program twice produces the same bytes. |
| 10 | `a_global_word_is_defined_with_its_alignment_and_value` | A global word is announced, aligned, labelled, and given its value. |
| 11 | `a_string_literal_is_emitted_null_terminated_and_escaped` | A string literal is emitted null-terminated, with its bytes escaped for the assembler. |
| 12 | `an_unprintable_byte_is_emitted_as_an_octal_escape` | A byte that has no escape and does not print is emitted as an octal escape. |
| 13 | `an_uninitialized_global_is_reserved_in_bss` | An uninitialized global is reserved in `__bss` rather than written out as zeroes. |
| 14 | `a_symbol_address_is_loaded_as_a_page_and_an_offset` | Taking a symbol's address is the two-instruction page-plus-offset pair ARM64 requires. |
| 15 | `calls_and_branches_emit_their_instructions` | A call and the branches read as the instructions they are. |
| 16 | `empty_main_skeleton` | The assembly skeleton of a function that returns zero, directive for directive. |
| 17 | `every_section_together` | A file using every section, so the section order and each definition form are pinned together. |
| 18 | `the_skeleton_assembles_cleanly` | The skeleton assembles with no errors and nothing to say. |
| 19 | `every_section_assembles_cleanly` | A file using every section assembles too, which a snapshot alone would not catch. |
| 20 | `a_far_slot_survives_a_round_trip` | A frame too large for any immediate offset still stores and loads the right value. |
| 21 | `a_char_slot_sign_extends_when_it_is_read_back` | A `char` slot round-trips through the byte instructions, sign-extending on the way back. |
| 22 | `parameters_are_readable_from_their_slots_after_the_prologue` | Parameters arrive in registers and are readable from their slots once the prologue has run. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
