# tomlproc

[![CI](https://github.com/KarpelesLab/tomlproc/actions/workflows/ci.yml/badge.svg)](https://github.com/KarpelesLab/tomlproc/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tomlproc.svg)](https://crates.io/crates/tomlproc)
[![docs.rs](https://img.shields.io/docsrs/tomlproc)](https://docs.rs/tomlproc)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A [TOML 1.1.0](https://toml.io/en/v1.1.0) parser and serializer for Rust, with
**no dependencies** — nothing outside the standard library, no build scripts,
no proc macros, no `unsafe`. `no_std` with an allocator, and it builds without
one too.

The whole of TOML 1.1.0 is implemented: all four string flavours, all four
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

`to_string_pretty` writes the same document with values laid out for a reader:
arrays one element per line, and strings holding newlines as multi-line
strings rather than one long line of `\n` escapes.

Values round-trip: parse, serialize and re-parse gives an equal document.
Formatting does not — comments, blank lines and the choice between a header
and an inline table belong to the source text, not to the value model.

## serde

Mapping documents onto your own types is behind the off-by-default `serde`
feature — with it off, the crate still has no dependencies at all:

```toml
[dependencies]
tomlproc = { version = "0.1", features = ["serde"] }
```

```rust
#[derive(serde::Serialize, serde::Deserialize)]
struct Config {
    name: String,
    ports: Vec<u16>,
}

let config: Config = tomlproc::serde::from_str(text)?;
let text = tomlproc::serde::to_string(&config)?;
```

`Value`, `Table` and `Datetime` implement `Serialize`/`Deserialize`, and
`tomlproc::serde` provides `from_str`, `from_slice`, `from_value`, `from_table`,
`to_value`, `to_string` and `to_string_pretty`. Errors name the key that did not
fit:

```console
TOML error at `servers.beta.port`: invalid type: string "80", expected u16
```

`Error::key_path()` returns that path on its own, for building your own
diagnostics.

## no_std

The crate is `#![no_std]`, and its two default features can be turned off
independently:

| Feature | Default | What it adds |
| --- | --- | --- |
| `std` | yes | Implies `alloc`, and indexes tables by hash |
| `alloc` | via `std` | The value model, the parser and the serializer |
| `serde` | no | The `serde` integration; implies `alloc` |

```toml
# embedded, with a heap: the whole API, on ordered maps instead of hash maps
tomlproc = { version = "0.1", default-features = false, features = ["alloc"] }
```

With `alloc` but no `std` the public API is unchanged, and so is what the
parser accepts — the differential harness agrees on all 401,684 inputs in that
configuration too. CI builds both bare-metal configurations for
`thumbv7em-none-eabi` and `riscv32imc-unknown-none-elf`, where `std` does not
exist to be accidentally depended on.

Turning `alloc` off as well leaves `Datetime`, `Date`, `Time` and `Offset`,
which parse and format with no allocator. The value model cannot follow: a
`Table` owns its keys and values, so it *is* allocation. A no-allocator parser
would be a different shape — a borrowing event parser, where escapes have no
slice to point at and the table rules have nowhere to remember what they have
seen. That is not implemented.

## Diagnostics

`parse_spans` records where every value was written, keyed by the same dotted
path a `serde` error reports, so the two fit together:

```rust
let (doc, spans) = tomlproc::parse_spans(source)?;

if let Err(error) = tomlproc::serde::from_table::<Config>(doc) {
    if let Some(span) = error.key_path().and_then(|path| spans.get(&path)) {
        eprintln!("{}:{}: {}", span.line, span.column, error.message());
        eprintln!("  {}", &source[span.value.clone()]);
    }
}
```

Spans are recorded per key/value pair and per table header; a path pointing
inside a value resolves to the nearest enclosing one. Plain `parse` records
nothing, and pays nothing.

## Conformance

### TOML 1.1

TOML [1.1.0](https://toml.io/en/v1.1.0) was released in December 2025, and this
crate implements it. Over 1.0.0 it adds, all of which parse here:

```toml
tbl = {                     # newlines and a trailing comma
    key = "a string",       # inside an inline table
}
esc = "\e[1m \x41"           # the \e and \xHH escapes
dt  = 2010-02-03 14:15      # seconds are optional
t   = 14:15
```

1.1 only adds to 1.0, so every 1.0 document still parses unchanged. What this
crate *writes* stays inside 1.0, so its output can still be read by an older
parser: seconds are always written, inline tables stay on one line, and control
characters are escaped as `\u00XX`.

That last point has one visible consequence: a time written `14:15` is read as
14:15:00 and written back as `14:15:00`. The value is the same; the text is
normalized, as it already is for comments, blank lines and table layout.

### Strictness

The parser is strict. It rejects, with a position, everything the
specification calls invalid, including:

- duplicate keys, whether written bare, dotted or quoted;
- redefining a table, or claiming a table that a dotted key created;
- adding to an inline table after the fact;
- mixing `[table]` and `[[array of tables]]` at the same name;
- using a dotted key to redefine a table a header already defined;
- integers outside the range of a 64-bit signed integer, leading zeros, and
  misplaced underscores;
- dates and times that do not exist (`2023-02-29`, `25:00:00`);
- control characters in strings and comments, and bare carriage returns.

Two places where the specification leaves a choice:

- `\r\n` inside a multi-line string is normalized to `\n`, which the spec
  explicitly permits.
- Fractional seconds beyond nanosecond precision are truncated, which the spec
  calls implementation-specific.

Conformance is checked by differential testing against the `toml` crate, the
reference implementation, in [`tools/difftest`](tools/difftest): every `.toml`
file in a corpus you point it at, plus 400,000 generated inputs — fragment soup
aimed at the lexer's decision points, and whole statements aimed at the table
redefinition rules. Both parsers must agree on what to accept, and on the value
they produce. The current state is complete agreement, over ~1,700 real-world
files and the generated set:

```console
$ cargo run --release --manifest-path tools/difftest/Cargo.toml -- ~/.cargo/registry/src
1684 files + 400000 generated inputs
agree: 401684  different values: 0  only tomlproc accepts: 0  only the reference accepts: 0
```

That harness depends on the reference implementation, so it lives outside the
crate: `tomlproc` itself still has no dependencies, and `tools/` is excluded
from the published package.

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
