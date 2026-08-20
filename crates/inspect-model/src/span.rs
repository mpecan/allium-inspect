//! Byte spans into a spec file, and the line/column resolution the UI needs.
//!
//! Every construct the `allium` parser emits carries a `{start, end}` byte
//! offset. That is the right thing to keep — it survives round-tripping and it
//! is what lets the UI show the exact source behind a node or a failed clause —
//! but a browser needs a line and a column to render it. [`LineIndex`] does that
//! conversion once per file rather than rescanning for every lookup.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A half-open byte range `[start, end)` into a spec file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    /// A span covering `[start, end)`.
    ///
    /// A reversed pair is normalised rather than rejected. These come from a
    /// parser we do not control, and a nonsensical range is not worth failing
    /// an entire graph over — an empty highlight is a better outcome than no
    /// graph.
    #[must_use]
    pub fn new(start: usize, end: usize) -> Self {
        if start <= end { Self { start, end } } else { Self { start: end, end: start } }
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether the span covers no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Whether `offset` falls inside the span.
    #[must_use]
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    /// Whether this span lies entirely within `other`.
    #[must_use]
    pub fn within(&self, other: &Span) -> bool {
        self.start >= other.start && self.end <= other.end
    }

    /// The slice of `source` this span covers, or `None` when it runs past the
    /// end or splits a UTF-8 character.
    ///
    /// Returning `None` rather than panicking is deliberate: spans arrive from
    /// a separate process, and a stale or mismatched file must degrade to "no
    /// preview" rather than take the server down.
    #[must_use]
    pub fn slice<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }
}

/// Line start offsets for one source file, for resolving spans to line/column.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the first character of each line.
    starts: Vec<usize>,
    len: usize,
}

/// A one-based line and column, as an editor would show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/lib/api/")]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

impl LineIndex {
    /// Index `source`.
    #[must_use]
    pub fn new(source: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(source.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts, len: source.len() }
    }

    /// The one-based line and column of `offset`.
    ///
    /// The column counts characters, not bytes: a byte column puts the caret in
    /// the wrong place on any line containing a non-ASCII character, and spec
    /// prose contains plenty of them.
    #[must_use]
    pub fn position(&self, source: &str, offset: usize) -> Position {
        let offset = offset.min(self.len);
        // `partition_point` gives the count of starts at or before `offset`,
        // which is the one-based line number directly.
        let line = self.starts.partition_point(|&start| start <= offset);
        let line_start = self.starts.get(line.saturating_sub(1)).copied().unwrap_or(0);
        let column =
            source.get(line_start..offset).map_or(offset - line_start, |text| text.chars().count())
                + 1;
        Position { line: line.max(1), column }
    }

