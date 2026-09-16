# Unit Test Report — Parser: Statements, Declarations & Recovery

## Unit

Source under test: `src/parser/mod.rs` (everything outside the `expr` submodule: `Parser::program`,
`item`, `function`, `params`/`param`, `block`/`block_item`, `statement` and its per-keyword handlers,
`declarator`/`type_spec`/`finish_var_decl`/`array_type`/`initializer`/`name`, the error-recovery
machinery `recover_from`/`skip_to_boundary`, and the depth-limited-recursion guard `nested`), plus
two integration test files that exercise it end-to-end: `tests/parser_no_panic.rs` (corpus-truncation
and adversarial-input robustness) and `tests/parser_snapshots.rs` (whole-program AST snapshots over
`tests/programs/`). This unit depends on the Parser: Expressions unit (`src/parser/expr.rs`) for
expression parsing within statements/initializers, and on the Lexer units for its token input, but
owns the declaration grammar, the statement grammar, and — uniquely to this unit — all error
recovery and the stack-safety guard.

## Date

2026-08-23

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: black-box, example-based unit testing over the declaration/statement grammar via
equivalence partitioning (mirroring the AST/Expression units' technique of asserting on tree shape,
never on an evaluated result), combined with three methodologies specific to this unit's unique
responsibilities — recovery/robustness testing for the "reports and keeps going" invariant,
resource-exhaustion (stack-safety) testing for the "does not overflow the stack" invariant, and
corpus-based mutation-adjacent testing (systematic truncation) for the "it terminates" invariant —
each chosen because it is the methodology this module's own doc comment identifies as the property
that must hold for *every* input, not just well-formed ones.**

`src/parser/mod.rs`'s module doc comment states three properties that hold for every input, valid or
not: it reports and keeps going, it terminates, and it does not overflow the stack. Unlike the
Expression unit (whose correctness is almost entirely about shape), this unit's hardest bugs live in
these three cross-cutting properties, so the methodology is organized around proving each one
directly rather than trusting it to fall out of ordinary example tests.

1. **Equivalence partitioning over the declaration and statement grammars**, exhaustively:
   `every_declaration_form_parses` covers all 8 declaration shapes (plain, initialized, sized array,
   array with full initializer list, empty initializer list, trailing comma, `char`, unsized array
   parameter); `every_statement_form_parses` covers all 12 statement forms (block, nested block,
   empty, expression, local declaration, bare/valued return, break, continue, if, if/else, while) —
   each wrapped in a minimal function so the test is really isolating the statement grammar itself.
   `functions_take_zero_through_nine_parameters` sweeps a boundary specifically called out in the
   test's own comment as ABI-relevant (Phase 4 will cross an 8-register boundary at parameter 9), a
   direct instance of boundary-value analysis chosen because of a *downstream* system property, not
   just an arbitrary round number.

2. **Combinatorial (pairwise/full-combination) testing for optional-clause constructs.**
   `every_combination_of_for_clauses_parses` tests all 2³ = 8 combinations of present/absent
   `for`-loop clauses, verified by checking that each clause's dump slot is present or absent exactly
   according to that combination — this is full combinatorial coverage of a 3-boolean input space,
   which is tractable and complete here precisely because the space is small (a for loop's grammar
   is the textbook case where exhaustive combination testing is affordable and worthwhile, versus
   pairwise testing for larger spaces). `a_dangling_else_binds_to_the_nearest_if` checks the
   classic dangling-else ambiguity three `if` levels deep specifically ("so a rule that happened to
   work at two levels does not pass by luck" — the test's own stated rationale for choosing depth 3
   over depth 2, i.e. boundary-value analysis on ambiguity depth).

