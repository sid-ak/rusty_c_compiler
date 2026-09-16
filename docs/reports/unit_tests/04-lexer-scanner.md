# Unit Test Report — Lexer: Scanner

## Unit

Source under test: `src/lexer/mod.rs` (the `Lexer` struct and its scanning methods, `lex()`, and
`dump()`), plus `tests/lexer_snapshots.rs` (one integration-level snapshot test). This unit depends
on the Token Model unit (`src/lexer/token.rs`, reported separately) for the vocabulary it produces,
and on the Diagnostics unit for how it reports problems, but owns all of the scanning logic itself:
maximal-munch tokenization, comment/whitespace skipping, literal decoding, numeric-base decoding,
and error recovery.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: black-box, input/output example-based unit testing organized by equivalence
partitioning over the grammar's lexical categories, layered with explicit boundary-value analysis
on malformed input, a golden-master (`insta`) snapshot for whole-stream composition, and
mutation-adjacent robustness testing (arbitrary/adversarial byte streams) for the module's two
declared correctness properties: termination and always-reaching-`Eof`.**

The module's own doc comment states two invariants that must hold for *every* input, valid or not
— "it terminates" and "it reaches the end" (always emits `Eof`, even after an error). These two
properties, not just "correct tokens for valid input," are the actual object under test, and the
methodology is built specifically to attack them:

