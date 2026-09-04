//! A self-contained [TOML 1.0.0](https://toml.io/en/v1.0.0) parser and
//! serializer.
//!
//! `tomlproc` implements the whole of TOML 1.0.0 -- every string flavour, all
//! four date-time types, dotted keys, inline tables and arrays of tables --
//! with no dependencies outside the standard library.
//!
//! # Parsing
//!
//! [`parse`] turns a document into a [`Table`], an insertion-ordered map of
//! [`Value`]s:
//!
//! ```
//! let doc = tomlproc::parse(r#"
//!     title = "TOML Example"
//!
//!     [owner]
//!     name = "Tom Preston-Werner"
//!     dob = 1979-05-27T07:32:00-08:00
//!
//!     [[server]]
//!     ip = "10.0.0.1"
//!     ports = [8000, 8001]
//! "#).unwrap();
//!
//! assert_eq!(doc["title"].as_str(), Some("TOML Example"));
//! assert_eq!(doc["owner"]["dob"].as_datetime().unwrap().date.unwrap().year, 1979);
//! assert_eq!(doc["server"][0]["ports"][1].as_integer(), Some(8001));
//! ```
//!
//! Errors carry the line and column at which the problem was found:
//!
//! ```
//! let error = tomlproc::parse("a = 1\nb = [1, 2").unwrap_err();
//! assert_eq!(error.line(), 2);
//! assert_eq!(error.to_string(), "TOML parse error at line 2, column 5: unterminated array");
//! ```
//!
//! # Building and writing
//!
//! Tables can be built by hand and written back out with [`to_string`]:
//!
//! ```
//! let mut package = tomlproc::Table::new();
//! package.insert("name", "tomlproc");
//! package.insert("edition", "2024");
//!
//! let mut doc = tomlproc::Table::new();
//! doc.insert("package", package);
//!
//! assert_eq!(tomlproc::to_string(&doc), "[package]\nname = \"tomlproc\"\nedition = \"2024\"\n");
//! ```
//!
//! Parsing and serializing round-trip: key order, and the shape of tables and
//! arrays of tables, are preserved. Formatting is not -- comments, blank lines
//! and the choice between a header and an inline table belong to the document,
//! not to the value model.
//!
//! # Conformance
//!
//! The parser is strict, and rejects what the specification calls invalid:
//! duplicate keys, extending an inline table, redefining a table, mismatched
//! quotes, out-of-range integers and dates, bad underscore or leading-zero
//! placement in numbers, control characters in strings and comments, and
//! newlines inside inline tables. A bare carriage return is an error; `\r\n`
//! in a multi-line string is normalized to `\n`, as the specification permits.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod datetime;
mod error;
mod map;
mod parser;
mod ser;
mod value;

pub use crate::datetime::{Date, Datetime, DatetimeKind, Offset, Time};
pub use crate::error::Error;
pub use crate::map::{IntoIter, Iter, IterMut, Keys, Table, Values, ValuesMut};
pub use crate::ser::to_string;
pub use crate::value::Value;

/// Parses a TOML document.
///
/// ```
/// let doc = tomlproc::parse("key = \"value\"").unwrap();
/// assert_eq!(doc["key"].as_str(), Some("value"));
/// ```
pub fn parse(input: &str) -> Result<Table, Error> {
    parser::Parser::new(input).parse()
}

impl core::str::FromStr for Table {
    type Err = Error;

    /// Parses a TOML document; the same as [`parse`].
    fn from_str(s: &str) -> Result<Table, Error> {
        parse(s)
    }
}
