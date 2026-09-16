# Unit Test Report — Lexer: Scanner

## Unit

Source under test: `src/lexer/mod.rs` (the `Lexer` struct and its scanning methods, `lex()`, and
`dump()`), plus `tests/lexer_snapshots.rs` (one integration-level snapshot test). This unit depends
on the Token Model unit (`src/lexer/token.rs`, reported separately) for the vocabulary it produces,
and on the Diagnostics unit for how it reports problems, but owns all of the scanning logic itself:
maximal-munch tokenization, comment/whitespace skipping, literal decoding, numeric-base decoding,
and error recovery.

## Date

2026-08-23

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
always-`Eof`) are tested directly rather than only implied by passing example tests. The one
deliberate gap: true fuzz testing (`cargo-fuzz` against arbitrary byte streams over long,
minutes-long runs) is called out in `AGENTS.md`/`docs/dive-deep/testing.md` as a Phase 5
deliverable and is not yet wired up in this repo (confirmed absent: no `fuzz/` directory exists, and
`cargo fuzz` is not installed in the current environment — see
`reports/unit_tests/00-environment.md`); the `arbitrary_bytes_terminate` test here is a lightweight,
deterministic stand-in that exercises the same termination property over a fixed, unmemorized input
rather than program-generated adversarial input.

## Automated Test Code

