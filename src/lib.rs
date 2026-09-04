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
//! # Beyond the value model
//!
//! [`parse_spans`] also reports where each value was written, so a value that
//! turns out to be wrong later can be pointed back at its line and column.
//!
//! The optional `serde` feature adds [`tomlproc::serde`](crate::serde), which
//! maps documents onto your own types. It is off by default, and it is the
//! only thing that gives the crate a dependency.
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
// On docs.rs, mark what the `serde` feature adds.
#![cfg_attr(docsrs, feature(doc_cfg))]

mod datetime;
mod error;
mod macros;
mod map;
mod parser;
mod ser;
#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
pub mod serde;
mod span;
mod value;

pub use crate::datetime::{Date, Datetime, DatetimeKind, Offset, Time};
pub use crate::error::Error;
pub use crate::map::{
    Entry, IntoIter, Iter, IterMut, Keys, OccupiedEntry, Table, VacantEntry, Values, ValuesMut,
};
pub use crate::ser::{to_string, to_string_pretty};
pub use crate::span::{Span, Spans};
pub use crate::value::Value;

/// Parses a TOML document.
///
/// ```
/// let doc = tomlproc::parse("key = \"value\"").unwrap();
/// assert_eq!(doc["key"].as_str(), Some("value"));
/// ```
pub fn parse(input: &str) -> Result<Table, Error> {
    Ok(parser::Parser::new(input, false).parse()?.0)
}

/// Parses a TOML document, also reporting where each value was written.
///
/// The [`Spans`] are keyed by dotted path, the same way
/// [`Error::key_path`] spells one, so a value that later turns out to be
/// wrong can be pointed back at its place in the source. Recording them costs
/// a little time and memory, which is why [`parse`] does not.
///
/// ```
/// let source = "[server]\nport = 8080\n";
/// let (doc, spans) = tomlproc::parse_spans(source).unwrap();
///
/// assert_eq!(doc["server"]["port"].as_integer(), Some(8080));
///
/// let span = spans.get("server.port").unwrap();
/// assert_eq!((span.line, span.column), (2, 1));
/// assert_eq!(&source[span.value.clone()], "8080");
/// assert_eq!(&source[span.range.clone()], "port = 8080");
/// ```
pub fn parse_spans(input: &str) -> Result<(Table, Spans), Error> {
    let (table, spans) = parser::Parser::new(input, true).parse()?;
    Ok((table, spans.expect("spans were asked for")))
}

/// Parses a TOML document from bytes, which must be UTF-8.
///
/// ```
/// let doc = tomlproc::parse_bytes(b"key = 'value'").unwrap();
/// assert_eq!(doc["key"].as_str(), Some("value"));
///
/// let error = tomlproc::parse_bytes(b"key = 'v\xff'").unwrap_err();
/// assert_eq!(error.to_string(), "TOML parse error at line 1, column 9: input is not valid UTF-8");
/// ```
pub fn parse_bytes(input: &[u8]) -> Result<Table, Error> {
    match core::str::from_utf8(input) {
        Ok(input) => parse(input),
        Err(error) => {
            // Report where the bad byte is, in the same shape as a syntax
            // error, by measuring the part that did decode.
            let offset = error.valid_up_to();
            let valid = core::str::from_utf8(&input[..offset]).expect("valid up to here");
            let line = valid.bytes().filter(|c| *c == b'\n').count() + 1;
            let column = valid
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                + 1;
            Err(Error::parse(
                "input is not valid UTF-8",
                line,
                column,
                offset,
            ))
        }
    }
}

impl core::str::FromStr for Table {
    type Err = Error;

    /// Parses a TOML document; the same as [`parse`].
    fn from_str(s: &str) -> Result<Table, Error> {
        parse(s)
    }
}
