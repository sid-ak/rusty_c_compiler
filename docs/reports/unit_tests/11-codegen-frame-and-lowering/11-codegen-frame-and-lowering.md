# Unit Test Report — Code Generation: Frames and Lowering

## Unit

Source under test: `src/codegen/frame.rs`, `src/codegen/expr.rs`, `src/codegen/stmt.rs`, and
`src/codegen/mod.rs`, with `tests/codegen_programs.rs` at the integration level.

This is the unit that turns an analyzed program into instructions. It has two halves that have to
agree with each other exactly:

- The frame. Each function gets a region of the stack — a *frame* — and every variable and every
  intermediate value in it is given a fixed place there. This compiler deliberately does not attempt
  register allocation: a value lives in memory and is pulled into a register only for the instant it
  is used. That trades speed for the removal of an entire category of bug, and it means the frame
  layout arithmetic is the thing that has to be right.
- The lowering. Expressions become instructions that read their operands in source order;
  statements become branches and labels; calls place their arguments where the platform's calling
  convention says they go.

The half that catches people out is where those two meet. A value's *width* — one byte for a `char`,
four for an `int`, eight for an address — has to be the same on both sides of every store and load.
A caller that writes four bytes where the callee reads eight produces a program that compiles,
links, runs, and is wrong in a way that depends on what was in memory.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

Approach: arithmetic tested as arithmetic at the unit level, and behavior tested by running the
program at the integration level, with operands chosen so that a wrong answer is a different answer.

1. Frame layout is checked by computing it. Offsets, sizes, alignment, and the total frame size
   are numbers, and are asserted as numbers — including at the boundaries where the platform's rules
   change, such as a frame large enough that the instruction which reserves it can no longer encode
   its own size.

2. Everything else is checked by running the program. A snapshot says what was emitted, and an
   assembler says it is legal, but only running the program says the answer is right. So the bulk of
   this unit's coverage is small C programs compiled, linked, executed, and compared against what
   they should print.

3. Non-commutative operators get asymmetric operands, always. `2 - 2` is `0` whichever way round
   the lowering reads its operands, so a compiler with them transposed passes it. `10 - 3` is `7`
   one way and `-7` the other. Every subtraction, division, remainder, and comparison in this unit's
   tests is written to tell the difference.

4. Each behavior gets its own program. A single large program exercising twenty features reports
   one failure whichever of the twenty broke. The integration tests are roughly two hundred small
   programs, each about one thing, so a failure names the construct rather than the file.

5. The cases that fail as a crash rather than as a wrong number are singled out. A `continue` in
   a `for` loop that jumps to the condition instead of the step produces a loop that never advances —
   a hang, not a wrong answer — so there is a test whose loop terminates only if the step runs. A
   mis-sized pointer produces garbage rather than an off-by-one, so there is a test for each shape
   where a pointer crosses a boundary: as the ninth argument, forwarded from an already-decayed
   parameter, followed by a value whose position depends on the pointer's size.

### Why this test methodology?

Code generation is the stage where being "nearly right" is indistinguishable from being right until
it is catastrophically not. An instruction sequence that is subtly wrong still assembles, still
links, and still runs; the failure surfaces as a wrong number in an unrelated place, or as a crash
in a program that does not contain the mistake.

That is the argument for executing rather than inspecting. It is also the argument for the two bugs
this unit's tests did not find on their own — both were found by whole programs in the corpus, and
both are described in [the Phase 4 explanation](../../../explanations/Phase-4-Explanation.md). A unit
test asks whether a piece does what it was written to do. Only a program asks whether the pieces
agree.

## Test Coverage

Covered: frame layout including the large-frame boundary; every operator and its grouping; every
statement and control-flow form, including every combination of present and absent `for` clauses;
calls at every arity from zero to twelve, which crosses the boundary where arguments stop arriving
in registers; recursion, single, double, and mutual; arrays, locally, globally, and across a call;
`char` storage and promotion; globals and string data; and the exit status a program leaves behind.

Not covered here: whether the emitted program agrees with `clang` on a program nobody wrote a test
for. That is the differential suite, reported in
[14 — The Differential Harness](../14-differential-harness.md).

## Automated Test Code

