//! Macros for building values in code.

/// Builds a [`Table`](crate::Table) from `key => value` pairs.
///
/// Keys are anything that converts into a `String`, values anything that
/// converts into a [`Value`](crate::Value) -- including a nested `table!`.
///
/// ```
/// use tomlproc::{table, array};
///
/// let doc = table! {
///     "title" => "TOML Example",
///     "owner" => table! {
///         "name" => "Tom Preston-Werner",
///         "ports" => array![8000, 8001],
///     },
/// };
///
/// assert_eq!(doc["owner"]["ports"][1].as_integer(), Some(8001));
/// assert_eq!(tomlproc::table!().len(), 0);
/// ```
///
/// To build a document from literal TOML syntax instead, parse it:
/// `tomlproc::parse(r#"…"#)`.
#[macro_export]
macro_rules! table {
    () => { $crate::Table::new() };
    ($($key:expr => $value:expr),+ $(,)?) => {{
        let mut table = $crate::Table::new();
        $( table.insert($key, $value); )+
        table
    }};
}

/// Builds an array [`Value`](crate::Value) whose elements need not share a
/// type.
///
/// `Value::from(vec![…])` covers the same-type case; this one exists for the
/// mixed arrays TOML 1.0 allows.
///
/// ```
/// use tomlproc::array;
///
/// let mixed = array![1, "two", 3.0, array![4]];
/// assert_eq!(mixed[1].as_str(), Some("two"));
/// assert_eq!(array![].as_array().map(Vec::len), Some(0));
/// ```
#[macro_export]
macro_rules! array {
    () => { $crate::Value::Array($crate::__private::Vec::new()) };
    ($($value:expr),+ $(,)?) => {{
        let mut items = $crate::__private::Vec::new();
        $( items.push($crate::Value::from($value)); )+
        $crate::Value::Array(items)
    }};
}
