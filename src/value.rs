//! The TOML value model.

use core::fmt;
use core::ops::{Index, IndexMut};

use crate::datetime::{Date, Datetime, Time};
use crate::map::Table;

/// Any TOML value.
///
/// ```
/// use tomlproc::Value;
///
/// let table = tomlproc::parse("port = 8080\nhosts = [\"a\", \"b\"]").unwrap();
/// assert_eq!(table["port"].as_integer(), Some(8080));
/// assert_eq!(table["hosts"][1].as_str(), Some("b"));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A string.
    String(String),
    /// A 64-bit signed integer.
    Integer(i64),
    /// A 64-bit float.
    Float(f64),
    /// A boolean.
    Boolean(bool),
    /// A date-time, in one of TOML's four flavours.
    Datetime(Datetime),
    /// An array. TOML 1.0 arrays may mix value types.
    Array(Vec<Value>),
    /// A table.
    Table(Table),
}

impl Value {
    /// The name of this value's type, as TOML calls it: `"string"`,
    /// `"integer"`, `"float"`, `"boolean"`, `"datetime"`, `"array"` or
    /// `"table"`.
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(..) => "string",
            Value::Integer(..) => "integer",
            Value::Float(..) => "float",
            Value::Boolean(..) => "boolean",
            Value::Datetime(..) => "datetime",
            Value::Array(..) => "array",
            Value::Table(..) => "table",
        }
    }

    /// Returns the string, or `None` if this is not a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the integer, or `None` if this is not an integer.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the float, or `None` if this is not a float.
    ///
    /// Integers are *not* converted; TOML keeps the two types distinct.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Returns the boolean, or `None` if this is not a boolean.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Returns the date-time, or `None` if this is not a date-time.
    pub fn as_datetime(&self) -> Option<&Datetime> {
        match self {
            Value::Datetime(dt) => Some(dt),
            _ => None,
        }
    }

    /// Returns the array, or `None` if this is not an array.
    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Returns the array mutably, or `None` if this is not an array.
    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Returns the table, or `None` if this is not a table.
    pub fn as_table(&self) -> Option<&Table> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    /// Returns the table mutably, or `None` if this is not a table.
    pub fn as_table_mut(&mut self) -> Option<&mut Table> {
        match self {
            Value::Table(t) => Some(t),
            _ => None,
        }
    }

    /// Whether this is a string.
    pub fn is_str(&self) -> bool {
        matches!(self, Value::String(..))
    }

    /// Whether this is an integer.
    pub fn is_integer(&self) -> bool {
        matches!(self, Value::Integer(..))
    }

    /// Whether this is a float.
    pub fn is_float(&self) -> bool {
        matches!(self, Value::Float(..))
    }

    /// Whether this is a boolean.
    pub fn is_bool(&self) -> bool {
        matches!(self, Value::Boolean(..))
    }

    /// Whether this is a date-time.
    pub fn is_datetime(&self) -> bool {
        matches!(self, Value::Datetime(..))
    }

    /// Whether this is an array.
    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(..))
    }

    /// Whether this is a table.
    pub fn is_table(&self) -> bool {
        matches!(self, Value::Table(..))
    }

    /// Looks up a key in a table, or `None` for any other kind of value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_table()?.get(key)
    }

    /// Looks up a key in a table mutably, or `None` for any other kind of
    /// value.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.as_table_mut()?.get_mut(key)
    }

    /// Looks up an index in an array, or `None` for any other kind of value.
    pub fn get_index(&self, index: usize) -> Option<&Value> {
        self.as_array()?.get(index)
    }

    /// Follows a dotted path such as `"servers.alpha.ip"`, descending through
    /// tables and — for segments that are decimal numbers — arrays.
    ///
    /// Path segments are taken literally, so this cannot reach a key that
    /// itself contains a `.`.
    ///
    /// ```
    /// let doc = tomlproc::parse(r#"
    ///     [[server]]
    ///     ports = [80, 443]
    /// "#).unwrap();
    ///
    /// let value = tomlproc::Value::Table(doc);
    /// assert_eq!(value.get_path("server.0.ports.1").and_then(|v| v.as_integer()), Some(443));
    /// assert_eq!(value.get_path("server.1"), None);
    /// ```
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let mut current = self;
        for segment in path.split('.') {
            current = match current {
                Value::Table(table) => table.get(segment)?,
                Value::Array(array) => array.get(segment.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(current)
    }
}

impl Index<&str> for Value {
    type Output = Value;

    /// Looks up a key in a table.
    ///
    /// # Panics
    ///
    /// Panics if this is not a table, or if the key is missing. Use
    /// [`Value::get`] to handle those cases.
    fn index(&self, key: &str) -> &Value {
        self.get(key)
            .unwrap_or_else(|| panic!("cannot index a TOML {} with `{key}`", self.type_name()))
    }
}

impl IndexMut<&str> for Value {
    /// Looks up a key in a table mutably.
    ///
    /// # Panics
    ///
    /// Panics if this is not a table, or if the key is missing.
    fn index_mut(&mut self, key: &str) -> &mut Value {
        let type_name = self.type_name();
        self.get_mut(key)
            .unwrap_or_else(|| panic!("cannot index a TOML {type_name} with `{key}`"))
    }
}

impl Index<usize> for Value {
    type Output = Value;

    /// Indexes into an array.
    ///
    /// # Panics
    ///
    /// Panics if this is not an array, or if the index is out of bounds. Use
    /// [`Value::get_index`] to handle those cases.
    fn index(&self, index: usize) -> &Value {
        self.get_index(index)
            .unwrap_or_else(|| panic!("cannot index a TOML {} with {index}", self.type_name()))
    }
}

impl IndexMut<usize> for Value {
    /// Indexes into an array mutably.
    ///
    /// # Panics
    ///
    /// Panics if this is not an array, or if the index is out of bounds.
    fn index_mut(&mut self, index: usize) -> &mut Value {
        let type_name = self.type_name();
        self.as_array_mut()
            .and_then(|array| array.get_mut(index))
            .unwrap_or_else(|| panic!("cannot index a TOML {type_name} with {index}"))
    }
}

/// Writes the value in TOML's *value* syntax: a table becomes an inline table,
/// an array becomes a bracketed list.
///
/// To write a whole document, with `[table]` headers instead of inline tables,
/// use [`to_string`](crate::to_string).
impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        crate::ser::write_value(&mut out, self);
        f.write_str(&out)
    }
}

