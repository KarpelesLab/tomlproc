//! Documents the specification calls invalid. A parser that accepts these is
//! not a TOML parser.

use tomlproc::parse;

#[track_caller]
fn rejected(input: &str) {
    match parse(input) {
        Ok(table) => panic!("expected `{input}` to be rejected, got {table:?}"),
        Err(error) => assert!(error.line() > 0, "`{input}` should report a position"),
    }
}

#[track_caller]
fn rejected_all(inputs: &[&str]) {
    for input in inputs {
        rejected(input);
    }
}

#[test]
fn malformed_key_value_pairs() {
    rejected_all(&[
        "key = # INVALID",
        "key =",
        "= 1",
        "key",
        "key value",
        "first = \"Tom\" last = \"Preston-Werner\"",
        "a = 1 b = 2",
        "= \"no key name\"",
        "a.= 1",
        ".a = 1",
        "a..b = 1",
    ]);
}

#[test]
fn duplicate_keys() {
    rejected_all(&[
        "name = \"Tom\"\nname = \"Pradyun\"",
        "spelling = \"favorite\"\n\"spelling\" = \"favourite\"",
        // A dotted key cannot overwrite the value it would have to descend
        // into.
        "fruit.apple = 1\nfruit.apple.smooth = true",
        "a = 1\na.b = 2",
        "[t]\nb = 1\nb.c = 2",
    ]);
}

#[test]
fn tables_cannot_be_defined_twice() {
    rejected_all(&[
        "[fruit]\napple = \"red\"\n\n[fruit]\norange = \"orange\"",
        "[fruit]\napple = \"red\"\n\n[fruit.apple]\ntexture = \"smooth\"",
        "[a]\n[a.b]\n[a.b]",
        // `[fruit.apple]` exists only because a dotted key made it, so it may
        // not be claimed by a header.
        "[fruit]\napple.color = \"red\"\n\n[fruit.apple]\nshape = \"round\"",
        "[fruit]\napple.taste.sweet = true\n\n[fruit.apple.taste]\nsour = false",
    ]);
}

#[test]
fn a_dotted_key_cannot_reach_into_a_table_defined_by_a_header() {
    rejected_all(&[
        "[a.b]\nc = 1\n\n[a]\nb.d = 2",
        "[a]\n[a.b]\nc = 1\n[a]\nb.d = 2",
    ]);
}

#[test]
fn inline_tables_are_sealed() {
    rejected_all(&[
        "product = { name = \"Nail\" }\nproduct.sku = 738594937",
        "[product]\ndetails = { color = \"gray\" }\n\n[product.details.more]\nx = 1",
        "a = { b = 1 }\n[a]",
        "a = { b = {} }\n[a.b]",
        "a = { b = 1 }\n[[a]]",
    ]);
}

#[test]
fn inline_tables_must_fit_on_one_line() {
    rejected_all(&[
        "a = {\n  b = 1\n}",
        "a = { b = 1,\n c = 2 }",
        // No trailing comma, unlike arrays.
        "a = { b = 1, }",
        "a = { , }",
        "a = { b = 1 c = 2 }",
    ]);
}

#[test]
fn arrays_of_tables_and_tables_do_not_mix() {
    rejected_all(&[
        "[fruit]\nname = \"apple\"\n\n[[fruit]]\nname = \"orange\"",
        "[[fruit]]\nname = \"apple\"\n\n[fruit]\nname = \"orange\"",
        // A static array is not an array of tables.
        "fruit = []\n[[fruit]]",
        "fruit = [{}]\n[[fruit]]",
        // A header cannot descend into a static array either.
        "points = [{ x = 1 }]\n[points.extra]",
    ]);
}

#[test]
fn malformed_headers() {
    rejected_all(&[
        "[]",
        "[a",
        "a]",
        "[[a]",
        "[a]]",
        "[a.]",
        "[.a]",
        "[a..b]",
        "[a] b = 1",
        "[[]]",
    ]);
}

#[test]
fn malformed_strings() {
    rejected_all(&[
        "a = \"unterminated",
        "a = 'unterminated",
        "a = \"\"\"unterminated",
        "a = '''unterminated",
        "a = \"a\nb\"",
        "a = 'a\nb'",
        "a = \"\\q\"",
        "a = \"\\u00\"",
        "a = \"\\uD800\"",
        "a = \"\\U00110000\"",
        "a = \"basic \\ backslash\"",
        // Four or more closing quotes leave a stray delimiter behind.
        "a = \"\"\"abc\"\"\"\"\"\"",
    ]);
}

#[test]
fn control_characters_are_rejected() {
    rejected_all(&[
        "a = \"line\u{0}break\"",
        "a = 'line\u{1}break'",
        "a = \"\"\"line\u{7f}break\"\"\"",
        "# comment with a \u{0} in it\na = 1",
        // A bare carriage return is not a newline.
        "a = 1\rb = 2",
    ]);
}

#[test]
fn malformed_numbers() {
    rejected_all(&[
        "a = 1_",
        "a = _1",
        "a = 1__2",
        "a = 0_x1",
        "a = 01",
        "a = -01",
        "a = 0xg",
        "a = 0x",
        "a = +0x1",
        "a = 1.",
        "a = .1",
        "a = 1.2.3",
        "a = 1e",
        "a = 1e+",
        "a = 1.e2",
        "a = 3.e+20",
        "a = INF",
        "a = Inf",
        "a = NaN",
        "a = True",
        "a = tru",
        "a = truefalse",
        // Outside the range of a signed 64-bit integer.
        "a = 9223372036854775808",
        "a = -9223372036854775809",
        "a = 0xFFFFFFFFFFFFFFFFF",
        // Overflowing to infinity is a mistake, not a way to write `inf`.
        "a = 1e501",
        "a = -1e501",
    ]);
}

#[test]
fn malformed_datetimes() {
    rejected_all(&[
        "a = 1979-05-99",
        "a = 1979-13-01",
        "a = 2023-02-29",
        "a = 1979-05-27T25:32:00Z",
        // A leap second (:60) is allowed; :61 is not.
        "a = 1979-05-27T07:32:61Z",
        "a = 1979-05-27T07:32:00+25:00",
        "a = 1979-05-27T07:32:00.Z",
        "a = 1979-05-27T07:32Z",
        "a = 07:32",
        "a = 1979-05-27TZ",
    ]);
}

#[test]
fn malformed_arrays() {
    rejected_all(&[
        "a = [",
        "a = [1",
        "a = [1,,2]",
        "a = [,]",
        "a = [1 2]",
        "a = ]",
    ]);
}

#[test]
fn errors_point_at_the_problem() {
    let error = parse("a = 1\nb = 2\nc = @\n").unwrap_err();
    assert_eq!((error.line(), error.column()), (3, 5));

    let error = parse("[a]\n[a]\n").unwrap_err();
    assert_eq!(error.line(), 2);
    assert!(error.message().contains("already defined"), "{error}");

    // A column counts characters, not bytes.
    let error = parse("k = \"héllo\" x").unwrap_err();
    assert_eq!(error.column(), 13);
}

#[test]
fn nesting_is_bounded() {
    // Parsing is recursive, so unbounded nesting would exhaust the stack
    // instead of returning an error.
    rejected(&format!("a = {}1{}", "[".repeat(129), "]".repeat(129)));
    rejected(&format!(
        "a = {}1{}",
        "{ b = ".repeat(129),
        " }".repeat(129)
    ));
    rejected(&"a = [".repeat(200_000));
}
