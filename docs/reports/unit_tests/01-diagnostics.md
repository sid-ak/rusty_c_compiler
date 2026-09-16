# Unit Test Report — Diagnostics

## Unit

This unit is the compiler's shared error-reporting system.

- **Positions in the file.** Every error is tied to a byte range in the source file (a "span"),
  rather than a line/column number directly — line and column are worked out only when a message
  actually needs to be printed.
- **The error itself.** A category (which stage of the compiler found it), a message, a position,
  and optional extra explanatory notes.
- **A collector.** A run of the compiler gathers every error it finds into one collector, rather
  than stopping at the first problem — so a file with several mistakes reports all of them at once.
- **A renderer.** Turns a raw byte position into a human-readable line and column number, and prints
  the offending line of source code with a caret (`^`) underneath pointing at the exact spot — the
  same style of message `clang` uses.

Every other part of the compiler (the scanner, the parser, and later the type-checker and code
generator) reports its errors exclusively through this system. That makes this unit's correctness a
precondition for every error message anyone will ever see out of `rustycc` — a bug here doesn't
just break one message, it potentially breaks all of them.

This unit's tests don't run the scanner or the parser at all. They build sample errors and sample
files by hand, so what's being tested here is only this piece, in isolation, not how the rest of the
compiler uses it.

Source under test: `src/diagnostics.rs`.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

- White-box unit testing, using equivalence partitioning and boundary-value analysis.
- Golden master, pinned output test for the exact rendered diagnostic string.

### Why this test methodology?

This part of the compiler is simple, self-contained logic: it does no file I/O, doesn't depend on
any other part of the system, and doesn't do anything that depends on timing or environment. That
makes it a good candidate for thorough, exact-answer testing — checking a precise expected result
for a carefully chosen input — rather than broader or randomized testing.

The testing was done in three layers, each aimed at a different kind of mistake this code is prone
to:

1. **Testing every distinct kind of input, not just the typical ones.** The different ways a
   position or a range of positions can be given are grouped into meaningful categories — for
   example, "a normal range," "a range written backwards," "a zero-width range," and "two ranges
   being merged." One representative example from each category is tested, since the underlying
   logic doesn't change within a category.

2. **Testing at the exact edges where a mistake is most likely, and most damaging.** This code's
   central job is turning a raw position in a file into the line and column number a person reads —
   get that wrong by one, and every error message in the whole compiler points at the wrong spot. So
   testing specifically targets the edges: the very start of a file, the exact character where one
   line ends and the next begins, a file with no blank line at the end versus one that has one, a
   position past the actual end of the file, a completely empty file, tab characters, and
   Windows-style line endings (which use two characters for a line break instead of one).

3. **Pinning down the complete, exact appearance of a rendered error message.** Rather than checking
   the message text, the copied source line, and the pointer underneath it as three separate things,
   one test locks in the entire multi-line result exactly as it should look. That way, if the pieces
   ever stop combining correctly — even if each one still works fine on its own — the test catches
   it immediately.

### Other testing approaches

A few additional cases round out the coverage, beyond the three techniques above:

- **Pointer edge cases.** The underline drawn beneath an error is checked at its own edges: it
  doesn't run past the end of the line it's printed under, it still shows something even when the
  problem it's pointing at has no width at all, and it stays lined up correctly under a line that's
  indented with tab characters.
- **Tolerating unreadable input.** A real C source file isn't guaranteed to be clean, readable text
  — it can contain arbitrary bytes. One test confirms this code doesn't crash when asked to render
  an error against a line containing invalid text, which is a firm project-wide rule: no part of the
  compiler is allowed to crash on user input, however malformed.
- **Consistent ordering.** When several problems are found in one run, they always need to come back
  in the order they appear in the file — including the tricky case where two problems land at the
  exact same position, where they must stay in the order they were originally found rather than
  being shuffled.

## Test Coverage

Every documented guarantee this code makes has at least one test that would fail if that guarantee
were broken: safely handling an out-of-range position, files with and without a trailing blank
line, treating a tab as exactly one column, Windows-style line endings, keeping the pointer within
the line it's drawn under, still showing a pointer for a zero-width problem, degrading gracefully on
unreadable input, and keeping equal-priority problems in a stable order.

The one thing intentionally left untested here is how errors from *different* compiler stages
interact when they land at the same position (for example, a scanning error and a parsing error at
the same spot) — that depends on how those other stages behave, not on this code, so it's covered by
their own tests instead.

## Automated Test Code

The table below lists every automated test for this unit, what it's checking and why, and what was
fed in and expected back — in plain terms rather than code syntax.

<!-- inventory: src/diagnostics/tests.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `span_length_is_its_byte_extent` | A span's length is its byte extent, and a zero-width span is empty. |
| 2 | `reversed_span_clamps_to_empty` | A reversed range clamps to empty rather than underflowing on `len`. |
| 3 | `joining_spans_covers_both` | Joining two spans covers both, in either order. |
| 4 | `spans_sort_in_source_order` | Spans sort in source order, which is the order diagnostics are reported in. |
| 5 | `notes_attach_without_changing_the_message` | Notes attach to a diagnostic without disturbing its message or span. |
| 6 | `unsupported_reads_the_same_from_either_pass` | Every pass phrases an out-of-subset construct identically, whichever pass noticed it. |
| 7 | `offsets_resolve_to_line_and_column` | Offsets resolve at the file start, at a line start, mid-line, at a line end, and at EOF. |
| 8 | `eof_without_a_trailing_newline_stays_on_the_last_line` | The end of a file with no trailing newline is a position on the last line. |
| 9 | `eof_after_a_trailing_newline_opens_a_new_line` | A trailing newline opens a line, so the end of such a file is the start of the line after. |
| 10 | `offset_past_the_end_clamps` | An offset past the end of the file clamps to the end rather than escaping the source. |
| 11 | `empty_source_is_one_empty_line` | An empty file has one line, and its only position is 1:1. |
| 12 | `a_tab_advances_the_column_by_one` | A tab is one column, not a jump to the next tab stop — the documented, clang-matching rule. |
| 13 | `crlf_numbers_lines_the_same_as_lf` | CRLF files number their lines identically to LF files. |
| 14 | `crlf_line_renders_without_the_carriage_return` | A carriage return is trimmed from the rendered line, so the caret is not pushed by it. |
| 15 | `single_line_diagnostic_rendering_is_stable` | The exact rendered form of a single-line diagnostic, pinned so later phases inherit it. |
| 16 | `caret_padding_reproduces_tabs` | The caret sits under a tab-indented construct rather than beside it, because the padding reproduces the tabs instead of counting them as one space each. |
| 17 | `multi_line_span_does_not_spill_past_the_line` | A span reaching past the end of its line underlines to the line end and no further. |
| 18 | `empty_span_renders_a_single_caret` | A zero-width span still points somewhere: one caret, no underline. |
| 19 | `diagnostic_at_eof_renders` | A span at the very end of a file renders without panicking on the empty final line. |
| 20 | `invalid_utf8_line_still_renders` | Invalid UTF-8 in the offending line is rendered lossily rather than aborting the report. |
| 21 | `bag_reports_in_source_order` | The bag reports in source order however the diagnostics went in. |
| 22 | `bag_sort_is_stable_within_a_position` | Two diagnostics at the same position keep the order the pass found them in. |
| 23 | `empty_bag_is_empty` | An empty bag is what a clean run leaves behind. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
