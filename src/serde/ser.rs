//! Turning any [`Serialize`] value into a [`Value`].

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use ::serde::ser::{self, Serialize};

use crate::datetime::Datetime;
use crate::error::Error;
use crate::map::Table;
use crate::value::Value;

/// The name of the newtype struct a [`Datetime`] serializes itself as.
///
/// A TOML date-time is not a string, but serde has no date type, so
/// [`Datetime`] wraps its text in a newtype struct under a name no real type
/// would use. [`Serializer`] recognises the name and produces a real
/// [`Value::Datetime`]; every other serializer sees straight through the
/// newtype to the string.
pub(crate) const DATETIME_NAME: &str = "$__tomlproc_private_Datetime";

/// The field name carrying a date-time when it travels as a map.
pub(crate) const DATETIME_FIELD: &str = "$__tomlproc_private_datetime";

type Result<T> = core::result::Result<T, Error>;

/// A [`serde::Serializer`](::serde::Serializer) that produces a [`Value`].
///
/// Use [`to_value`](crate::serde::to_value) rather than this directly, unless
/// you are writing your own `Serialize` plumbing.
#[derive(Debug, Clone, Copy)]
pub struct Serializer;

/// `None` means "there is nothing to write here": TOML has no null, so an
/// `Option::None` is dropped by the table that holds it rather than written.
type Produced = Option<Value>;

impl ser::Serializer for Serializer {
    type Ok = Produced;
    type Error = Error;
    type SerializeSeq = SerializeArray;
    type SerializeTuple = SerializeArray;
    type SerializeTupleStruct = SerializeArray;
    type SerializeTupleVariant = SerializeArrayVariant;
    type SerializeMap = SerializeTable;
    type SerializeStruct = SerializeTable;
    type SerializeStructVariant = SerializeTableVariant;

    fn serialize_bool(self, value: bool) -> Result<Produced> {
        Ok(Some(Value::Boolean(value)))
    }

    fn serialize_i8(self, value: i8) -> Result<Produced> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i16(self, value: i16) -> Result<Produced> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i32(self, value: i32) -> Result<Produced> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_i64(self, value: i64) -> Result<Produced> {
        Ok(Some(Value::Integer(value)))
    }

    fn serialize_u8(self, value: u8) -> Result<Produced> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_u16(self, value: u16) -> Result<Produced> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_u32(self, value: u32) -> Result<Produced> {
        self.serialize_i64(i64::from(value))
    }

    fn serialize_u64(self, value: u64) -> Result<Produced> {
        // TOML integers are signed 64-bit; anything larger has no home.
        match i64::try_from(value) {
            Ok(value) => self.serialize_i64(value),
            Err(_) => Err(Error::custom(format!(
                "{value} is out of the range of a TOML integer (a signed 64-bit integer)"
            ))),
        }
    }

    fn serialize_f32(self, value: f32) -> Result<Produced> {
        self.serialize_f64(f64::from(value))
    }

    fn serialize_f64(self, value: f64) -> Result<Produced> {
        Ok(Some(Value::Float(value)))
    }

    fn serialize_char(self, value: char) -> Result<Produced> {
        Ok(Some(Value::String(value.to_string())))
    }

