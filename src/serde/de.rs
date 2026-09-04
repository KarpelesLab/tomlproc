//! Turning a [`Value`] into any [`Deserialize`] type.

use core::fmt;

use ::serde::de::{
    self, Deserialize, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use ::serde::forward_to_deserialize_any;

use crate::datetime::Datetime;
use crate::error::Error;
use crate::map::Table;
use crate::serde::ser::{DATETIME_FIELD, DATETIME_NAME};
use crate::value::Value;

type Result<T> = core::result::Result<T, Error>;

/// A [`serde::Deserializer`](::serde::Deserializer) reading out of a [`Value`].
///
/// Use [`from_str`](crate::serde::from_str) or
/// [`from_value`](crate::serde::from_value) rather than this directly, unless
/// you are writing your own `Deserialize` plumbing.
#[derive(Debug)]
pub struct Deserializer {
    value: Value,
}

impl Deserializer {
    /// Reads out of `value`.
    pub fn new(value: Value) -> Deserializer {
        Deserializer { value }
    }
}

impl IntoDeserializer<'_, Error> for Value {
    type Deserializer = Deserializer;

    fn into_deserializer(self) -> Deserializer {
        Deserializer::new(self)
    }
}

impl<'de> de::Deserializer<'de> for Deserializer {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.value {
            Value::String(value) => visitor.visit_string(value),
            Value::Integer(value) => visitor.visit_i64(value),
            Value::Float(value) => visitor.visit_f64(value),
            Value::Boolean(value) => visitor.visit_bool(value),
            // A date-time travels as a one-entry map under a private key, so
            // that a `Value` or a `Datetime` on the other side can pick it out
            // of an otherwise untyped stream.
            Value::Datetime(value) => visitor.visit_map(DatetimeAccess::new(value)),
            Value::Array(values) => visitor.visit_seq(ArrayAccess::new(values)),
            Value::Table(table) => visitor.visit_map(TableAccess::new(table)),
        }
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        // TOML has no null: a value that is here is a `Some`, and one that is
        // absent never reaches a deserializer at all.
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        if name == DATETIME_NAME {
            return self.deserialize_any(visitor);
        }
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        if name == DATETIME_NAME && !self.value.is_datetime() {
            return Err(de::Error::invalid_type(
                self.value.unexpected(),
                &"a TOML date-time",
            ));
        }
        self.deserialize_any(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        match self.value {
            // A variant with no payload is written as its bare name.
            Value::String(variant) => visitor.visit_enum(VariantSelector {
                variant,
                value: None,
            }),
            // Anything else is externally tagged: `{ variant = payload }`.
            Value::Table(table) if table.len() == 1 => {
                let (variant, value) = table.into_iter().next().expect("exactly one entry");
                visitor.visit_enum(VariantSelector {
                    variant,
                    value: Some(value),
                })
            }
            Value::Table(_) => Err(Error::custom(
                "an enum variant must be a table with exactly one key, or a bare variant name",
            )),
            value => Err(de::Error::invalid_type(
                value.unexpected(),
                &"an enum variant",
            )),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct seq tuple tuple_struct map identifier ignored_any
    }
}

impl Value {
    /// How this value is described in serde's "invalid type" messages.
    fn unexpected(&self) -> de::Unexpected<'_> {
        match self {
            Value::String(value) => de::Unexpected::Str(value),
            Value::Integer(value) => de::Unexpected::Signed(*value),
            Value::Float(value) => de::Unexpected::Float(*value),
            Value::Boolean(value) => de::Unexpected::Bool(*value),
            Value::Datetime(_) => de::Unexpected::Other("a date-time"),
            Value::Array(_) => de::Unexpected::Seq,
            Value::Table(_) => de::Unexpected::Map,
        }
    }
}

/// Walks a table's entries, tagging any error with the key it came from.
struct TableAccess {
    entries: crate::map::IntoIter,
    pending: Option<(String, Value)>,
}

impl TableAccess {
    fn new(table: Table) -> TableAccess {
        TableAccess {
            entries: table.into_iter(),
            pending: None,
        }
    }
}

