//! Recording where each value was written.

use tomlproc::{Value, parse, parse_spans};

const DOCUMENT: &str = r#"# a comment
title = "Example"
physical.color = "orange"

[owner]
name = "Tom"
ports = [80, 443]
inline = { a = 1 }

[[server]]
ip = "10.0.0.1"

[[server]]
ip = "10.0.0.2"
"#;

#[test]
fn spans_point_at_the_source() {
    let (_, spans) = parse_spans(DOCUMENT).unwrap();

    let span = spans.get("title").unwrap();
    assert_eq!((span.line, span.column), (2, 1));
    assert_eq!(&DOCUMENT[span.range.clone()], "title = \"Example\"");
    assert_eq!(&DOCUMENT[span.value.clone()], "\"Example\"");

    // A dotted key is one pair, under its full path.
    let span = spans.get("physical.color").unwrap();
    assert_eq!(&DOCUMENT[span.range.clone()], "physical.color = \"orange\"");
    assert_eq!(span.line, 3);

    // A value inside a table carries the table in its path.
    let span = spans.get("owner.name").unwrap();
    assert_eq!(&DOCUMENT[span.value.clone()], "\"Tom\"");
    assert_eq!(span.line, 6);

    // Table headers get a span of their own.
    assert_eq!(
        &DOCUMENT[spans.get("owner").unwrap().range.clone()],
        "[owner]"
    );
    assert_eq!(
        &DOCUMENT[spans.get("server.1").unwrap().range.clone()],
        "[[server]]"
    );
    assert_eq!(spans.get("server.1").unwrap().line, 13);
    assert_eq!(
        &DOCUMENT[spans.get("server.1.ip").unwrap().value.clone()],
        "\"10.0.0.2\""
    );
}

#[test]
fn a_path_inside_a_value_falls_back_to_the_value() {
    let (_, spans) = parse_spans(DOCUMENT).unwrap();

    // Array elements have no span of their own...
    assert_eq!(spans.get_exact("owner.ports.1"), None);
    // ...so the array they are in is what gets underlined.
    assert_eq!(spans.get("owner.ports.1"), spans.get_exact("owner.ports"));
    assert_eq!(spans.get("owner.inline.a"), spans.get_exact("owner.inline"));
    assert_eq!(spans.get("nothing.like.this"), None);
}

#[test]
fn columns_count_characters_and_survive_crlf() {
    let source = "# héllo\r\n\"kéy\" = 1\r\n";
    let (_, spans) = parse_spans(source).unwrap();
    let span = spans.get("kéy").unwrap();
    assert_eq!((span.line, span.column), (2, 1));
    assert_eq!(&source[span.value.clone()], "1");

    // A bare key is ASCII, so a non-ASCII one has to be quoted -- and the
    // column is still counted in characters, not bytes.
    let source = "  \"déep\" = [1]";
    let (_, spans) = parse_spans(source).unwrap();
    assert_eq!(spans.get("déep").unwrap().column, 3);
}

#[test]
fn every_recorded_value_re_parses_to_what_it_stands_for() {
    let (doc, spans) = parse_spans(DOCUMENT).unwrap();
    let doc = Value::Table(doc);

    let mut checked = 0;
    for (path, span) in spans.iter() {
        let text = &DOCUMENT[span.value.clone()];
        let value = doc
            .get_path(path)
            .expect("a recorded path exists in the document");
        // Table headers stand for the header line, not for a value.
        if value.is_table() && text.starts_with('[') {
            continue;
        }
        let reparsed =
            parse(&format!("x = {text}")).unwrap_or_else(|e| panic!("{path}: {text}: {e}"));
        assert_eq!(&reparsed["x"], value, "{path}");
        checked += 1;
    }
    assert_eq!(checked, 7);
}

#[test]
fn parse_and_parse_spans_agree_on_the_document() {
    assert_eq!(parse_spans(DOCUMENT).unwrap().0, parse(DOCUMENT).unwrap());
}

#[cfg(feature = "serde")]
#[test]
fn a_serde_error_points_back_at_the_source() {
    #[derive(serde::Deserialize, Debug)]
    struct Config {
        server: Vec<Server>,
    }

    #[derive(serde::Deserialize, Debug)]
    struct Server {
        ip: std::net::IpAddr,
    }

    let source = "[[server]]\nip = \"10.0.0.1\"\n\n[[server]]\nip = \"not an address\"\n";
    let (doc, spans) = parse_spans(source).unwrap();

    let error = tomlproc::serde::from_table::<Config>(doc).unwrap_err();
    assert_eq!(error.key_path().as_deref(), Some("server.1.ip"));

    // The good half of the document does deserialize.
    let good = tomlproc::serde::from_str::<Config>(&source[..27]).unwrap();
    assert_eq!(good.server[0].ip.to_string(), "10.0.0.1");

    let span = spans.get(&error.key_path().unwrap()).unwrap();
    assert_eq!(span.line, 5);
    assert_eq!(&source[span.value.clone()], "\"not an address\"");
}
