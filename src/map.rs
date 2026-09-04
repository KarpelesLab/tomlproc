//! The insertion-ordered map that backs TOML tables.

use core::fmt;
use core::ops::Index;
use std::collections::HashMap;

use crate::value::Value;

/// A TOML table: a map from keys to [`Value`]s that remembers the order in
/// which keys were first inserted.
///
/// Iteration order is insertion order, which is also the order the serializer
/// writes keys in, so a parsed document keeps its original layout when written
/// back out. Lookups are O(1).
///
/// Two tables compare equal when they hold the same key/value pairs, regardless
/// of order.
///
/// ```
/// use tomlproc::{Table, Value};
///
/// let mut table = Table::new();
/// table.insert("name", "tomlproc");
/// table.insert("stars", 5);
///
/// assert_eq!(table["name"].as_str(), Some("tomlproc"));
/// assert_eq!(table.keys().collect::<Vec<_>>(), ["name", "stars"]);
/// ```
#[derive(Clone, Default)]
pub struct Table {
    entries: Vec<(String, Value)>,
    index: HashMap<String, usize>,
}

impl Table {
    /// Creates an empty table.
    pub fn new() -> Table {
        Table::default()
    }

    /// Creates an empty table with room for `capacity` entries.
    pub fn with_capacity(capacity: usize) -> Table {
        Table {
            entries: Vec::with_capacity(capacity),
            index: HashMap::with_capacity(capacity),
        }
    }

    /// The number of entries in the table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `key` is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    /// Returns the value for `key`, if any.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.index.get(key).map(|&i| &self.entries[i].1)
    }

    /// Returns a mutable reference to the value for `key`, if any.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        match self.index.get(key) {
            Some(&i) => Some(&mut self.entries[i].1),
            None => None,
        }
    }

    /// Returns the stored key and its value, if the key is present.
    pub fn get_key_value(&self, key: &str) -> Option<(&str, &Value)> {
        self.index.get(key).map(|&i| {
            let (k, v) = &self.entries[i];
            (k.as_str(), v)
        })
    }

    /// Inserts a value, returning the previous one if the key was already
    /// present.
    ///
    /// Replacing a value keeps the key at its original position.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        let key = key.into();
        let value = value.into();
        match self.index.get(&key) {
            Some(&i) => Some(core::mem::replace(&mut self.entries[i].1, value)),
            None => {
                self.index.insert(key.clone(), self.entries.len());
                self.entries.push((key, value));
                None
            }
        }
    }

    /// Removes `key`, returning its value if it was present.
    ///
    /// The remaining entries keep their relative order, which makes this O(n).
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let i = self.index.remove(key)?;
        let (_, value) = self.entries.remove(i);
        for slot in self.index.values_mut() {
            if *slot > i {
                *slot -= 1;
            }
        }
        Some(value)
    }

    /// Removes every entry.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// Sorts the entries by key, changing the order they iterate and serialize
    /// in.
    pub fn sort_keys(&mut self) {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
        self.reindex();
    }

    fn reindex(&mut self) {
        self.index.clear();
        for (i, (key, _)) in self.entries.iter().enumerate() {
            self.index.insert(key.clone(), i);
        }
    }

    /// An iterator over the entries, in insertion order.
    pub fn iter(&self) -> Iter<'_> {
        Iter {
            inner: self.entries.iter(),
        }
    }

    /// A mutable iterator over the entries, in insertion order.
    pub fn iter_mut(&mut self) -> IterMut<'_> {
        IterMut {
            inner: self.entries.iter_mut(),
        }
    }

    /// An iterator over the keys, in insertion order.
    pub fn keys(&self) -> Keys<'_> {
        Keys {
            inner: self.entries.iter(),
        }
    }

    /// An iterator over the values, in insertion order.
    pub fn values(&self) -> Values<'_> {
        Values {
            inner: self.entries.iter(),
        }
    }

    /// A mutable iterator over the values, in insertion order.
    pub fn values_mut(&mut self) -> ValuesMut<'_> {
        ValuesMut {
            inner: self.entries.iter_mut(),
        }
    }
}

impl fmt::Debug for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

/// Tables are equal when they hold the same entries, whatever their order.
impl PartialEq for Table {
    fn eq(&self, other: &Table) -> bool {
        self.len() == other.len() && self.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl Index<&str> for Table {
    type Output = Value;

    /// Looks up a key.
    ///
    /// # Panics
    ///
    /// Panics if the key is not present; use [`Table::get`] to handle a missing
    /// key.
    fn index(&self, key: &str) -> &Value {
        self.get(key)
            .unwrap_or_else(|| panic!("no such key in TOML table: `{key}`"))
    }
}

impl<K: Into<String>, V: Into<Value>> FromIterator<(K, V)> for Table {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Table {
        let iter = iter.into_iter();
        let mut table = Table::with_capacity(iter.size_hint().0);
        for (key, value) in iter {
            table.insert(key, value);
        }
        table
    }
}

impl<K: Into<String>, V: Into<Value>> Extend<(K, V)> for Table {
    fn extend<I: IntoIterator<Item = (K, V)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

/// An iterator over a table's entries, in insertion order.
///
/// Created by [`Table::iter`].
#[derive(Debug, Clone)]
pub struct Iter<'a> {
    inner: core::slice::Iter<'a, (String, Value)>,
}

/// A mutable iterator over a table's entries, in insertion order.
///
/// Created by [`Table::iter_mut`].
#[derive(Debug)]
pub struct IterMut<'a> {
    inner: core::slice::IterMut<'a, (String, Value)>,
}

/// An owning iterator over a table's entries, in insertion order.
///
/// Created by `IntoIterator for Table`.
#[derive(Debug)]
pub struct IntoIter {
    inner: std::vec::IntoIter<(String, Value)>,
}

/// An iterator over a table's keys, in insertion order.
///
/// Created by [`Table::keys`].
#[derive(Debug, Clone)]
pub struct Keys<'a> {
    inner: core::slice::Iter<'a, (String, Value)>,
}

