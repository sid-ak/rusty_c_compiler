//! Source positions and the errors reported against them.
//!
//! Every pass reports through these types rather than inventing its own error style, and every
//! position in the compiler is a [`Span`] of byte offsets. Line and column numbers are derived
//! only when a diagnostic is rendered, by [`SourceMap`], so no pass has to carry them around.

use std::fmt;
use std::path::Path;

/// A half-open byte range `[start, end)` into the source file.
///
/// Byte offsets, not line and column numbers, are what flows through the compiler: they are cheap
/// to produce, cheap to compare, and independent of how the file is split into lines.
///
/// Ordering is by start offset and then by end, which is source order — the order diagnostics are
/// reported in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    /// Byte offset of the first byte of the range.
    pub start: usize,
    /// Byte offset one past the last byte of the range.
    pub end: usize,
}

impl Span {
    /// A span covering `[start, end)`.
    ///
    /// A reversed range is clamped to empty at `start` rather than rejected, so a caller that
    /// miscomputes an end offset produces a harmless position instead of a panic later.
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end: end.max(start),
        }
    }

    /// The empty span at `offset`, for a problem with a position but no extent, such as something
    /// missing at the end of a file.
    pub fn empty_at(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    /// The number of bytes the span covers.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers no bytes at all.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The smallest span covering both `self` and `other`, for a construct assembled from parts.
    pub fn to(self, other: Span) -> Self {
        Self::new(self.start.min(other.start), self.end.max(other.end))
    }
}

/// Which pass reported a diagnostic.
///
/// The pass is recorded rather than baked into the message so that tests can assert a specific
/// pass produced a specific error, instead of matching on message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticKind {
    /// Reported by the lexer, about the shape of the raw text.
    Lex,
    /// Reported by the parser, about the structure of the token stream.
    Parse,
    /// Reported by semantic analysis, about what the program means.
    Semantic,
    /// Reported by the compiler about itself: a limit reached, or an invariant broken.
    Internal,
}

/// One problem found in the user's program, located at a [`Span`].
///
/// Diagnostics are values, not exceptions: a pass accumulates them and keeps going, so one broken
/// construct does not hide the rest of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// The pass that reported it.
    pub kind: DiagnosticKind,
    /// The one-line description shown after `error:`.
    pub message: String,
    /// Where in the source the problem is.
    pub span: Span,
    /// Extra lines shown beneath the message, for context the message itself would overcrowd.
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// A diagnostic from `kind` at `span`.
    pub fn new(kind: DiagnosticKind, span: Span, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            span,
            notes: Vec::new(),
        }
    }

    /// A lexer diagnostic at `span`.
    pub fn lex(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Lex, span, message)
    }

    /// This diagnostic with `note` appended, for chaining at the construction site.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

/// A 1-based line and column, as a diagnostic displays it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Location {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number, counted in bytes from the start of the line.
    pub column: usize,
}

impl fmt::Display for Location {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.line, self.column)
    }
}

/// One source file, indexed so a byte offset can be turned into a line and column.
///
/// The index is a table of line-start offsets built once when the map is created, so a lookup is a
/// binary search rather than a rescan of the file. Diagnostics are rendered through here and
/// nowhere else, which is what keeps every pass's errors looking the same.
///
/// Columns are counted in bytes from the start of the line, 1-based, matching what `clang` reports
/// for the same position. A tab is therefore one column, not a jump to the next tab stop; the
/// caret line reproduces the source line's tabs instead of padding with spaces, so the caret still
/// lines up under the offending text whatever width the terminal renders a tab at.
#[derive(Debug, Clone)]
pub struct SourceMap<'source> {
    /// The file's path, as it appears at the front of a rendered diagnostic.
    path: &'source Path,
    /// The raw bytes of the file. Bytes, not text: a C file need not be valid UTF-8.
    source: &'source [u8],
    /// Byte offset of the first byte of each line. Always starts with 0, so it is never empty.
    line_starts: Vec<usize>,
}

impl<'source> SourceMap<'source> {
    /// Index `source`, which came from `path`.
    pub fn new(path: &'source Path, source: &'source [u8]) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(
            source
                .iter()
                .enumerate()
                .filter(|(_, &byte)| byte == b'\n')
                .map(|(offset, _)| offset + 1),
        );

