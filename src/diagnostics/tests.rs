//! Unit tests for spans, the source map, caret rendering, and the diagnostic bag.

use super::*;

/// A span's length is its byte extent, and a zero-width span is empty.
#[test]
fn span_length_is_its_byte_extent() {
    assert_eq!(Span::new(3, 7).len(), 4);
    assert!(!Span::new(3, 7).is_empty());
    assert!(Span::empty_at(3).is_empty());
}

/// A reversed range clamps to empty rather than underflowing on `len`.
#[test]
fn reversed_span_clamps_to_empty() {
    let span = Span::new(9, 4);

    assert_eq!(span, Span::empty_at(9));
    assert_eq!(span.len(), 0);
}

/// Joining two spans covers both, in either order.
#[test]
fn joining_spans_covers_both() {
    let left = Span::new(2, 5);
    let right = Span::new(11, 14);

    assert_eq!(left.to(right), Span::new(2, 14));
    assert_eq!(right.to(left), Span::new(2, 14));
}

/// Spans sort in source order, which is the order diagnostics are reported in.
#[test]
fn spans_sort_in_source_order() {
    let mut spans = [Span::new(10, 12), Span::new(0, 4), Span::new(0, 2)];
    spans.sort();

    assert_eq!(spans, [Span::new(0, 2), Span::new(0, 4), Span::new(10, 12)]);
}

/// Notes attach to a diagnostic without disturbing its message or span.
#[test]
fn notes_attach_without_changing_the_message() {
    let diagnostic = Diagnostic::lex(Span::new(0, 1), "stray character")
        .with_note("delete it")
        .with_note("or quote it");

    assert_eq!(diagnostic.kind, DiagnosticKind::Lex);
    assert_eq!(diagnostic.message, "stray character");
    assert_eq!(diagnostic.span, Span::new(0, 1));
    assert_eq!(diagnostic.note_messages(), ["delete it", "or quote it"]);
}

/// Every pass phrases an out-of-subset construct identically, whichever pass noticed it.
#[test]
fn unsupported_reads_the_same_from_either_pass() {
    let from_lexer = Diagnostic::unsupported(DiagnosticKind::Lex, Span::new(0, 1), "'&'");
    let from_parser = Diagnostic::unsupported(DiagnosticKind::Parse, Span::new(0, 6), "'struct'");

    assert_eq!(from_lexer.message, "unsupported in this C subset: '&'");
    assert_eq!(
        from_parser.message,
        "unsupported in this C subset: 'struct'"
    );
    assert_eq!(from_lexer.kind, DiagnosticKind::Lex);
    assert_eq!(from_parser.kind, DiagnosticKind::Parse);
}

/// A three-line fixture with no trailing newline, used for the position tests below.
const THREE_LINES: &[u8] = b"int a;\nint bb;\nint c;";

/// Build a map over `source` named `t.c`, the fixture path the position tests share.
fn map(source: &[u8]) -> SourceMap<'_> {
    SourceMap::new(Path::new("t.c"), source)
}

/// Assert `offset` resolves to `line`:`column` in `source`.
fn assert_location(source: &[u8], offset: usize, line: usize, column: usize) {
    assert_eq!(
        map(source).location(offset),
        Location { line, column },
        "offset {offset}"
    );
}

/// Offsets resolve at the file start, at a line start, mid-line, at a line end, and at EOF.
#[test]
fn offsets_resolve_to_line_and_column() {
    assert_location(THREE_LINES, 0, 1, 1); // file start
    assert_location(THREE_LINES, 4, 1, 5); // mid-line
    assert_location(THREE_LINES, 6, 1, 7); // the newline itself, at the line end
    assert_location(THREE_LINES, 7, 2, 1); // the next line's start
    assert_location(THREE_LINES, 20, 3, 6); // last byte of a file with no trailing newline
}

/// The end of a file with no trailing newline is a position on the last line.
#[test]
fn eof_without_a_trailing_newline_stays_on_the_last_line() {
    assert_location(THREE_LINES, THREE_LINES.len(), 3, 7);
}

/// A trailing newline opens a line, so the end of such a file is the start of the line after.
#[test]
fn eof_after_a_trailing_newline_opens_a_new_line() {
    let source = b"int a;\n";

    assert_location(source, source.len(), 2, 1);
}

/// An offset past the end of the file clamps to the end rather than escaping the source.
#[test]
fn offset_past_the_end_clamps() {
    assert_eq!(
        map(THREE_LINES).location(9_999),
        map(THREE_LINES).location(THREE_LINES.len())
    );
}

/// An empty file has one line, and its only position is 1:1.
#[test]
fn empty_source_is_one_empty_line() {
    assert_location(b"", 0, 1, 1);
}

/// A tab is one column, not a jump to the next tab stop — the documented, clang-matching rule.
#[test]
fn a_tab_advances_the_column_by_one() {
    let source = b"\t\tx;";

    assert_location(source, 0, 1, 1); // before the first tab
    assert_location(source, 1, 1, 2); // between the tabs
    assert_location(source, 2, 1, 3); // `x`, two tabs in
}