/// An iterator over a table's values, in insertion order.
///
/// Created by [`Table::values`].
#[derive(Debug, Clone)]
pub struct Values<'a> {
    inner: core::slice::Iter<'a, (String, Value)>,
}

/// A mutable iterator over a table's values, in insertion order.
///
/// Created by [`Table::values_mut`].
#[derive(Debug)]
pub struct ValuesMut<'a> {
    inner: core::slice::IterMut<'a, (String, Value)>,
}

macro_rules! forward_iterator {
    ($name:ty, $item:ty, $map:expr) => {
        impl<'a> Iterator for $name {
            type Item = $item;

            fn next(&mut self) -> Option<$item> {
                self.inner.next().map($map)
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                self.inner.size_hint()
            }
        }

        impl<'a> DoubleEndedIterator for $name {
            fn next_back(&mut self) -> Option<$item> {
                self.inner.next_back().map($map)
            }
        }

        impl<'a> ExactSizeIterator for $name {
            fn len(&self) -> usize {
                self.inner.len()
            }
        }
    };
}

forward_iterator!(Iter<'a>, (&'a str, &'a Value), |(k, v)| (k.as_str(), v));
forward_iterator!(IterMut<'a>, (&'a str, &'a mut Value), |(k, v)| (
    k.as_str(),
    v
));
forward_iterator!(Keys<'a>, &'a str, |(k, _)| k.as_str());
forward_iterator!(Values<'a>, &'a Value, |(_, v)| v);
forward_iterator!(ValuesMut<'a>, &'a mut Value, |(_, v)| v);

impl Iterator for IntoIter {
    type Item = (String, Value);

    fn next(&mut self) -> Option<(String, Value)> {
        self.inner.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl DoubleEndedIterator for IntoIter {
    fn next_back(&mut self) -> Option<(String, Value)> {
        self.inner.next_back()
    }
}

impl ExactSizeIterator for IntoIter {
    fn len(&self) -> usize {
        self.inner.len()
    }
}

impl<'a> IntoIterator for &'a Table {
    type Item = (&'a str, &'a Value);
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Iter<'a> {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Table {
    type Item = (&'a str, &'a mut Value);
    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> IterMut<'a> {
        self.iter_mut()
    }
}

impl IntoIterator for Table {
    type Item = (String, Value);
    type IntoIter = IntoIter;

    fn into_iter(self) -> IntoIter {
        IntoIter {
            inner: self.entries.into_iter(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_insertion_order() {
        let mut table = Table::new();
        table.insert("z", 1);
        table.insert("a", 2);
        table.insert("m", 3);
        assert_eq!(table.keys().collect::<Vec<_>>(), ["z", "a", "m"]);

        // Overwriting keeps the original position.
        table.insert("z", 9);
        assert_eq!(table.keys().collect::<Vec<_>>(), ["z", "a", "m"]);
        assert_eq!(table["z"], Value::Integer(9));
        assert_eq!(table.len(), 3);
    }

    #[test]
    fn remove_keeps_the_index_consistent() {
        let mut table: Table = [("a", 1), ("b", 2), ("c", 3)].into_iter().collect();
        assert_eq!(table.remove("b"), Some(Value::Integer(2)));
        assert_eq!(table.remove("b"), None);
        assert_eq!(table.get("c"), Some(&Value::Integer(3)));
        assert_eq!(table.keys().collect::<Vec<_>>(), ["a", "c"]);

        table.insert("d", 4);
        assert_eq!(table.get("d"), Some(&Value::Integer(4)));
        assert_eq!(table.get("a"), Some(&Value::Integer(1)));
    }

    #[test]
    fn equality_ignores_order() {
        let a: Table = [("x", 1), ("y", 2)].into_iter().collect();
        let b: Table = [("y", 2), ("x", 1)].into_iter().collect();
        assert_eq!(a, b);

        let c: Table = [("y", 2)].into_iter().collect();
        assert_ne!(a, c);
    }

    #[test]
    fn sort_keys_reindexes() {
        let mut table: Table = [("c", 3), ("a", 1), ("b", 2)].into_iter().collect();
        table.sort_keys();
        assert_eq!(table.keys().collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(table.get("c"), Some(&Value::Integer(3)));
    }
}
