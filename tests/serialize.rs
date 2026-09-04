//! Writing documents back out, and parse/serialize round-trips.

use tomlproc::{Table, Value, parse, to_string, to_string_pretty};

const EXAMPLE: &str = r#"
# This is the example from the TOML home page.
title = "TOML Example"

[owner]
name = "Tom Preston-Werner"
dob = 1979-05-27T07:32:00-08:00

[database]
enabled = true
ports = [ 8000, 8001, 8002 ]
data = [ ["delta", "phi"], [3.14] ]
temp_targets = { cpu = 79.5, case = 72.0 }

[servers]

[servers.alpha]
ip = "10.0.0.1"
role = "frontend"

[servers.beta]
ip = "10.0.0.2"
role = "backend"

[[fruits]]
name = "apple"

[[fruits.varieties]]
name = "red delicious"

[[fruits]]
name = "banana"
"#;

#[track_caller]
fn round_trips(input: &str) {
    let first = parse(input).expect("the input parses");
    let written = to_string(&first);
    let second = parse(&written)
        .unwrap_or_else(|e| panic!("re-parsing failed: {e}\n--- written ---\n{written}"));
    assert_eq!(first, second, "--- written ---\n{written}");
}

#[test]
fn the_example_document_round_trips() {
    round_trips(EXAMPLE);
    // ...and stays stable on a second pass.
    let written = to_string(&parse(EXAMPLE).unwrap());
    assert_eq!(to_string(&parse(&written).unwrap()), written);
}

#[test]
fn every_value_type_round_trips() {
    round_trips(
        r#"
string = "a\tb"
literal = 'c:\windows'
multiline = """one\ntwo"""
integer = -42
hex = 0xdeadbeef
float = 6.626e-34
whole_float = 1.0
infinity = -inf
boolean = false
offset_datetime = 1979-05-27T07:32:00.999-08:00
local_datetime = 1979-05-27T07:32:00
local_date = 1979-05-27
local_time = 07:32:00
array = [1, "two", 3.0, [4], { five = 5 }]
empty_array = []
empty_table = {}
"#,
    );
}

#[test]
fn awkward_keys_round_trip() {
    round_trips(
        r#"
"" = "empty"
"needs quotes" = 1
"a.b" = 2
"quote\"inside" = 3

["header needs quotes"]
"nested.key" = 4

[["array header"]]
x = 5
"#,
    );
}

#[test]
fn nested_arrays_of_tables_round_trip() {
    round_trips(
        r#"
[[a]]
[[a.b]]
[[a.b.c]]
x = 1
[[a.b.c]]
x = 2
[[a.b]]
[[a]]
"#,
    );
}

#[test]
fn output_is_idiomatic() {
    let mut inner = Table::new();
    inner.insert("name", "Tom");

    let mut doc = Table::new();
    doc.insert("title", "example");
    doc.insert("owner", inner);
    doc.insert("tags", vec!["a", "b"]);

    assert_eq!(
        to_string(&doc),
        "title = \"example\"\ntags = [\"a\", \"b\"]\n\n[owner]\nname = \"Tom\"\n"
    );
}

#[test]
fn scalars_are_written_before_sub_table_headers() {
    // If `zzz` were written after `[aaa]`, it would be read back as a member of
    // that table.
    let mut doc = Table::new();
    doc.insert("aaa", Table::new());
    doc.insert("zzz", 1);

    let written = to_string(&doc);
    assert_eq!(written, "zzz = 1\n\n[aaa]\n");
    assert_eq!(parse(&written).unwrap(), doc);
}

#[test]
fn empty_tables_and_arrays_survive() {
    let mut doc = Table::new();
    doc.insert("empty_table", Table::new());
    doc.insert("empty_array", Value::Array(Vec::new()));
    doc.insert(
        "array_of_empty_tables",
        Value::Array(vec![Value::Table(Table::new())]),
    );

    let written = to_string(&doc);
    assert_eq!(
        written,
        "empty_array = []\n\n[empty_table]\n\n[[array_of_empty_tables]]\n"
    );
    assert_eq!(parse(&written).unwrap(), doc);
}

