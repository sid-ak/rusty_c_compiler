//! Source positions and the errors reported against them.
//!
//! Every pass reports through these types rather than inventing its own error style, and every
//! position in the compiler is a [`Span`] of byte offsets. Line and column numbers are derived
//! only when a diagnostic is rendered, so no pass has to carry them around.

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
}
