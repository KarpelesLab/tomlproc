//! The insertion-ordered map that backs TOML tables.

use core::fmt;
use core::ops::Index;

use alloc::string::String;
use alloc::vec::Vec;

use crate::collections::Map;
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
    index: Map<String, usize>,
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
            index: Map::default(),
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

    /// Returns the entry for `key`, so it can be inspected or filled in in one
    /// lookup.
    ///
    /// ```
    /// let mut table = tomlproc::Table::new();
    /// use tomlproc::Value;
    ///
    /// table.entry("hits").or_insert(1);
    /// table.entry("hits").and_modify(|v| *v = Value::Integer(2));
    /// table.entry("misses").or_insert_with(|| Value::Integer(0));
    ///
    /// assert_eq!(table["hits"].as_integer(), Some(2));
    /// assert_eq!(table["misses"].as_integer(), Some(0));
    /// ```
    pub fn entry(&mut self, key: impl Into<String>) -> Entry<'_> {
        let key = key.into();
        match self.index.get(&key).copied() {
            Some(index) => Entry::Occupied(OccupiedEntry { table: self, index }),
            None => Entry::Vacant(VacantEntry { table: self, key }),
        }
    }

    /// Keeps only the entries for which `keep` returns `true`, in insertion
    /// order.
    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&str, &mut Value) -> bool,
    {
        self.entries.retain_mut(|(key, value)| keep(key, value));
        self.reindex();
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

    /// Follows a dotted path such as `"servers.alpha.ip"`, descending through
    /// tables and -- for segments that are decimal numbers -- arrays.
    ///
    /// Path segments are taken literally, so this cannot reach a key that
    /// itself contains a `.`; use [`get`](Table::get) at that step instead.
    ///
    /// ```
    /// let doc = tomlproc::parse(r#"
    ///     [[server]]
    ///     ip = "10.0.0.1"
    ///     ports = [80, 443]
    /// "#).unwrap();
    ///
    /// assert_eq!(doc.get_path("server.0.ip").and_then(|v| v.as_str()), Some("10.0.0.1"));
    /// assert_eq!(doc.get_path("server.0.ports.1").and_then(|v| v.as_integer()), Some(443));
    /// assert_eq!(doc.get_path("server.1.ip"), None);
    /// ```
    pub fn get_path(&self, path: &str) -> Option<&Value> {
        let mut segments = path.split('.');
        let mut current = self.get(segments.next()?)?;
        for segment in segments {
            current = match current {
                Value::Table(table) => table.get(segment)?,
                Value::Array(array) => array.get(segment.parse::<usize>().ok()?)?,
                _ => return None,
            };
        }
        Some(current)
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

/// A view into one entry of a [`Table`], vacant or occupied.
///
/// Created by [`Table::entry`].
#[derive(Debug)]
pub enum Entry<'a> {
    /// The key is not in the table.
    Vacant(VacantEntry<'a>),
    /// The key is in the table.
    Occupied(OccupiedEntry<'a>),
}

impl<'a> Entry<'a> {
    /// The key this entry is for.
    pub fn key(&self) -> &str {
        match self {
            Entry::Vacant(entry) => entry.key(),
            Entry::Occupied(entry) => entry.key(),
        }
    }

    /// Returns the value, inserting `default` first if the key was vacant.
    pub fn or_insert(self, default: impl Into<Value>) -> &'a mut Value {
        match self {
            Entry::Vacant(entry) => entry.insert(default),
            Entry::Occupied(entry) => entry.into_mut(),
        }
    }

    /// Returns the value, inserting what `default` produces if the key was
    /// vacant.
    pub fn or_insert_with<V: Into<Value>, F: FnOnce() -> V>(self, default: F) -> &'a mut Value {
        match self {
            Entry::Vacant(entry) => entry.insert(default()),
            Entry::Occupied(entry) => entry.into_mut(),
        }
    }

    /// Runs `f` on the value if the key was occupied, and returns the entry.
    pub fn and_modify<F: FnOnce(&mut Value)>(self, f: F) -> Entry<'a> {
        match self {
            Entry::Vacant(entry) => Entry::Vacant(entry),
            Entry::Occupied(mut entry) => {
                f(entry.get_mut());
                Entry::Occupied(entry)
            }
        }
    }
}

