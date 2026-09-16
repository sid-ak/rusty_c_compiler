# Unit Test Report — Semantic Analysis: The Analyzer

## Unit

Source under test: `src/sema/mod.rs`, with `tests/sema_snapshots.rs` and `tests/invalid_programs.rs`
at the integration level.

This is the pass that decides whether a program that is *well-formed* is also *meaningful*. The
parser has already established that the text is grammatical; that is a different question from
whether it makes sense. `f(1, 2)` parses whatever `f` is; whether it is a function, and whether it
takes two arguments, is this unit's problem.

It does two things at once:

- **Rejects.** Thirty-one distinct checks, each with its own message and its own position in the
  source: an undeclared name, a call with the wrong number of arguments, `break` outside a loop, a
  value-returning function whose control flow can reach its closing brace, an array used where a
  number belongs.
- **Records.** For every node in the tree, it writes down what the code generator will need and
  would otherwise have to work out again: the type of every expression, the declaration every name
  refers to, the implicit conversions C requires, what storage each function needs, and one label
  per distinct string literal.

It walks the program twice. The first pass registers everything declared at the top level; the
second walks the function bodies. That order is what lets a function call something defined further
down the file.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: a rejection fixture per rule at the integration level, structural tests on the walk
itself at the unit level, and snapshots of the recorded annotations.**

The three layers exist because the unit does three separable things, and testing them together would
make every failure ambiguous.

1. **One rejected program per rule, as a file.** `tests/programs/invalid/` holds a small C program
   for each of the thirty-one checks, each naming in its own header the rule it violates. The test
   asserts the specific message and the specific position, not merely that something was rejected —
   a test that only checks for rejection passes against a compiler that rejects for the wrong
   reason, which is a worse failure than accepting, because the message sends the reader to the
   wrong line.

2. **`clang` is consulted about every one of them.** For each invalid program the suite also records
   what `clang` does with it. Almost all are rejected by both; four are real C that `clang` builds
   and this compiler turns down on purpose, and those four are listed in the architecture document.
   A test fails if the list and the corpus ever disagree — which is the mechanism that keeps a
   deliberate restriction from being indistinguishable from a bug.

3. **The walk is tested structurally, apart from the rules.** That the second pass sees what the
   first one collected; that one error does not stop the walk, so a file with four mistakes reports
   four; that no tree, however it was built, can drive the walk off the stack. This last one is
   tested by constructing a tree far deeper than any parser would produce and handing it in
   directly — the analyzer is a public entry point, and testing it only with trees the parser
   actually emits would leave its own depth guard unexercised.

4. **The annotations are snapshotted.** What this pass records is a large structured object, and the
   only practical way to notice an unintended change in it is to compare the whole thing against a
   checked-in copy. The snapshots cover every program in the corpus.

### Why this test methodology?

The two halves of this pass fail in opposite ways, and need opposite tests. A missing *rejection* is
a program that should not compile and does — found by having a program per rule. A wrong
*annotation* is a program that compiles and then behaves incorrectly, with nothing to see at this
stage at all — found by pinning the whole recorded output, because there is no single value to
assert on.

The deliberate choice here is that the rules are tested through files rather than through
constructed trees. A file is the thing a user actually hands the compiler, it can be run through
`clang` for a second opinion, and it does not go stale when an internal type changes shape.

## Test Coverage

Every one of the thirty-one checks has a program that provokes it, and a test that asserts its
message and position. Every program in the valid corpus has a snapshot of its annotations. The
walk's own properties — two passes, error recovery, depth — are covered at the unit level.

Not covered here: whether the annotations are *useful*, which is only answerable by generating code
from them and running it. That is the code generation units' work, and ultimately the differential
suite's.

## Automated Test Code

