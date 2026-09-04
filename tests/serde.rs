//! The `serde` feature: mapping documents onto Rust types.

#![cfg(feature = "serde")]

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use tomlproc::serde::{from_str, from_value, to_string, to_string_pretty, to_value};
use tomlproc::{Datetime, Table, Value};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Config {
    title: String,
    port: u16,
    ratio: f64,
    enabled: bool,
    tags: Vec<String>,
    owner: Owner,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
    servers: BTreeMap<String, Server>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Owner {
    name: String,
    dob: Datetime,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Server {
    ip: String,
    role: Role,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[serde(rename_all = "lowercase")]
enum Role {
    Frontend,
    Backend,
}

const DOCUMENT: &str = r#"
title = "Example"
port = 8080
ratio = 0.5
enabled = true
tags = ["a", "b"]

[owner]
name = "Tom"
dob = 1979-05-27T07:32:00-08:00

[servers.alpha]
ip = "10.0.0.1"
role = "frontend"

[servers.beta]
ip = "10.0.0.2"
role = "backend"
"#;

fn example() -> Config {
    Config {
        title: "Example".into(),
        port: 8080,
        ratio: 0.5,
        enabled: true,
        tags: vec!["a".into(), "b".into()],
        owner: Owner {
            name: "Tom".into(),
            dob: "1979-05-27T07:32:00-08:00".parse().unwrap(),
        },
        comment: None,
        servers: BTreeMap::from([
            (
                "alpha".into(),
                Server {
                    ip: "10.0.0.1".into(),
                    role: Role::Frontend,
                },
            ),
            (
                "beta".into(),
                Server {
                    ip: "10.0.0.2".into(),
                    role: Role::Backend,
                },
            ),
        ]),
    }
}

#[test]
fn deserializes_a_document() {
    assert_eq!(from_str::<Config>(DOCUMENT).unwrap(), example());
}

#[test]
fn round_trips_through_a_document() {
    let written = to_string(&example()).unwrap();
    assert_eq!(from_str::<Config>(&written).unwrap(), example());
    // ...and the document is the idiomatic one, not a wall of inline tables.
    assert!(written.contains("\n[owner]\n"), "{written}");
    assert!(written.contains("\n[servers.alpha]\n"), "{written}");
    assert!(
        to_string_pretty(&example())
            .unwrap()
            .contains("tags = [\n    \"a\",\n")
    );
}

#[test]
fn datetimes_survive_the_round_trip() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Times {
        offset: Datetime,
        local: Datetime,
        date: Datetime,
        time: Datetime,
    }

    let input = "\
offset = 1979-05-27T07:32:00Z
local = 1979-05-27T07:32:00
date = 1979-05-27
time = 07:32:00.999
";
    let times: Times = from_str(input).unwrap();
    assert_eq!(times.date.date.unwrap().year, 1979);
    assert_eq!(to_string(&times).unwrap(), input);
}

#[test]
fn a_datetime_reads_from_a_plain_string_in_other_formats() {
    // What a format with no date type hands over.
    use serde::de::IntoDeserializer;
    use serde::de::value::StrDeserializer;

    let deserializer: StrDeserializer<'_, serde::de::value::Error> =
        "1979-05-27T07:32:00Z".into_deserializer();
    let datetime = Datetime::deserialize(deserializer).unwrap();
    assert_eq!(datetime.to_string(), "1979-05-27T07:32:00Z");
}

// ----- options and absent keys --------------------------------------------

#[test]
fn none_is_an_absent_key() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Options {
        set: Option<u8>,
        unset: Option<u8>,
    }

    let options = Options {
        set: Some(1),
        unset: None,
    };
    assert_eq!(to_string(&options).unwrap(), "set = 1\n");
    assert_eq!(from_str::<Options>("set = 1").unwrap(), options);
    assert_eq!(
        from_str::<Options>("").unwrap(),
        Options {
            set: None,
            unset: None
        }
    );
}

#[test]
fn a_none_with_nowhere_to_go_is_an_error() {
    assert!(to_value(&None::<u8>).is_err());
    assert!(to_value(&vec![None::<u8>]).is_err());
    assert!(to_value(&()).is_err());
}

// ----- enums --------------------------------------------------------------

#[derive(Serialize, Deserialize, PartialEq, Debug)]
enum Shape {
    Empty,
    Radius(f64),
    Pair(u8, u8),
    Rect { w: u8, h: u8 },
}

#[test]
fn every_enum_shape_round_trips() {
    for (shape, expected) in [
        (Shape::Empty, "shape = \"Empty\"\n"),
        (Shape::Radius(1.5), "[shape]\nRadius = 1.5\n"),
        (Shape::Pair(1, 2), "[shape]\nPair = [1, 2]\n"),
        (Shape::Rect { w: 1, h: 2 }, "[shape.Rect]\nw = 1\nh = 2\n"),
    ] {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Holder {
            shape: Shape,
        }

        let holder = Holder { shape };
        let written = to_string(&holder).unwrap();
        assert_eq!(written, expected);
        assert_eq!(from_str::<Holder>(&written).unwrap(), holder);
    }
}

#[test]
fn internally_tagged_enums_work() {
    // Internal tagging makes serde buffer the whole value and replay it, which
    // only works if the deserializer is fully self-describing.
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    #[serde(tag = "type")]
    enum Message {
        Ping { id: u32 },
        Pong { id: u32, delay: f64 },
    }

    let message = Message::Pong { id: 7, delay: 0.5 };
    let written = to_string(&message).unwrap();
    assert_eq!(written, "type = \"Pong\"\nid = 7\ndelay = 0.5\n");
    assert_eq!(from_str::<Message>(&written).unwrap(), message);
}

#[test]
fn flattened_fields_work() {
    // `flatten` replays a buffered map, the other classic self-describing test.
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Outer {
        name: String,
        #[serde(flatten)]
        inner: Inner,
        #[serde(flatten)]
        rest: BTreeMap<String, Value>,
    }

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Inner {
        port: u16,
    }

    let input = "name = \"a\"\nport = 1\nextra = \"x\"\n";
    let outer: Outer = from_str(input).unwrap();
    assert_eq!(outer.inner.port, 1);
    assert_eq!(outer.rest["extra"], Value::from("x"));
    assert_eq!(
        from_str::<Outer>(&to_string(&outer).unwrap()).unwrap(),
        outer
    );
}

// ----- maps ---------------------------------------------------------------

#[test]
fn map_keys_that_are_not_strings() {
    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct Maps {
        by_number: BTreeMap<u32, String>,
        by_variant: BTreeMap<Role, u8>,
    }

    let maps = Maps {
        by_number: BTreeMap::from([(1, "one".into()), (2, "two".into())]),
        by_variant: BTreeMap::from([(Role::Frontend, 1)]),
    };
    let written = to_string(&maps).unwrap();
    assert!(written.contains("1 = \"one\""), "{written}");
    assert!(written.contains("frontend = 1"), "{written}");
    assert_eq!(from_str::<Maps>(&written).unwrap(), maps);
}

#[test]
fn a_hash_map_of_values_holds_anything() {
    let map: HashMap<String, Value> = from_str("a = 1\nb = [1, 'x']\nc = 1979-05-27").unwrap();
    assert_eq!(map["a"], Value::Integer(1));
    assert_eq!(map["b"].as_array().unwrap().len(), 2);
    // The date-time survives being read into an untyped value.
    assert!(map["c"].is_datetime(), "{:?}", map["c"]);
}

// ----- the value model itself ---------------------------------------------

#[test]
fn values_and_tables_serialize_as_themselves() {
    let doc: Table = tomlproc::parse(DOCUMENT).unwrap();
    assert_eq!(to_value(&doc).unwrap(), Value::Table(doc.clone()));
    assert_eq!(from_value::<Table>(Value::Table(doc.clone())).unwrap(), doc);
    assert_eq!(to_string(&doc).unwrap(), tomlproc::to_string(&doc));
}

#[test]
fn a_value_survives_a_round_trip_through_serde() {
    let doc: Table = tomlproc::parse(DOCUMENT).unwrap();
    let value = to_value(&doc).unwrap();
    let back: Value = from_value(value).unwrap();
    assert_eq!(back, Value::Table(doc));
}

// ----- errors -------------------------------------------------------------

#[test]
fn errors_name_the_key_that_did_not_fit() {
    let error = from_str::<Config>("title = 1").unwrap_err();
    assert_eq!(error.key_path().as_deref(), Some("title"));
    assert!(
        error.to_string().contains("invalid type: integer `1`"),
        "{error}"
    );

    let error = from_str::<Config>(&DOCUMENT.replace("ip = \"10.0.0.2\"", "ip = 2")).unwrap_err();
    assert_eq!(error.key_path().as_deref(), Some("servers.beta.ip"));

    let error = from_str::<Config>(&DOCUMENT.replace("[\"a\", \"b\"]", "[\"a\", 2]")).unwrap_err();
    assert_eq!(error.key_path().as_deref(), Some("tags.1"));
}

#[test]
fn a_syntax_error_still_reports_its_position() {
    let error = from_str::<Config>("title = ").unwrap_err();
    assert!(error.has_position(), "{error}");
    assert_eq!(error.line(), 1);
    assert_eq!(error.key_path(), None);
}

#[test]
fn what_toml_cannot_represent_is_refused() {
    // No byte strings.
    assert!(to_value(&serde_bytes_stand_in()).is_err());
    // No integers past i64.
    assert!(to_value(&u64::MAX).is_err());
    // A document is a table.
    assert!(to_string(&[1, 2, 3]).is_err());
    assert!(to_string(&"just a string").is_err());
}

/// A value that serializes through `serialize_bytes`.
fn serde_bytes_stand_in() -> impl Serialize {
    struct Bytes;
    impl Serialize for Bytes {
        fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(&[1, 2, 3])
        }
    }
    Bytes
}

#[test]
fn missing_and_unknown_fields() {
    #[derive(Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    struct Strict {
        needed: u8,
    }

    assert!(
        from_str::<Strict>("")
            .unwrap_err()
            .to_string()
            .contains("missing field")
    );
    assert_eq!(from_str::<Strict>("needed = 1").unwrap().needed, 1);
    let error = from_str::<Strict>("needed = 1\nextra = 2").unwrap_err();
    assert!(error.to_string().contains("unknown field"), "{error}");
}
