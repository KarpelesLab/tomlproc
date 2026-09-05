//! The error type returned by parsing and serialization.

use core::fmt;

#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "serde")]
use alloc::string::ToString;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;

/// What an error's message is made of.
///
/// With an allocator the parser builds messages that name the offending key;
/// without one, every message it can produce is a fixed string.
#[cfg(feature = "alloc")]
type Message = String;
#[cfg(not(feature = "alloc"))]
type Message = &'static str;

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
    message: Message,
    line: usize,
    column: usize,
    offset: usize,
    /// The keys, outermost first, leading to the value this error is about.
    /// Filled in as the error travels back out of a nested value.
    #[cfg(feature = "alloc")]
    path: Vec<String>,
}

impl Error {
    /// Builds an error anchored at a position in the source document.
    #[cfg(feature = "alloc")]
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

    /// Builds an error that is not tied to a position in a document, with a
    /// message built at runtime. Only the `serde` integration needs one.
    #[cfg(feature = "serde")]
    pub(crate) fn custom(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
            line: 0,
            column: 0,
            offset: 0,
            path: Vec::new(),
        }
    }

    /// Builds an error whose message is fixed, which is all the parts of the
    /// crate that run without an allocator can produce.
    pub(crate) fn fixed(message: &'static str) -> Self {
        Error {
            #[cfg(feature = "alloc")]
            message: String::from(message),
            #[cfg(not(feature = "alloc"))]
            message,
            line: 0,
            column: 0,
            offset: 0,
            #[cfg(feature = "alloc")]
            path: Vec::new(),
        }
    }

    /// Records that this error is about a value found under `key`.
    ///
    /// Called as the error travels back out of a nested value, so the
    /// outermost key ends up first.
    #[cfg(feature = "alloc")]
    #[cfg_attr(not(feature = "serde"), allow(dead_code))]
    pub(crate) fn prepend_key(&mut self, key: impl Into<String>) {
        self.path.insert(0, key.into());
    }

    /// The human-readable description of what went wrong, without any position
    /// prefix.
    pub fn message(&self) -> &str {
        self.message_str()
    }

    /// The message as a `&str`, whichever of the two forms it is stored in.
    #[cfg(feature = "alloc")]
    fn message_str(&self) -> &str {
        self.message.as_str()
    }

    #[cfg(not(feature = "alloc"))]
    fn message_str(&self) -> &str {
        self.message
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
    #[cfg(feature = "alloc")]
    #[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
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
            return write!(
                f,
                "TOML parse error at line {}, column {}: {}",
                self.line,
                self.column,
                self.message_str()
            );
        }
        #[cfg(feature = "alloc")]
        if let Some(path) = self.key_path() {
            return write!(f, "TOML error at `{path}`: {}", self.message_str());
        }
        f.write_str(self.message_str())
    }
}

impl core::error::Error for Error {}

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
