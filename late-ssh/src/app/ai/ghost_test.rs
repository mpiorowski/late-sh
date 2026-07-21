use super::*;

#[test]
fn merge_ghost_settings_preserves_existing_profile_fields() {
    let merged = merge_ghost_settings(&json!({
        "bio": "already set",
        "theme_id": "late"
    }));
    assert_eq!(merged["bot"], serde_json::Value::Bool(true));
    assert_eq!(
        merged["bio"],
        serde_json::Value::String("already set".to_string())
    );
    assert_eq!(
        merged["theme_id"],
        serde_json::Value::String("late".to_string())
    );
}

#[test]
fn tiny_rng_next_usize_stays_in_range() {
    let mut rng = TinyRng::new(42);
    for _ in 0..100 {
        let v = rng.next_usize(5);
        assert!(v < 5);
    }
}

#[test]
fn tiny_rng_next_usize_zero_and_one() {
    let mut rng = TinyRng::new(42);
    assert_eq!(rng.next_usize(0), 0);
    assert_eq!(rng.next_usize(1), 0);
}

#[test]
fn tiny_rng_next_between_inclusive_stays_in_range() {
    let mut rng = TinyRng::new(42);
    for _ in 0..100 {
        let v = rng.next_between_inclusive(20, 200);
        assert!((20..=200).contains(&v));
    }
}

#[test]
fn tiny_rng_next_between_inclusive_equal_bounds() {
    let mut rng = TinyRng::new(42);
    for _ in 0..10 {
        assert_eq!(rng.next_between_inclusive(50, 50), 50);
    }
}

#[test]
fn contains_mention_matches_exact_handle() {
    assert!(contains_mention("hey @bot can you help", "bot"));
    assert!(contains_mention("hey @BoT can you help", "bot"));
    assert!(!contains_mention("hey @botty can you help", "bot"));
}

#[test]
fn contains_mention_ignores_email_like_tokens() {
    assert!(!contains_mention("mail me at hi@bot.dev", "bot"));
}

#[test]
fn contains_mention_ignores_reply_quote_prefix() {
    assert!(!contains_mention(
        "> @bot: earlier message
thanks",
        "bot"
    ));
    assert!(contains_mention(
        "> @bot: earlier message
thanks @bot",
        "bot"
    ));
    assert!(contains_mention(
        "> @alice: earlier message
hey @bot what do you think",
        "bot"
    ));
}

#[test]
fn extract_mention_handles_reads_only_live_reply_text() {
    assert_eq!(
        extract_mention_handles("> @alice old line\n@bartender buy @bob a drink"),
        vec!["bartender".to_string(), "bob".to_string()]
    );
}

#[test]
fn bartender_round_intent_requires_exact_command() {
    assert!(bartender_round_intent("@bartender round", "bartender"));
    assert!(bartender_round_intent("@bartender ROUND!", "bartender"));
    assert!(bartender_round_intent(
        "@bartender round for everyone",
        "bartender"
    ));
    assert!(bartender_round_intent(
        "@bartender round for everyone!",
        "bartender"
    ));
    assert!(!bartender_round_intent("round for everyone", "bartender"));
    assert!(!bartender_round_intent(
        "@bartender pour a round for everyone",
        "bartender"
    ));
    assert!(bartender_round_intent(
        "@bartender round for the house",
        "bartender"
    ));
    assert!(!bartender_round_intent("@bartender confirm", "bartender"));
}

#[test]
fn is_dm_room_matches_kind_or_visibility() {
    assert!(is_dm_room("dm", "dm"));
    assert!(is_dm_room("topic", "dm"));
    assert!(is_dm_room("dm", "private"));
    assert!(!is_dm_room("topic", "private"));
    assert!(!is_dm_room("topic", "public"));
}

#[test]
fn should_handle_bot_mention_event_in_public_room() {
    let bot = Uuid::from_u128(7);
    assert!(should_handle_bot_mention_event(
        "hey @bot can you help",
        None,
        bot,
        "bot"
    ));
}