<!-- inventory: src/sema/tests.rs, tests/sema_snapshots.rs, tests/invalid_programs.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `a_call_to_a_function_defined_later_resolves` | A call to a function defined further down the file resolves, which is what pass A is for. |
| 2 | `a_call_to_an_undeclared_function_is_rejected` | A call to a name that is never declared anywhere is still rejected. |
| 3 | `four_errors_yield_exactly_four_diagnostics_in_source_order` | Analysis keeps walking after an error, so four mistakes produce four messages in source order. |
| 4 | `a_redeclaration_names_the_declaration_it_collides_with` | A redeclaration points at the declaration it collides with as well as at itself. |
| 5 | `a_declaration_followed_by_a_matching_definition_is_accepted` | A forward declaration followed by a matching definition is one function, not two. |
| 6 | `a_definition_disagreeing_with_its_declaration_is_rejected` | A definition that disagrees with an earlier declaration is rejected, whichever part differs. |
| 7 | `two_definitions_of_one_function_are_rejected` | Defining the same function twice is rejected even when the two signatures agree. |
| 8 | `a_call_with_the_wrong_arity_is_rejected` | A call has to pass as many arguments as the function declares. |
| 9 | `a_non_constant_global_initializer_is_rejected` | A global initializer has to be a constant, since it is written into the data section. |
| 10 | `a_folded_constant_global_initializer_is_accepted` | A constant expression folded from literals is a legal global initializer. |
| 11 | `the_representative_program_is_accepted` | A valid program produces no diagnostics at all. |
| 12 | `every_expression_node_of_a_valid_program_is_typed` | Every expression node in a valid program has a type recorded against it. |
| 13 | `every_identifier_of_a_valid_program_is_bound` | Every identifier in a valid program resolves to a symbol that analysis recorded. |
| 14 | `a_tree_nested_past_the_limit_reports_the_limit_on_a_small_stack` | An expression nested past the limit reports it rather than running off the stack. |
| 15 | `every_rule_accepts_its_legal_program` | Every rule's legal program analyzes with no diagnostics at all. |
| 16 | `every_rule_rejects_its_illegal_program_at_the_right_span` | Every rule's illegal program produces exactly its message, pointing at exactly its source text. |
| 17 | `the_rule_table_has_no_duplicate_rows` | Every rule in the table is distinct, so a row cannot be silently duplicated instead of added. |
| 18 | `a_collision_note_points_at_the_earlier_declaration` | A redeclaration's note points at the declaration it collides with, rather than only saying so. |
| 19 | `reachability_accepts_only_the_shapes_that_always_return` | Control-flow reachability is judged on the shapes that decide it, not on the last statement. |
| 20 | `main_may_reach_its_closing_brace` | `main` may fall off its end, because C defines an implicit `return 0` there. |
| 21 | `a_void_function_may_reach_its_closing_brace` | A `void` function may reach its closing brace, since it has nothing to return. |
| 22 | `an_array_argument_is_marked_to_decay_and_reads_back_as_a_pointer` | An array passed as an argument is marked to decay, and reads back as a pointer. |
| 23 | `indexing_an_array_types_as_its_element_without_decaying_it` | Indexing an array types as its element, and the base is not marked to decay. |
| 24 | `a_char_is_marked_to_promote_in_arithmetic_and_not_elsewhere` | A `char` is marked to promote in arithmetic, and only where a promotion actually applies. |
| 25 | `an_int_assigned_to_a_char_is_marked_to_truncate` | An `int` stored into a `char` is marked to truncate. |
| 26 | `a_value_that_needs_no_conversion_is_not_marked` | Nothing that needs no conversion carries one. |
| 27 | `the_frame_inventory_lists_every_local_and_parameter_with_its_layout` | The frame inventory lists every local and parameter of a function, with its layout. |
| 28 | `a_function_with_nothing_to_store_has_an_empty_frame` | A function with no locals and no parameters has an empty frame rather than none. |
| 29 | `identical_string_literals_share_one_label` | Two occurrences of one literal share a label; two different literals get two. |
| 30 | `every_string_literal_node_carries_its_label` | Each string literal node carries the label its bytes interned to. |
| 31 | `the_annotation_dump_is_the_same_on_every_run` | Analyzing the same program twice produces the same annotations, character for character. |
| 32 | `a_char_array_may_be_initialized_from_a_string_literal` | A `char` array initialized from a string literal stays legal, terminator included. |
| 33 | `a_string_literal_longer_than_its_array_is_reported_once` | A string literal too long for the array it fills is still reported, and only once. |
| 34 | `assigning_to_an_array_reports_one_problem_not_two` | Assigning an array reports that it is not assignable, and does not also report the types. |
| 35 | `a_void_function_returning_a_value_reports_one_problem_not_two` | A bare `return` in a `void` function is the `void` rule alone, not a compatibility failure too. |
| 36 | `every_program_analyzes` | Every program in the corpus analyzes, which is the exit criterion `--check` is held to. |
| 37 | `every_defined_function_has_a_frame` | Every program in the corpus has a frame for each function it defines. |
| 38 | `the_dump_is_deterministic_across_runs` | Analyzing the same program twice produces the same annotations. |
| 39 | `arithmetic_program_annotations` | The annotations for the arithmetic corpus program: the type of every operator and operand. |
| 40 | `control_flow_program_annotations` | The annotations for the control-flow corpus program: conditions and the frames around them. |
| 41 | `functions_program_annotations` | The annotations for the functions corpus program: signatures, calls, and parameter slots. |
| 42 | `arrays_program_annotations` | The annotations for the arrays corpus program: where decay is recorded, and where it is not. |
| 43 | `strings_program_annotations` | The annotations for the strings corpus program: interned literals and `char` promotions. |
| 44 | `the_corpus_has_programs_in_it` | The corpus is not empty, so the checks over it are not passing by having nothing to check. |
| 45 | `every_invalid_program_is_rejected_with_its_stated_message` | Every invalid program is rejected, with the message its header promises. |
| 46 | `clang_agrees_or_the_deviation_is_recorded` | Every invalid program is rejected by `clang` too, or says why it is not. |
| 47 | `every_deviation_cites_a_decision_and_a_reason` | Every deviation names the ADR that decided it, and gives a reason. |
| 48 | `the_architecture_document_lists_exactly_the_deviations_in_the_corpus` | The deviations in the corpus and the ones in the architecture document are the same set. |
| 49 | `every_invalid_program_is_in_the_coverage_matrix` | Every invalid program has a row in the corpus's coverage matrix. |
| 50 | `each_rule_is_covered_by_exactly_one_program` | Every rule named in the corpus is named once, so two files cannot cover one rule and none another. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
