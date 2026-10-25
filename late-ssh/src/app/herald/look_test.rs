use std::io::Cursor;

use image::{ImageFormat, ImageReader};

use super::bluesky_test::png;
use super::look::{parse, quarters, schema};
use super::state::{ALT_MAX_CHARS, Look, Warning};

fn answer(alt: &str, warning: &str, reason: &str) -> String {
    serde_json::json!({ "alt": alt, "warning": warning, "reason": reason }).to_string()
}

/// Every warning the schema offers parses to its own variant, `none` to
/// a clean look, and the text comes back trimmed.
#[test]
fn every_warning_in_the_schema_parses() {
    let offered: Vec<String> = schema()["properties"]["warning"]["enum"]
        .as_array()
        .expect("enum")
        .iter()
        .map(|value| value.as_str().expect("string").to_string())
        .collect();
    let parsed: Vec<Option<Warning>> = offered
        .iter()
        .map(
            |warning| match parse(&answer(" A cat. ", warning, " why ")) {
                Ok(Look::Seen {
                    alt,
                    warning,
                    reason,
                }) => {
                    assert_eq!((alt.as_str(), reason.as_str()), ("A cat.", "why"));
                    warning
                }
                other => panic!("{warning} parsed to {other:?}"),
            },
        )
        .collect();
    assert_eq!(
        parsed,
        vec![
            None,
            Some(Warning::Nudity),
            Some(Warning::Suggestive),
            Some(Warning::Explicit),
            Some(Warning::Gore),
            Some(Warning::Offensive),
        ]
    );
}

/// An answer outside the closed set, an empty description or a body that
/// is not the schema's is refused, never guessed into a look.
#[test]
fn a_bad_answer_is_refused() {
    let unknown = parse(&answer("A cat.", "spicy", "")).expect_err("unknown warning");
    assert!(unknown.to_string().contains("spicy"), "{unknown}");
    assert!(parse(&answer("   ", "none", "")).is_err(), "empty alt");
    assert!(parse("{\"alt\": \"A cat.\"}").is_err(), "missing fields");
}

/// An over-long description is cut to the cap with an ellipsis, on a
/// character boundary.
#[test]
fn a_long_alt_is_cut_at_the_cap() {
    let long = "é".repeat(ALT_MAX_CHARS + 50);
    let Look::Seen { alt, .. } = parse(&answer(&long, "none", "")).expect("parse") else {
        panic!("seen");
    };
    assert_eq!(alt.chars().count(), ALT_MAX_CHARS);
    assert!(alt.ends_with('…'), "{alt}");
    let exact = "a".repeat(ALT_MAX_CHARS);
    let Look::Seen { alt, .. } = parse(&answer(&exact, "none", "")).expect("parse") else {
        panic!("seen");
    };
    assert_eq!(alt, exact, "at the cap is kept whole");
}

/// The quarters tile the picture: an odd side's extra pixel goes to the
/// right and bottom quarters.
#[test]
fn the_quarters_tile_the_picture() {
    let sizes: Vec<(u32, u32)> = quarters(&png(641, 201, false))
        .expect("quarters")
        .iter()
        .map(|quarter| {
            ImageReader::with_format(Cursor::new(quarter), ImageFormat::Png)
                .into_dimensions()
                .expect("a png")
        })
        .collect();
    assert_eq!(sizes, vec![(320, 100), (321, 100), (320, 101), (321, 101)]);
}