        Self {
            path,
            source,
            line_starts,
        }
    }

    /// The path this map was built from.
    pub fn path(&self) -> &Path {
        self.path
    }

    /// The 1-based line and column of `offset`.
    ///
    /// An offset past the end of the file is clamped to the end, so a span built from a truncated
    /// or malformed construct still renders somewhere sensible instead of failing.
    pub fn location(&self, offset: usize) -> Location {
        let offset = offset.min(self.source.len());
        let index = self.line_index(offset);
        let start = self.line_starts.get(index).copied().unwrap_or(0);

        Location {
            line: index + 1,
            column: offset - start + 1,
        }
    }

    /// Render `diagnostic` as the message line, the offending source line, and a caret underline.
    ///
    /// A span covering more than one line is underlined only on its first line, since a caret that
    /// ran past the end of the line it is printed under would point at nothing.
    pub fn render(&self, diagnostic: &Diagnostic) -> String {
        let location = self.location(diagnostic.span.start);
        let line = self.line_bytes(self.line_index(diagnostic.span.start.min(self.source.len())));
        let column_offset = location.column - 1;

        let mut rendered = format!(
            "{}:{location}: error: {}\n",
            self.path.display(),
            diagnostic.message
        );
        rendered.push_str(&String::from_utf8_lossy(line));
        rendered.push('\n');
        rendered.push_str(&caret_line(line, column_offset, diagnostic.span.len()));

        for note in &diagnostic.notes {
            rendered.push_str("\nnote: ");
            rendered.push_str(note);
        }

        rendered
    }

    /// The index into `line_starts` of the line containing `offset`.
    fn line_index(&self, offset: usize) -> usize {
        self.line_starts
            .partition_point(|&start| start <= offset)
            .saturating_sub(1)
    }

    /// The bytes of line `index`, with the line terminator — `\n` or `\r\n` — trimmed off.
    fn line_bytes(&self, index: usize) -> &'source [u8] {
        let start = self.line_starts.get(index).copied().unwrap_or(0);
        let end = self
            .line_starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.source.len());
        let line = self.source.get(start..end).unwrap_or_default();
        let line = line.strip_suffix(b"\n").unwrap_or(line);

        line.strip_suffix(b"\r").unwrap_or(line)
    }
}

/// The underline printed beneath a source line: padding to `column_offset`, then `width` carets.
///
/// Padding copies a tab through as a tab so the caret tracks the source line however wide the
/// terminal draws one, and skips UTF-8 continuation bytes so a multi-byte character costs one
/// column rather than one per byte. The underline is clamped to the end of the line and is never
/// narrower than a single `^`, since a zero-width span still has a position worth pointing at.
fn caret_line(line: &[u8], column_offset: usize, width: usize) -> String {
    /// Whether `byte` continues a multi-byte UTF-8 sequence rather than starting a character.
    fn is_continuation(byte: u8) -> bool {
        byte & 0b1100_0000 == 0b1000_0000
    }

    let mut caret: String = line
        .iter()
        .take(column_offset)
        .filter(|&&byte| byte == b'\t' || !is_continuation(byte))
        .map(|&byte| if byte == b'\t' { '\t' } else { ' ' })
        .collect();

    let underlined = line
        .iter()
        .skip(column_offset)
        .take(width)
        .filter(|&&byte| !is_continuation(byte))
        .count()
        .max(1);
    caret.push('^');
    caret.extend(std::iter::repeat_n('~', underlined - 1));

    caret
}

/// Diagnostics collected over one run of a pass.
///
/// A pass accumulates into a bag and keeps going rather than returning at the first problem, so a
/// file with four mistakes reports four of them. The bag hands them back in source order however
/// they went in, because a pass that revisits an earlier construct — a forward reference resolved
/// late, a recovery point backed up to — would otherwise report out of order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticBag {
    diagnostics: Vec<Diagnostic>,
}

impl DiagnosticBag {
    /// An empty bag.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record `diagnostic`.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// How many diagnostics have been recorded.
    pub fn len(&self) -> usize {
        self.diagnostics.len()
    }

    /// Whether nothing has been recorded, which is what a clean run looks like.
    pub fn is_empty(&self) -> bool {
        self.diagnostics.is_empty()
    }

    /// The recorded diagnostics in source order.
    ///
    /// The sort is stable, so two diagnostics at the same position stay in the order the pass
    /// found them.
    pub fn into_sorted(mut self) -> Vec<Diagnostic> {
        self.diagnostics.sort_by_key(|diagnostic| diagnostic.span);
        self.diagnostics
    }
}

impl Extend<Diagnostic> for DiagnosticBag {
    fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, diagnostics: I) {
        self.diagnostics.extend(diagnostics);
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(diagnostic.notes, ["delete it", "or quote it"]);
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
}