3. **Recovery/robustness testing for the "reports and keeps going" invariant**, this unit's most
   distinctive methodology. Tests are split into positive recovery-quality checks and
   negative-space bounds:
   - `parsing_resumes_after_a_bad_statement` and `recovery_does_not_swallow_the_following_function`
     assert not just that recovery doesn't crash, but that it recovers to the *right place* — the
     statement after a mistake, or the next function, are proven still present in the resulting
     tree, which is a stronger and more specific claim than "no panic."
   - `two_errors_produce_exactly_two_diagnostics` and
     `an_out_of_subset_type_definition_reports_once`/`an_unbalanced_brace_at_eof_reports_once` are
     the negative-space companion: independent mistakes must produce *exactly* one diagnostic each,
     not a cascade — this is the specific defect class ("error cascade") that ad hoc recovery logic
     is most prone to, and it is tested by exact count, not just "at least one."
   - `a_file_of_closing_parentheses_terminates` (bounded diagnostic count, not just termination) and
     `a_file_of_operators_reports_a_bounded_number_of_times` (integration test, asserting diagnostic
     count ≤ token count) go further: they prove recovery cannot amplify a small malformed input into
     an unboundedly large diagnostic stream, a distinct failure mode from both crashing and simply
     failing to recover.

4. **Resource-exhaustion / boundary testing for the "does not overflow the stack" invariant.** This
   is the most rigorous methodology in the unit and is worth calling out specifically:
   `nesting_past_the_limit_is_reported` and `deeply_nested_blocks_are_reported` test the
   `MAX_NESTING_DEPTH` boundary is enforced (for both expression nesting and block nesting — two
   independent recursive-descent paths that must both respect the same guard);
   `nesting_within_the_limit_parses` is the boundary's other side, proving the guard does not
   over-reject; and `nesting_stays_within_a_small_stack` is a genuine resource-constrained test — it
   spawns a real OS thread with a stack deliberately set to 512 KiB (a quarter of the test harness's
   normal 2 MiB, a sixteenth of the production binary's 8 MiB main-thread stack) and proves parsing
   at the depth limit does not overflow *that* stack. This is not simulated or mocked — it exercises
   the actual call stack under real memory pressure, which is the only way to validate a
   stack-safety margin claim rather than merely assert a counter is compared correctly. The margin
   figure itself (4 KB/level, comfortably under the reduced budget) is stated in the source as
   measured, not guessed, and this test is what keeps that measurement from silently going stale as
   the parser's stack frames grow over time.

5. **Corpus-based mutation-adjacent robustness testing** (`tests/parser_no_panic.rs`, run as part of
   this unit because it specifically targets the statement/declaration/recovery machinery under
   malformed input): every corpus program is truncated at every token boundary
   (`token_level_truncations_do_not_panic`) and at *every single byte offset*
   (`byte_level_truncations_do_not_panic`) — for the 5 corpus programs (~1.5–2.4 KB each), this is
   several thousand distinct parse calls per test run, none of which may panic or hang. This
   technique is explicitly a "cheap precursor" to real fuzzing (stated in the file's own doc
   comment), generating adversarial inputs for free by mutating known-valid programs rather than
   requiring a corpus of adversarial inputs to be hand-written — every prefix of a valid program is
   exactly the shape of input a parser is likely to mishandle (an opened, never-closed construct).
   `a_truncated_program_is_reported` is the sanity check on the whole techique: it proves the
   assertion being made is not vacuous by checking a genuinely truncated program *does* report a
   problem (a parser that silently accepted everything would otherwise pass every truncation test for
   the wrong reason). This is complemented by 8 hand-written adversarial fixtures in `tests/
   adversarial/` (`adversarial_inputs_do_not_panic`, plus 3 more tests targeting specific ones: empty
   input, deep nesting, an operator-only file, and a very-long-token file) aimed at the specific
   failure modes a recursive-descent parser is prone to that pure truncation would not reliably
   generate (e.g. deliberately deep nesting, rather than nesting that happens to occur in the
   corpus).

6. **Golden-master (snapshot) testing over whole real programs** (`tests/parser_snapshots.rs`): each
   of the 5 corpus programs (arithmetic, control flow, functions, arrays, strings) is parsed and
   its full AST dump is pinned against a checked-in `insta` snapshot, proving the individually-tested
   grammar productions compose correctly over realistic, feature-complete programs — the same
   "unit tests prove one production, snapshots prove they compose" rationale used in the Lexer
   Scanner unit. `every_program_is_in_the_coverage_matrix` and `the_corpus_has_programs_in_it` are
   process/meta tests that keep the corpus itself honest (a program with no row in
   `COVERAGE.md` is a program nobody knows the purpose of, per that file's own stated policy) —
   included here because a corpus that silently stops being checked is a coverage regression just as
   real as a missing test.

**Coverage assessment.** All 8 declaration shapes, all 12 statement forms, all 8 `for`-clause
combinations, all major syntax-error categories (missing token, wrong token, keyword-as-identifier,
declaration in illegal position, unsupported C construct — 18 distinct out-of-subset constructs
tested by `unsupported_constructs_are_named`), and all three stated cross-cutting invariants
(reports-and-continues, terminates, stack-safe) have direct, targeted tests — not merely incidental
coverage from testing the happy path. The corpus-truncation tests provide coverage of the input
space that hand-written cases cannot feasibly enumerate (every prefix of every corpus program, at
both token and byte granularity). The one explicitly acknowledged gap, consistent with the
Diagnostics/Lexer units: true `cargo-fuzz`-driven fuzzing of the whole front end is a documented
Phase 5 deliverable, not yet present in this repo (confirmed: no `fuzz/` directory exists and
`cargo fuzz` is not installed — see `reports/unit_tests/00-environment.md`); the corpus-truncation
suite here is the interim, deterministic technique the project's own documentation
(`AGENTS.md`, `docs/dive-deep/testing.md`) describes as the "cheap precursor" to it.

## Automated Test Code

48 tests total: 30 in `src/parser/mod.rs` under `#[cfg(test)] mod tests`, 8 in
`tests/parser_no_panic.rs`, and 10 in `tests/parser_snapshots.rs`.

### `src/parser/mod.rs` (30 tests)

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 1 | `the_parser_gives_every_node_a_distinct_id` | `BROAD` fixture (~15-line program spanning most of the grammar) | >40 AST nodes; all ids distinct |
| 2 | `every_span_points_into_the_source` | `BROAD` fixture | Every node span: `start<=end`, `end<=len(source)`, non-empty |
| 3 | `an_empty_file_is_an_empty_program` | `""` | `"(program)\n"` |
| 4 | `a_function_definition_parses` | `int main(void) { return 0; }` | Exact dump: func-def → params → block → return → int-lit |
| 5 | `empty_and_void_parameter_lists_agree` | `int f() { }` vs `int f(void) { }` | Identical dumps |
| 6 | `a_forward_declaration_precedes_its_definition` | `int f(int n); int f(int n) { return n; }` | `(program (func-decl ...) (func-def ...))` |
| 7 | `functions_take_zero_through_nine_parameters` | 0 through 9 params | `(param ` count matches param count, each |
| 8 | `every_declaration_form_parses` | 8 declaration forms (see methodology) | Exact shape string per case |
| 9 | `every_statement_form_parses` | 12 statement forms wrapped in `void f(void){...}` | Exact shape string per case |
| 10 | `bodies_may_be_a_single_unbraced_statement` | `if(a)b;`, `while(a)b;`, `for(;;)b;` | Dump does not contain `(block (block` (no implicit re-wrapping) |
| 11 | `a_dangling_else_binds_to_the_nearest_if` | 3-deep nested `if`/`else` | `else` binds to innermost `if` |
| 12 | `every_combination_of_for_clauses_parses` | All 8 combos of init/cond/step present or absent | Each dump slot present iff its clause was written |
| 13 | `a_for_initializer_may_declare` | `for (int i = 0; i < 3; i = i + 1) x;` | Dump contains `(init (local-var int i (init (int-lit 0))))` |
| 14 | `a_declaration_may_not_be_a_branch_body` | Decl as `if`/`while`/`for` body, 3 cases | `["a declaration is not allowed here"]` each |
| 15 | `syntax_errors_name_the_token_they_wanted` | 10 malformed programs | 10 exact "expected X, found Y" / custom messages |
| 16 | `a_keyword_used_as_an_identifier_is_rejected` | `int while;`, `int f(void){int return;}` | `"expected an identifier, found 'while'"`, `"...'return'"` |
| 17 | `two_errors_produce_exactly_two_diagnostics` | Two `int a = ;` mistakes in one function | Exactly 2 identical "expected an expression, found ';'" |
| 18 | `parsing_resumes_after_a_bad_statement` | `int f(void) { int a = ; return 7; }` | 1 diagnostic; tree still contains `return 7` |
| 19 | `recovery_does_not_swallow_the_following_function` | Missing `;` in `f`, valid `g` follows | 1 diagnostic; both functions present in tree |
| 20 | `an_unbalanced_brace_at_eof_reports_once` | `int f(void) { int a = 1;` (no closing brace) | `["expected '}', found end of file"]` |
| 21 | `a_file_of_closing_parentheses_terminates` | 33 `)` characters | Non-empty, but < 5 diagnostics (bounded) |
| 22 | `a_file_of_braces_terminates` | 4 unbalanced-brace fixtures | Each: parse completes with non-empty diagnostics |
| 23 | `unsupported_constructs_are_named` | 18 out-of-subset constructs (`struct`, `union`, `sizeof`, pointers, etc.) | Each: first message == `"unsupported in this C subset: '<word>'"` |
| 24 | `an_out_of_subset_type_definition_reports_once` | `struct`/`union`/`enum`/`typedef` definitions | Each: exactly 1 diagnostic |
| 25 | `parsing_resumes_after_an_unsupported_definition` | `struct point {...}; int main(...){...}` | 1 diagnostic; `main` still parses (1 item) |
| 26 | `nesting_past_the_limit_is_reported` | Expression nested `MAX_NESTING_DEPTH*4` deep | First message starts `"nesting is too deep"` |
| 27 | `deeply_nested_blocks_are_reported` | Blocks nested `MAX_NESTING_DEPTH*4` deep | Same |
| 28 | `nesting_stays_within_a_small_stack` | Same deep-nesting input, parsed on a 512 KiB thread stack | Parses (no overflow); reports depth-limit message |
| 29 | `nesting_within_the_limit_parses` | Nested `MAX_NESTING_DEPTH/4` deep | Dump contains `(int-lit 1)` (parses successfully) |
| 30 | `an_empty_token_slice_parses` | `parse(&[])` (no `Eof` even) | Empty program; no diagnostics |

### `tests/parser_no_panic.rs` (8 tests)

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 31 | `token_level_truncations_do_not_panic` | Every corpus program, truncated at every token boundary | Every truncation parses without panic/hang |
| 32 | `byte_level_truncations_do_not_panic` | Every corpus program, truncated at every byte offset | Same, finer granularity |
| 33 | `a_truncated_program_is_reported` | `functions.c` truncated to 2/3 length | `parse(prefix) > 0` diagnostics (sanity check the technique isn't vacuous) |
| 34 | `adversarial_inputs_do_not_panic` | 8+ hand-written adversarial `.c` fixtures | All parse without panic |
| 35 | `inputs_with_no_tokens_are_empty_programs` | `empty.c`, `only_whitespace.c`, `only_comment.c` | Empty program; 0 diagnostics, each |
| 36 | `deep_nesting_reports_the_depth_limit` | `deep_parens.c`, `deep_blocks.c`, `deep_unclosed_parens.c` | Each: a diagnostic starting `"nesting is too deep"` |
| 37 | `a_file_of_operators_reports_a_bounded_number_of_times` | `only_operators.c` | Non-empty diagnostics; count ≤ token count |
| 38 | `very_long_tokens_are_carried_through` | `long_identifier.c` | 0 diagnostics; 1 item parsed |

### `tests/parser_snapshots.rs` (10 tests)

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 39 | `every_program_is_in_the_coverage_matrix` | Every corpus `.c` file name | Present in `COVERAGE.md` |
| 40 | `the_corpus_has_programs_in_it` | Corpus directory listing | ≥ 5 programs |
| 41 | `every_program_parses` | Every corpus program | Dump starts with `"(program\n"` |
| 42 | `the_dump_is_deterministic_across_runs` | Every corpus program, dumped twice | Byte-identical both times |
| 43 | `arithmetic_program_tree` | `arithmetic.c` | Matches pinned snapshot |
| 44 | `control_flow_program_tree` | `control_flow.c` | Matches pinned snapshot |
| 45 | `functions_program_tree` | `functions.c` | Matches pinned snapshot |
| 46 | `arrays_program_tree` | `arrays.c` | Matches pinned snapshot |
| 47 | `strings_program_tree` | `strings.c` | Matches pinned snapshot |
| 48 | `spans_are_recorded_for_every_node` | `int add(int a, int b) { return a + b; }` | Exact inline snapshot with `@start..end` on every node |

## Actual Outputs

Executed as part of `cargo test --lib`, `cargo test --test parser_no_panic`, and
`cargo test --test parser_snapshots` (full unedited capture in
`reports/unit_tests/cargo_test_output.txt`):

```
     Running unittests src/lib.rs
test parser::tests::a_file_of_braces_terminates ... ok
test parser::tests::a_declaration_may_not_be_a_branch_body ... ok
test parser::tests::a_file_of_closing_parentheses_terminates ... ok
test parser::tests::a_forward_declaration_precedes_its_definition ... ok
test parser::tests::a_for_initializer_may_declare ... ok
test parser::tests::a_keyword_used_as_an_identifier_is_rejected ... ok
test parser::tests::a_function_definition_parses ... ok
test parser::tests::an_empty_token_slice_parses ... ok
test parser::tests::an_empty_file_is_an_empty_program ... ok
test parser::tests::a_dangling_else_binds_to_the_nearest_if ... ok
test parser::tests::an_out_of_subset_type_definition_reports_once ... ok
test parser::tests::an_unbalanced_brace_at_eof_reports_once ... ok
test parser::tests::empty_and_void_parameter_lists_agree ... ok
test parser::tests::every_span_points_into_the_source ... ok
test parser::tests::deeply_nested_blocks_are_reported ... ok
test parser::tests::bodies_may_be_a_single_unbraced_statement ... ok
test parser::tests::every_declaration_form_parses ... ok
test parser::tests::nesting_past_the_limit_is_reported ... ok
test parser::tests::nesting_within_the_limit_parses ... ok
test parser::tests::functions_take_zero_through_nine_parameters ... ok
test parser::tests::parsing_resumes_after_a_bad_statement ... ok
test parser::tests::nesting_stays_within_a_small_stack ... ok
test parser::tests::parsing_resumes_after_an_unsupported_definition ... ok
test parser::tests::every_combination_of_for_clauses_parses ... ok
test parser::tests::recovery_does_not_swallow_the_following_function ... ok
test parser::tests::every_statement_form_parses ... ok
test parser::tests::syntax_errors_name_the_token_they_wanted ... ok
test parser::tests::two_errors_produce_exactly_two_diagnostics ... ok
test parser::tests::the_parser_gives_every_node_a_distinct_id ... ok
test parser::tests::unsupported_constructs_are_named ... ok

     Running tests/parser_no_panic.rs
test a_truncated_program_is_reported ... ok
test inputs_with_no_tokens_are_empty_programs ... ok
test a_file_of_operators_reports_a_bounded_number_of_times ... ok
test deep_nesting_reports_the_depth_limit ... ok
test very_long_tokens_are_carried_through ... ok
test adversarial_inputs_do_not_panic ... ok
test token_level_truncations_do_not_panic ... ok
test byte_level_truncations_do_not_panic ... ok
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s

     Running tests/parser_snapshots.rs
test every_program_is_in_the_coverage_matrix ... ok
test the_corpus_has_programs_in_it ... ok
test every_program_parses ... ok
test the_dump_is_deterministic_across_runs ... ok
test spans_are_recorded_for_every_node ... ok
test strings_program_tree ... ok
test arithmetic_program_tree ... ok
test control_flow_program_tree ... ok
test arrays_program_tree ... ok
test functions_program_tree ... ok
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

**Result: all 48 tests passed** (30 unit + 8 corpus-truncation integration + 10 snapshot
integration). No failures, no ignored tests. All 5 corpus-program snapshots and the inline
spans-shown snapshot compared cleanly with no diff. `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` reported no violations against any of the three files. Note:
`byte_level_truncations_do_not_panic` and `token_level_truncations_do_not_panic` alone represent
several thousand individual parse invocations (every byte offset × 5 corpus programs), all executing
without panic or hang, within the reported 0.76s for the whole `parser_no_panic` binary.
