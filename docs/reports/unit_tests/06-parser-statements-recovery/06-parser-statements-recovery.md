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

2026-09-16

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
both token and byte granularity). The corpus-truncation suite is what the project's own documentation calls the
"cheap precursor" to fuzzing: deterministic, fast, and run on every change. `fuzz/fuzz_targets/parse.rs`
is the generated version, which mutates arbitrary bytes for as long as it is given and asserts the
same properties; `fuzz/regressions/` is where anything it finds becomes an ordinary test in this
suite. Both are reported in
[14 — The Differential Harness](../14-differential-harness.md).

## Automated Test Code

The tests live in `src/parser/tests.rs`, with the AST snapshots in `tests/parser_snapshots.rs` and
the truncation and adversarial sweeps in `tests/frontend_no_panic.rs`. The table is generated from
the tests themselves, so it cannot fall out of step with them.

<!-- inventory: src/parser/tests.rs, tests/parser_snapshots.rs, tests/frontend_no_panic.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `the_parser_lists_every_node_it_built_exactly_once` | Every node the parser builds gets an identity no other node has, and `ast::nodes` lists every one of them. Phase 3 keys its annotations by these ids, so a collision would give two nodes the same type and an omission would leave a node with none. |
| 2 | `every_span_points_into_the_source` | Every span the parser records is a real range inside the file it came from. |
| 3 | `an_empty_file_is_an_empty_program` | An empty file is a valid translation unit with nothing in it. |
| 4 | `a_function_definition_parses` | A function definition carries its return type, name, parameters, and body. |
| 5 | `empty_and_void_parameter_lists_agree` | An empty parameter list and an explicit `(void)` mean the same thing. |
| 6 | `a_forward_declaration_precedes_its_definition` | A declaration without a body is a forward declaration, and may be followed by the definition it promised. |
| 7 | `functions_take_zero_through_nine_parameters` | Zero through nine parameters parse. Nine matters because the ninth crosses the eight-register boundary the ABI draws in Phase 4. |
| 8 | `every_declaration_form_parses` | Every declaration form the grammar allows parses to the shape it describes. |
| 9 | `every_statement_form_parses` | Every statement form the grammar allows parses to the shape it describes. |
| 10 | `bodies_may_be_a_single_unbraced_statement` | A body may be a single statement without braces, at any of the three loop and branch forms. |
| 11 | `a_dangling_else_binds_to_the_nearest_if` | A dangling `else` binds to the nearest `if`, checked three deep so a rule that happened to work at two levels does not pass by luck. |
| 12 | `every_combination_of_for_clauses_parses` | All eight combinations of a present or absent `for` clause parse, each keeping its own slot in the dump so an omitted clause cannot be mistaken for a shifted one. |
| 13 | `a_for_initializer_may_declare` | A `for` initializer may declare its own variable. |
| 14 | `a_declaration_may_not_be_a_branch_body` | A declaration is a block item, not a statement, so it may not be a branch or loop body. |
| 15 | `syntax_errors_name_the_token_they_wanted` | Each syntax error names what was wanted and what was there, in source spelling. |
| 16 | `a_keyword_used_as_an_identifier_is_rejected` | A keyword where a name belongs is reported as the keyword it is. |
| 17 | `two_errors_produce_exactly_two_diagnostics` | Two independent mistakes in one function produce two diagnostics, not a cascade. |
| 18 | `parsing_resumes_after_a_bad_statement` | Recovery resumes at the next statement, so what follows a mistake still parses. |
| 19 | `recovery_does_not_swallow_the_following_function` | A mistake in one function does not consume the next one. |
| 20 | `an_unbalanced_brace_at_eof_reports_once` | A brace left open at the end of the file is reported once, not once per line after it. |
| 21 | `a_file_of_closing_parentheses_terminates` | Recovery cannot loop: a file of nothing but closing parentheses ends, having complained a bounded number of times. |
| 22 | `a_file_of_braces_terminates` | A file of nothing but braces ends too, whichever way they are unbalanced. |
| 23 | `unsupported_constructs_are_named` | Every C construct the subset leaves out is named as such, rather than reported as a program that does not parse. |
| 24 | `an_out_of_subset_type_definition_reports_once` | A type definition is one diagnostic, not one for the keyword and another for the `};` left behind after recovery skipped its body. |
| 25 | `parsing_resumes_after_an_unsupported_definition` | A definition the subset lacks does not take the declarations after it down with it. |
| 26 | `every_deep_shape_meets_the_depth_limit` | Every way of deepening the tree past the limit is a diagnostic rather than a stack overflow. |
| 27 | `deep_input_stays_within_a_small_stack` | Everything done with a parsed tree fits on a stack far smaller than any the compiler runs on, however deep the input tried to make it. |
| 28 | `every_shape_within_the_limit_parses` | Every shape nested inside the limit still parses, so the guard rejects only what it must. |
| 29 | `the_depth_limit_charges_nothing_to_what_follows` | Meeting the limit costs nothing afterwards: the levels a rejected or finished construct used are all given back, so what follows it is judged from the depth it is actually at. |
| 30 | `an_operand_gives_back_its_levels_to_the_chain_around_it` | A chain whose operands are chains of their own is charged for its depth, not its length. |
| 31 | `an_empty_token_slice_parses` | An empty token slice is a valid, empty parse rather than an out-of-bounds read. The Phase 5 fuzz targets can hand the parser one, so it may not assume the lexer's trailing `Eof`. |
| 32 | `every_program_is_in_the_coverage_matrix` | Every program in the corpus has a row in the coverage matrix. |
| 33 | `the_corpus_has_programs_in_it` | The corpus is not empty, so the checks over it are not passing by having nothing to check. |
| 34 | `every_program_parses` | Every program in the corpus parses, which is the exit criterion `--dump-ast` is held to. |
| 35 | `the_dump_is_deterministic_across_runs` | Dumping the same program twice produces the same bytes. |
| 36 | `arithmetic_program_tree` | The whole tree for the arithmetic corpus program: every operator and how they group. |
| 37 | `control_flow_program_tree` | The whole tree for the control-flow corpus program: every branch and loop form. |
| 38 | `functions_program_tree` | The whole tree for the functions corpus program: declarations, definitions, and calls. |
| 39 | `arrays_program_tree` | The whole tree for the arrays corpus program: declaration, indexing, and passing. |
| 40 | `strings_program_tree` | The whole tree for the strings corpus program: literals, `char` arrays, and escapes. |
| 41 | `spans_are_recorded_for_every_node` | A dump with spans on, for one small program, so the positions the parser records are pinned somewhere rather than only being asserted to exist. |
| 42 | `a_stream_without_its_eof_parses_as_if_it_had_one` | A token stream that does not end in `Eof` parses exactly as the same stream with it. |
| 43 | `token_level_truncations_do_not_panic` | Every prefix of every corpus program, cut at a token boundary, parses without panicking. |
| 44 | `byte_level_truncations_do_not_panic` | Every prefix cut at an arbitrary byte parses too, which covers what a token-boundary cut cannot produce: half a literal, half a comment, half an operator. |
| 45 | `a_truncated_program_is_reported` | Cutting a program short is never silently fine: a prefix that stops mid-construct is reported. |
| 46 | `adversarial_inputs_do_not_panic` | Every hand-written awkward input parses without panicking or hanging. |
| 47 | `inputs_with_no_tokens_are_empty_programs` | The inputs with nothing in them parse to an empty program and report nothing. |
| 48 | `deep_nesting_reports_the_depth_limit` | The deeply nested inputs meet the depth limit and say so, rather than exhausting the stack. |
| 49 | `a_file_of_operators_reports_a_bounded_number_of_times` | A file of nothing but operators reports a bounded number of times rather than once per token. |
| 50 | `very_long_tokens_are_carried_through` | A very long name and a very long literal are carried through rather than truncated or refused. |
| 51 | `the_whole_front_end_survives_the_awkward_inputs` | Every awkward input goes through the whole front end, not only the parser, without panicking. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that unit's test run, together with when the source and
its tests were written, is checked in beside this report as
[`evidence_parser.md`](evidence_parser.md). It is regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than
trusted.