<!-- inventory: src/codegen/frame/tests.rs, tests/codegen_programs.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `every_frame_size_is_a_multiple_of_sixteen` | Every frame is a multiple of sixteen bytes, which AAPCS64 requires of the stack pointer. |
| 2 | `an_empty_frame_still_saves_the_frame_pointer_and_return_address` | A frame with nothing in it is still big enough for the saved frame pointer and return address. |
| 3 | `slots_start_above_the_saved_registers` | Slots start above the saved pair, so nothing can be written over it. |
| 4 | `every_slot_is_aligned_as_its_type_requires` | Each slot is aligned as its type requires. |
| 5 | `no_two_slots_share_a_byte` | No two slots overlap, and none runs past the end of the frame. |
| 6 | `an_unknown_slot_has_no_offset` | A slot that was never allocated has no offset, rather than a plausible wrong one. |
| 7 | `layout_is_deterministic` | Laying out the same frame twice gives the same answer. |
| 8 | `a_small_frame_opens_with_the_pre_indexed_form` | A frame small enough for the pre-indexed form opens with the two documented instructions. |
| 9 | `a_large_frame_lowers_the_stack_pointer_separately` | A frame too large for that immediate lowers the stack pointer separately instead. |
| 10 | `parameters_are_spilled_into_their_slots` | Parameters are copied out of their argument registers into their slots before the body runs. |
| 11 | `locals_are_not_spilled_in_the_prologue` | A local is not spilled from anywhere: it has no incoming register. |
| 12 | `the_epilogue_mirrors_the_prologue` | The epilogue undoes the prologue and returns, in the form that matches the frame's size. |
| 13 | `temporaries_are_counted_by_depth_not_by_quantity` | The temporary count is the depth of the deepest expression, not the number of expressions. |
| 14 | `an_outgoing_argument_area_sits_below_the_saved_registers` | A function that passes arguments on the stack reserves room for them at the bottom of its frame. |
| 15 | `argument_slots_are_distinct_across_calls_and_positions` | Argument slots are distinct across positions and across levels of call nesting. |
| 16 | `arithmetic_and_precedence` | Arithmetic groups the way C says it does, and each operator computes what it should. |
| 17 | `non_commutative_operators_read_left_then_right` | Non-commutative operators read their operands in source order. |
| 18 | `division_and_remainder_truncate_toward_zero` | Division and remainder truncate toward zero, which is what C requires of negative operands. |
| 19 | `constants_at_the_edges_of_int` | Constants at the edges of `int` survive the immediate-building path. |
| 20 | `locals_store_and_load` | Locals are stored and read back, and assignment yields the value assigned. |
| 21 | `short_circuit_skips_the_right_operand` | `&&` and `\|\|` skip the right operand when the left one already decides the answer. |
| 22 | `logical_operators_normalize_to_zero_or_one` | `&&` and `\|\|` produce exactly `0` or `1`, not whichever operand decided the answer. |
| 23 | `prefix_and_postfix_differ_in_the_value_they_produce` | Prefix and postfix increment differ in the value they produce, not only in what they store. |
| 24 | `arrays_index_read_and_write` | Arrays index by element, read and write, and scale the index by the element's size. |
| 25 | `a_short_initializer_list_zeroes_the_rest` | A brace list shorter than the array zeroes every element it does not reach. |
| 26 | `chars_store_in_one_byte_and_sign_extend` | A `char` occupies one byte and sign-extends when it is read back. |
| 27 | `nested_expressions_do_not_share_temporaries` | A deeply nested expression gives every level its own temporary. |
| 28 | `branches_take_one_arm` | `if` and `else` pick exactly one arm, and chains nest correctly. |
| 29 | `while_loops_run_and_terminate` | `while` tests before each iteration and runs until its condition fails. |
| 30 | `for_loops_handle_every_clause_combination` | Every combination of present and absent `for` clauses behaves as C says. |
| 31 | `continue_in_a_for_still_runs_the_step` | `continue` in a `for` runs the step clause, so the loop still advances. |
| 32 | `break_and_continue_bind_to_the_innermost_loop` | `break` and `continue` apply to the innermost loop enclosing them, not to any outer one. |
| 33 | `a_return_inside_nested_loops_leaves_the_function` | A `return` inside nested loops leaves the function, not just the loop. |
| 34 | `main_without_a_return_exits_zero` | `main` that reaches its closing brace exits zero, which C defines for `main` alone. |
| 35 | `arguments_arrive_at_every_arity` | Arguments arrive with the right values, at every arity including past the register boundary. |
| 36 | `a_pointer_passed_on_the_stack_arrives_whole` | A pointer passed on the stack arrives whole, and does not misalign what follows it. |
| 37 | `nested_calls_do_not_clobber_placed_arguments` | An argument that is itself a call does not clobber an argument already evaluated. |
| 38 | `nested_calls_survive_nine_arguments` | Nested calls survive at the stack-argument boundary too. |
| 39 | `recursion_computes_what_it_should` | Recursion works, at the shapes that exercise it hardest. |
| 40 | `mutual_recursion_works` | Two functions that call each other resolve and terminate. |
| 41 | `a_forward_declared_call_resolves` | A call to a function declared first and defined later resolves. |
| 42 | `a_void_function_is_called_as_a_statement` | A `void` function is called for its effect and returns nothing. |
| 43 | `an_array_is_mutated_through_a_call` | An array passed to a function is mutated in place and the caller sees the change. |
| 44 | `char_arguments_are_promoted` | A `char` argument is promoted before the call and arrives as the value C says it has. |
| 45 | `globals_hold_their_values` | Globals are read and written, keep their values across calls, and start out as declared. |
| 46 | `global_arrays_are_laid_out_and_iterated` | Global arrays are laid out element by element, zero-filled past their initializer. |
| 47 | `string_literals_print` | A string literal reaches `print_string` as the address of its bytes. |
| 48 | `escapes_round_trip_to_their_bytes` | Escapes reach stdout as the bytes the lexer decoded, not as the text that was written. |
| 49 | `a_repeated_literal_is_interned_once` | A literal written twice is one entry in the read-only section, and prints the same both times. |
| 50 | `a_char_array_prints_as_a_string` | A `char` array holding a string is passed to the shim as the pointer it decays to. |
| 51 | `construct_snapshots` | The emitted assembly for each construct, pinned so a regression is a readable diff. |
| 52 | `every_construct_assembles_cleanly` | Every construct's assembly is something the assembler accepts without comment. |
| 53 | `lowering_is_deterministic` | Lowering the same program twice produces the same bytes. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that unit's test run, together with when the source and
its tests were written, is checked in beside this report as
[`evidence_codegen_frame.md`](evidence_codegen_frame.md). It is regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than
trusted.
