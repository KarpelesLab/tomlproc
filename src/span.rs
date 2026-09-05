//! Where each value was written in the source.

use alloc::string::String;
use core::ops::Range;

use crate::collections::Map;

/// The stretch of source a value came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Byte range of the whole `key = value` pair, or of the `[header]` line
    /// for a table.
    pub range: Range<usize>,
    /// Byte range of just the value. For a table header this is the same as
    /// `range`.
    pub value: Range<usize>,
    /// The 1-based line the pair starts on.
    pub line: usize,
    /// The 1-based column, counted in characters, the pair starts at.
    pub column: usize,
}

/// Where every value in a document was written, keyed by its dotted path.
///
/// Returned by [`parse_spans`](crate::parse_spans). Paths are built the same
/// way [`Error::key_path`](crate::Error::key_path) builds them -- key names
/// joined with `.`, array elements as their index -- so an error from the
/// `serde` integration can be pointed straight back at the source:
///
/// ```
/// # #[cfg(feature = "serde")] fn main() {
/// #[derive(serde::Deserialize, Debug)]
/// struct Config {
///     port: u16,
/// }
///
/// let source = "# a comment\nport = 99999\n";
/// let (doc, spans) = tomlproc::parse_spans(source).unwrap();
///
/// let error = tomlproc::serde::from_table::<Config>(doc).unwrap_err();
/// let span = spans.get(&error.key_path().unwrap()).unwrap();
///
/// assert_eq!(span.line, 2);
/// assert_eq!(&source[span.value.clone()], "99999");
/// # }
/// # #[cfg(not(feature = "serde"))] fn main() {}
/// ```
///
/// A key that itself contains a `.` cannot be told apart from a path, the same
/// limitation [`Table::get_path`](crate::Table::get_path) has.
#[derive(Debug, Clone, Default)]
pub struct Spans {
    entries: Map<String, Span>,
}

impl Spans {
    /// Looks up the span for a dotted path.
    ///
    /// Spans are recorded for each key/value pair and each table header, so a
    /// path that points *inside* a value -- an array element, say -- has no
    /// span of its own. Rather than return nothing, this falls back to the
    /// nearest enclosing value, which is the one worth underlining anyway.
    pub fn get(&self, path: &str) -> Option<&Span> {
        if let Some(span) = self.entries.get(path) {
            return Some(span);
        }
        let mut path = path;
        while let Some((prefix, _)) = path.rsplit_once('.') {
            if let Some(span) = self.entries.get(prefix) {
                return Some(span);
            }
            path = prefix;
        }
        None
    }

    /// Looks up the span for a path, without falling back to an enclosing one.
    pub fn get_exact(&self, path: &str) -> Option<&Span> {
        self.entries.get(path)
    }

    /// The number of recorded spans.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every path and its span, in no particular order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Span)> {
        self.entries
            .iter()
            .map(|(path, span)| (path.as_str(), span))
    }

    pub(crate) fn record(&mut self, path: String, span: Span) {
        self.entries.insert(path, span);
    }
}
