use super::*;

#[test]
fn every_unit_parses() {
    let cases = [
        ("500ms", 500),
        ("30s", 30_000),
        ("5m", 300_000),
        ("6h", 21_600_000),
        ("180d", 15_552_000_000),
    ];
    for (text, millis) in cases {
        let parsed = DurationSetting::parse(text).expect(text);
        assert_eq!(parsed.as_millis(), millis, "on {text}");
    }
}

#[test]
fn ms_wins_over_s() {
    assert_eq!(DurationSetting::parse("5ms").expect("5ms").as_millis(), 5);
    assert_eq!(DurationSetting::parse("5s").expect("5s").as_millis(), 5_000);
}

#[test]
fn a_duration_round_trips_through_its_written_form() {
    for text in ["72h", "0s", "1ms", "180d"] {
        let parsed = DurationSetting::parse(text).expect(text);
        assert_eq!(parsed.to_string(), text);
    }
}

#[test]
fn the_defaults_the_schema_documents_parse() {
    for text in ["6h", "72h", "24h", "60s", "2h"] {
        assert!(DurationSetting::parse(text).is_ok(), "{text}");
    }
}

#[test]
fn a_value_without_a_unit_is_rejected() {
    for text in [
        "30", "", "s", "ms", "-5s", "1.5s", "30 s", "30sec", "thirty",
    ] {
        assert_eq!(
            DurationSetting::parse(text),
            Err(DurationError::Shape),
            "`{text}` must not parse"
        );
    }
}

#[test]
fn surrounding_space_is_trimmed() {
    assert_eq!(
        DurationSetting::parse("  6h ").expect("6h").as_millis(),
        21_600_000
    );
}

#[test]
fn a_duration_past_the_representable_range_is_rejected() {
    let text = format!("{}d", u64::MAX);
    assert_eq!(
        DurationSetting::parse(&text),
        Err(DurationError::Overflow),
        "{text}"
    );
}

#[test]
fn json_is_the_written_string() {
    let value = DurationSetting::parse("72h").expect("72h");
    let json = serde_json::to_string(&value).expect("serialize");
    assert_eq!(json, "\"72h\"");
    let back: DurationSetting = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, value);
}

#[test]
fn a_bad_json_duration_names_the_value() {
    let error = serde_json::from_str::<DurationSetting>("\"30 seconds\"").expect_err("rejected");
    assert!(error.to_string().contains("30 seconds"), "{error}");
}