1. **Equivalence partitioning over lexical categories**, one test group per category the grammar
   defines: keywords (`keywords_lex_as_keywords`), identifiers
   (`identifiers_lex_as_identifiers`, plus the keyword/identifier boundary via
   `keyword_prefixes_lex_as_identifiers`), integer literals across all three supported bases
   (`integer_literals_decode_by_base` — decimal, hex upper/lower-case `0x`/`0X`, octal, and the
   `i32::MAX` boundary), character literals including every escape
   (`character_literals_decode_to_one_byte`, `every_escape_decodes` — iterates the full `ESCAPES`
   table so a table entry can't silently go untested), string literals
   (`string_literals_store_decoded_bytes`, `quotes_nest_inside_the_other_literal_form`), and every
   operator/punctuator (`operators_and_punctuators_lex_as_themselves` — all 24 single/paired-token
   symbols in one parametrized test).

2. **Boundary-value analysis specifically at maximal-munch decision points.**
   `maximal_munch_splits_operators_the_documented_way` targets exactly the ambiguous byte sequences
   a greedy scanner can get wrong: `a<=b` (must not split as `<` `=`), `a<-b` (must split, since
   `<-` is not an operator), `a+++b`/`a---b` (the classic C "maximal munch" trap: must lex as
   `++` `+`, not `+` `++`), and `a= =b` (a space *must* prevent munching `==`). This is the single
   highest-value test in the unit, since maximal-munch bugs are exactly the kind that pass every
   "does `x==y` lex right" test while still being wrong on adjacent-operator inputs nobody thought
   to write down separately.

3. **Boundary-value analysis over comment/trivia handling**: comment at EOF with no trailing
   newline (`line_comment_at_eof_without_a_newline`), non-nesting block comments
   (`block_comments_do_not_nest` — `/* outer /* inner */` must close at the *first* `*/`, matching C
   semantics exactly), and the lone-`/`-is-division case (`a_lone_slash_is_division`) — the
   negative-space companion to comment recognition.

4. **Exhaustive malformed-input table** (`each_malformed_construct_has_its_own_diagnostic`) — 17
   distinct malformed inputs in one parametrized test, each asserting the exact message *and* that
   the diagnostic's span covers the **whole** offending construct (not just the triggering byte),
   via the shared `assert_one_error` helper. This single test is effectively a full equivalence
   partition of the "what can go wrong while scanning" space: unterminated string (both at EOF and
   at a bare newline), unterminated/empty/overlong character literal, unknown escape (in both
   literal forms), unterminated block comment, missing hex digits, invalid digit in each of the
   three bases, integer overflow (both just-over and wildly-over `i32::MAX`), and three different
   stray-character bytes.

5. **Robustness / mutation-adjacent testing against the no-panic and always-terminates
   invariants**, which is where this unit goes beyond ordinary example-based testing:
   `arbitrary_bytes_terminate` runs all 256 byte values cycled to 4096 bytes through the scanner and
   asserts it still reaches `Eof`; `invalid_utf8_is_a_diagnostic_not_a_panic` specifically targets
   the fact that this scanner reads `&[u8]`, not `&str`, by feeding it a raw `0xff` byte (invalid as
   a UTF-8 lead byte) and asserting a diagnostic, not a panic, results; `a_file_of_stray_characters_
   terminates` and `repeated_errors_still_terminate` chain many consecutive errors (8 stray bytes; 7
   alternating unterminated char/string literals) and assert the scan still completes rather than
   looping — this is the direct test of the "every step consumes at least one byte" termination
   argument stated in the module doc comment, expressed as a test rather than only as a proof in
   prose. `a_trailing_backslash_does_not_overrun` targets the specific off-by-one risk of an escape
   sequence's lookahead reading past the end of the buffer when the backslash is the very last byte
   of the file — a classic scanner buffer-overrun bug class, tested directly rather than trusted to
   the type system.

6. **Golden-master (snapshot) testing** for whole-stream composition:
   `every_token_reports_its_line_and_column` pins the exact multi-line dump of a small multi-line,
   tab-indented program (asserting line/column *and* token kind together, so a regression in either
   is caught), and `tests/lexer_snapshots.rs`'s `representative_program_token_stream` runs a larger
   program touching every lexable construct in the subset through `insta::assert_snapshot!` against
   a checked-in `.snap` file — this is the test that proves individual-category correctness
   *composes*, i.e. that scanning one program with everything in it produces the same result as the
   sum of the individually-tested categories, which is not guaranteed by the per-category tests
   alone (a shared-state bug between categories, e.g. an off-by-one that only appears after a
   preceding token of a specific kind, would only show up here).

7. **Diagnostic-ordering and multiplicity testing**: `several_errors_are_reported_in_source_order`
   (4 distinct errors, asserts strictly increasing span offsets) and
   `a_single_error_does_not_cascade` (4 malformed inputs, each asserted to produce *exactly one*
   diagnostic, not a cascade of follow-on errors) — the latter is a form of negative testing
   specifically valuable in recursive-descent-adjacent recovery logic, where an unhandled error path
   commonly produces a flood of spurious follow-on diagnostics.

**Coverage assessment.** Every scanning method in `Lexer` is reached: `scan_word`, `scan_number`
(all three radixes, all three error paths — missing digits, invalid digit, overflow),
`scan_char_literal` and `scan_string_literal` (all error paths: unterminated, empty, over-length,
bad escape), `skip_line_comment`/`skip_block_comment` (including the unterminated case),
`one_or_two`/`paired_only` (every doubled and non-doubled operator), and
`unsupported_punctuation` (every one of the 5 punctuation bytes this subset's grammar has no token
for at all: `# ? : ^ ~`, each with its own explanatory note tested in
`unsupported_punctuation_says_what_to_do_instead`). The two stated invariants (termination,
always-`Eof`) are tested directly rather than only implied by passing example tests.

The `arbitrary_bytes_terminate` test here is a deterministic version of the same property over a
fixed input. The generated version of it is `fuzz/fuzz_targets/lex.rs`, which mutates its input for
as long as it is allowed to run and asserts the same two invariants plus a third — that every span
points inside the input — and which is reported in
[14 — The Differential Harness](14-differential-harness.md) alongside the rest of the machinery that
is test code rather than compiler code. The two are complementary: the unit test runs in
milliseconds on every change, and the fuzz target runs for fifteen minutes when someone asks it
to.

## Automated Test Code

The tests live in `src/lexer/tests.rs`, the scanner module's own test file, plus one whole-stream
snapshot in `tests/lexer_snapshots.rs`. The table is generated from the tests themselves, so it
cannot fall out of step with them; `scripts/test_inventory.py --check` fails the documentation build
if it has.

<!-- inventory: src/lexer/tests.rs, tests/lexer_snapshots.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `keywords_lex_as_keywords` | Every keyword lexes as itself. |
| 2 | `keyword_prefixes_lex_as_identifiers` | A keyword with anything attached is one identifier, not a keyword plus leftovers. |
| 3 | `identifiers_lex_as_identifiers` | Identifiers may start with a letter or underscore and continue with digits. |
| 4 | `integer_literals_decode_by_base` | Decimal, hex, and octal literals decode to the same value the base implies. |
| 5 | `character_literals_decode_to_one_byte` | A character literal decodes to the one byte it denotes, escape or not. |
| 6 | `every_escape_decodes` | Every escape in the table decodes inside a character literal. |
| 7 | `string_literals_store_decoded_bytes` | A string literal stores decoded bytes, so nothing downstream re-parses escapes. |
| 8 | `quotes_nest_inside_the_other_literal_form` | A string literal may contain an unescaped single quote, and vice versa. |
| 9 | `operators_and_punctuators_lex_as_themselves` | Every operator and punctuator lexes as its own token. |
| 10 | `maximal_munch_splits_operators_the_documented_way` | Multi-character operators win over their prefixes, and a longer run splits greedily. |
| 11 | `empty_input_is_only_eof` | An empty input is just the end of the input. |
| 12 | `whitespace_is_not_a_token` | Whitespace separates tokens without becoming one. |
| 13 | `comments_are_skipped_wherever_they_appear` | Both comment forms are skipped, including between a token and its operator. |
| 14 | `line_comment_at_eof_without_a_newline` | A comment running to the end of a file with no trailing newline still ends cleanly. |
| 15 | `block_comments_do_not_nest` | A block comment does not nest: the first `*/` closes it. |
| 16 | `a_lone_slash_is_division` | A `/` that does not begin a comment is division. |
| 17 | `every_token_reports_its_line_and_column` | The dump of a multi-line fixture, which pins the line and column of every token. |
| 18 | `dump_column_widens_for_large_line_numbers` | The position column widens with the file, so a dump of a long program still lines up. |
| 19 | `a_tab_advances_the_column_by_one` | A tab advances the column by one, so the token after it starts one column further on. |
| 20 | `crlf_lexes_the_same_as_lf` | CRLF input produces the same tokens, and the same line numbers, as LF input. |
| 21 | `invalid_utf8_is_a_diagnostic_not_a_panic` | Invalid UTF-8 is a diagnostic, not a panic, and the scan still reaches the end. |
| 22 | `arbitrary_bytes_terminate` | A file of arbitrary bytes still terminates and still ends in `Eof`. |
| 23 | `each_malformed_construct_has_its_own_diagnostic` | Every malformed-input path reports its own diagnostic, spanning the whole construct. |
| 24 | `unsupported_punctuation_is_named_not_called_stray` | Punctuation of real C that this grammar has no token for is named as unsupported rather than as a stray byte, in the same words the parser uses for the constructs it catches. |
| 25 | `unsupported_punctuation_says_what_to_do_instead` | Each unsupported character says what to reach for instead. |
| 26 | `integer_overflow_explains_the_limit` | An over-large literal says what the limit is and how to write `INT_MIN` within it. |
| 27 | `a_single_error_does_not_cascade` | One bad construct produces one diagnostic, not a cascade. |
| 28 | `scanning_resumes_on_the_line_after_an_unterminated_literal` | An unterminated literal resynchronizes at the end of its line, so the next line still lexes. |
| 29 | `an_unterminated_block_comment_runs_to_the_end_of_file` | An unterminated block comment resynchronizes at end of file, swallowing the rest. |
| 30 | `a_directive_is_one_diagnostic_spanning_the_directive` | A preprocessor directive is one unsupported construct: one diagnostic whose span covers the directive, not a report for the `#` followed by more for the rest of the line. |
| 31 | `scanning_resumes_on_the_line_after_a_directive` | Scanning resumes on the line after a directive, so the code that follows still lexes. |
| 32 | `a_directive_continues_across_spliced_lines` | A backslash before the newline splices the next line onto the directive (C11 5.1.1.2, phase 2), so a multi-line macro is still one directive and its body is not lexed as code. |
| 33 | `a_directive_may_be_indented_or_follow_a_comment` | A directive may follow whitespace or a comment on its own line: what matters is that no token precedes the `#` on that line (C11 6.10p2). |
| 34 | `a_hash_after_a_token_on_its_line_is_a_single_character` | A `#` after a token on the same line is not a directive, so it is the single unsupported character and the rest of the line lexes as usual. |
| 35 | `a_newline_inside_a_comment_does_not_start_a_directive` | A newline inside a block comment does not start a new line for a directive, because the comment is replaced by one space before directives are recognized (C11 5.1.1.2, phase 3). |
| 36 | `a_directive_at_the_end_of_file_terminates` | A directive ending at the end of file, including one whose last line is spliced into nothing, still ends the stream in `Eof`. |
| 37 | `several_errors_are_reported_in_source_order` | A file with several mistakes reports all of them, in source order. |
| 38 | `a_file_of_stray_characters_terminates` | A file of nothing but stray characters terminates, reporting one error per character. |
| 39 | `repeated_errors_still_terminate` | An error path cannot loop: repeated unterminated constructs still reach the end. |
| 40 | `a_trailing_backslash_does_not_overrun` | A backslash at the very end of a file is an unterminated literal, not an overrun. |
| 41 | `spans_slice_back_to_the_source_text` | Spans are byte ranges over the original source, so slicing one back out gives the token. |
| 42 | `representative_program_token_stream` | The whole token stream, with each token's start and end position, for the program above. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