    /// The number of lines in the indexed source.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The byte range of the one-based line `line`, including its newline.
    #[must_use]
    pub fn line_span(&self, line: usize) -> Option<Span> {
        let start = *self.starts.get(line.checked_sub(1)?)?;
        let end = self.starts.get(line).copied().unwrap_or(self.len);
        Some(Span::new(start, end))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "entity Message {\n    body: String\n}\n";

    #[test]
    fn new_orders_its_bounds() {
        assert_eq!(Span::new(3, 7), Span { start: 3, end: 7 });
    }

    #[test]
    fn new_normalises_a_reversed_range() {
        assert_eq!(Span::new(7, 3), Span { start: 3, end: 7 });
    }

    #[test]
    fn len_and_emptiness_agree() {
        assert_eq!(Span::new(3, 7).len(), 4);
        assert!(!Span::new(3, 7).is_empty());
        assert_eq!(Span::new(5, 5).len(), 0);
        assert!(Span::new(5, 5).is_empty());
    }

    #[test]
    fn contains_is_half_open() {
        let span = Span::new(3, 7);
        assert!(!span.contains(2));
        assert!(span.contains(3));
        assert!(span.contains(6));
        assert!(!span.contains(7), "the end offset is outside a half-open range");
    }

    #[test]
    fn within_is_inclusive_of_identical_bounds() {
        let outer = Span::new(0, 10);
        assert!(Span::new(0, 10).within(&outer));
        assert!(Span::new(2, 8).within(&outer));
        assert!(!Span::new(2, 11).within(&outer));
        assert!(!Span::new(0, 11).within(&outer));
    }

    #[test]
    fn slice_returns_the_covered_text() {
        assert_eq!(Span::new(7, 14).slice(SRC), Some("Message"));
    }

    #[test]
    fn slice_of_an_out_of_range_span_is_none() {
        assert_eq!(Span::new(0, SRC.len() + 10).slice(SRC), None);
    }

    #[test]
    fn slice_that_splits_a_character_is_none() {
        // A stale span against a file containing multi-byte text must not
        // panic; it must decline to preview.
        let source = "-- naïve\n";
        assert_eq!(Span::new(0, 6).slice(source), None);
    }

    #[test]
    fn position_is_one_based_at_the_start() {
        let index = LineIndex::new(SRC);
        assert_eq!(index.position(SRC, 0), Position { line: 1, column: 1 });
    }

    #[test]
    fn position_resolves_later_lines() {
        let index = LineIndex::new(SRC);
        // Offset 21 is the `b` of `body` on line 2, four spaces in.
        assert_eq!(index.position(SRC, 21), Position { line: 2, column: 5 });
    }

    #[test]
    fn position_at_a_newline_stays_on_that_line() {
        let index = LineIndex::new(SRC);
        assert_eq!(index.position(SRC, 16), Position { line: 1, column: 17 });
    }

    #[test]
    fn position_past_the_end_clamps_to_the_end() {
        let index = LineIndex::new(SRC);
        let clamped = index.position(SRC, SRC.len() + 500);
        assert_eq!(clamped, index.position(SRC, SRC.len()));
    }

    #[test]
    fn column_counts_characters_not_bytes() {
        // Two two-byte characters before the target, so the byte offset and the
        // character column genuinely differ — with only one, they coincide and
        // the test cannot tell the two implementations apart.
        let source = "-- naïve chëck word\n";
        let index = LineIndex::new(source);
        let offset = source.find("word").expect("the fixture contains 'word'");
        assert_eq!(offset, 17, "the byte offset runs ahead of the character count");
        assert_eq!(index.position(source, offset), Position { line: 1, column: 16 });
    }

    #[test]
    fn a_column_inside_a_character_falls_back_to_the_byte_distance() {
        // A stale offset from a file that was edited under us can land mid
        // character. Slicing there yields nothing, so the column falls back to
        // the byte distance from the line start — which is only right if it is
        // measured from the line, not from the file.
        let source = "ab\ncé";
        let index = LineIndex::new(source);
        // Offset 5 splits the two bytes of `é` on line 2, which starts at 3.
        assert_eq!(index.position(source, 5), Position { line: 2, column: 3 });
    }

    #[test]
    fn empty_source_has_one_line() {
        let index = LineIndex::new("");
        assert_eq!(index.line_count(), 1);
        assert_eq!(index.position("", 0), Position { line: 1, column: 1 });
    }

    #[test]
    fn line_count_counts_a_trailing_newline_as_ending_the_last_line() {
        // "a\nb\n" is two lines of content, not three.
        assert_eq!(LineIndex::new("a\nb\n").line_count(), 3);
        assert_eq!(LineIndex::new("a\nb").line_count(), 2);
    }

    #[test]
    fn line_span_covers_the_line_and_its_newline() {
        let index = LineIndex::new(SRC);
        let span = index.line_span(1).expect("line 1 exists");
        assert_eq!(span.slice(SRC), Some("entity Message {\n"));
    }

    #[test]
    fn line_span_of_the_last_line_runs_to_the_end() {
        let source = "a\nbc";
        let index = LineIndex::new(source);
        let span = index.line_span(2).expect("line 2 exists");
        assert_eq!(span.slice(source), Some("bc"));
    }

    #[test]
    fn line_span_out_of_range_is_none() {
        let index = LineIndex::new(SRC);
        assert_eq!(index.line_span(0), None, "lines are one-based");
        assert_eq!(index.line_span(99), None);
    }
}