#[test]
fn a_value_displays_as_toml_value_syntax() {
    let doc = parse("a = { b = [1, 2], c = \"x\" }\nd = 1979-05-27").unwrap();
    assert_eq!(doc["a"].to_string(), "{ b = [1, 2], c = \"x\" }");
    assert_eq!(doc["d"].to_string(), "1979-05-27");
    assert_eq!(Value::from(1.5).to_string(), "1.5");
    assert_eq!(Value::from("a\nb").to_string(), "\"a\\nb\"");
}

#[test]
fn floats_never_come_back_as_integers() {
    for value in [
        0.0,
        -0.0,
        1.0,
        1e300,
        1e-300,
        f64::MAX,
        f64::MIN_POSITIVE,
        12345678901234567890.0,
    ] {
        let mut doc = Table::new();
        doc.insert("x", value);
        let written = to_string(&doc);
        let back = parse(&written).unwrap_or_else(|e| panic!("{written:?}: {e}"));
        assert_eq!(back["x"].as_float(), Some(value), "{written}");
    }
}

#[test]
fn strings_with_anything_in_them_round_trip() {
    for s in [
        "",
        "\"",
        "\\",
        "a\nb",
        "\t",
        "\u{0}\u{1f}\u{7f}",
        "héllo 🎉",
        "'",
        "'''",
        "\"\"\"",
    ] {
        let mut doc = Table::new();
        doc.insert("x", s);
        let written = to_string(&doc);
        let back = parse(&written).unwrap_or_else(|e| panic!("{written:?}: {e}"));
        assert_eq!(back["x"].as_str(), Some(s), "{written:?}");
    }
}

#[test]
fn pretty_output_spreads_arrays_over_lines() {
    let doc =
        parse("ports = [8000, 8001]\nempty = []\nnested = [[1, 2], [3]]\nmixed = [1, { a = 1 }]")
            .unwrap();
    assert_eq!(
        to_string_pretty(&doc),
        "\
ports = [
    8000,
    8001,
]
empty = []
nested = [
    [
        1,
        2,
    ],
    [
        3,
    ],
]
mixed = [
    1,
    { a = 1 },
]
"
    );
}

#[test]
fn pretty_output_uses_multiline_strings() {
    let doc = parse("a = \"one\\ntwo\"\nb = \"single line\"").unwrap();
    assert_eq!(
        to_string_pretty(&doc),
        "a = \"\"\"\none\ntwo\"\"\"\nb = \"single line\"\n"
    );
}

#[test]
fn pretty_output_round_trips() {
    // Everything awkward a multi-line string could hold: quote runs, trailing
    // quotes and backslashes, tabs and control characters.
    for s in [
        "a\nb",
        "\n",
        "\n\n",
        "a\n\"\"\"b",
        "a\nb\"",
        "a\nb\\",
        "a\n\tb",
        "a\n\u{1}b",
        "\"\"\"\n\"\"\"",
    ] {
        let mut doc = Table::new();
        doc.insert("x", s);
        let written = to_string_pretty(&doc);
        let back = parse(&written).unwrap_or_else(|e| panic!("{written:?}: {e}"));
        assert_eq!(back["x"].as_str(), Some(s), "{written:?}");
    }
}

#[test]
fn pretty_and_plain_agree_on_the_document() {
    let doc = parse(EXAMPLE).unwrap();
    assert_eq!(parse(&to_string_pretty(&doc)).unwrap(), doc);
}

#[test]
fn a_table_of_only_sub_tables_needs_no_header_of_its_own() {
    // `[servers]` would be implied by `[servers.alpha]`, so it is left out.
    let doc =
        parse("[servers]\n[servers.alpha]\nip = \"a\"\n[servers.beta]\nip = \"b\"\n").unwrap();
    assert_eq!(
        to_string(&doc),
        "[servers.alpha]\nip = \"a\"\n\n[servers.beta]\nip = \"b\"\n"
    );
    assert_eq!(parse(&to_string(&doc)).unwrap(), doc);

    // An empty table has nothing to imply it, so it keeps its header.
    let doc = parse("[servers]").unwrap();
    assert_eq!(to_string(&doc), "[servers]\n");

    // ...and so does one with a value of its own.
    let doc = parse("[servers]\ncount = 1\n[servers.alpha]\nip = \"a\"\n").unwrap();
    assert_eq!(
        to_string(&doc),
        "[servers]\ncount = 1\n\n[servers.alpha]\nip = \"a\"\n"
    );
}
