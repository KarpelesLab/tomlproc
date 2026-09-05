//! Writing values back out as TOML.

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::map::Table;
use crate::value::Value;

/// How much prettier than plain the output should be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    /// One line per value, arrays inline.
    Plain,
    /// Arrays spread over lines, strings holding newlines written as
    /// multi-line strings.
    Pretty,
}

/// Serializes a table as a TOML document.
///
/// Keys are written in the order the table holds them, which for a parsed
/// document is the order they appeared in the source. Sub-tables become
/// `[header]` sections and arrays of tables become `[[header]]` sections, so
/// the output is idiomatic rather than one long line of inline tables.
///
/// This cannot fail: every [`Value`] has a TOML representation.
///
/// ```
/// let mut table = tomlproc::Table::new();
/// table.insert("title", "example");
///
/// let mut owner = tomlproc::Table::new();
/// owner.insert("name", "Tom");
/// table.insert("owner", owner);
///
/// assert_eq!(tomlproc::to_string(&table), "title = \"example\"\n\n[owner]\nname = \"Tom\"\n");
/// ```
pub fn to_string(table: &Table) -> String {
    let mut out = String::new();
    write_table(&mut out, table, &mut Vec::new(), Style::Plain);
    out
}

/// Serializes a table as a TOML document, laid out for a human to read.
///
/// The document is the same as [`to_string`] writes; only the formatting of
/// values differs. Arrays are spread one element per line, and a string
/// holding a newline is written as a multi-line string rather than one long
/// line of `\n` escapes.
///
/// ```
/// let mut table = tomlproc::Table::new();
/// table.insert("ports", vec![8000, 8001]);
///
/// assert_eq!(tomlproc::to_string_pretty(&table), "ports = [\n    8000,\n    8001,\n]\n");
/// ```
pub fn to_string_pretty(table: &Table) -> String {
    let mut out = String::new();
    write_table(&mut out, table, &mut Vec::new(), Style::Pretty);
    out
}

/// Whether a value is written as its own `[header]` section rather than inline.
fn is_section(value: &Value) -> bool {
    match value {
        Value::Table(_) => true,
        // An empty array cannot be written as `[[header]]`: there would be
        // nothing to write. It stays an inline `key = []`.
        Value::Array(items) => !items.is_empty() && items.iter().all(Value::is_table),
        _ => false,
    }
}

/// Whether a table needs a `[header]` line of its own.
///
/// A table holding nothing but sub-tables is already implied by their headers,
/// so `[servers]` above `[servers.alpha]` is noise. An empty table has nothing
/// to imply it, so it keeps its header.
fn needs_header(table: &Table) -> bool {
    table.is_empty() || table.values().any(|value| !is_section(value))
}

fn write_table(out: &mut String, table: &Table, path: &mut Vec<String>, style: Style) {
    // Every plain key has to come before the first sub-table header, or it
    // would land in that sub-table instead.
    for (key, value) in table {
        if !is_section(value) {
            out.push_str(&key_to_string(key));
            out.push_str(" = ");
            write_styled(out, value, style, 0);
            out.push('\n');
        }
    }
    for (key, value) in table {
        if !is_section(value) {
            continue;
        }
        path.push(key_to_string(key));
        match value {
            Value::Table(child) => {
                if needs_header(child) {
                    write_header(out, path, false);
                }
                write_table(out, child, path, style);
            }
            Value::Array(items) => {
                for item in items {
                    write_header(out, path, true);
                    if let Value::Table(child) = item {
                        write_table(out, child, path, style);
                    }
                }
            }
            _ => unreachable!("only tables and arrays of tables are sections"),
        }
        path.pop();
    }
}

fn write_header(out: &mut String, path: &[String], array: bool) {
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(if array { "[[" } else { "[" });
    out.push_str(&path.join("."));
    out.push_str(if array { "]]\n" } else { "]\n" });
}

/// Writes a value in TOML's value syntax, using inline tables for tables.
pub(crate) fn write_value(out: &mut String, value: &Value) {
    write_styled(out, value, Style::Plain, 0);
}