macro_rules! from_integer {
    ($($ty:ty),*) => {$(
        impl From<$ty> for Value {
            fn from(value: $ty) -> Value {
                Value::Integer(i64::from(value))
            }
        }
    )*};
}

from_integer!(i8, i16, i32, i64, u8, u16, u32);

impl From<f32> for Value {
    fn from(value: f32) -> Value {
        Value::Float(f64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Value {
        Value::Float(value)
    }
}

impl From<bool> for Value {
    fn from(value: bool) -> Value {
        Value::Boolean(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Value {
        Value::String(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Value {
        Value::String(value.to_owned())
    }
}

impl From<Table> for Value {
    fn from(value: Table) -> Value {
        Value::Table(value)
    }
}

impl From<Datetime> for Value {
    fn from(value: Datetime) -> Value {
        Value::Datetime(value)
    }
}

impl From<Date> for Value {
    fn from(value: Date) -> Value {
        Value::Datetime(value.into())
    }
}

impl From<Time> for Value {
    fn from(value: Time) -> Value {
        Value::Datetime(value.into())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(value: Vec<T>) -> Value {
        Value::Array(value.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Value> + Clone, const N: usize> From<[T; N]> for Value {
    fn from(value: [T; N]) -> Value {
        Value::Array(value.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Value>> FromIterator<T> for Value {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Value {
        Value::Array(iter.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_are_type_strict() {
        let value = Value::Integer(7);
        assert_eq!(value.as_integer(), Some(7));
        assert_eq!(value.as_float(), None);
        assert_eq!(value.type_name(), "integer");
    }

    #[test]
    fn conversions() {
        assert_eq!(Value::from(1u8), Value::Integer(1));
        assert_eq!(Value::from("hi"), Value::String("hi".into()));
        assert_eq!(
            Value::from(vec![1, 2]),
            Value::Array(vec![Value::Integer(1), Value::Integer(2)])
        );
        assert_eq!(Value::from([true, false])[1], Value::Boolean(false));
    }

    #[test]
    fn get_path_walks_tables_and_arrays() {
        let mut inner = Table::new();
        inner.insert("ports", vec![80, 443]);
        let mut root = Table::new();
        root.insert("server", Value::Array(vec![Value::Table(inner)]));
        let value = Value::Table(root);

        assert_eq!(
            value.get_path("server.0.ports.0"),
            Some(&Value::Integer(80))
        );
        assert_eq!(value.get_path("server.0.ports.2"), None);
        assert_eq!(value.get_path("server.x"), None);
        assert_eq!(value.get_path("nope"), None);
    }
}
