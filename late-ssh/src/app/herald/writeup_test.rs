use super::state::ProjectRefusal;
use super::writeup::{Writeup, parse};

/// Every verdict the prompt offers parses; a feature's words come back
/// trimmed, a refusal keeps its reason for the operator.
#[test]
fn every_verdict_parses() {
    assert_eq!(
        parse(r#"{"verdict": "feature", "text": "  A tiny terminal clock. \n", "reason": ""}"#)
            .expect("feature"),
        Writeup::Words("A tiny terminal clock.".to_string())
    );
    for (verdict, refusal) in [
        ("commercial", ProjectRefusal::Commercial),
        ("unfit", ProjectRefusal::Unfit),
        ("unclear", ProjectRefusal::Unclear),
    ] {
        assert_eq!(
            parse(&format!(
                r#"{{"verdict": "{verdict}", "text": "", "reason": " why "}}"#
            ))
            .expect(verdict),
            Writeup::Refused {
                refusal,
                reason: "why".to_string(),
            }
        );
    }
}

/// A feature with no words, an unknown verdict and anything but the object are
/// errors: the shape is only the prompt's, so nothing here is guessed.
#[test]
fn a_bad_answer_is_refused() {
    assert!(parse(r#"{"verdict": "feature", "text": "   "}"#).is_err());
    assert!(parse(r#"{"verdict": "maybe", "text": "words"}"#).is_err());
    assert!(parse(r#"{"text": "words"}"#).is_err());
    assert!(parse("not json").is_err());
}
