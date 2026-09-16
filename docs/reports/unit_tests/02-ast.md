# Unit Test Report — AST

## Unit

Source under test: `src/ast.rs`

This unit defines the abstract syntax tree: the node types the parser builds (`Program`, `Item`,
`FuncDef`/`FuncDecl`, `Param`, `VarDecl`, `Block`, `Stmt`/`StmtKind`, `Expr`/`ExprKind`, and their
supporting types `TypeSpec`, `Name`, `BaseType`, `BinOp`, `UnOp`, `IncDec`, `NodeId`/`NodeIds`), and
the deterministic S-expression dumper (`dump`, `dump_expression`, `nodes`) that renders a tree to
text. Per ADR 0004, the tree is immutable/plain-data — later passes annotate a side table keyed by
`NodeId` rather than mutating nodes — so this unit has no dependency on the parser, lexer, or any
later pass; its tests build trees by hand.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: white-box unit testing with structural (golden-string) assertions, exhaustiveness
checks enforced by the compiler itself, and an invariant/property test for interior mutability.**

The central design fact driving this unit's test strategy is stated directly in the module's own
doc comment: asserting that an expression *evaluates* to the right answer (e.g. `1+2*3 == 7`) is
not a valid way to test a tree shape, because a bug in operator precedence can produce a tree that
still happens to evaluate correctly. Every test here therefore asserts on the **shape** of the
dumped S-expression rather than on any evaluated result — the AST has no evaluator at this phase, so
there is nothing to evaluate anyway, but the principle is why the dump format exists and why parser
tests (unit #6) lean on it so heavily.

1. **Golden-string (example-based) assertions on the dump output.** `a_hand_built_tree_dumps` and
   `shown_spans_annotate_only_the_nodes_that_have_them` build a small tree by hand (a function
   `int twice(int n) { return n * 2; }`, and a global variable with an initializer) and assert the
   *exact* multi-line S-expression string produced, character for character. This is the strongest
   assertion available for a text-rendering function: it can't be satisfied by an accidentally-close
   output, unlike a substring or "contains" check.

2. **Equivalence partitioning over node/expression forms**, run twice: once for `literals_dump_as_
   they_were_written` (every literal kind: negative int, printable char, escaped char, escaped
   string, empty string), and once — exhaustively — for `every_form_dumps_to_a_distinct_line`, which
   constructs one instance of *every* `StmtKind` and *every* `ExprKind` variant and asserts all of
   their dump head-lines are pairwise distinct. This is the test that would catch a copy-paste bug
   where two variants render to the same head text (which would be invisible in any single-variant
   test but would make dumps ambiguous in practice).

3. **Compiler-enforced exhaustiveness as a testing technique.** `every_operator_variant_is_listed`
   matches every operator in `BinOp::ALL`/`UnOp::ALL`/`IncDec::ALL`/`BaseType::ALL` against an
   exhaustive `match` with no wildcard arm. This test does not assert anything at runtime — its
   entire value is that it **fails to compile** if a new enum variant is ever added without also
   being added to the corresponding `ALL` constant. This is a deliberate methodology choice: for a
   fixed, closed set of AST node kinds, "does every variant appear somewhere" is a fact the Rust
   compiler itself can check exhaustively, which is strictly stronger than any number of runtime
   test cases could offer, and it costs one test function to wire up. The dumper's own `match`
   expressions over `StmtKind`/`ExprKind` (with no wildcard arm) provide the identical guarantee for
   the dump function itself — a variant added without a corresponding dump line is a compile error,
   not a latent bug, which is why `every_form_dumps_to_a_distinct_line` only needs to check
   *distinctness*, not *presence*.

4. **Property-style test for a structural invariant**: `the_ast_has_no_interior_mutability` asserts
   `Program`, `Stmt`, and `Expr` are all `Sync` via a generic helper `require_sync::<T>()`. Since
   `Cell`/`RefCell` are `!Sync`, this is a compile-time check (not a runtime assertion) that no node
   type has quietly grown an interior-mutable field — directly enforcing the ADR 0004 invariant that
   the tree stays plain data. This is included because it is exactly the kind of defect code review
   alone tends to miss (a `RefCell<Option<Ty>>` "just for now" is an easy, well-intentioned
   regression) and that a type-system check catches for free, forever, in every future change.

5. **Determinism testing**: `the_dump_is_deterministic` dumps the same tree twice (with spans shown
   and hidden) and asserts byte-for-byte equality. This exists because the dump function's stated
   purpose is to serve as a snapshot baseline for the parser's own tests — if the dumper were
   non-deterministic (e.g. iterated a `HashMap` internally), every downstream snapshot test would be
   flaky, so determinism is tested here at the source rather than trusted.

6. **Identity/uniqueness testing**: `ids_are_handed_out_in_order` and
   `node_ids_are_unique_across_a_program` test `NodeIds` directly (monotonic counter) and then via a
   full tree walk (`nodes()`), asserting the walk finds exactly the expected count (6, with the
   reasoning for that count — which nodes get ids and which don't — spelled out in the test's own
   comment) and that all ids in that walk are pairwise distinct via a `HashSet`. This is the
   correctness precondition Phase 3 (semantic analysis) will depend on completely, since it plans to
   key its type/scope annotations by `NodeId` — a collision here would silently cross-contaminate two
   unrelated nodes' analysis results later, which is exactly the kind of bug that is easy to
   introduce (e.g. forgetting to advance the counter in a new node constructor) and hard to notice
   without a dedicated test.

**Coverage assessment.** Every `StmtKind` and `ExprKind` variant is constructed and dumped by at
least one test; `every_form_dumps_to_a_distinct_line` alone touches all 4 statement forms (in
addition to the ones covered by `a_hand_built_tree_dumps` and `shown_spans...`) and all 10
expression forms. All three `TypeSpec` shapes (scalar, sized array, unsized array) are covered by
`types_spell_themselves` and `only_arrays_are_arrays`. All 13 `BinOp`, 5 `UnOp`, and 2 `IncDec`
variants are exercised via their `ALL` constants in `operators_spell_themselves_distinctly` (no
duplicate/empty spellings) and `every_operator_variant_is_listed` (exhaustiveness). The one
deliberate scope boundary: this unit does not test span *values* being correct for parsed code
(spans here are all the placeholder `ANYWHERE` constant) — verifying that the parser assigns
correct, non-overlapping spans to real source is covered by the Parser units' own tests
(`every_span_points_into_the_source`, `spans_are_recorded_for_every_node`), which is the right owner
since span computation is parser logic, not AST logic.

## Automated Test Code

The tests live in `src/ast/tests.rs`, the module's own test file, and use a hand-rolled `Builder`
helper that assigns node identities the way the parser would, rather than the real parser — so this
unit is tested independently of the one that builds its values.

The table below is generated from the tests themselves, out of the doc comment each one carries, so
it cannot fall out of step with them. `scripts/test_inventory.py --check` fails the documentation
build if it has.

<!-- inventory: src/ast/tests.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `ids_are_handed_out_in_order` | Ids are handed out in order and never repeat. |
| 2 | `node_ids_are_unique_across_a_program` | Walking a tree finds every node once. Phase 3 keys its annotations by these ids, so a collision would quietly give two nodes the same type. |
| 3 | `the_ast_has_no_interior_mutability` | The AST carries no interior mutability, which is what ADR 0004 forbids. |
| 4 | `the_dump_is_deterministic` | The same tree dumps to the same bytes every time, which is what a snapshot depends on. |
| 5 | `a_hand_built_tree_dumps` | A hand-built tree round-trips through the dumper, indented one level per depth. |
| 6 | `the_dump_ends_with_a_newline` | A dump is a file's worth of output, so it ends with a newline. |
| 7 | `shown_spans_annotate_only_the_nodes_that_have_them` | Showing spans annotates each node that has one, and leaves the grouping lines bare. |
| 8 | `an_expression_dumps_on_its_own` | One expression dumps on its own, without a program built around it to hold it. |
| 9 | `every_form_dumps_to_a_distinct_line` | Every statement and expression form has a line of its own in the dump, carrying its own id and span, and no two forms share one. |
| 10 | `literals_dump_as_they_were_written` | A literal dumps in the form it was written, escapes and all, through the same table the lexer decoded it with. |
| 11 | `an_item_reports_its_own_identity` | An item reports the identity and extent of whichever form it is. |
| 12 | `types_spell_themselves` | A type spells itself the way it was declared. |
| 13 | `only_arrays_are_arrays` | The three type shapes are distinguishable, and only an array claims to be one. |
| 14 | `operators_spell_themselves_distinctly` | Every operator spells itself as source text, and no two share a spelling within a set. |
| 15 | `every_operator_variant_is_listed` | The `ALL` lists are complete: a variant added without being listed fails to compile here. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
