//! The examples from `README.md`, kept compiling so the README cannot drift
//! away from the API.

#![cfg(feature = "alloc")]

#[test]
fn readme_examples() {
    let doc = tomlproc::parse(
        r#"
        title = "TOML Example"

        [owner]
        name = "Tom Preston-Werner"
        dob = 1979-05-27T07:32:00-08:00

        [[server]]
        ip = "10.0.0.1"
        ports = [8000, 8001]
    "#,
    )
    .unwrap();

    assert_eq!(doc["title"].as_str(), Some("TOML Example"));
    assert_eq!(
        doc["owner"]["dob"]
            .as_datetime()
            .unwrap()
            .date
            .unwrap()
            .year,
        1979
    );
    assert_eq!(doc["server"][0]["ports"][1].as_integer(), Some(8001));

    let value = tomlproc::Value::Table(doc);
    assert_eq!(
        value.get_path("server.0.ip").and_then(|v| v.as_str()),
        Some("10.0.0.1")
    );

    let error = tomlproc::parse("a = 1\nb = [1, 2").unwrap_err();
    assert_eq!(error.line(), 2);
    assert_eq!(
        error.to_string(),
        "TOML parse error at line 2, column 5: unterminated array"
    );

    let mut package = tomlproc::Table::new();
    package.insert("name", "tomlproc");
    package.insert("keywords", vec!["toml", "parser"]);
    let mut doc = tomlproc::Table::new();
    doc.insert("package", package);
    assert_eq!(
        tomlproc::to_string(&doc),
        "[package]\nname = \"tomlproc\"\nkeywords = [\"toml\", \"parser\"]\n",
    );
}
