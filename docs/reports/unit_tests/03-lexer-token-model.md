# Unit Test Report — Lexer: Token Model

## Unit

Source under test: `src/lexer/token.rs`

This unit defines the token vocabulary shared between the lexer and the parser: `TokenKind` (every
kind of token the grammar admits), `Keyword` (the ten reserved words this subset implements, built
via the `keywords!` macro that derives both directions of the keyword↔spelling mapping from one
list), `UNSUPPORTED_KEYWORDS` (the 22 C89 keywords this subset deliberately omits, tracked so they
can be reported as "unsupported" rather than as a generic parse error), the `ESCAPES` table (the 11
escape sequences this subset decodes), and `Token`/`spell_literal` (pairing a `TokenKind` with a
`Span`, and rendering a decoded literal's bytes back into source-like text for diagnostics and the
AST dump). This unit does not scan source text itself — that is the Scanner unit
(`reports/unit_tests/04-lexer-scanner.md`) — it only defines the vocabulary the scanner produces
tokens *in*.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: white-box unit testing with two enforcement mechanisms doing most of the coverage
work — compiler-checked exhaustiveness, and round-trip/inverse-function testing — supplemented by
equivalence partitioning over rendering.**

This unit's core property is that it defines two tables (`Keyword` and `ESCAPES`) that must stay
consistent in *both directions*: text→value for scanning, and value→text for error messages and the
AST dump. A table that only tests one direction can silently drift (e.g. a scanner that decodes
`\v` correctly but a renderer that spells it back as `\x0b`), so round-trip testing is the primary
methodology here, not incidental to it:

1. **Round-trip (inverse function) testing.** `keywords_round_trip_through_the_table` takes every
   `Keyword::ALL` variant, spells it, and asserts `from_identifier` recovers the original variant —
   proving encode and decode agree for all 10 keywords in one test rather than 10 separate ones.
   `escape_table_round_trips` does the same for all 11 `ESCAPES` entries in both directions
   (`escape_byte` and the private `escape_letter`). This is the strongest single technique available
   for a bidirectional table: it is equivalent to exhaustively partition-testing every entry, in one
   assertion per table.

2. **Compiler-enforced exhaustiveness**, the same technique used in the AST unit: `every_kind()`
   builds one sample of all 30 `TokenKind` variants and matches them in an exhaustive `match` with
   no wildcard, so a variant added to the enum without a sample added here is a compile error, not a
   silent test gap. `every_kind_sample_is_distinct` then checks (via `std::mem::discriminant` in a
   `HashSet`) that the 30 samples really are 30 distinct variants, guarding against the "sample list
   was updated by copy-pasting an existing line" failure mode a purely compile-time check can't
   catch.

3. **Equivalence partitioning over `Display`/rendering.** Token spelling naturally partitions into
   fixed-spelling kinds (operators, punctuators, keywords — spelling independent of any payload) and
   value-carrying kinds (`Ident`, `IntLit`, `CharLit`, `StrLit` — spelling depends on payload).
   `fixed_spellings_are_the_source_text` and `every_kind_has_a_non_empty_spelling` cover the first
   partition; `literals_spell_themselves_back` and `unprintable_bytes_render_as_hex` cover the
   second, specifically at the boundary between "printable, no escape needed", "has a named escape",
   and "has neither" (falls back to `\xHH`) — a three-way partition of `spell_literal`'s internal
   `if`/`else if`/`else`, each with its own representative case.

4. **Negative-space / boundary testing for keyword recognition.**
   `keywords_with_affixes_are_identifiers` is boundary-value analysis on the identifier/keyword
   boundary: for every keyword, six adjacent-but-different strings are tried (suffix, trailing
   underscore, trailing digit, leading letter, leading underscore, different case) and all must
   resolve as plain identifiers, not the keyword. This exists because keyword recognition in this
   design happens *after* scanning a whole identifier (documented explicitly in the module comment),
   and a regression to "keyword recognized by prefix match" would silently break real programs using
   names like `integer` or `returns` — exactly the bug class this test is aimed at.

