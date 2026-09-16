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

Source: [diagnostics.rs](https://github.com/sid-ak/rusty_c_compiler/blob/main/src/diagnostics.rs)
Date: 2026-08-23
Engineer: Sidharth Anandkumar

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

| # | Test | Purpose | Input | Expected output |
|---|------|---------|-------|------------------|
| 1 | `span_length_is_its_byte_extent` | Confirms a span's length is calculated correctly, including for an empty span. | A span from byte 3 to byte 7, and a zero-width span at byte 3. | The first span is 4 bytes long and not empty; the zero-width span is empty. |
| 2 | `reversed_span_clamps_to_empty` | Confirms a span whose start comes after its end doesn't break anything — it's treated as empty instead. | A span running backwards, from byte 9 to byte 4. | Treated the same as an empty span at byte 9, with zero length. |
| 3 | `joining_spans_covers_both` | Confirms combining two spans produces one span covering both, regardless of the order they're combined in. | Two separate spans, combined both ways. | Both combinations produce the identical single span covering the full range. |
| 4 | `spans_sort_in_source_order` | Confirms spans sort by where they appear in the file. | Three spans, listed out of order. | Sorting puts them back into the order they appear in the source. |
| 5 | `notes_attach_without_changing_the_message` | Confirms attaching explanatory notes to an error doesn't alter its category, message, or position. | An error with two notes attached. | The category, message, and position stay unchanged; both notes are attached in the order added. |
| 6 | `unsupported_reads_the_same_from_either_pass` | Confirms that when different parts of the compiler report "this isn't part of the supported C subset," the wording is identical no matter which part reported it. | The same kind of not-supported error, once as if found while scanning and once as if found while parsing. | Both produce the same wording style, differing only in what construct they name. |
| 7 | `offsets_resolve_to_line_and_column` | Confirms raw file positions convert correctly to line and column numbers. | A 3-line sample file, checked at the start, mid-line, end of a line, start of the next line, and near the end. | Each position converts to the correct line and column. |
| 8 | `eof_without_a_trailing_newline_stays_on_the_last_line` | Confirms the end of a file with no blank final line is still reported as being on the last real line. | The very last position of a 3-line file with no trailing blank line. | Reported as line 3. |
| 9 | `eof_after_a_trailing_newline_opens_a_new_line` | Confirms a file that does end with a line break is treated as having one extra, empty line after it. | The very last position of a file ending in a line break. | Reported as line 2 (the new, empty line). |
| 10 | `offset_past_the_end_clamps` | Confirms a position beyond the file's actual end doesn't cause a problem — it's treated as if it were exactly at the end. | A position far past the end of a file, compared with the position exactly at the end. | Both resolve to the identical line and column. |
| 11 | `empty_source_is_one_empty_line` | Confirms a completely empty file is handled gracefully, as one blank line. | An empty file. | Reported as line 1, column 1. |
| 12 | `a_tab_advances_the_column_by_one` | Confirms a tab character counts as exactly one column — matching how `clang` reports positions — rather than jumping to the next tab stop. | A line starting with two tab characters. | Each position after a tab advances the column count by exactly one. |
| 13 | `crlf_numbers_lines_the_same_as_lf` | Confirms files using Windows-style line endings are numbered identically to files using the standard line ending. | The same 3-line sample file, once with each line-ending style. | Both produce identical line numbers throughout. |
| 14 | `crlf_line_renders_without_the_carriage_return` | Confirms the extra character used in Windows-style line endings doesn't leak into a printed error message. | A Windows-line-ending file, with a sample error on line 2. | The rendered message has no leftover Windows line-ending characters and correctly names line 2, column 5. |
| 15 | `single_line_diagnostic_rendering_is_stable` | Locks in the exact, complete appearance of a rendered error — the file position, the message, the copied source line, the pointer beneath it, and an attached note — so any future change to the format is caught immediately. | A short sample program with an "unterminated string literal" error and an explanatory note. | The rendered message matches a fixed, exact multi-line layout. |
| 16 | `caret_padding_reproduces_tabs` | Confirms the pointer under an error still lines up correctly when the source line is indented with tabs. | A line indented with two tabs, with an error spanning part of it. | The pointer is preceded by the same two tab characters, so it still lines up under the right text. |
| 17 | `multi_line_span_does_not_spill_past_the_line` | Confirms the pointer never runs past the end of the printed line, even when the underlying problem technically spans multiple lines. | An error whose position spans across two line breaks. | The pointer is drawn only to the end of the first line, not beyond it. |
| 18 | `empty_span_renders_a_single_caret` | Confirms a problem with no width at all (something expected but missing) still shows a visible pointer. | A zero-width error at a specific position. | Exactly one pointer character is shown at that position. |
| 19 | `diagnostic_at_eof_renders` | Confirms an error reported at the very end of a file, on an empty final line, renders without crashing. | An "unexpected end of file" error at the very end of a short file. | Renders correctly, naming line 2 (the empty line after the content). |
| 20 | `invalid_utf8_line_still_renders` | Confirms the compiler doesn't crash when reporting on a file that contains a byte that isn't valid readable text. | A line containing one invalid, unreadable byte, with an error pointing at it. | The message still renders, showing what it can of the offending line rather than crashing. |
| 21 | `bag_reports_in_source_order` | Confirms that when multiple problems are collected during a run, they come back out in file order, regardless of the order they were found in. | Three errors added in a scrambled order. | Retrieved back in correct source order. |
| 22 | `bag_sort_is_stable_within_a_position` | Confirms two problems found at the exact same position keep the order they were originally found in, rather than being shuffled. | Two errors added at the identical position. | Both come back in the same order they were added. |
| 23 | `empty_bag_is_empty` | Confirms the problem collector correctly reports whether it has anything in it. | A freshly created, empty collector, then one error added to it. | Reports empty and a count of zero beforehand; not-empty and a count of one afterward. |

## Actual Outputs

All 23 automated tests for this unit passed — every actual result matched its expected result
exactly, with no failures. This was confirmed by running the full test suite (`cargo test`) as well
as the project's formatting and linting checks (`cargo fmt`, `cargo clippy`), all of which completed
cleanly. The complete, unedited console output from that run is kept in `scripts/cargo_test_output.txt`
for reference.
