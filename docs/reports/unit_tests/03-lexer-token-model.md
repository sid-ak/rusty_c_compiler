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

2026-08-23

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

All 15 tests live in `src/lexer/token.rs` under `#[cfg(test)] mod tests`.

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 1 | `every_kind_has_a_non_empty_spelling` | All 30 `TokenKind` samples | `.to_string()` non-empty for every one |
| 2 | `every_kind_sample_is_distinct` | 30 samples, `mem::discriminant` set | `HashSet` len == 30 (no duplicate variant) |
| 3 | `fixed_spellings_are_the_source_text` | `LtEq`, `Semi`, `AmpAmp`, `Ident("a")`, `IntLit(1)` | `Some("<=")`, `Some(";")`, `Some("&&")`, `None`, `None` |
| 4 | `spellings_read_as_source` | `format!("expected '{}', found '{}'", Semi, RBrace)` | `"expected ';', found '}'"` |
| 5 | `keywords_round_trip_through_the_table` | Every `Keyword::ALL` variant, spelled then parsed | `from_identifier(spelling) == Some(keyword)` for all 10 |
| 6 | `the_keyword_set_is_the_documented_one` | `Keyword::ALL` spellings | `["int","char","void","if","else","while","for","return","break","continue"]` |
| 7 | `keywords_with_affixes_are_identifiers` | Each keyword + 6 affix variants (suffix `eger`, `_`, `1`; prefix `x`, `_`; uppercased) | `from_identifier(candidate) == None` for all |
| 8 | `the_two_keyword_lists_partition_c89` | `Keyword::ALL` ∪ `UNSUPPORTED_KEYWORDS`, sorted | Exactly the 32 C89 keywords, alphabetized |
| 9 | `unsupported_keywords_are_recognized_by_name` | `"struct"`, `"sizeof"`, `"int"`, `"total"`, `"Struct"` | `Some("struct")`, `Some("sizeof")`, `None`, `None`, `None` |
| 10 | `escape_table_round_trips` | Every `ESCAPES` `(letter, byte)` pair | `escape_byte(letter)==Some(byte)`; `escape_letter(byte)==Some(letter)` |
| 11 | `unknown_escape_letters_decode_to_nothing` | `q`, `z`, `8`, `x` | `escape_byte(...) == None` for each |
| 12 | `literals_spell_themselves_back` | `CharLit('\n')`, `CharLit('a')`, `StrLit("a\tb\0c")`, `StrLit("")`, `IntLit(-5)` | `"'\n'"` (raw `\n` escape), `"'a'"`, `"\"a\\tb\\0c\""`, `"\"\""`, `"-5"` |
| 13 | `unprintable_bytes_render_as_hex` | `CharLit(0x01)`, `CharLit(0xff)` | `"'\x01'"`, `"'\xff'"` |
| 14 | `spelling_a_literal_body_adds_no_quotes` | `""`, `"hi"`, `"a\tb"`, `[0x01]` | `""`, `"hi"`, `"a\\tb"`, `"\\x01"` |
| 15 | `token_pairs_a_kind_with_a_span` | `Token::new(Semi, Span::new(4,5))` | `.kind==Semi`, `.span==Span(4,5)`, `.to_string()==";"` |

## Actual Outputs

Executed as part of `cargo test --lib` (full unedited capture in
`reports/unit_tests/cargo_test_output.txt`):

```
test lexer::token::tests::escape_table_round_trips ... ok
test lexer::token::tests::every_kind_has_a_non_empty_spelling ... ok
test lexer::token::tests::every_kind_sample_is_distinct ... ok
test lexer::token::tests::fixed_spellings_are_the_source_text ... ok
test lexer::token::tests::keywords_round_trip_through_the_table ... ok
test lexer::token::tests::keywords_with_affixes_are_identifiers ... ok
test lexer::token::tests::literals_spell_themselves_back ... ok
test lexer::token::tests::spelling_a_literal_body_adds_no_quotes ... ok
test lexer::token::tests::spellings_read_as_source ... ok
test lexer::token::tests::the_keyword_set_is_the_documented_one ... ok
test lexer::token::tests::the_two_keyword_lists_partition_c89 ... ok
test lexer::token::tests::token_pairs_a_kind_with_a_span ... ok
test lexer::token::tests::unknown_escape_letters_decode_to_nothing ... ok
test lexer::token::tests::unprintable_bytes_render_as_hex ... ok
test lexer::token::tests::unsupported_keywords_are_recognized_by_name ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

**Result: all 15 tests passed.** No failures, no lint or formatting violations reported against this
file by `cargo clippy --all-targets -- -D warnings` / `cargo fmt --check`.