#[test]
fn should_handle_bot_mention_event_in_private_room_when_bot_is_member() {
    let bot = Uuid::from_u128(7);
    let targets = [Uuid::from_u128(1), bot];
    assert!(should_handle_bot_mention_event(
        "hey @bot can you help",
        Some(&targets),
        bot,
        "bot"
    ));
}

#[test]
fn should_handle_bot_mention_event_in_private_room_when_bot_is_not_yet_member() {
    let bot = Uuid::from_u128(7);
    let targets = [Uuid::from_u128(1), Uuid::from_u128(2)];
    assert!(should_handle_bot_mention_event(
        "hey @bot can you help",
        Some(&targets),
        bot,
        "bot"
    ));
    assert!(!should_handle_bot_mention_event(
        "normal room traffic",
        Some(&targets),
        bot,
        "bot"
    ));
}

#[test]
fn parse_bartender_order_pours_within_spendable() {
    let raw = r#"{"action": "pour", "drink": "Segfault Sour", "price": 400, "line": "one segfault sour, that is 400 chips"}"#;
    assert_eq!(
        parse_bartender_order(raw, 900, "bartender", &[]),
        BartenderDecision::Pour {
            drink: "Segfault Sour".to_string(),
            price: 400,
            line: "one segfault sour, that is 400 chips".to_string(),
        }
    );
}

#[test]
fn parse_bartender_order_refuses_out_of_range_price() {
    // Below the floor or above the ceiling is a model slip: serve the line
    // uncharged rather than clamp to a number the receipt never quoted.
    let cheap = r#"{"action": "pour", "drink": "tap water", "price": 5, "line": "here"}"#;
    assert_eq!(
        parse_bartender_order(cheap, 5000, "bartender", &[]),
        BartenderDecision::Say {
            line: "here".to_string()
        }
    );

    let dear = r#"{"action": "pour", "drink": "the vault", "price": 99999, "line": "here"}"#;
    assert_eq!(
        parse_bartender_order(dear, 5000, "bartender", &[]),
        BartenderDecision::Say {
            line: "here".to_string()
        }
    );
}

#[test]
fn parse_bartender_order_downgrades_unaffordable_pour() {
    // In range, but more than the patron can spend: no charge, just the line.
    let raw = r#"{"action": "pour", "drink": "top shelf", "price": 800, "line": "the good stuff"}"#;
    assert_eq!(
        parse_bartender_order(raw, 300, "bartender", &[]),
        BartenderDecision::Say {
            line: "the good stuff".to_string()
        }
    );
}

#[test]
fn parse_bartender_order_chat_and_offer_never_charge() {
    for action in ["chat", "offer", "something-else"] {
        let raw = format!(
            r#"{{"action": "{action}", "drink": null, "price": null, "line": "welcome in"}}"#
        );
        assert_eq!(
            parse_bartender_order(&raw, 900, "bartender", &[]),
            BartenderDecision::Say {
                line: "welcome in".to_string()
            }
        );
    }
}

#[test]
fn parse_bartender_order_creates_gift_for_known_recipient() {
    let alice_id = Uuid::from_u128(1);
    let recipients = [BartenderGiftRecipient {
        id: alice_id,
        handle: "alice".to_string(),
    }];
    let raw = r#"{"action": "gift", "recipient": "alice", "drink": "Kernel Panic Punch", "price": 300, "line": "kernel panic punch for @alice, 300 chips."}"#;
    assert_eq!(
        parse_bartender_order(raw, 900, "bartender", &recipients),
        BartenderDecision::Gift {
            recipient_id: alice_id,
            recipient_handle: "alice".to_string(),
            drink: "Kernel Panic Punch".to_string(),
            price: 300,
            line: "kernel panic punch for @alice, 300 chips.".to_string(),
        }
    );
}

#[test]
fn parse_bartender_order_refuses_gift_for_unknown_recipient() {
    let raw = r#"{"action": "gift", "recipient": "mallory", "drink": "Kernel Panic Punch", "price": 300, "line": "who is that one for?"}"#;
    assert_eq!(
        parse_bartender_order(raw, 900, "bartender", &[]),
        BartenderDecision::Say {
            line: "who is that one for?".to_string()
        }
    );
}

