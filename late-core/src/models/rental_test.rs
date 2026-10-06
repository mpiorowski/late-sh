use serde_json::json;

use super::*;

#[test]
fn duration_copy_reads_hours_for_the_day_tier_and_days_for_the_month_tier() {
    assert_eq!(duration_label(RENTAL_DAY_SECS), "24 hours");
    assert_eq!(duration_label(RENTAL_MONTH_SECS), "30 days");
    assert_eq!(duration_tag(RENTAL_DAY_SECS), "24h");
    assert_eq!(duration_tag(RENTAL_MONTH_SECS), "30d");
    // A window that is not a whole number of days keeps reading in hours.
    assert_eq!(duration_label(36 * 3_600), "36 hours");
    assert_eq!(duration_tag(36 * 3_600), "36h");
}

#[test]
fn duration_secs_reads_the_payload_and_falls_back_to_the_callers_window() {
    assert_eq!(
        duration_secs(
            &json!({"duration_secs": RENTAL_MONTH_SECS}),
            RENTAL_DAY_SECS
        ),
        RENTAL_MONTH_SECS
    );
    assert_eq!(
        duration_secs(&json!({"emoji": "🐱"}), RENTAL_DAY_SECS),
        RENTAL_DAY_SECS
    );
}

#[test]
fn badge_rental_reads_emoji_and_slot_from_the_payload() {
    let rental = BadgeRental::from_payload(&json!({
        "emoji": "🐱",
        "slot": "chat_badge",
        "tier": "basic",
        "duration_secs": RENTAL_DAY_SECS,
    }))
    .expect("badge rental payload");
    assert_eq!(rental.emoji, "🐱");
    assert_eq!(rental.slot, BadgeSlot::Badge);
    assert_eq!(rental.slot.effect_kind(), "chat_badge");

    let flag = BadgeRental::from_payload(&json!({"emoji": "🏴", "slot": "chat_flag"}))
        .expect("flag rental payload");
    assert_eq!(flag.slot, BadgeSlot::Flag);
    assert_eq!(flag.slot.effect_kind(), "chat_flag");
}

#[test]
fn badge_rental_refuses_a_payload_nothing_could_render() {
    assert_eq!(
        BadgeRental::from_payload(&json!({"slot": "chat_badge"})),
        None
    );
    assert_eq!(
        BadgeRental::from_payload(&json!({"emoji": "  ", "slot": "chat_badge"})),
        None
    );
    assert_eq!(
        BadgeRental::from_payload(&json!({"emoji": "🐱", "slot": "bonsai_variant"})),
        None
    );
    assert_eq!(BadgeRental::from_payload(&json!({"emoji": "🐱"})), None);
}

#[test]
fn title_payload_is_trimmed_clamped_and_never_blank() {
    assert_eq!(
        title_from_payload(&json!({"text": "  the insufferable  "})).as_deref(),
        Some("the insufferable")
    );
    assert_eq!(title_from_payload(&json!({"text": "   "})), None);
    assert_eq!(title_from_payload(&json!({"duration_secs": 1})), None);

    let long = "x".repeat(TITLE_MAX_LEN + 8);
    assert_eq!(
        title_from_payload(&json!({ "text": long })),
        Some("x".repeat(TITLE_MAX_LEN))
    );
}

#[test]
fn custom_title_payload_flag_marks_the_buyer_written_skus() {
    assert!(is_custom_title(
        &json!({"custom": true, "duration_secs": RENTAL_DAY_SECS})
    ));
    assert!(!is_custom_title(&json!({"text": "the night clerk"})));
    assert!(!is_custom_title(&json!({"custom": false})));
    // A payload without the key (the retired curated rows) is not custom.
    assert!(!is_custom_title(&json!({})));
}

#[test]
fn custom_title_trims_and_collapses_whitespace() {
    assert_eq!(
        CustomTitle::parse("  the  night   clerk ")
            .expect("title")
            .as_str(),
        "the night clerk"
    );
    assert_eq!(
        CustomTitle::parse("nightside")
            .expect("title")
            .into_string(),
        "nightside"
    );
}

#[test]
fn custom_title_refuses_what_a_chat_row_cannot_wear() {
    assert_eq!(CustomTitle::parse("   "), Err(CustomTitleError::Empty));
    assert_eq!(CustomTitle::parse(""), Err(CustomTitleError::Empty));
    assert_eq!(
        CustomTitle::parse(&"x".repeat(TITLE_MAX_LEN + 1)),
        Err(CustomTitleError::TooLong),
        "over the cap is refused, never clamped: the buyer pays for what they typed"
    );
    assert!(CustomTitle::parse(&"x".repeat(TITLE_MAX_LEN)).is_ok());
    assert_eq!(CustomTitle::parse("@mira"), Err(CustomTitleError::Mention));
    assert_eq!(
        CustomTitle::parse("night\nclerk"),
        Err(CustomTitleError::Unprintable)
    );
    assert_eq!(
        CustomTitle::parse("night\u{202E}clerk"),
        Err(CustomTitleError::Unprintable)
    );
    assert_eq!(
        CustomTitle::parse("night\u{200B}clerk"),
        Err(CustomTitleError::Unprintable)
    );
}

#[test]
fn custom_title_counts_characters_not_bytes() {
    // Twenty multi-byte characters still fit; twenty-one do not.
    let twenty = "ł".repeat(TITLE_MAX_LEN);
    assert_eq!(CustomTitle::parse(&twenty).expect("title").as_str(), twenty);
    assert_eq!(
        CustomTitle::parse(&"ł".repeat(TITLE_MAX_LEN + 1)),
        Err(CustomTitleError::TooLong)
    );
}