5. **Set-completeness testing.** `the_two_keyword_lists_partition_c89` unions `Keyword::ALL` and
   `UNSUPPORTED_KEYWORDS`, sorts, and asserts the result equals the full, explicit 32-word C89
   keyword list. This is not redundant with `the_keyword_set_is_the_documented_one` (which pins only
   the 10 *implemented* keywords) — it specifically catches a keyword being missing from **both**
   lists, which would be invisible to any test of either list alone and would silently let a real C
   reserved word (e.g. `static`) be accepted as an ordinary identifier, contradicting real C
   semantics.

**Coverage assessment.** All 10 implemented keywords, all 22 unsupported keywords, all 11 escape
sequences, and all 30 `TokenKind` variants have direct test coverage. Every public function in the
module (`Keyword::spelling`, `Keyword::from_identifier`, `unsupported_keyword`, `escape_byte`,
`spell_literal`, `TokenKind::fixed_spelling`, the `Display` impls, `Token::new`) is exercised.
Boundary/negative cases are tested for the keyword/identifier boundary and for the
printable/escaped/hex-fallback boundary in literal spelling. What is *not* tested here is the
scanner's use of this vocabulary (e.g., does the scanner actually call `Keyword::from_identifier` at
the right point) — that is Scanner-unit territory and is covered there
(`keyword_prefixes_lex_as_identifiers` etc. in `04-lexer-scanner.md`), keeping this unit's tests
scoped strictly to the vocabulary's own internal consistency.

## Automated Test Code

The tests live in `src/lexer/token/tests.rs`, the module's own test file.

The table below is generated from the tests themselves, out of the doc comment each one carries, so
it cannot fall out of step with them. `scripts/test_inventory.py --check` fails the documentation
build if it has.

<!-- inventory: src/lexer/token/tests.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `every_kind_has_a_non_empty_spelling` | Every token kind renders as something a diagnostic can print. |
| 2 | `every_kind_sample_is_distinct` | No two samples are the same variant, so the coverage list above is not padded. |
| 3 | `fixed_spellings_are_the_source_text` | Operators and punctuators spell themselves; value-carrying kinds spell their value. |
| 4 | `spellings_read_as_source` | The parser's expected-versus-found wording reads as source text. |
| 5 | `keywords_round_trip_through_the_table` | Every keyword round-trips: its spelling is recognized back as itself. |
| 6 | `the_keyword_set_is_the_documented_one` | The subset's ten keywords are the ones the grammar names, and nothing else. |
| 7 | `keywords_with_affixes_are_identifiers` | A keyword with any prefix or suffix character is an identifier, not a keyword. |
| 8 | `the_two_keyword_lists_partition_c89` | The two keyword lists partition C89's 32 reserved words: nothing is in both, and nothing real C reserves is missing from either. A word in neither would be silently accepted as a variable name, which is how `int static;` would sneak through. |
| 9 | `unsupported_keywords_are_recognized_by_name` | A word the subset leaves out is recognized as such; one it implements, and one nobody reserves, are not. |
| 10 | `escape_table_round_trips` | The escape table reads the same in both directions, which is what keeps the lexer's decoding and the renderer's spelling from drifting apart. |
| 11 | `unknown_escape_letters_decode_to_nothing` | A letter outside the table is not an escape. |
| 12 | `literals_spell_themselves_back` | Literals spell back out in a form a C programmer would recognize. |
| 13 | `unprintable_bytes_render_as_hex` | A byte with no escape and no printable form still renders, rather than being dropped. |
| 14 | `spelling_a_literal_body_adds_no_quotes` | Spelling a literal body is quote-free, so both literal forms and the AST dump share it. |
| 15 | `token_pairs_a_kind_with_a_span` | A token pairs a kind with the span it was scanned from, and displays as its kind. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