35 tests total: 34 in `src/lexer/mod.rs` under `#[cfg(test)] mod tests`, plus 1 in
`tests/lexer_snapshots.rs`.

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 1 | `keywords_lex_as_keywords` | Each of `Keyword::ALL`'s spelling | `TokenKind::Keyword(that_keyword)` |
| 2 | `keyword_prefixes_lex_as_identifiers` | `"integer"`, `"if_"`, `"_int"`, `"returns"` | Each an `Ident` of that text |
| 3 | `identifiers_lex_as_identifiers` | `"x"`, `"_"`, `"_x9"`, `"camelCase"`, `"SHOUT"`, `"a1b2"` | `Ident` of same text |
| 4 | `integer_literals_decode_by_base` | `"0","7","42","0x0","0xff","0XFF","0755","010","2147483647"` | `0,7,42,0,255,255,493,8,i32::MAX` |
| 5 | `character_literals_decode_to_one_byte` | `'a'`, `' '`, `'\n'`, `'\0'`, `'\\'`, `'\''`, `'"'` | `CharLit(b'a')`, `(b' ')`, `(b'\n')`, `(0)`, `(b'\\')`, `(b'\'')`, `(b'"')` |
| 6 | `every_escape_decodes` | Each `(letter, byte)` in `ESCAPES` as `'\<letter>'` | `CharLit(byte)` |
| 7 | `string_literals_store_decoded_bytes` | `""`, `"hi"`, `"a\tb\0c"`, `"quote:\" done"` | Empty vec; `b"hi"`; `[a,\t,b,0,c]`; `b"quote:\" done"` |
| 8 | `quotes_nest_inside_the_other_literal_form` | `"it's"` | `StrLit(b"it's")` |
| 9 | `operators_and_punctuators_lex_as_themselves` | All 24 fixed operator/punctuator spellings | Matching `TokenKind` each |
| 10 | `maximal_munch_splits_operators_the_documented_way` | `a<=b`, `a<-b`, `a++ +b`, `a+++b`, `a---b`, `a==b`, `a= =b` | Documented splits (see methodology) |
| 11 | `empty_input_is_only_eof` | `""` | `[]` (no tokens besides implicit Eof) |
| 12 | `whitespace_is_not_a_token` | `" \t\r\n  ;  \n"` | `[Semi]` |
| 13 | `comments_are_skipped_wherever_they_appear` | 4 comment placements around `;`/`a+b` | Comments produce no tokens |
| 14 | `line_comment_at_eof_without_a_newline` | `";// trailing"`, `"// only a comment"` | `[Semi]`; `[]` |
| 15 | `block_comments_do_not_nest` | `"/* outer /* inner */ ;"` | `[Semi]` (closes at first `*/`) |
| 16 | `a_lone_slash_is_division` | `"a / b"` | `[Ident(a), Slash, Ident(b)]` |
| 17 | `every_token_reports_its_line_and_column` | `"int x;\nif (x)\n\treturn 0;\n"` | Exact pinned dump string (line:col-line:col per token) |
| 18 | `dump_column_widens_for_large_line_numbers` | 10,000 blank lines + `"int x;\n"` | Dump columns all aligned; alignment column > 16 |
| 19 | `a_tab_advances_the_column_by_one` | `"\t\t;"` | First token's column == 3 |
| 20 | `crlf_lexes_the_same_as_lf` | CRLF vs LF two-line fixture | Same token kinds; CRLF 4th token on line 2 |
| 21 | `invalid_utf8_is_a_diagnostic_not_a_panic` | `b"int \xff x;"` | 1 diagnostic containing `"byte 0xff"`; stream still ends `Eof` |
| 22 | `arbitrary_bytes_terminate` | All 256 byte values, cycled to 4096 bytes | Stream ends `Eof` (no panic, no hang) |
| 23 | `each_malformed_construct_has_its_own_diagnostic` | 17 malformed inputs (see methodology) | Each: exactly 1 diagnostic, exact message, span == whole construct |
| 24 | `unsupported_punctuation_is_named_not_called_stray` | `& \| # ? : ^ ~` | Each: `"unsupported in this C subset: '<char>'"` |
| 25 | `unsupported_punctuation_says_what_to_do_instead` | `a & b`, `a \| b`, `#include...`, `a ? b : c`, `a ^ b` | Notes: "did you mean '&&'?" etc. (5 distinct notes) |
| 26 | `integer_overflow_explains_the_limit` | `"2147483648"` | Note: `"the maximum is 2147483647; write INT_MIN as -2147483647 - 1"` |
| 27 | `a_single_error_does_not_cascade` | `'\q'`, `0x`, `'ab'`, `"unterminated` | Each: exactly 1 diagnostic |
| 28 | `scanning_resumes_on_the_line_after_an_unterminated_literal` | `"\"oops\nint x;\n"` | 1 diagnostic; tokens `[StrLit("oops"), Int, Ident(x), Semi, Eof]` |
| 29 | `an_unterminated_block_comment_runs_to_the_end_of_file` | `"int x;\n/* oops\nint y;\n"` | 1 diagnostic; tokens `[Int, Ident(x), Semi, Eof]` (rest swallowed) |
| 30 | `several_errors_are_reported_in_source_order` | 4-error fixture (bad hex, empty char, stray `@`, unterminated string) | 4 messages in order; span offsets strictly increasing |
| 31 | `a_file_of_stray_characters_terminates` | `"@$#\`@$#\`"` (8 stray chars) | 8 diagnostics; token stream is `[Eof]` only |
| 32 | `repeated_errors_still_terminate` | `"'\n'\n'\n'\n\"\n\"\n\"\n"` (7 unterminated literals) | Non-empty diagnostics; stream ends `Eof` |
| 33 | `a_trailing_backslash_does_not_overrun` | `'\`, `"\`, `'`, `"` (backslash/quote at very EOF) | Each: exactly 1 diagnostic; stream ends `Eof` |
| 34 | `spans_slice_back_to_the_source_text` | `"int total = 0xff;"` | Every token's span is a valid slice of source; literal token's span == `"0xff"` |
| 35 (integration) | `representative_program_token_stream` (`tests/lexer_snapshots.rs`) | ~35-line program touching every lexable construct (both comment forms, all 3 int bases, char/string escapes, every operator/punctuator) | Diagnostics empty; full token-stream dump matches pinned `insta` snapshot |

## Actual Outputs

Executed as part of `cargo test --lib` and `cargo test --test lexer_snapshots` (full unedited
capture in `reports/unit_tests/cargo_test_output.txt`):

```
test lexer::tests::a_file_of_stray_characters_terminates ... ok
test lexer::tests::a_lone_slash_is_division ... ok
test lexer::tests::a_single_error_does_not_cascade ... ok
test lexer::tests::a_tab_advances_the_column_by_one ... ok
test lexer::tests::a_trailing_backslash_does_not_overrun ... ok
test lexer::tests::an_unterminated_block_comment_runs_to_the_end_of_file ... ok
test lexer::tests::block_comments_do_not_nest ... ok
test lexer::tests::character_literals_decode_to_one_byte ... ok
test lexer::tests::comments_are_skipped_wherever_they_appear ... ok
test lexer::tests::crlf_lexes_the_same_as_lf ... ok
test lexer::tests::each_malformed_construct_has_its_own_diagnostic ... ok
test lexer::tests::empty_input_is_only_eof ... ok
test lexer::tests::every_escape_decodes ... ok
test lexer::tests::every_token_reports_its_line_and_column ... ok
test lexer::tests::identifiers_lex_as_identifiers ... ok
test lexer::tests::integer_literals_decode_by_base ... ok
test lexer::tests::integer_overflow_explains_the_limit ... ok
test lexer::tests::arbitrary_bytes_terminate ... ok
test lexer::tests::invalid_utf8_is_a_diagnostic_not_a_panic ... ok
test lexer::tests::keywords_lex_as_keywords ... ok
test lexer::tests::keyword_prefixes_lex_as_identifiers ... ok
test lexer::tests::line_comment_at_eof_without_a_newline ... ok
test lexer::tests::maximal_munch_splits_operators_the_documented_way ... ok
test lexer::tests::operators_and_punctuators_lex_as_themselves ... ok
test lexer::tests::quotes_nest_inside_the_other_literal_form ... ok
test lexer::tests::repeated_errors_still_terminate ... ok
test lexer::tests::scanning_resumes_on_the_line_after_an_unterminated_literal ... ok
test lexer::tests::several_errors_are_reported_in_source_order ... ok
test lexer::tests::spans_slice_back_to_the_source_text ... ok
test lexer::tests::string_literals_store_decoded_bytes ... ok
test lexer::tests::unsupported_punctuation_is_named_not_called_stray ... ok
test lexer::tests::unsupported_punctuation_says_what_to_do_instead ... ok
test lexer::tests::whitespace_is_not_a_token ... ok
test lexer::tests::dump_column_widens_for_large_line_numbers ... ok

test result: ok. (34 of these ran as part of the 142 in `unittests src/lib.rs`)

     Running tests/lexer_snapshots.rs
test representative_program_token_stream ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Result: all 35 tests passed** (34 unit tests + 1 snapshot integration test). No failures. The
`representative_program_token_stream` snapshot compared cleanly against the checked-in
`tests/snapshots/lexer_snapshots__representative_program_token_stream.snap` with no diff (a diff
would have failed the test, not merely printed a warning — `insta` fails the assertion on any
mismatch unless explicitly reviewed and re-accepted via `cargo insta accept`). `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` reported no violations against either file.
