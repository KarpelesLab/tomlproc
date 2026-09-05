//! Mapping TOML documents onto your own types, with [serde].
//!
//! Available with the `serde` feature, which is off by default -- without it
//! the crate has no dependencies at all.
//!
//! ```toml
//! [dependencies]
//! tomlproc = { version = "0.1", features = ["serde"] }
//! ```
//!
//! ```
//! # #[cfg(feature = "serde")] fn main() {
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Config {
//!     name: String,
//!     ports: Vec<u16>,
//!     #[serde(default)]
//!     verbose: bool,
//! }
//!
//! let config: Config = tomlproc::serde::from_str(r#"
//!     name = "alpha"
//!     ports = [8000, 8001]
//! "#).unwrap();
//!
//! assert_eq!(config, Config { name: "alpha".into(), ports: vec![8000, 8001], verbose: false });
//! assert_eq!(
//!     tomlproc::serde::to_string(&config).unwrap(),
//!     "name = \"alpha\"\nports = [8000, 8001]\nverbose = false\n",
//! );
//! # }
//! # #[cfg(not(feature = "serde"))] fn main() {}
//! ```
//!
//! # How TOML and serde line up
//!
//! - **`Option::None` is an absent key.** TOML has no null, so a `None` field
//!   is left out of its table rather than written, and a missing key
//!   deserializes back to `None`. A `None` with nowhere to be left out -- on
//!   its own, or inside an array -- is an error.
//! - **Enums are externally tagged**, as in every self-describing format: a
//!   variant with no payload is its bare name, and one with a payload is a
//!   one-key table, `{ variant = payload }`.
//! - **Map keys are strings**, so a map keyed by a number, character, bool or
//!   fieldless enum is written as text and read back out of it.
//! - **Date-times survive.** [`Datetime`](crate::Datetime) travels under a
//!   private newtype that this module's [`Serializer`] and [`Deserializer`]
//!   recognise, so a date-time stays a date-time; other formats see its text.
//! - **Strings are owned.** Deserialization reads out of a parsed
//!   [`Value`], so a `&'de str` field cannot borrow from the input -- use
//!   `String`.
//!
//! Errors from this module carry the [`key_path`](crate::Error::key_path) of
//! the value that would not fit:
//!
//! ```
//! # #[cfg(feature = "serde")] fn main() {
//! #[derive(serde::Deserialize, Debug)]
//! struct Config {
//!     server: Server,
//! }
//! #[derive(serde::Deserialize, Debug)]
//! struct Server {
//!     ports: Vec<u16>,
//! }
//!
//! let error = tomlproc::serde::from_str::<Config>("[server]\nports = [80, 'https']").unwrap_err();
//! assert_eq!(error.key_path().as_deref(), Some("server.ports.1"));
//! assert_eq!(
//!     error.to_string(),
//!     "TOML error at `server.ports.1`: invalid type: string \"https\", expected u16",
//! );
//! # }
//! # #[cfg(not(feature = "serde"))] fn main() {}
//! ```
//!
//! [serde]: https://serde.rs

mod de;
mod ser;

use alloc::format;
use alloc::string::String;

use ::serde::Serialize;
use ::serde::de::DeserializeOwned;

pub use self::de::Deserializer;
pub use self::ser::{
    SerializeArray, SerializeArrayVariant, SerializeTable, SerializeTableVariant, Serializer,
};

use crate::error::Error;
use crate::map::Table;
use crate::value::Value;

/// Parses a TOML document into a type.
///
/// ```
/// # #[cfg(feature = "serde")] fn main() {
/// #[derive(serde::Deserialize)]
/// struct Config {
///     port: u16,
/// }
///
/// let config: Config = tomlproc::serde::from_str("port = 8080").unwrap();
/// assert_eq!(config.port, 8080);
/// # }
/// # #[cfg(not(feature = "serde"))] fn main() {}
/// ```
pub fn from_str<T: DeserializeOwned>(input: &str) -> Result<T, Error> {
    from_table(crate::parse(input)?)
}

/// Parses a TOML document from bytes, which must be UTF-8, into a type.
pub fn from_slice<T: DeserializeOwned>(input: &[u8]) -> Result<T, Error> {
    from_table(crate::parse_bytes(input)?)
}

/// Converts an already-parsed document into a type.
pub fn from_table<T: DeserializeOwned>(table: Table) -> Result<T, Error> {
    from_value(Value::Table(table))
}

/// Converts an already-parsed value into a type.
///
/// ```
/// # #[cfg(feature = "serde")] fn main() {
/// let doc = tomlproc::parse("ports = [80, 443]").unwrap();
/// let ports: Vec<u16> = tomlproc::serde::from_value(doc["ports"].clone()).unwrap();
/// assert_eq!(ports, [80, 443]);
/// # }
/// # #[cfg(not(feature = "serde"))] fn main() {}
/// ```
pub fn from_value<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    T::deserialize(Deserializer::new(value))
}

/// Converts a value into a [`Value`].
///
/// ```
/// # #[cfg(feature = "serde")] fn main() {
/// let value = tomlproc::serde::to_value(&[1, 2, 3]).unwrap();
/// assert_eq!(value.to_string(), "[1, 2, 3]");
/// # }
/// # #[cfg(not(feature = "serde"))] fn main() {}
/// ```
pub fn to_value<T: ?Sized + Serialize>(value: &T) -> Result<Value, Error> {
    value
        .serialize(Serializer)?
        .ok_or_else(|| Error::custom("TOML has no null; a `None` cannot be written"))
}

/// Serializes a value as a TOML document.
///
/// The value has to become a table: a TOML document is a table, so a bare
/// integer or array has nowhere to go.
pub fn to_string<T: ?Sized + Serialize>(value: &T) -> Result<String, Error> {
    Ok(crate::to_string(&to_document(value)?))
}

/// Serializes a value as a TOML document, laid out for a human to read.
///
/// The same document [`to_string`] writes, formatted as
/// [`crate::to_string_pretty`] does.
pub fn to_string_pretty<T: ?Sized + Serialize>(value: &T) -> Result<String, Error> {
    Ok(crate::to_string_pretty(&to_document(value)?))
}

fn to_document<T: ?Sized + Serialize>(value: &T) -> Result<Table, Error> {
    match to_value(value)? {
        Value::Table(table) => Ok(table),
        other => Err(Error::custom(format!(
            "a TOML document is a table, but this serialized as {} {}",
            if other.is_array() { "an" } else { "a" },
            other.type_name(),
        ))),
    }
}