impl<'de> MapAccess<'de> for TableAccess {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        match self.entries.next() {
            Some((key, value)) => {
                let deserialized = seed.deserialize(KeyDeserializer { key: key.clone() })?;
                self.pending = Some((key, value));
                Ok(Some(deserialized))
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        let (key, value) = self.pending.take().expect("serde asks for the key first");
        seed.deserialize(Deserializer::new(value))
            .map_err(|mut error| {
                error.prepend_key(key);
                error
            })
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len())
    }
}

/// Walks an array, tagging any error with the index it came from.
struct ArrayAccess {
    items: std::vec::IntoIter<Value>,
    index: usize,
}

impl ArrayAccess {
    fn new(items: Vec<Value>) -> ArrayAccess {
        ArrayAccess {
            items: items.into_iter(),
            index: 0,
        }
    }
}

impl<'de> SeqAccess<'de> for ArrayAccess {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(&mut self, seed: T) -> Result<Option<T::Value>> {
        let Some(value) = self.items.next() else {
            return Ok(None);
        };
        let index = self.index;
        self.index += 1;
        seed.deserialize(Deserializer::new(value))
            .map(Some)
            .map_err(|mut error| {
                error.prepend_key(index.to_string());
                error
            })
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len())
    }
}

/// Presents a date-time as the one-entry map described on [`DATETIME_FIELD`].
struct DatetimeAccess {
    datetime: Datetime,
    done: bool,
}

impl DatetimeAccess {
    fn new(datetime: Datetime) -> DatetimeAccess {
        DatetimeAccess {
            datetime,
            done: false,
        }
    }
}

impl<'de> MapAccess<'de> for DatetimeAccess {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.done {
            return Ok(None);
        }
        self.done = true;
        seed.deserialize(KeyDeserializer {
            key: DATETIME_FIELD.to_owned(),
        })
        .map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        seed.deserialize(Deserializer::new(Value::String(self.datetime.to_string())))
    }
}

/// Deserializes a table key.
///
/// Keys are always strings in TOML, but a map may be keyed by a number, a
/// character, a bool or a fieldless enum; each is read back out of its text.
struct KeyDeserializer {
    key: String,
}

macro_rules! key_from_str {
    ($($method:ident => $visit:ident($ty:ty)),* $(,)?) => {$(
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
            match self.key.parse::<$ty>() {
                Ok(value) => visitor.$visit(value),
                Err(_) => Err(Error::custom(format!(
                    "the key `{}` is not {}",
                    self.key,
                    stringify!($ty),
                ))),
            }
        }
    )*};
}

impl<'de> de::Deserializer<'de> for KeyDeserializer {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_string(self.key)
    }

    key_from_str! {
        deserialize_bool => visit_bool(bool),
        deserialize_i8 => visit_i8(i8),
        deserialize_i16 => visit_i16(i16),
        deserialize_i32 => visit_i32(i32),
        deserialize_i64 => visit_i64(i64),
        deserialize_u8 => visit_u8(u8),
        deserialize_u16 => visit_u16(u16),
        deserialize_u32 => visit_u32(u32),
        deserialize_u64 => visit_u64(u64),
        deserialize_f32 => visit_f32(f32),
        deserialize_f64 => visit_f64(f64),
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_enum(VariantSelector {
            variant: self.key,
            value: None,
        })
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_some(self)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    forward_to_deserialize_any! {
        i128 u128 char str string bytes byte_buf unit unit_struct seq tuple tuple_struct map
        struct identifier ignored_any
    }
}

/// The chosen variant of an enum, and its payload if it has one.
struct VariantSelector {
    variant: String,
    value: Option<Value>,
}

impl<'de> EnumAccess<'de> for VariantSelector {
    type Error = Error;
    type Variant = VariantPayload;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, VariantPayload)> {
        let variant = seed.deserialize(KeyDeserializer { key: self.variant })?;
        Ok((variant, VariantPayload { value: self.value }))
    }
}

