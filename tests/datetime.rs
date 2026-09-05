//! The date-time types, which are the whole of the crate's API when it is
//! built without an allocator. Everything here runs in every configuration.

use tomlproc::{Datetime, DatetimeKind};

#[track_caller]
fn parse(s: &str) -> Datetime {
    s.parse()
        .unwrap_or_else(|e| panic!("expected `{s}` to parse, got: {e}"))
}

#[test]
fn round_trips() {
    for s in [
        "1979-05-27T07:32:00Z",
        "1979-05-27T00:32:00-07:00",
        "1979-05-27T00:32:00.999999+01:30",
        "1979-05-27T07:32:00",
        "1979-05-27",
        "07:32:00",
        "00:32:00.999",
        "2024-02-29T23:59:60Z",
    ] {
        assert_eq!(parse(s).to_string(), s, "{s}");
    }
}

#[test]
fn the_written_form_is_normalized() {
    // A space separator becomes `T`, a lowercase `z` becomes `Z`, and a time
    // written without seconds gets them.
    assert_eq!(
        parse("1979-05-27 07:32:00Z").to_string(),
        "1979-05-27T07:32:00Z"
    );
    assert_eq!(
        parse("1979-05-27t07:32:00z").to_string(),
        "1979-05-27T07:32:00Z"
    );
    assert_eq!(parse("14:15").to_string(), "14:15:00");
    assert_eq!(parse("2010-02-03 14:15").to_string(), "2010-02-03T14:15:00");
}

#[test]
fn kinds() {
    assert_eq!(
        parse("1979-05-27T07:32:00Z").kind(),
        DatetimeKind::OffsetDatetime
    );
    assert_eq!(
        parse("1979-05-27T07:32:00").kind(),
        DatetimeKind::LocalDatetime
    );
    assert_eq!(parse("1979-05-27").kind(), DatetimeKind::LocalDate);
    assert_eq!(parse("07:32:00").kind(), DatetimeKind::LocalTime);
}

#[test]
fn fractional_seconds_beyond_nanoseconds_are_truncated() {
    assert_eq!(
        parse("07:32:00.1234567891").time.unwrap().nanosecond,
        123_456_789
    );
    assert_eq!(parse("07:32:00.5").to_string(), "07:32:00.5");
}

#[test]
fn seconds_are_optional() {
    assert_eq!(parse("07:32").time.unwrap().second, 0);
    assert_eq!(
        parse("2010-02-03T14:15Z").kind(),
        DatetimeKind::OffsetDatetime
    );
    // A fraction still belongs to the seconds it is written after.
    assert!("07:32.5".parse::<Datetime>().is_err());
}

#[test]
fn rejects_invalid() {
    for s in [
        "1979-05-32",
        "1979-13-01",
        "2023-02-29",
        "1979-05-27T24:00:00",
        "1979-05-27T07:61:00",
        "1979-05-27T07:32:00+24:00",
        "1979-05-27T07:32:00.",
        "1979-05-2",
        "1979-05-27T07:32:00Z trailing",
    ] {
        assert!(s.parse::<Datetime>().is_err(), "{s} should not parse");
    }
}

#[test]
fn errors_are_readable() {
    let error = "1979-05-32".parse::<Datetime>().unwrap_err();
    assert_eq!(error.message(), "date is out of range");
    assert!(!error.has_position());
}