/// An entry for a key that is not in the table.
#[derive(Debug)]
pub struct VacantEntry<'a> {
    table: &'a mut Table,
    key: String,
}

impl<'a> VacantEntry<'a> {
    /// The key this entry is for.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The key this entry is for, given back.
    pub fn into_key(self) -> String {
        self.key
    }

    /// Inserts a value and returns it, appending the key to the table's order.
    pub fn insert(self, value: impl Into<Value>) -> &'a mut Value {
        self.table.insert(self.key, value);
        &mut self.table.entries.last_mut().expect("just inserted").1
    }
}

/// An entry for a key that is in the table.
#[derive(Debug)]
pub struct OccupiedEntry<'a> {
    table: &'a mut Table,
    index: usize,
}

impl<'a> OccupiedEntry<'a> {
    /// The key this entry is for.
    pub fn key(&self) -> &str {
        &self.table.entries[self.index].0
    }

    /// The value.
    pub fn get(&self) -> &Value {
        &self.table.entries[self.index].1
    }

    /// The value, mutably.
    pub fn get_mut(&mut self) -> &mut Value {
        &mut self.table.entries[self.index].1
    }

    /// The value, mutably, for as long as the table is borrowed.
    pub fn into_mut(self) -> &'a mut Value {
        &mut self.table.entries[self.index].1
    }

    /// Replaces the value, returning the old one and keeping the key's
    /// position.
    pub fn insert(&mut self, value: impl Into<Value>) -> Value {
        core::mem::replace(self.get_mut(), value.into())
    }

    /// Removes the entry and returns its value. The remaining entries keep
    /// their relative order.
    pub fn remove(self) -> Value {
        let key = self.table.entries[self.index].0.clone();
        self.table.remove(&key).expect("the entry is occupied")
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
    inner: alloc::vec::IntoIter<(String, Value)>,
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
    use alloc::vec;

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
    fn get_path_walks_tables_and_arrays() {
        let mut table = Table::new();
        table.insert("a", Value::Array(vec![Value::from([1, 2])]));
        table.insert("b.c", 3);

        assert_eq!(table.get_path("a.0.1"), Some(&Value::Integer(2)));
        assert_eq!(table.get_path("a.0.2"), None);
        assert_eq!(table.get_path("a.x"), None);
        assert_eq!(table.get_path("nope"), None);
        // A key containing a dot is out of reach of a path.
        assert_eq!(table.get_path("b.c"), None);
        assert_eq!(table.get("b.c"), Some(&Value::Integer(3)));
    }

    #[test]
    fn entry_api() {
        let mut table = Table::new();
        assert_eq!(table.entry("a").key(), "a");
        table.entry("a").or_insert(1);
        table.entry("a").or_insert(2);
        assert_eq!(table["a"], Value::Integer(1));

        table.entry("a").and_modify(|v| *v = Value::Integer(3));
        assert_eq!(table["a"], Value::Integer(3));

        match table.entry("b") {
            Entry::Vacant(entry) => assert_eq!(entry.into_key(), "b"),
            Entry::Occupied(_) => panic!("`b` should be vacant"),
        }

        table.insert("b", 2);
        match table.entry("a") {
            Entry::Occupied(entry) => assert_eq!(entry.remove(), Value::Integer(3)),
            Entry::Vacant(_) => panic!("`a` should be occupied"),
        }
        // The index survives the removal.
        assert_eq!(table.get("b"), Some(&Value::Integer(2)));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn retain_keeps_order_and_reindexes() {
        let mut table: Table = [("a", 1), ("b", 2), ("c", 3)].into_iter().collect();
        table.retain(|_, value| value.as_integer() != Some(2));
        assert_eq!(table.keys().collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(table.get("c"), Some(&Value::Integer(3)));
        assert_eq!(table.get("b"), None);
    }

    #[test]
    fn sort_keys_reindexes() {
        let mut table: Table = [("c", 3), ("a", 1), ("b", 2)].into_iter().collect();
        table.sort_keys();
        assert_eq!(table.keys().collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(table.get("c"), Some(&Value::Integer(3)));
    }
}