/// The payload of a chosen variant.
struct VariantPayload {
    value: Option<Value>,
}

impl VariantPayload {
    fn value(self, expected: &str) -> Result<Value> {
        self.value.ok_or_else(|| {
            Error::custom(format!("expected {expected}, but the variant has no value"))
        })
    }
}

impl<'de> VariantAccess<'de> for VariantPayload {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        match self.value {
            None => Ok(()),
            Some(_) => Err(Error::custom(
                "this variant takes no value, but one was given",
            )),
        }
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value> {
        seed.deserialize(Deserializer::new(self.value("a value")?))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value> {
        use ::serde::Deserializer as _;

        Deserializer::new(self.value("a tuple")?).deserialize_seq(visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        use ::serde::Deserializer as _;

        Deserializer::new(self.value("a struct")?).deserialize_map(visitor)
    }
}

// ----- the value model's own impls ----------------------------------------

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: ::serde::Deserializer<'de>>(
        deserializer: D,
    ) -> core::result::Result<Value, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any TOML value")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> core::result::Result<Value, E> {
        Ok(Value::Boolean(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> core::result::Result<Value, E> {
        Ok(Value::Integer(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> core::result::Result<Value, E> {
        match i64::try_from(value) {
            Ok(value) => Ok(Value::Integer(value)),
            Err(_) => Err(E::custom(format!(
                "{value} is out of the range of a TOML integer (a signed 64-bit integer)"
            ))),
        }
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> core::result::Result<Value, E> {
        Ok(Value::Float(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> core::result::Result<Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> core::result::Result<Value, E> {
        Ok(Value::String(value))
    }

    fn visit_some<D: ::serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> core::result::Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_newtype_struct<D: ::serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> core::result::Result<Value, D::Error> {
        Value::deserialize(deserializer)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> core::result::Result<Value, A::Error> {
        let mut values = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(value) = seq.next_element()? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> core::result::Result<Value, A::Error> {
        let Some(first) = map.next_key::<String>()? else {
            return Ok(Value::Table(Table::new()));
        };
        // A date-time arrives as a one-entry map under a private key.
        if first == DATETIME_FIELD {
            let text: String = map.next_value()?;
            return text.parse().map(Value::Datetime).map_err(de::Error::custom);
        }
        let mut table = Table::with_capacity(map.size_hint().unwrap_or(0) + 1);
        table.insert(first, map.next_value::<Value>()?);
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            table.insert(key, value);
        }
        Ok(Value::Table(table))
    }
}

impl<'de> Deserialize<'de> for Table {
    fn deserialize<D: ::serde::Deserializer<'de>>(
        deserializer: D,
    ) -> core::result::Result<Table, D::Error> {
        match Value::deserialize(deserializer)? {
            Value::Table(table) => Ok(table),
            other => Err(de::Error::invalid_type(other.unexpected(), &"a TOML table")),
        }
    }
}

impl<'de> Deserialize<'de> for Datetime {
    fn deserialize<D: ::serde::Deserializer<'de>>(
        deserializer: D,
    ) -> core::result::Result<Datetime, D::Error> {
        deserializer.deserialize_struct(DATETIME_NAME, &[DATETIME_FIELD], DatetimeVisitor)
    }
}

struct DatetimeVisitor;

impl<'de> Visitor<'de> for DatetimeVisitor {
    type Value = Datetime;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a TOML date-time")
    }

    /// From another format, a date-time is just its text.
    fn visit_str<E: de::Error>(self, value: &str) -> core::result::Result<Datetime, E> {
        value.parse().map_err(E::custom)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> core::result::Result<Datetime, A::Error> {
        let Some(key) = map.next_key::<String>()? else {
            return Err(de::Error::custom(
                "expected a TOML date-time, found an empty table",
            ));
        };
        if key != DATETIME_FIELD {
            return Err(de::Error::custom(
                "expected a TOML date-time, found a table",
            ));
        }
        let text: String = map.next_value()?;
        text.parse().map_err(de::Error::custom)
    }
}
