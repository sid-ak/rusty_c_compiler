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

    /// A parser diagnostic at `span`.
    pub fn parse(span: Span, message: impl Into<String>) -> Self {
        Self::new(DiagnosticKind::Parse, span, message)
    }

    /// A diagnostic for real C that this subset deliberately does not implement.
    ///
    /// One phrasing wherever such a construct turns up, because which pass notices it is an
    /// accident of spelling rather than something the reader should have to know: the lexer
    /// catches the ones that are punctuation, since `?` and `#` are not tokens of this grammar at
    /// all, and the parser catches the ones that are words or shapes.
    ///
    /// `construct` names what was written — a quoted spelling such as `'struct'` for something the
    /// user typed verbatim, or a phrase such as `pointer declarators` for a shape.
    pub fn unsupported(kind: DiagnosticKind, span: Span, construct: impl fmt::Display) -> Self {
        Self::new(
            kind,
            span,
            format!("unsupported in this C subset: {construct}"),
        )
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
mod tests;
