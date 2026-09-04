//! Documents the specification calls valid.

use tomlproc::{DatetimeKind, Table, Value, parse};

#[track_caller]
fn doc(input: &str) -> Table {
    parse(input).unwrap_or_else(|e| panic!("expected `{input}` to parse, got: {e}"))
}

#[track_caller]
fn value(input: &str) -> Value {
    doc(&format!("x = {input}"))
        .remove("x")
        .expect("the key was parsed")
}

#[track_caller]
fn string(input: &str) -> String {
    value(input).as_str().expect("a string").to_owned()
}

// ----- keys ---------------------------------------------------------------

#[test]
fn bare_keys() {
    let table = doc("key = 1\nbare_key = 2\nbare-key = 3\n1234 = 4\n");
    assert_eq!(
        table.keys().collect::<Vec<_>>(),
        ["key", "bare_key", "bare-key", "1234"]
    );
}

#[test]
fn quoted_keys() {
    let table = doc(r#"
"127.0.0.1" = "value"
"character encoding" = "value"
"ʎǝʞ" = "value"
'quoted "value"' = "value"
"" = "empty, but valid"
"#);
    assert_eq!(table.len(), 5);
    assert!(table.contains_key("127.0.0.1"));
    assert!(table.contains_key("quoted \"value\""));
    // The empty key is valid, if discouraged; both quote styles name it.
    assert_eq!(table[""].as_str(), Some("empty, but valid"));
}

#[test]
fn dotted_keys() {
    let table = doc(r#"
name = "Orange"
physical.color = "orange"
physical.shape = "round"
site."google.com" = true
"#);
    assert_eq!(table["physical"]["color"].as_str(), Some("orange"));
    assert_eq!(table["site"]["google.com"].as_bool(), Some(true));
}

#[test]
fn whitespace_around_dots_is_ignored() {
    let table = doc("fruit . flavor = \"banana\"\n[ a . b ]\nc = 1\n");
    assert_eq!(table["fruit"]["flavor"].as_str(), Some("banana"));
    assert_eq!(table["a"]["b"]["c"].as_integer(), Some(1));
}

#[test]
fn dotted_keys_may_be_extended_by_a_later_sub_table() {
    // The spec's own example: `apple` is created by a dotted key, so
    // `[fruit.apple]` would be invalid, but adding a *new* sub-table is fine.
    let table = doc(r#"
[fruit]
apple.color = "red"
apple.taste.sweet = true

[fruit.apple.texture]
smooth = true
"#);
    assert_eq!(
        table["fruit"]["apple"]["taste"]["sweet"].as_bool(),
        Some(true)
    );
    assert_eq!(
        table["fruit"]["apple"]["texture"]["smooth"].as_bool(),
        Some(true)
    );
}

// ----- strings ------------------------------------------------------------

#[test]
fn basic_strings() {
    assert_eq!(
        string(r#""I'm a string. \"You can quote me\". Name\tJos\u00E9\nLocation\tSF.""#),
        "I'm a string. \"You can quote me\". Name\tJosé\nLocation\tSF."
    );
    assert_eq!(string(r#""\b\t\n\f\r\"\\""#), "\u{8}\t\n\u{c}\r\"\\");
    assert_eq!(string(r#""\U0001F600""#), "\u{1F600}");
    assert_eq!(string(r#""""#), "");
}

#[test]
fn literal_strings() {
    assert_eq!(
        string(r#"'C:\Users\nodejs\templates'"#),
        r"C:\Users\nodejs\templates"
    );
    assert_eq!(
        string(r#"'Tom "Dubs" Preston-Werner'"#),
        "Tom \"Dubs\" Preston-Werner"
    );
    assert_eq!(string("''"), "");
}

#[test]
fn multiline_basic_strings() {
    // The newline right after the opening delimiter is trimmed.
    assert_eq!(
        string("\"\"\"\nRoses are red\nViolets are blue\"\"\""),
        "Roses are red\nViolets are blue"
    );

    // A backslash at the end of a line eats the newline and the indentation.
    let folded = string(
        "\"\"\"\\\n     The quick brown \\\n     fox jumps over \\\n     the lazy dog.\\\n     \"\"\"",
    );
    assert_eq!(folded, "The quick brown fox jumps over the lazy dog.");

    // One or two quotes are content; three end the string.
    assert_eq!(
        string(r#""""Here are two quotation marks: "". Simple.""""#),
        "Here are two quotation marks: \"\". Simple."
    );
    assert_eq!(
        string(r#""""Here are three quotation marks: ""\".""""#),
        "Here are three quotation marks: \"\"\"."
    );
    assert_eq!(string("\"\"\"\"\"\""), "");
    assert_eq!(string("\"\"\"a\"\"\"\""), "a\"");
    assert_eq!(string("\"\"\"a\"\"\"\"\""), "a\"\"");
}

#[test]
fn multiline_literal_strings() {
    assert_eq!(
        string("'''\nThe first newline is\ntrimmed.'''"),
        "The first newline is\ntrimmed."
    );
    assert_eq!(
        string(r#"'''I [dw]on't need \d{2} apples'''"#),
        r"I [dw]on't need \d{2} apples"
    );
    assert_eq!(string("''''that's it'''"), "'that's it");
    assert_eq!(string("''''''"), "");
}

#[test]
fn crlf_is_normalized_inside_multiline_strings() {
    assert_eq!(string("\"\"\"\r\na\r\nb\"\"\""), "a\nb");
    assert_eq!(string("'''\r\na\r\nb'''"), "a\nb");
}

#[test]
fn tabs_are_allowed_in_strings() {
    assert_eq!(string("\"a\tb\""), "a\tb");
    assert_eq!(string("'a\tb'"), "a\tb");
}

// ----- numbers ------------------------------------------------------------

#[test]
fn integers() {
    for (input, expected) in [
        ("+99", 99),
        ("42", 42),
        ("0", 0),
        ("-17", -17),
        ("1_000", 1000),
        ("5_349_221", 5_349_221),
        ("0xDEADBEEF", 0xDEAD_BEEF),
        ("0xdead_beef", 0xDEAD_BEEF),
        ("0o01234567", 0o0123_4567),
        ("0o755", 0o755),
        ("0b11010110", 0b1101_0110),
        ("9223372036854775807", i64::MAX),
        ("-9223372036854775808", i64::MIN),
    ] {
        assert_eq!(value(input).as_integer(), Some(expected), "{input}");
    }
}

#[test]
fn floats() {
    for (input, expected) in [
        ("+1.0", 1.0),
        ("3.7415", 3.7415),
        ("-0.01", -0.01),
        ("5e+22", 5e22),
        ("1e06", 1e6),
        ("-2E-2", -2E-2),
        ("6.626e-34", 6.626e-34),
        ("224_617.445_991_228", 224_617.445_991_228),
        ("0.0", 0.0),
        ("-0.0", -0.0),
    ] {
        assert_eq!(value(input).as_float(), Some(expected), "{input}");
    }

    assert_eq!(value("inf").as_float(), Some(f64::INFINITY));
    assert_eq!(value("+inf").as_float(), Some(f64::INFINITY));
    assert_eq!(value("-inf").as_float(), Some(f64::NEG_INFINITY));
    for input in ["nan", "+nan", "-nan"] {
        assert!(
            value(input).as_float().expect("a float").is_nan(),
            "{input}"
        );
    }
}

#[test]
fn integers_and_floats_are_distinct_types() {
    assert!(value("1").is_integer());
    assert!(value("1.0").is_float());
    assert!(value("1e0").is_float());
    assert_eq!(value("1").as_float(), None);
}

#[test]
fn booleans() {
    assert_eq!(value("true").as_bool(), Some(true));
    assert_eq!(value("false").as_bool(), Some(false));
}

// ----- date-times ---------------------------------------------------------

#[test]
fn datetimes() {
    for (input, kind) in [
        ("1979-05-27T07:32:00Z", DatetimeKind::OffsetDatetime),
        ("1979-05-27T00:32:00-07:00", DatetimeKind::OffsetDatetime),
        ("1979-05-27 07:32:00Z", DatetimeKind::OffsetDatetime),
        (
            "1979-05-27T07:32:00.999999-07:00",
            DatetimeKind::OffsetDatetime,
        ),
        ("1979-05-27T07:32:00", DatetimeKind::LocalDatetime),
        ("1979-05-27T00:32:00.999999", DatetimeKind::LocalDatetime),
        ("1979-05-27", DatetimeKind::LocalDate),
        ("07:32:00", DatetimeKind::LocalTime),
        ("00:32:00.999999", DatetimeKind::LocalTime),
    ] {
        let value = value(input);
        assert_eq!(
            value.as_datetime().expect("a datetime").kind(),
            kind,
            "{input}"
        );
    }
}

#[test]
fn a_bare_date_in_an_array_is_not_followed_into_the_next_element() {
    let value = value("[1979-05-27, 1979-05-28]");
    let array = value.as_array().expect("an array");
    assert_eq!(array.len(), 2);
    assert_eq!(
        array[1]
            .as_datetime()
            .expect("a datetime")
            .date
            .expect("a date")
            .day,
        28
    );
}

// ----- arrays -------------------------------------------------------------

#[test]
fn arrays() {
    assert_eq!(value("[]").as_array().expect("an array").len(), 0);
    assert_eq!(value("[ 1, 2, 3 ]").as_array().expect("an array").len(), 3);
    assert_eq!(
        value(r#"[ "all", 'strings', """are the same""", '''type''' ]"#)
            .as_array()
            .expect("an array")
            .len(),
        4
    );
    // TOML 1.0 arrays may mix types.
    assert_eq!(
        value(r#"[ 0.1, 1, "a", true, [1] ]"#)
            .as_array()
            .expect("an array")
            .len(),
        5
    );
}

#[test]
fn arrays_may_span_lines_with_comments_and_a_trailing_comma() {
    let value = value("[\n  1, # the first\n  2, # the second\n\n  # a lone comment\n  3,\n]");
    assert_eq!(value.as_array().expect("an array").len(), 3);
}

// ----- tables -------------------------------------------------------------

#[test]
fn tables() {
    let table = doc(r#"
[table-1]
key1 = "some string"
key2 = 123

[table-2]
key1 = "another string"
"#);
    assert_eq!(table["table-1"]["key2"].as_integer(), Some(123));
    assert_eq!(table["table-2"]["key1"].as_str(), Some("another string"));
}

#[test]
fn nested_tables_may_arrive_out_of_order() {
    let table = doc("[x.y.z.w]\na = 1\n\n[x]\nb = 2\n");
    assert_eq!(table["x"]["y"]["z"]["w"]["a"].as_integer(), Some(1));
    assert_eq!(table["x"]["b"].as_integer(), Some(2));
}

#[test]
fn empty_tables_are_kept() {
    let table = doc("[a]\n[b]\n");
    assert!(table["a"].as_table().expect("a table").is_empty());
    assert_eq!(table.len(), 2);
}

#[test]
fn a_table_name_may_be_quoted() {
    let table = doc(r#"["a.b"]
c = 1
"#);
    assert_eq!(table["a.b"]["c"].as_integer(), Some(1));
}

// ----- inline tables ------------------------------------------------------

#[test]
fn inline_tables() {
    let table = doc(r#"
name = { first = "Tom", last = "Preston-Werner" }
point = { x = 1, y = 2 }
animal = { type.name = "pug" }
empty = {}
"#);
    assert_eq!(table["name"]["first"].as_str(), Some("Tom"));
    assert_eq!(table["animal"]["type"]["name"].as_str(), Some("pug"));
    assert!(table["empty"].as_table().expect("a table").is_empty());
}

#[test]
fn inline_tables_may_use_several_dotted_keys_under_one_parent() {
    let table = doc(r#"a = { b.c = 1, b.d = 2 }"#);
    assert_eq!(table["a"]["b"]["c"].as_integer(), Some(1));
    assert_eq!(table["a"]["b"]["d"].as_integer(), Some(2));
}

// ----- arrays of tables ---------------------------------------------------

#[test]
fn arrays_of_tables() {
    let table = doc(r#"
[[products]]
name = "Hammer"
sku = 738594937

[[products]]  # an empty one

[[products]]
name = "Nail"
sku = 284758393
"#);
    let products = table["products"].as_array().expect("an array");
    assert_eq!(products.len(), 3);
    assert_eq!(products[0]["name"].as_str(), Some("Hammer"));
    assert!(products[1].as_table().expect("a table").is_empty());
    assert_eq!(products[2]["sku"].as_integer(), Some(284_758_393));
}

#[test]
fn nested_arrays_of_tables() {
    let table = doc(r#"
[[fruits]]
name = "apple"

[fruits.physical]
color = "red"
shape = "round"

[[fruits.varieties]]
name = "red delicious"

[[fruits.varieties]]
name = "granny smith"

[[fruits]]
name = "banana"

[[fruits.varieties]]
name = "plantain"
"#);
    let fruits = table["fruits"].as_array().expect("an array");
    assert_eq!(fruits.len(), 2);
    assert_eq!(fruits[0]["physical"]["color"].as_str(), Some("red"));
    assert_eq!(
        fruits[0]["varieties"].as_array().expect("an array").len(),
        2
    );
    assert_eq!(fruits[1]["varieties"][0]["name"].as_str(), Some("plantain"));
}

// ----- document structure -------------------------------------------------

#[test]
fn comments_and_blank_lines() {
    let table =
        doc("# a full-line comment\n\n\nkey = \"value\" # a trailing comment\n\n# another\n");
    assert_eq!(table.len(), 1);
}

#[test]
fn an_empty_document_is_valid() {
    assert!(doc("").is_empty());
    assert!(doc("\n\n# nothing here\n").is_empty());
}

#[test]
fn a_leading_byte_order_mark_is_ignored() {
    assert_eq!(doc("\u{feff}a = 1").len(), 1);
}

#[test]
fn crlf_line_endings() {
    let table = doc("a = 1\r\n[t]\r\nb = 2\r\n");
    assert_eq!(table["t"]["b"].as_integer(), Some(2));
}

#[test]
fn a_document_need_not_end_with_a_newline() {
    assert_eq!(doc("a = 1").len(), 1);
}

#[test]
fn values_may_nest_to_the_documented_limit() {
    let input = format!("a = {}1{}", "[".repeat(128), "]".repeat(128));
    assert!(doc(&input).contains_key("a"));
}