/// Writes a value, spreading arrays over lines when the style asks for it.
///
/// `depth` is how many array brackets enclose this value, which is what the
/// indentation of a spread-out array is measured in.
fn write_styled(out: &mut String, value: &Value, style: Style, depth: usize) {
    match value {
        Value::String(s) if style == Style::Pretty && s.contains('\n') => {
            write_multiline_string(out, s);
        }
        Value::String(s) => write_string(out, s),
        Value::Integer(i) => out.push_str(&i.to_string()),
        Value::Float(f) => write_float(out, *f),
        Value::Boolean(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Datetime(dt) => out.push_str(&dt.to_string()),
        Value::Array(items) if style == Style::Pretty && !items.is_empty() => {
            out.push_str("[\n");
            for item in items {
                for _ in 0..=depth {
                    out.push_str("    ");
                }
                write_styled(out, item, style, depth + 1);
                out.push_str(",\n");
            }
            for _ in 0..depth {
                out.push_str("    ");
            }
            out.push(']');
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_styled(out, item, style, depth);
            }
            out.push(']');
        }
        Value::Table(table) => {
            if table.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{ ");
            for (i, (key, value)) in table.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&key_to_string(key));
                out.push_str(" = ");
                // An inline table has to stay on one line, whatever the style.
                write_styled(out, value, Style::Plain, 0);
            }
            out.push_str(" }");
        }
    }
}

fn write_float(out: &mut String, value: f64) {
    if value.is_nan() {
        out.push_str(if value.is_sign_negative() {
            "-nan"
        } else {
            "nan"
        });
    } else if value.is_infinite() {
        out.push_str(if value < 0.0 { "-inf" } else { "inf" });
    } else {
        // `{:?}` keeps the value round-trippable and always writes either a
        // decimal point or an exponent, so the result reads back as a float
        // rather than an integer.
        out.push_str(&format!("{value:?}"));
    }
}

fn write_string(out: &mut String, value: &str) {
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Writes a string holding newlines as a multi-line basic string, which reads
/// far better than one long line of `\\n` escapes.
///
/// Every quote and backslash is escaped, so no run of characters in the
/// content can be mistaken for the closing delimiter or an escape.
fn write_multiline_string(out: &mut String, value: &str) {
    // The newline right after the opening delimiter is trimmed when read back,
    // so it costs nothing and lets the content start in column one.
    out.push_str("\"\"\"\n");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' | '\t' => out.push(c),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c < ' ' || c == '\u{7f}' => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push_str("\"\"\"");
}

/// Renders a key, quoting it only when it cannot be written bare.
pub(crate) fn key_to_string(key: &str) -> String {
    if !key.is_empty()
        && key
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        key.to_owned()
    } else {
        let mut out = String::with_capacity(key.len() + 2);
        write_string(&mut out, key);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_quoted_only_when_needed() {
        assert_eq!(key_to_string("plain-key_1"), "plain-key_1");
        assert_eq!(key_to_string(""), "\"\"");
        assert_eq!(key_to_string("with space"), "\"with space\"");
        assert_eq!(key_to_string("a.b"), "\"a.b\"");
        assert_eq!(key_to_string("clé"), "\"clé\"");
    }

    #[test]
    fn floats_stay_floats() {
        let mut out = String::new();
        write_float(&mut out, 1.0);
        assert_eq!(out, "1.0");

        for (value, expected) in [
            (f64::INFINITY, "inf"),
            (f64::NEG_INFINITY, "-inf"),
            (f64::NAN, "nan"),
        ] {
            let mut out = String::new();
            write_float(&mut out, value);
            assert_eq!(out, expected);
        }
    }

    #[test]
    fn strings_escape_control_characters() {
        let mut out = String::new();
        write_string(&mut out, "a\tb\nc\u{1}\u{7f}\"\\");
        assert_eq!(out, "\"a\\tb\\nc\\u0001\\u007F\\\"\\\\\"");
    }
}
