use super::brief::parse;

/// The answer's words come back trimmed; an empty answer and one that is
/// not the schema's are refused.
#[test]
fn the_brief_parses_its_words_and_refuses_an_empty_answer() {
    assert_eq!(
        parse(r#"{"text": "  A new late CLI is out. \n"}"#).expect("words"),
        "A new late CLI is out."
    );
    assert!(parse(r#"{"text": "   "}"#).is_err());
    assert!(parse(r#"{"words": "A new late CLI is out."}"#).is_err());
    assert!(parse("not json").is_err());
}
