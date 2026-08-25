use super::*;

#[test]
fn an_allowed_verdict_carries_no_refusal() {
    assert_eq!(
        parse_title_screen(r#"{"allowed": true, "reason": ""}"#),
        TitleScreen::Allowed
    );
    // A model that allows but still writes a reason is still an allow.
    assert_eq!(
        parse_title_screen(r#"{"allowed": true, "reason": "reads fine"}"#),
        TitleScreen::Allowed
    );
}

#[test]
fn a_refusal_quotes_the_models_phrase_on_one_line() {
    assert_eq!(
        parse_title_screen("{\"allowed\": false, \"reason\": \"slur aimed\\n at a group\"}"),
        TitleScreen::Refused {
            reason: "That title did not pass the house screen: slur aimed at a group".to_string()
        }
    );
}

#[test]
fn a_refusal_without_a_usable_phrase_falls_back_to_the_house_line() {
    let house = TitleScreen::Refused {
        reason: HOUSE_REFUSAL.to_string(),
    };
    assert_eq!(
        parse_title_screen(r#"{"allowed": false, "reason": ""}"#),
        house
    );
    assert_eq!(parse_title_screen(r#"{"allowed": false}"#), house);
    // An essay is not banner copy.
    assert_eq!(
        parse_title_screen(&format!(
            r#"{{"allowed": false, "reason": "{}"}}"#,
            "x".repeat(REASON_MAX_LEN + 1)
        )),
        house
    );
}

#[test]
fn unreadable_json_refuses_rather_than_allows() {
    let house = TitleScreen::Refused {
        reason: HOUSE_REFUSAL.to_string(),
    };
    assert_eq!(parse_title_screen("not json at all"), house);
    assert_eq!(parse_title_screen(""), house);
    assert_eq!(parse_title_screen(r#"{"reason": "no verdict"}"#), house);
    assert_eq!(parse_title_screen(r#"{"allowed": "yes"}"#), house);
}

#[test]
fn a_fenced_verdict_is_still_read() {
    assert_eq!(
        parse_title_screen("```json\n{\"allowed\": true, \"reason\": \"\"}\n```"),
        TitleScreen::Allowed
    );
}

#[tokio::test]
async fn a_disabled_ai_service_yields_no_verdict_rather_than_an_allow() {
    let ai = AiService::new(false, Some("unused".to_string()));
    assert_eq!(
        screen_custom_title(&ai, "the night clerk")
            .await
            .expect("screen"),
        TitleScreen::Unavailable
    );

    // Enabled with no key is the same story: nothing screened it.
    let ai = AiService::new(true, None);
    assert_eq!(
        screen_custom_title(&ai, "the night clerk")
            .await
            .expect("screen"),
        TitleScreen::Unavailable
    );
}