    fn serialize_str(self, value: &str) -> Result<Produced> {
        Ok(Some(Value::String(value.to_owned())))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Produced> {
        Err(Error::custom(
            "TOML has no byte-string type; serialize bytes as an array of integers or as text",
        ))
    }

    fn serialize_none(self) -> Result<Produced> {
        Ok(None)
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Produced> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Produced> {
        Err(Error::custom(
            "TOML has no null; a unit value cannot be written",
        ))
    }

    fn serialize_unit_struct(self, name: &'static str) -> Result<Produced> {
        Err(Error::custom(format!(
            "TOML has no null; the unit struct `{name}` cannot be written"
        )))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Produced> {
        // A variant with no payload is just its name.
        Ok(Some(Value::String(variant.to_owned())))
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Produced> {
        if name == DATETIME_NAME {
            let text = value.serialize(KeySerializer)?;
            return Ok(Some(Value::Datetime(text.parse::<Datetime>()?)));
        }
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Produced> {
        // The externally tagged form every self-describing format uses:
        // `{ variant = payload }`.
        let mut table = Table::new();
        table.insert(variant, expect_value(value.serialize(self)?)?);
        Ok(Some(Value::Table(table)))
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<SerializeArray> {
        Ok(SerializeArray {
            items: Vec::with_capacity(len.unwrap_or(0)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<SerializeArray> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SerializeArray> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<SerializeArrayVariant> {
        Ok(SerializeArrayVariant {
            variant,
            array: SerializeArray {
                items: Vec::with_capacity(len),
            },
        })
    }

    fn serialize_map(self, len: Option<usize>) -> Result<SerializeTable> {
        Ok(SerializeTable {
            table: Table::with_capacity(len.unwrap_or(0)),
            key: None,
        })
    }

    fn serialize_struct(self, _name: &'static str, len: usize) -> Result<SerializeTable> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<SerializeTableVariant> {
        Ok(SerializeTableVariant {
            variant,
            table: SerializeTable {
                table: Table::with_capacity(len),
                key: None,
            },
        })
    }
}

/// Rejects the `None` that a lone `Option::None` produces, in the places where
/// there is no key to drop.
fn expect_value(produced: Produced) -> Result<Value> {
    produced.ok_or_else(|| Error::custom("TOML has no null; a `None` cannot be written here"))
}

/// Collects the elements of a sequence, tuple or tuple struct.
#[derive(Debug)]
pub struct SerializeArray {
    items: Vec<Value>,
}

impl SerializeArray {
    fn push<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        let value = value.serialize(Serializer)?;
        // Unlike a table, an array has no key to leave out.
        self.items.push(expect_value(value)?);
        Ok(())
    }
}

impl ser::SerializeSeq for SerializeArray {
    type Ok = Produced;
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Produced> {
        Ok(Some(Value::Array(self.items)))
    }
}

impl ser::SerializeTuple for SerializeArray {
    type Ok = Produced;
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Produced> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SerializeArray {
    type Ok = Produced;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        self.push(value)
    }

    fn end(self) -> Result<Produced> {
        ser::SerializeSeq::end(self)
    }
}

/// Collects a tuple variant into `{ variant = [...] }`.
#[derive(Debug)]
pub struct SerializeArrayVariant {
    variant: &'static str,
    array: SerializeArray,
}

impl ser::SerializeTupleVariant for SerializeArrayVariant {
    type Ok = Produced;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        self.array.push(value)
    }

    fn end(self) -> Result<Produced> {
        let mut table = Table::new();
        table.insert(self.variant, Value::Array(self.array.items));
        Ok(Some(Value::Table(table)))
    }
}

/// Collects the entries of a map or struct.
#[derive(Debug)]
pub struct SerializeTable {
    table: Table,
    key: Option<String>,
}

impl SerializeTable {
    /// Stores a pair, dropping it if the value is a `None`: an absent key is
    /// how TOML says "no value".
    fn insert<T: ?Sized + Serialize>(&mut self, key: String, value: &T) -> Result<()> {
        let value = value.serialize(Serializer).map_err(|mut error| {
            error.prepend_key(&key);
            error
        })?;
        if let Some(value) = value {
            self.table.insert(key, value);
        }
        Ok(())
    }
}

impl ser::SerializeMap for SerializeTable {
    type Ok = Produced;
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        self.key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        let key = self.key.take().expect("serde calls serialize_key first");
        self.insert(key, value)
    }

    fn end(self) -> Result<Produced> {
        Ok(Some(Value::Table(self.table)))
    }
}

impl ser::SerializeStruct for SerializeTable {
    type Ok = Produced;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.insert(key.to_owned(), value)
    }

    fn end(self) -> Result<Produced> {
        ser::SerializeMap::end(self)
    }
}

/// Collects a struct variant into `{ variant = { ... } }`.
#[derive(Debug)]
pub struct SerializeTableVariant {
    variant: &'static str,
    table: SerializeTable,
}

impl ser::SerializeStructVariant for SerializeTableVariant {
    type Ok = Produced;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        self.table.insert(key.to_owned(), value)
    }

    fn end(self) -> Result<Produced> {
        let mut table = Table::new();
        table.insert(self.variant, Value::Table(self.table.table));
        Ok(Some(Value::Table(table)))
    }
}

/// Renders a map key as a string, which is all TOML keys can be.
///
/// Numbers, characters, booleans and unit variants are rendered rather than
/// refused, so a `HashMap<u32, _>` or a map keyed by a fieldless enum
/// serializes cleanly.
struct KeySerializer;

macro_rules! key_from_display {
    ($($method:ident($ty:ty)),* $(,)?) => {$(
        fn $method(self, value: $ty) -> Result<String> {
            Ok(value.to_string())
        }
    )*};
}

impl ser::Serializer for KeySerializer {
    type Ok = String;
    type Error = Error;
    type SerializeSeq = ser::Impossible<String, Error>;
    type SerializeTuple = ser::Impossible<String, Error>;
    type SerializeTupleStruct = ser::Impossible<String, Error>;
    type SerializeTupleVariant = ser::Impossible<String, Error>;
    type SerializeMap = ser::Impossible<String, Error>;
    type SerializeStruct = ser::Impossible<String, Error>;
    type SerializeStructVariant = ser::Impossible<String, Error>;

    key_from_display! {
        serialize_bool(bool),
        serialize_i8(i8),
        serialize_i16(i16),
        serialize_i32(i32),
        serialize_i64(i64),
        serialize_u8(u8),
        serialize_u16(u16),
        serialize_u32(u32),
        serialize_u64(u64),
        serialize_char(char),
        serialize_str(&str),
    }

    fn serialize_f32(self, _value: f32) -> Result<String> {
        Err(key_error("a float"))
    }

    fn serialize_f64(self, _value: f64) -> Result<String> {
        Err(key_error("a float"))
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<String> {
        Err(key_error("a byte string"))
    }

    fn serialize_none(self) -> Result<String> {
        Err(key_error("none"))
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<String> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<String> {
        Err(key_error("a unit value"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<String> {
        Err(key_error("a unit struct"))
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<String> {
        Ok(variant.to_owned())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<String> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<String> {
        Err(key_error("an enum variant with a payload"))
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Err(key_error("a sequence"))
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Err(key_error("a tuple"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Err(key_error("a tuple struct"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        Err(key_error("a tuple variant"))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Err(key_error("a map"))
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Err(key_error("a struct"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Err(key_error("a struct variant"))
    }
}

fn key_error(what: &str) -> Error {
    Error::custom(format!("a TOML key must be a string, but {what} was given"))
}

// ----- the value model's own impls ----------------------------------------

impl Serialize for Value {
    fn serialize<S: ::serde::Serializer>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error> {
        match self {
            Value::String(value) => serializer.serialize_str(value),
            Value::Integer(value) => serializer.serialize_i64(*value),
            Value::Float(value) => serializer.serialize_f64(*value),
            Value::Boolean(value) => serializer.serialize_bool(*value),
            Value::Datetime(value) => value.serialize(serializer),
            Value::Array(values) => values.serialize(serializer),
            Value::Table(table) => table.serialize(serializer),
        }
    }
}

impl Serialize for Table {
    fn serialize<S: ::serde::Serializer>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error> {
        use ::serde::ser::SerializeMap as _;

        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (key, value) in self {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl Serialize for Datetime {
    fn serialize<S: ::serde::Serializer>(
        &self,
        serializer: S,
    ) -> core::result::Result<S::Ok, S::Error> {
        // Wrapped in a newtype only this crate's `Serializer` looks for, so a
        // date-time stays a date-time here and becomes a string elsewhere.
        serializer.serialize_newtype_struct(DATETIME_NAME, &self.to_string())
    }
}