/// CRLF files number their lines identically to LF files.
#[test]
fn crlf_numbers_lines_the_same_as_lf() {
    let crlf = map(b"int a;\r\nint bb;\r\nint c;");
    let lf = map(THREE_LINES);

    // The `\r` occupies a column, so compare each line's start rather than raw offsets.
    assert_eq!(crlf.location(0).line, lf.location(0).line);
    assert_eq!(crlf.location(8), Location { line: 2, column: 1 });
    assert_eq!(crlf.location(17), Location { line: 3, column: 1 });
}

/// A carriage return is trimmed from the rendered line, so the caret is not pushed by it.
#[test]
fn crlf_line_renders_without_the_carriage_return() {
    let source = b"int a;\r\nint bb;\r\n";
    let diagnostic = Diagnostic::lex(Span::new(12, 14), "example");

    let rendered = map(source).render(&diagnostic);

    assert!(!rendered.contains('\r'), "rendered: {rendered:?}");
    assert!(rendered.contains("t.c:2:5: error: example"));
}

/// The exact rendered form of a single-line diagnostic, pinned so later phases inherit it.
#[test]
fn single_line_diagnostic_rendering_is_stable() {
    let source = b"int main(void) {\n    char *s = \"hi;\n}\n";
    let diagnostic = Diagnostic::lex(Span::new(31, 35), "unterminated string literal")
        .with_note("string literals may not span a line");

    insta::assert_snapshot!(map(source).render(&diagnostic), @r###"
    t.c:2:15: error: unterminated string literal
        char *s = "hi;
                  ^~~~
    note: string literals may not span a line
    "###);
}

/// The caret sits under a tab-indented construct rather than beside it, because the padding
/// reproduces the tabs instead of counting them as one space each.
#[test]
fn caret_padding_reproduces_tabs() {
    let source = b"\t\treturn;";
    let diagnostic = Diagnostic::lex(Span::new(2, 8), "example");

    let rendered = map(source).render(&diagnostic);

    assert!(rendered.ends_with("\n\t\t^~~~~~"), "rendered: {rendered:?}");
}

/// A span reaching past the end of its line underlines to the line end and no further.
#[test]
fn multi_line_span_does_not_spill_past_the_line() {
    let source = b"ab\ncd\nef\n";
    let diagnostic = Diagnostic::lex(Span::new(1, 7), "example");

    let rendered = map(source).render(&diagnostic);

    // The span reaches into the third line, but only `b` remains on the first.
    let caret = rendered.lines().last().unwrap_or_default();
    assert_eq!(caret, " ^", "rendered: {rendered:?}");
}

/// A zero-width span still points somewhere: one caret, no underline.
#[test]
fn empty_span_renders_a_single_caret() {
    let source = b"int a\n";
    let diagnostic = Diagnostic::lex(Span::empty_at(5), "expected ';'");

    let rendered = map(source).render(&diagnostic);

    assert!(rendered.ends_with("\n     ^"), "rendered: {rendered:?}");
}

/// A span at the very end of a file renders without panicking on the empty final line.
#[test]
fn diagnostic_at_eof_renders() {
    let source = b"int a;\n";
    let diagnostic = Diagnostic::lex(Span::empty_at(source.len()), "unexpected end of file");

    let rendered = map(source).render(&diagnostic);

    assert!(rendered.starts_with("t.c:2:1: error: unexpected end of file"));
}

/// Invalid UTF-8 in the offending line is rendered lossily rather than aborting the report.
#[test]
fn invalid_utf8_line_still_renders() {
    let source = b"int \xff = 1;\n";
    let diagnostic = Diagnostic::lex(Span::new(4, 5), "stray byte");

    let rendered = map(source).render(&diagnostic);

    assert!(rendered.starts_with("t.c:1:5: error: stray byte"));
}

/// The bag reports in source order however the diagnostics went in.
#[test]
fn bag_reports_in_source_order() {
    let mut bag = DiagnosticBag::new();
    bag.push(Diagnostic::lex(Span::new(40, 41), "third"));
    bag.push(Diagnostic::lex(Span::new(4, 5), "first"));
    bag.push(Diagnostic::lex(Span::new(12, 13), "second"));

    let messages: Vec<_> = bag
        .into_sorted()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();

    assert_eq!(messages, ["first", "second", "third"]);
}

/// Two diagnostics at the same position keep the order the pass found them in.
#[test]
fn bag_sort_is_stable_within_a_position() {
    let mut bag = DiagnosticBag::new();
    bag.push(Diagnostic::lex(Span::new(0, 1), "found first"));
    bag.push(Diagnostic::lex(Span::new(0, 1), "found second"));

    let messages: Vec<_> = bag
        .into_sorted()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect();

    assert_eq!(messages, ["found first", "found second"]);
}

/// An empty bag is what a clean run leaves behind.
#[test]
fn empty_bag_is_empty() {
    let mut bag = DiagnosticBag::new();
    assert!(bag.is_empty());
    assert_eq!(bag.len(), 0);

    bag.push(Diagnostic::lex(Span::empty_at(0), "something"));

    assert!(!bag.is_empty());
    assert_eq!(bag.len(), 1);
}
