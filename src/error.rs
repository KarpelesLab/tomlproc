//! The error type returned by parsing and serialization.

use core::fmt;

/// An error produced while parsing or serializing TOML.
///
/// Parse errors carry the 1-based [`line`](Error::line) and
/// [`column`](Error::column) at which the problem was found, plus the byte
/// [`offset`](Error::offset) into the input. Errors that are not tied to a
/// position in a document report a line of `0`; see [`Error::has_position`].
///
/// Errors raised while mapping a document onto a Rust type instead carry the
/// [`key_path`](Error::key_path) of the value that would not fit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
    line: usize,
    column: usize,
    offset: usize,
    /// The keys, outermost first, leading to the value this error is about.
    /// Filled in as the error travels back out of a nested value.
    path: Vec<String>,
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
            path: Vec::new(),
        }
    }

    /// Builds an error that is not tied to a position in a document.
    pub(crate) fn custom(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            line: 0,
            column: 0,
            offset: 0,
            path: Vec::new(),
        }
    }

    /// Records that this error is about a value found under `key`.
    ///
    /// Called as the error travels back out of a nested value, so the
    /// outermost key ends up first.
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    pub(crate) fn prepend_key(&mut self, key: impl Into<String>) {
        self.path.insert(0, key.into());
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

    /// The dotted path of the key whose value this error is about, if it is
    /// about one -- `Some("servers.alpha.port")`, say.
    ///
    /// Array elements appear as their index. Only errors from the `serde`
    /// integration carry a path.
    pub fn key_path(&self) -> Option<String> {
        if self.path.is_empty() {
            None
        } else {
            Some(self.path.join("."))
        }
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
        } else if let Some(path) = self.key_path() {
            write!(f, "TOML error at `{path}`: {}", self.message)
        } else {
            f.write_str(&self.message)
        }
    }
}

impl std::error::Error for Error {}

#[cfg(feature = "serde")]
impl ::serde::de::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Error {
        Error::custom(message.to_string())
    }
}

#[cfg(feature = "serde")]
impl ::serde::ser::Error for Error {
    fn custom<T: fmt::Display>(message: T) -> Error {
        Error::custom(message.to_string())
    }
}
