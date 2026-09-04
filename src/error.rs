//! The error type returned by parsing and serialization.

use core::fmt;

/// An error produced while parsing or serializing TOML.
///
/// Parse errors carry the 1-based [`line`](Error::line) and
/// [`column`](Error::column) at which the problem was found, plus the byte
/// [`offset`](Error::offset) into the input. Errors that are not tied to a
/// position in a document (for instance, trying to serialize a value that is
/// not a table) report a line of `0`; see [`Error::has_position`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
    line: usize,
    column: usize,
    offset: usize,
}

impl Error {
    /// Builds an error anchored at a position in the source document.
    pub(crate) fn parse(
        message: impl Into<String>,
        line: usize,
        column: usize,
        offset: usize,
    ) -> Self {
        Error {
            message: message.into(),
            line,
            column,
            offset,
        }
    }

    /// Builds an error that is not tied to a position in a document.
    pub(crate) fn custom(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            line: 0,
            column: 0,
            offset: 0,
        }
    }

    /// The human-readable description of what went wrong, without any position
    /// prefix.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The 1-based line at which the error was detected, or `0` if the error is
    /// not tied to a position.
    pub fn line(&self) -> usize {
        self.line
    }

    /// The 1-based column, counted in characters, at which the error was
    /// detected, or `0` if the error is not tied to a position.
    pub fn column(&self) -> usize {
        self.column
    }

    /// The byte offset into the input at which the error was detected, or `0`
    /// if the error is not tied to a position.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Whether this error points at a specific location in a document.
    pub fn has_position(&self) -> bool {
        self.line != 0
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.has_position() {
            write!(
                f,
                "TOML parse error at line {}, column {}: {}",
                self.line, self.column, self.message
            )
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for Error {}
