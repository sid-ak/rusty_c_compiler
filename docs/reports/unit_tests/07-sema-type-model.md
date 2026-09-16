# Unit Test Report — Semantic Analysis: The Type Model

## Unit

Source under test: `src/sema/types.rs`.

This unit is the compiler's model of what a value *is*. It knows four things, and nothing else in
the compiler is allowed to know them independently:

- How big a value is, and how it has to be aligned in memory. An `int` is four bytes, a `char` is
  one, an array is its element size times its length.
- Promotion. C says a `char` behaves as an `int` the moment it takes part in arithmetic, a
  comparison, or a logical operator. Storage is one byte; computation is 32-bit.
- Decay. An array becomes the address of its first element — but in this subset that happens at
  exactly one place, the argument position of a call, and nowhere else.
- Compatibility. Whether a value of one type may be assigned to, returned as, or passed as
  another.

It is deliberately the bottom of the dependency graph: it does not know about scopes, about the
syntax tree, or about diagnostics. Everything above it asks it questions and reports the answers.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

Approach: exhaustive matrix testing over every pair of types, with the axis of the matrix built
from a list the compiler itself refuses to let go stale.

The rules this unit implements are not single cases, they are *tables*. "Is this assignable to that"
has an answer for every pair of types, and the interesting answers are the ones nobody would think
to write a test for — assigning an array to an `int`, returning `void` from a function that promised
a `char`. Sampling such a table is how a wrong cell survives.

So most tests here enumerate the whole matrix rather than picking from it, and the interesting
design is in how the axis is kept honest:

1. The axis is a list of one value of every type the compiler has — and a separate test matches
   that list against an exhaustive `match` with no catch-all arm. Adding a new type without adding
   it to the list is then a *compile error*, not a silently smaller matrix. This matters more than
   it sounds: an omitted row does not fail any matrix test, it simply stops being checked, and
   nothing about the test output would look different.

2. Each rule is checked in both directions. A compatibility rule tested only where it says "yes"
   passes just as well if it says yes to everything. Every matrix test asserts the cells that must
   be false as firmly as the ones that must be true.

3. Properties, not only examples, where the rule is a property. Promotion is idempotent —
   promoting an already-promoted type changes nothing — and that is asserted over the whole axis
   rather than for one type, because it is the kind of rule an implementation gets right for the
   case it was written against and wrong for the one it was not.

4. Public entry points are tested with inputs the rest of the compiler never produces. This unit
   is reachable from anywhere in the crate, so a function that would be correct for every input the
   analyzer actually sends it is still a latent bug. Deeply nested and malformed type values are
   passed in deliberately.

### Why this test methodology?

This is pure, total, side-effect-free logic over a small closed set of values. That combination is
rare and worth exploiting: with a handful of types, the full cross product is small enough to
enumerate exactly, which makes "we tested every case" a literal statement rather than an aspiration.
Anywhere the input space is genuinely enumerable, enumerating it beats choosing representatives,
because choosing representatives is where the assumption that the untested cells resemble the tested
ones hides.

## Test Coverage

Every pair of types is covered for compatibility, and every type is covered for size, alignment,
promotion, and decay. The boundary cases are the ones where two types are *nearly* the same:
`int` against `char` (compatible, with a conversion), an array of `int` against an array of `char`
(not), a pointer to `int` against an array of `int` (compatible only across the one boundary where
decay happens).

The error type — the value the analyzer substitutes when it has already reported a problem — is in
the matrix too, and is required to be compatible with everything. That is not a rule about C, it is
a rule about not cascading: one mistake in a program should produce one message, and a type that
made every later rule fail would produce a page of them.

## Automated Test Code

<!-- inventory: src/sema/types/tests.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `sample_types_cover_every_variant` | Every `Ty` variant is represented in the sample list the matrix tests run over. |
| 2 | `scalar_layouts_match_the_arm64_c_abi` | `char` occupies one byte and `int` four, each aligned to its own size. |
| 3 | `pointer_layout_is_eight_bytes_regardless_of_pointee` | A pointer is eight bytes on ARM64 whatever it points at. |
| 4 | `array_layout_is_element_size_times_length` | An array is its length times its element size, aligned like its element. |
| 5 | `nested_array_layout_multiplies_the_dimensions` | A nested array multiplies its dimensions and keeps the innermost element's alignment. |
| 6 | `huge_array_layout_does_not_wrap` | An array long enough to overflow 32-bit arithmetic still reports a size rather than wrapping. |
| 7 | `unmeasurable_array_layout_reports_none_rather_than_overflowing` | An array too large to measure reports no layout rather than overflowing. |
| 8 | `incomplete_types_have_no_layout` | `void` and function types have no storage, so they have no layout. |
| 9 | `promotion_widens_char_and_nothing_else` | Promotion widens `char` to `int` and leaves every other type alone. |
| 10 | `promotes_predicate_agrees_with_the_promotion_rule` | `promotes` agrees with `promoted` on every type, so the predicate cannot drift from the rule. |
| 11 | `decay_rewrites_arrays_to_pointers_and_nothing_else` | Decay turns an array into a pointer to its element and leaves every other type alone. |
| 12 | `decays_predicate_agrees_with_the_decay_rule` | `decays` agrees with `decayed` on every type, so the predicate cannot drift from the rule. |
| 13 | `arithmetic_and_scalar_classify_every_type` | Only `int` and `char` are arithmetic; only they plus pointers are scalar. |
| 14 | `common_arithmetic_type_is_int_for_arithmetic_pairs_only` | Two arithmetic operands share `int`; any other pairing has no common type. |
| 15 | `assignability_matrix_matches_the_documented_rules` | Every cell of the assignment-compatibility matrix matches the documented rules, both directions. |
| 16 | `int_and_char_interconvert_with_truncation_on_the_narrowing_side` | `int` and `char` convert to each other, widening one way and truncating the other. |
| 17 | `pointers_accept_only_a_matching_pointee` | A pointer accepts only a pointer to the same element type. |
| 18 | `arrays_functions_and_void_are_never_assignable_targets` | Arrays, functions, and `void` are never assignable to, whatever the source type is. |
| 19 | `an_array_is_assignable_to_a_pointer_to_its_element` | An array is assignable to a matching pointer, which is the parameter-passing rule and only that. |
| 20 | `types_print_as_c_spells_them` | Types print the way C spells them, so a diagnostic can quote one directly. |
| 21 | `nested_array_prints_its_dimensions_in_declaration_order` | A nested array prints its dimensions outermost first, the order C declares them in. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