#[test]
fn parse_bartender_order_accepts_fenced_json_and_defaults_drink() {
    let raw = "```json\n{\"action\": \"pour\", \"price\": 200, \"line\": \"here you go\"}\n```";
    assert_eq!(
        parse_bartender_order(raw, 900, "bartender", &[]),
        BartenderDecision::Pour {
            drink: "house pour".to_string(),
            price: 200,
            line: "here you go".to_string(),
        }
    );
}

#[test]
fn parse_bartender_order_skips_garbage_and_empty_lines() {
    assert_eq!(
        parse_bartender_order("not json at all", 900, "bartender", &[]),
        BartenderDecision::Skip
    );
    assert_eq!(
        parse_bartender_order(r#"{"action": "pour", "price": 200}"#, 900, "bartender", &[]),
        BartenderDecision::Skip
    );
    assert_eq!(
        parse_bartender_order(
            r#"{"action": "chat", "line": "SKIP"}"#,
            900,
            "bartender",
            &[]
        ),
        BartenderDecision::Skip
    );
}

#[test]
fn parse_bartender_order_recovers_from_stray_trailing_quote() {
    // The exact shape Gemini produced: a spurious quote line after `line`,
    // which strict serde rejects outright. Recovery must still surface the
    // chat line instead of leaving the bartender mute.
    let raw = "{\n  \"action\": \"chat\",\n  \"drink\": null,\n  \"price\": null,\n  \"line\": \"The top shelf is closed for you tonight, friend. Here is ice water.\"\n\"\n}";
    assert_eq!(
        parse_bartender_order(raw, 900, "bartender", &[]),
        BartenderDecision::Say {
            line: "The top shelf is closed for you tonight, friend. Here is ice water.".to_string()
        }
    );
}

#[test]
fn parse_bartender_order_recovers_pour_fields_when_json_is_broken() {
    // A pour with the same trailing-quote corruption: action, drink, and
    // price all survive the hand-rolled recovery.
    let raw = "{\"action\": \"pour\", \"drink\": \"Kernel Panic Punch\", \"price\": 250, \"line\": \"one Kernel Panic Punch, 250 chips.\"\"}";
    assert_eq!(
        parse_bartender_order(raw, 900, "bartender", &[]),
        BartenderDecision::Pour {
            drink: "Kernel Panic Punch".to_string(),
            price: 250,
            line: "one Kernel Panic Punch, 250 chips.".to_string(),
        }
    );
}

#[test]
fn extract_json_string_field_stops_at_first_unescaped_quote() {
    let raw = r#"{"line": "he said \"hi\" then left.""#;
    assert_eq!(
        extract_json_string_field(raw, "line").as_deref(),
        Some(r#"he said "hi" then left."#)
    );
    assert_eq!(
        extract_json_string_field(r#"{"drink": null}"#, "drink"),
        None
    );
    assert_eq!(extract_json_string_field(r#"{"a": 1}"#, "line"), None);
}

#[test]
fn sanitize_generated_reply_strips_prefix_and_quotes() {
    let got = sanitize_generated_reply("bot: \"sure, try rg -n\" ", Some("bot"));
    assert_eq!(got.as_deref(), Some("sure, try rg -n"));
}

#[test]
fn sanitize_generated_reply_respects_custom_line_limit() {
    let got = sanitize_generated_reply_with_line_limit("one\ntwo\nthree\nfour\nfive", None, 4);
    assert_eq!(got.as_deref(), Some("one two three four"));
}

#[test]
fn mention_target_for_user_falls_back_to_short_id() {
    let user_id = Uuid::from_u128(0x0123_4567_89ab_cdef_1111_2222_3333_4444);
    assert_eq!(mention_target_for_user(Some(""), user_id), "@01234567");
    assert_eq!(mention_target_for_user(Some("!!!"), user_id), "@01234567");
}

#[test]
fn mention_target_for_user_prefers_sanitized_current_username() {
    let user_id = Uuid::from_u128(0x0123_4567_89ab_cdef_1111_2222_3333_4444);
    assert_eq!(
        mention_target_for_user(Some(" current-user "), user_id),
        "@current-user"
    );
}
