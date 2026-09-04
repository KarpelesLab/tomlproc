# tomlproc

[![CI](https://github.com/KarpelesLab/tomlproc/actions/workflows/ci.yml/badge.svg)](https://github.com/KarpelesLab/tomlproc/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tomlproc.svg)](https://crates.io/crates/tomlproc)
[![docs.rs](https://img.shields.io/docsrs/tomlproc)](https://docs.rs/tomlproc)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A [TOML 1.0.0](https://toml.io/en/v1.0.0) parser and serializer for Rust, with
**no dependencies** — nothing outside the standard library, no build scripts,
no proc macros, no `unsafe`.

The whole of TOML 1.0.0 is implemented: all four string flavours, all four
date-time types, dotted keys, inline tables, arrays of tables, and the
redefinition rules that decide which of those are legal together.

## Why

Reading a config file should not pull a dependency tree into a project, and it
should not cost a proc-macro crate's compile time. `tomlproc` is one small
crate that parses a document into a plain value tree you index directly:

```toml
[dependencies]
tomlproc = "0.1"
```

## Usage

```rust
let doc = tomlproc::parse(r#"
    title = "TOML Example"

    [owner]
    name = "Tom Preston-Werner"
    dob = 1979-05-27T07:32:00-08:00

    [[server]]
    ip = "10.0.0.1"
    ports = [8000, 8001]
"#)?;

assert_eq!(doc["title"].as_str(), Some("TOML Example"));
assert_eq!(doc["owner"]["dob"].as_datetime().unwrap().date.unwrap().year, 1979);
assert_eq!(doc["server"][0]["ports"][1].as_integer(), Some(8001));

// Or walk a path in one go, without unwrapping at each step.
assert_eq!(doc.get_path("server.0.ip").and_then(|v| v.as_str()), Some("10.0.0.1"));
```

Every accessor is type-strict, the way TOML is: `as_float` on an integer is
`None`, not a conversion. `get` and `get_path` return an `Option`; indexing with
`[]` panics on a missing key, like the standard collections.

### Errors

Parse errors say where they are, in a line and a *character* column:

```rust
let error = tomlproc::parse("a = 1\nb = [1, 2").unwrap_err();
assert_eq!(error.line(), 2);
assert_eq!(
    error.to_string(),
    "TOML parse error at line 2, column 5: unterminated array",
);
```

### Building and writing

`Table` is an insertion-ordered map, so a document keeps its key order from
parse through to serialization:

```rust
let mut package = tomlproc::Table::new();
package.insert("name", "tomlproc");
package.insert("keywords", vec!["toml", "parser"]);

let mut doc = tomlproc::Table::new();
doc.insert("package", package);

assert_eq!(
    tomlproc::to_string(&doc),
    "[package]\nname = \"tomlproc\"\nkeywords = [\"toml\", \"parser\"]\n",
);
```

Sub-tables are written as `[header]` sections and arrays of tables as
`[[header]]` sections, with plain keys always emitted ahead of the first
header, so the output reads like a document a person would write.

Values round-trip: parse, serialize and re-parse gives an equal document.
Formatting does not — comments, blank lines and the choice between a header
and an inline table belong to the source text, not to the value model.

## Conformance

The parser is strict. It rejects, with a position, everything the
specification calls invalid, including:

- duplicate keys, whether written bare, dotted or quoted;
- redefining a table, or claiming a table that a dotted key created;
- adding to an inline table after the fact;
- mixing `[table]` and `[[array of tables]]` at the same name;
- newlines or a trailing comma inside an inline table;
- integers outside the range of a 64-bit signed integer, leading zeros, and
  misplaced underscores;
- dates and times that do not exist (`2023-02-29`, `25:00:00`);
- control characters in strings and comments, and bare carriage returns.

Two places where the specification leaves a choice:

- `\r\n` inside a multi-line string is normalized to `\n`, which the spec
  explicitly permits.
- Fractional seconds beyond nanosecond precision are truncated, which the spec
  calls implementation-specific.

Arrays and inline tables may nest 128 deep. Parsing is recursive, so a
document like `a = [[[[…` would otherwise exhaust the stack; the limit turns
that into an ordinary parse error, which matters when the input is untrusted.

## Types

| Type | What it is |
| --- | --- |
| [`Value`] | A TOML value: string, integer, float, boolean, date-time, array or table |
| [`Table`] | An insertion-ordered map of keys to values, with O(1) lookup |
| [`Datetime`] | A date-time, in one of TOML's four flavours, via [`Date`], [`Time`] and [`Offset`] |
| [`Error`] | A parse failure, with a line, column and byte offset |

[`Value`]: https://docs.rs/tomlproc/latest/tomlproc/enum.Value.html
[`Table`]: https://docs.rs/tomlproc/latest/tomlproc/struct.Table.html
[`Datetime`]: https://docs.rs/tomlproc/latest/tomlproc/struct.Datetime.html
[`Date`]: https://docs.rs/tomlproc/latest/tomlproc/struct.Date.html
[`Time`]: https://docs.rs/tomlproc/latest/tomlproc/struct.Time.html
[`Offset`]: https://docs.rs/tomlproc/latest/tomlproc/enum.Offset.html
[`Error`]: https://docs.rs/tomlproc/latest/tomlproc/struct.Error.html

## Requirements

Rust 1.88 or newer, edition 2024.

## License

MIT — see [LICENSE](LICENSE).
