# Unit Test Report — Semantic Analysis: The Scope Stack

## Unit

Source under test: `src/sema/scope.rs`.

This unit answers one question: when a name appears in a program, which declaration does it refer
to? C answers it by nesting — the innermost declaration of that name wins, and when a block ends,
everything declared inside it stops existing. A stack of scopes is the structure that implements
that directly.

It also hands out the storage slots each function will need. Every local variable and every
temporary value gets a numbered place in the function's stack frame, and this unit assigns those
numbers, because it is the part of the compiler that knows what is alive at the same time as what.

Its tests pin the *structure*, not the messages. That using a `for`-loop variable after the loop
produces an "undeclared identifier" error is a test of the analyzer; that the binding has gone out
of scope by then is a test of this stack, and that is the one here.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

Approach: state-transition testing over the stack's operations, with shadowing and redeclaration
treated as the two boundaries that matter.

A scope stack is a small state machine: push a scope, declare names into it, look names up, pop the
scope. Almost every defect in one is a transition that does not restore what it should, so the tests
are organized around transitions rather than around single operations:

1. Nesting and unnesting, checked by what survives. A name declared in an inner scope must be
   invisible after that scope is popped, and a name declared in an outer scope must still resolve to
   the same declaration it did before the inner scope was pushed. The second half is the one that
   catches a stack that pops too much.

2. Shadowing, in both directions. An inner declaration of an existing name must hide the outer
   one for exactly as long as the inner scope lasts — and the outer one must come back unchanged,
   not merely come back. Tests assert *which* declaration was found, not only that one was, which is
   why the fixtures give each declaration a distinguishable position.

3. Redeclaration as the negative space. Declaring the same name twice in one scope must be
   refused; declaring it again in a nested scope must not be. These are one line apart in the
   implementation and opposite in meaning.

4. Slot numbering as an invariant, not an example. Every slot handed out must be distinct, and
   the count must match what the function actually needs. A test that checked only that slots exist
   would pass against an implementation that handed the same slot to two variables — which is a bug
   that produces a program that compiles, links, runs, and gives the wrong answer.

### Why this test methodology?

The failure this unit can cause is uniquely hard to find downstream. If a lookup resolves to the
wrong declaration, nothing crashes and nothing is reported: the compiler goes on to generate correct
code for the wrong variable. There is no later stage that can notice, because every later stage
trusts this one's answer by construction. That makes the case for testing it exhaustively at the
structural level rather than relying on end-to-end programs to reveal it — an end-to-end program
only reveals it if someone happened to write one where the two variables hold different values.

## Test Coverage

Covered: nesting to several levels, shadowing and unshadowing, redeclaration in the same scope and
in a nested one, lookup of a name that does not exist, the file scope that holds globals and
functions together, and slot assignment across all of those.

Deliberately not covered here: what the analyzer *says* when a lookup fails. That is a diagnostic
with a message and a position, and it is tested where messages are tested.

## Automated Test Code

<!-- inventory: src/sema/scope/tests.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `a_fresh_stack_is_at_file_scope` | A fresh stack is at file scope, which is depth zero. |
| 2 | `entering_and_leaving_blocks_tracks_depth` | Entering and leaving blocks moves the depth up and back down. |
| 3 | `leaving_file_scope_is_refused_rather_than_panicking` | Leaving file scope is refused rather than underflowing the stack. |
| 4 | `an_inner_declaration_shadows_an_outer_one_until_the_block_ends` | A name declared in an inner block hides the outer one, which returns when the block ends. |
| 5 | `a_global_is_shadowable_by_a_local` | A global is shadowable by a local, and the global is what resolves outside the function. |
| 6 | `a_body_local_cannot_redeclare_a_parameter` | A parameter shares the function body's scope, so a body local cannot redeclare it. |
| 7 | `a_nested_block_local_may_shadow_a_parameter` | A local in a nested block may shadow a parameter, and the parameter returns afterwards. |
| 8 | `same_scope_redeclaration_reports_the_original_declaration` | Redeclaring a name in one scope fails and hands back the declaration already there. |
| 9 | `a_failed_redeclaration_leaves_the_original_binding_intact` | A failed redeclaration leaves the original binding in place rather than half-replacing it. |
| 10 | `sibling_blocks_may_each_declare_the_same_name` | The same name in two sibling blocks is two separate declarations, not a redeclaration. |
| 11 | `a_for_init_variable_is_visible_in_the_body_and_not_after_the_loop` | A `for`-init variable is visible in the loop body and gone once the loop ends. |
| 12 | `a_local_shadows_a_function_of_the_same_name` | A local shadows a function of the same name, so the name stops being callable in that scope. |
| 13 | `an_undeclared_name_resolves_to_nothing` | Lookup finds nothing for a name that was never declared. |
| 14 | `slots_are_handed_to_locals_and_parameters_only` | Locals and parameters get consecutive slot ids; globals and functions get none. |
| 15 | `slot_numbering_restarts_for_each_function` | Slot numbering restarts at each function, since slots index that function's frame. |
| 16 | `slots_keep_counting_across_blocks_within_a_function` | Slots keep counting up across the blocks of one function, since one frame holds them all. |
| 17 | `a_symbol_outlives_the_scope_it_was_declared_in` | A symbol that went out of scope is still readable by id, which is what the annotations rely on. |
| 18 | `an_unknown_symbol_id_resolves_to_nothing` | An id from one stack does not name a symbol in another. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
