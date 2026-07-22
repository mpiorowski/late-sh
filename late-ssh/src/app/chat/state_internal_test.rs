use super::*;
use crate::app::common::theme;

fn names(matches: &[MentionMatch]) -> Vec<&str> {
    matches.iter().map(|m| m.name.as_str()).collect()
}

fn sorted_ids(mut ids: Vec<Uuid>) -> Vec<Uuid> {
    ids.sort();
    ids
}

#[test]
fn click_display_col_maps_to_char_offset_ascii() {
    // Clicking column N over "hello" lands the caret before the Nth char,
    // and a click past the end clamps to the char count.
    assert_eq!(char_offset_for_display_col("hello", 0), 0);
    assert_eq!(char_offset_for_display_col("hello", 3), 3);
    assert_eq!(char_offset_for_display_col("hello", 99), 5);
}

#[test]
fn click_display_col_accounts_for_wide_glyphs() {
    // '世' and '界' render two cells each: 世 spans cols 0..2, 界 2..4,
    // '!' at col 4. A click in a glyph's left half resolves to that glyph.
    let text = "世界!";
    assert_eq!(char_offset_for_display_col(text, 0), 0); // before 世
    assert_eq!(char_offset_for_display_col(text, 1), 0); // left half of 世
    assert_eq!(char_offset_for_display_col(text, 2), 1); // before 界
    assert_eq!(char_offset_for_display_col(text, 4), 2); // before '!'
}

#[test]
fn click_global_offset_splits_into_line_and_col() {
    // Newlines count as one char (matching build_composer_rows), so the
    // offset just past a '\n' is column 0 of the next logical line.
    let text = "ab\ncde";
    assert_eq!(global_char_to_line_col(text, 0), (0, 0));
    assert_eq!(global_char_to_line_col(text, 2), (0, 2));
    assert_eq!(global_char_to_line_col(text, 3), (1, 0));
    assert_eq!(global_char_to_line_col(text, 5), (1, 2));
}

#[test]
fn parse_gift_command_accepts_at_optional_username() {
    assert_eq!(
        parse_gift_command("/gift @alice 500"),
        Some(GiftParse::Gift {
            username: "alice".to_string(),
            amount: 500,
            message: None,
        })
    );
    assert_eq!(
        parse_gift_command("/gift alice 500"),
        Some(GiftParse::Gift {
            username: "alice".to_string(),
            amount: 500,
            message: None,
        })
    );
}

#[test]
fn parse_gift_command_captures_optional_message() {
    assert_eq!(
        parse_gift_command("/gift @alice 500 happy birthday"),
        Some(GiftParse::Gift {
            username: "alice".to_string(),
            amount: 500,
            message: Some("happy birthday".to_string()),
        })
    );
}

#[test]
fn parse_gift_command_rejects_invalid_amounts_and_junk() {
    assert_eq!(parse_gift_command("/gift"), Some(GiftParse::Invalid));
    assert_eq!(parse_gift_command("/gift @a 0"), Some(GiftParse::Invalid));
    assert_eq!(parse_gift_command("/gift @a -1"), Some(GiftParse::Invalid));
    assert_eq!(
        parse_gift_command("/gift @a 1000001"),
        Some(GiftParse::Invalid)
    );
    assert_eq!(parse_gift_command("/gift @a wat"), Some(GiftParse::Invalid));
    assert_eq!(parse_gift_command("/gifted @a 5"), None);
}

#[test]
fn read_cursor_flush_queue_coalesces_room_until_deadline() {
    let room_id = Uuid::from_u128(1);
    let now = Instant::now();
    let mut pending = PendingReadCursorFlush::default();

    pending.queue(room_id, now);
    let scheduled = pending.flush_at.unwrap();
    pending.queue(room_id, now + Duration::from_millis(250));

    assert_eq!(pending.flush_at, Some(scheduled));
    assert_eq!(pending.rooms.len(), 1);
    assert!(
        pending
            .take_due(scheduled - Duration::from_millis(1))
            .is_empty()
    );
    assert_eq!(pending.take_due(scheduled), vec![room_id]);
    assert!(pending.rooms.is_empty());
    assert_eq!(pending.flush_at, None);
}

#[test]
fn read_cursor_flush_queue_batches_unique_rooms() {
    let room_a = Uuid::from_u128(1);
    let room_b = Uuid::from_u128(2);
    let now = Instant::now();
    let mut pending = PendingReadCursorFlush::default();

    pending.queue(room_a, now);
    pending.queue(room_b, now + Duration::from_millis(50));
    pending.queue(room_a, now + Duration::from_millis(100));

    assert_eq!(
        sorted_ids(pending.take_due(now + READ_CURSOR_FLUSH_DELAY)),
        vec![room_a, room_b]
    );
    assert!(pending.rooms.is_empty());
    assert_eq!(pending.flush_at, None);
}

#[test]
fn read_cursor_flush_take_all_flushes_before_deadline() {
    let room_id = Uuid::from_u128(1);
    let now = Instant::now();
    let mut pending = PendingReadCursorFlush::default();

    pending.queue(room_id, now);

    assert_eq!(pending.take_all(), vec![room_id]);
    assert!(pending.rooms.is_empty());
    assert_eq!(pending.flush_at, None);
}

fn online(names: &[&str]) -> HashSet<String> {
    names.iter().map(|n| n.to_string()).collect()
}

#[test]
fn rank_mention_matches_orders_online_before_offline() {
    let all = vec![
        "alice".to_string(),
        "bob".to_string(),
        "carol".to_string(),
        "dave".to_string(),
    ];
    let ranked = rank_mention_matches(&all, "", || online(&["bob", "dave"]));
    assert_eq!(names(&ranked), vec!["bob", "dave", "alice", "carol"]);
    assert!(ranked[0].online && ranked[1].online);
    assert!(!ranked[2].online && !ranked[3].online);
}

#[test]
fn rank_mention_matches_prefix_filter_groups_online_first() {
    // "@a" with two online and one offline 'a'-prefixed users:
    // online 'a' names come first (alphabetically), then offline.
    let all = vec![
        "alice".to_string(),
        "alex".to_string(),
        "albert".to_string(),
        "bob".to_string(),
    ];
    let ranked = rank_mention_matches(&all, "a", || online(&["alice", "alex"]));
    assert_eq!(names(&ranked), vec!["alex", "alice", "albert"]);
    assert!(ranked[0].online && ranked[1].online);
    assert!(!ranked[2].online);
}

#[test]
fn rank_mention_matches_applies_prefix_filter() {
    let all = vec!["alice".to_string(), "albert".to_string(), "bob".to_string()];
    let ranked = rank_mention_matches(&all, "al", || online(&["bob"]));
    assert_eq!(names(&ranked), vec!["albert", "alice"]);
}

#[test]
fn rank_mention_matches_prefix_is_case_insensitive() {
    let all = vec!["Alice".to_string(), "alBert".to_string()];
    let ranked = rank_mention_matches(&all, "al", HashSet::new);
    assert_eq!(names(&ranked), vec!["alBert", "Alice"]);
}

#[test]
fn rank_mention_matches_falls_back_to_alpha_when_no_online_info() {
    let all = vec!["zed".to_string(), "alice".to_string(), "bob".to_string()];
    let ranked = rank_mention_matches(&all, "", HashSet::new);
    assert_eq!(names(&ranked), vec!["alice", "bob", "zed"]);
    assert!(ranked.iter().all(|m| !m.online));
}

#[test]
fn rank_mention_matches_skips_online_set_when_prefix_excludes_all() {
    // When the query filters everyone out, the online-set supplier must
    // not be invoked — it's the expensive path (locks ActiveUsers).
    let all = vec!["alice".to_string(), "bob".to_string()];
    let ranked = rank_mention_matches(&all, "zz", || {
        panic!("online_set should not be built when prefix filter is empty")
    });
    assert!(ranked.is_empty());
}

#[test]
fn rank_room_name_matches_filters_and_prefixes_non_dm_rooms() {
    let rust = make_room(Uuid::from_u128(1), "topic", "public", false, Some("rust"));
    let recipes = make_room(
        Uuid::from_u128(2),
        "topic",
        "public",
        false,
        Some("recipes"),
    );
    let dm = make_room(Uuid::from_u128(3), "dm", "dm", false, None);

    let rooms = [&rust.0, &recipes.0, &dm.0];
    let ranked = rank_room_name_matches(rooms, "r");

    assert_eq!(names(&ranked), vec!["recipes", "rust"]);
    assert!(ranked.iter().all(|m| m.prefix == "#"));
}

#[test]
fn online_username_set_returns_empty_for_none() {
    assert!(online_username_set(None).is_empty());
}

#[test]
fn online_username_set_lowercases_active_usernames() {
    use crate::state::ActiveUser;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    let mut users: HashMap<Uuid, ActiveUser> = HashMap::new();
    users.insert(
        Uuid::now_v7(),
        ActiveUser {
            username: "Alice".to_string(),
            fingerprint: None,
            peer_ip: None,
            audio_source: late_core::models::user::AudioSource::Icecast,
            sessions: Vec::new(),
            connection_count: 1,
            last_login_at: Instant::now(),
        },
    );
    users.insert(
        Uuid::now_v7(),
        ActiveUser {
            username: "BOB".to_string(),
            fingerprint: None,
            peer_ip: None,
            audio_source: late_core::models::user::AudioSource::Icecast,
            sessions: Vec::new(),
            connection_count: 2,
            last_login_at: Instant::now(),
        },
    );
    let active: ActiveUsers = Arc::new(Mutex::new(users));

    let set = online_username_set(Some(&active));
    assert_eq!(set, online(&["alice", "bob"]));
}

#[test]
fn reply_preview_text_uses_message_body_for_nested_replies() {
    let preview = reply_preview_text("> @mat: original message preview\nyou like blocks?");
    assert_eq!(preview, "you like blocks?");
}

#[test]
fn reply_preview_text_uses_news_title_for_news_messages() {
    let preview = reply_preview_text(
        "---NEWS--- Rust 1.95 Released || summary || https://example.com || ascii",
    );
    assert_eq!(preview, "Rust 1.95 Released");
}

#[test]
fn news_modal_source_uses_full_article_snapshot_payload() {
    use late_core::models::article::{Article, ArticleFeedItem};

    let created = chrono::DateTime::parse_from_rfc3339("2026-05-08T11:28:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let user_id = Uuid::from_u128(9);
    let item = ArticleFeedItem {
        article: Article {
            id: Uuid::from_u128(1),
            created,
            updated: created,
            user_id,
            url: "https://example.com/full".to_string(),
            title: "Full article title".to_string(),
            summary: "First full bullet keeps all words for two-line modal wrapping.\nSecond full bullet also keeps all words without chat truncation.\nThird full bullet remains available."
                .to_string(),
            ascii_art: ".:-".to_string(),
        },
        author_username: "mat".to_string(),
    };

    let (payload, author, source_created, article_id) =
        news_modal_source_from_articles(&[item], " https://example.com/full ").unwrap();

    assert_eq!(payload.title, "Full article title");
    assert!(payload.summary.contains("without chat truncation"));
    assert!(!payload.summary.contains("..."));
    assert_eq!(payload.ascii_art, ".:-");
    assert_eq!(author, "@mat");
    assert_eq!(source_created, created);
    assert_eq!(article_id, Uuid::from_u128(1));
}

#[test]
fn reply_preview_text_strips_markdown_markers() {
    let preview = reply_preview_text("**bold** `@graybeard` [docs](https://late.sh)");
    assert_eq!(preview, "bold @graybeard docs");
}

#[test]
fn reply_preview_text_preserves_unmatched_backtick_in_kaomoji() {
    let preview = reply_preview_text("(╯`Д´)╯︵ ┻━┻");
    assert_eq!(preview, "(╯`Д´)╯︵ ┻━┻");
}

#[test]
fn reply_preview_text_strips_double_backtick_code_markers() {
    let preview = reply_preview_text("``(╯`Д´)╯︵ ┻━┻``");
    assert_eq!(preview, "(╯`Д´)╯︵ ┻━┻");
}

#[test]
fn news_marker_detection_matches_announcement_messages() {
    assert!(news_reply_preview_text("---NEWS--- title || summary || url || ascii").is_some());
    assert!(news_reply_preview_text("regular chat message").is_none());
}

#[test]
fn moderation_server_toast_formats_kicks_and_bans() {
    let base_user_id = Uuid::now_v7();
    let kick = ModerationEvent::ServerUserAction {
        actor_user_id: Uuid::now_v7(),
        target_user_id: base_user_id,
        target_username: "alice".to_string(),
        action: ServerUserAction::Kick,
        reason: "bye".to_string(),
        terminated_sessions: 1,
    };
    let ban = ModerationEvent::ServerUserAction {
        actor_user_id: Uuid::now_v7(),
        target_user_id: base_user_id,
        target_username: "bob".to_string(),
        action: ServerUserAction::Ban,
        reason: "spam".to_string(),
        terminated_sessions: 2,
    };

    assert_eq!(
        moderation_server_toast(&kick),
        Some("@alice was kicked from the server".to_string())
    );
    assert_eq!(
        moderation_server_toast(&ban),
        Some("@bob was banned from the server".to_string())
    );
}

#[test]
fn moderation_server_toast_ignores_unbans_and_non_server_events() {
    let target_user_id = Uuid::now_v7();
    let unban = ModerationEvent::ServerUserAction {
        actor_user_id: Uuid::now_v7(),
        target_user_id,
        target_username: "alice".to_string(),
        action: ServerUserAction::Unban,
        reason: String::new(),
        terminated_sessions: 0,
    };
    let room = ModerationEvent::RoomAction {
        actor_user_id: Uuid::now_v7(),
        target_user_id,
        room_id: Uuid::now_v7(),
        room_slug: "lounge".to_string(),
        action: crate::moderation::command::RoomModAction::Kick,
        reason: String::new(),
        notified_sessions: 0,
    };

    assert_eq!(moderation_server_toast(&unban), None);
    assert_eq!(moderation_server_toast(&room), None);
}

// --- parse_dm_command ---

#[test]
fn parse_dm_with_at() {
    assert_eq!(parse_dm_command("/dm @alice"), Some("alice"));
}

#[test]
fn parse_dm_without_at() {
    assert_eq!(parse_dm_command("/dm bob"), Some("bob"));
}

#[test]
fn parse_dm_empty_username() {
    assert_eq!(parse_dm_command("/dm "), None);
    assert_eq!(parse_dm_command("/dm @"), None);
}

#[test]
fn parse_dm_not_dm_command() {
    assert_eq!(parse_dm_command("hello world"), None);
    assert_eq!(parse_dm_command("/dms alice"), None);
}

#[test]
fn parse_dm_trims_whitespace() {
    assert_eq!(parse_dm_command("/dm  @alice  "), Some("alice"));
}

// --- parse_roll_command ---

fn specs(items: &[(u32, u32)]) -> RollParse {
    RollParse::Specs(
        items
            .iter()
            .map(|&(count, sides)| DieSpec { count, sides })
            .collect(),
    )
}

#[test]
fn parse_roll_bare_defaults_to_d20() {
    assert_eq!(parse_roll_command("/roll"), Some(specs(&[(1, 20)])));
}

#[test]
fn parse_roll_single_die_without_count() {
    assert_eq!(parse_roll_command("/roll d6"), Some(specs(&[(1, 6)])));
}

#[test]
fn parse_roll_with_count() {
    assert_eq!(parse_roll_command("/roll 3d6"), Some(specs(&[(3, 6)])));
}

#[test]
fn parse_roll_mixed_dice() {
    assert_eq!(
        parse_roll_command("/roll 3d6 2d20"),
        Some(specs(&[(3, 6), (2, 20)]))
    );
}

#[test]
fn parse_roll_trims_extra_whitespace() {
    assert_eq!(
        parse_roll_command("  /roll   3d6  2d20  "),
        Some(specs(&[(3, 6), (2, 20)]))
    );
}

#[test]
fn parse_roll_rejects_malformed_args() {
    assert_eq!(parse_roll_command("/roll 3"), Some(RollParse::Invalid));
    assert_eq!(parse_roll_command("/roll d"), Some(RollParse::Invalid));
    assert_eq!(parse_roll_command("/roll 0d6"), Some(RollParse::Invalid));
    assert_eq!(parse_roll_command("/roll 1d1"), Some(RollParse::Invalid));
    assert_eq!(parse_roll_command("/roll xd6"), Some(RollParse::Invalid));
    assert_eq!(
        parse_roll_command("/roll 3d6 bogus"),
        Some(RollParse::Invalid)
    );
}

#[test]
fn parse_roll_enforces_caps() {
    assert_eq!(parse_roll_command("/roll 101d6"), Some(RollParse::Invalid));
    assert_eq!(parse_roll_command("/roll 1d1001"), Some(RollParse::Invalid));
}

#[test]
fn parse_roll_not_a_roll_command() {
    assert_eq!(parse_roll_command("hello"), None);
    assert_eq!(parse_roll_command("/rollover"), None);
}

#[test]
fn format_roll_result_single_group() {
    let specs = vec![DieSpec { count: 3, sides: 6 }];
    let rolls = vec![vec![1, 2, 5]];
    assert_eq!(format_roll_result(&specs, &rolls), "3d6: [1 2 5] = 8");
}

#[test]
fn format_roll_result_single_die_omits_count() {
    let specs = vec![DieSpec {
        count: 1,
        sides: 20,
    }];
    let rolls = vec![vec![12]];
    assert_eq!(format_roll_result(&specs, &rolls), "d20: [12] = 12");
}

#[test]
fn format_formula_mixed() {
    let specs = vec![
        DieSpec {
            count: 1,
            sides: 20,
        },
        DieSpec { count: 3, sides: 6 },
    ];
    assert_eq!(format_formula(&specs), "d20 3d6");
}

#[test]
fn format_roll_result_mixed_groups() {
    let specs = vec![
        DieSpec { count: 3, sides: 6 },
        DieSpec {
            count: 2,
            sides: 20,
        },
    ];
    let rolls = vec![vec![2, 2, 5], vec![12, 20]];
    assert_eq!(
        format_roll_result(&specs, &rolls),
        "3d6 2d20: [2 2 5] [12 20] = 41"
    );
}

#[test]
fn roll_dice_respects_sides_and_count() {
    let specs = vec![
        DieSpec { count: 5, sides: 6 },
        DieSpec {
            count: 3,
            sides: 20,
        },
    ];
    let rolls = roll_dice(&specs, &mut rand_core::OsRng);
    assert_eq!(rolls.len(), 2);
    assert_eq!(rolls[0].len(), 5);
    assert_eq!(rolls[1].len(), 3);
    for v in &rolls[0] {
        assert!((1..=6).contains(v));
    }
    for v in &rolls[1] {
        assert!((1..=20).contains(v));
    }
}

#[test]
fn new_chat_textarea_uses_theme_text_color() {
    let textarea = new_chat_textarea();
    assert_eq!(textarea.style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().bg, None);
}

#[test]
fn composer_cursor_visible_uses_explicit_theme_colors() {
    let mut textarea = new_chat_textarea();
    composer::set_themed_textarea_cursor_visible(&mut textarea, true);
    assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
    assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));
}

#[test]
fn composer_cursor_hidden_restores_plain_text_color() {
    let mut textarea = new_chat_textarea();
    composer::set_themed_textarea_cursor_visible(&mut textarea, true);
    composer::set_themed_textarea_cursor_visible(&mut textarea, false);
    assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().bg, None);
}

#[test]
fn common_textarea_theme_refreshes_existing_chat_textarea_colors() {
    theme::set_current_by_id("late");
    let mut textarea = new_chat_textarea();
    let late_text = textarea.style().fg;

    theme::set_current_by_id("contrast");
    composer::apply_themed_textarea_style(&mut textarea, true);

    assert_ne!(textarea.style().fg, late_text);
    assert_eq!(textarea.style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_line_style().fg, Some(theme::TEXT()));
    assert_eq!(textarea.cursor_style().fg, Some(theme::BG_CANVAS()));
    assert_eq!(textarea.cursor_style().bg, Some(theme::TEXT()));

    theme::set_current_by_id("late");
}

#[test]
fn wrapped_index_wraps_forward() {
    assert_eq!(wrapped_index(2, 1, 3), 0);
    assert_eq!(wrapped_index(1, 5, 3), 0);
}

#[test]
fn wrapped_index_wraps_backward() {
    assert_eq!(wrapped_index(0, -1, 3), 2);
    assert_eq!(wrapped_index(1, -5, 3), 2);
}

fn make_room(
    id: Uuid,
    kind: &str,
    visibility: &str,
    permanent: bool,
    slug: Option<&str>,
) -> (ChatRoom, Vec<ChatMessage>) {
    (
        ChatRoom {
            id,
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            kind: kind.to_string(),
            visibility: visibility.to_string(),
            auto_join: permanent,
            permanent,
            slug: slug.map(str::to_string),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        },
        Vec::new(),
    )
}

#[test]
fn visual_order_matches_cozy_rail_grouping() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);
    let lounge = Uuid::from_u128(10);
    let announcements = Uuid::from_u128(11);
    let public_alpha = Uuid::from_u128(20);
    let public_zeta = Uuid::from_u128(21);
    let private_beta = Uuid::from_u128(30);
    let game_table = Uuid::from_u128(40);
    let dm_bob = make_dm(bob, me);
    let dm_alice = make_dm(me, alice);

    let mut usernames = HashMap::new();
    usernames.insert(alice, "alice".to_string());
    usernames.insert(bob, "bob".to_string());

    let rooms = vec![
        make_room(public_zeta, "topic", "public", false, Some("zeta")),
        make_room(game_table, "game", "public", false, Some("bj-abc123")),
        make_room(lounge, "lounge", "public", true, Some("lounge")),
        (dm_bob.clone(), Vec::new()),
        make_room(private_beta, "topic", "private", false, Some("beta")),
        make_room(
            announcements,
            "topic",
            "public",
            true,
            Some("announcements"),
        ),
        (dm_alice.clone(), Vec::new()),
        make_room(public_alpha, "topic", "public", false, Some("alpha")),
    ];

    assert_eq!(
        visual_order_for_rooms(RoomVisualOrderInput {
            rooms: &rooms,
            user_id: me,
            usernames: &usernames,
            unread_counts: &HashMap::new(),
            room_last_message_at: &HashMap::new(),
            feeds_available: true,
            favorite_room_ids: &[],
            collapsed_sections: &HashSet::new(),
            ignored_user_ids: &HashSet::new(),
        }),
        vec![
            RoomSlot::Room(lounge),
            RoomSlot::Room(announcements),
            RoomSlot::Notifications,
            RoomSlot::News,
            RoomSlot::Feeds,
            RoomSlot::Discover,
            RoomSlot::Room(public_zeta),
            RoomSlot::Room(private_beta),
            RoomSlot::Room(public_alpha),
            RoomSlot::Room(dm_alice.id),
            RoomSlot::Room(dm_bob.id),
        ]
    );
}

#[test]
fn room_section_label_round_trips() {
    for section in [
        RoomSection::Favorites,
        RoomSection::Core,
        RoomSection::Channels,
        RoomSection::Updates,
        RoomSection::Dms,
    ] {
        assert_eq!(RoomSection::from_label(section.label()), Some(section));
    }
    assert_eq!(RoomSection::from_label("not-a-section"), None);
}

#[test]
fn collapsed_sections_drop_their_rooms_from_visual_order() {
    let me = Uuid::from_u128(1);
    let bob = Uuid::from_u128(3);
    let lounge = Uuid::from_u128(10);
    let announcements = Uuid::from_u128(11);
    let public_alpha = Uuid::from_u128(20);
    let dm_bob = make_dm(bob, me);
    let usernames = HashMap::new();

    let rooms = vec![
        make_room(lounge, "lounge", "public", true, Some("lounge")),
        make_room(
            announcements,
            "topic",
            "public",
            true,
            Some("announcements"),
        ),
        make_room(public_alpha, "topic", "public", false, Some("alpha")),
        (dm_bob.clone(), Vec::new()),
    ];
    let order = |collapsed: &HashSet<RoomSection>| {
        visual_order_for_rooms(RoomVisualOrderInput {
            rooms: &rooms,
            user_id: me,
            usernames: &usernames,
            unread_counts: &HashMap::new(),
            room_last_message_at: &HashMap::new(),
            feeds_available: false,
            favorite_room_ids: &[],
            collapsed_sections: collapsed,
            ignored_user_ids: &HashSet::new(),
        })
    };

    // Nothing collapsed: every section's rooms are present.
    let full = order(&HashSet::new());
    assert!(full.contains(&RoomSlot::Room(lounge)));
    assert!(full.contains(&RoomSlot::Room(public_alpha)));
    assert!(full.contains(&RoomSlot::Room(dm_bob.id)));

    // Channels collapsed: the channel drops out, Core/Updates/DMs stay.
    let channels_collapsed = HashSet::from([RoomSection::Channels]);
    let c = order(&channels_collapsed);
    assert!(!c.contains(&RoomSlot::Room(public_alpha)));
    assert!(c.contains(&RoomSlot::Room(lounge)));
    assert!(c.contains(&RoomSlot::News));
    assert!(c.contains(&RoomSlot::Room(dm_bob.id)));

    // Core collapsed: core rooms and the core synthetic slots drop out.
    let core_collapsed = HashSet::from([RoomSection::Core]);
    let co = order(&core_collapsed);
    assert!(!co.contains(&RoomSlot::Room(lounge)));
    assert!(!co.contains(&RoomSlot::Room(announcements)));
    assert!(!co.contains(&RoomSlot::Notifications));
    assert!(!co.contains(&RoomSlot::News));
    // Discover now lives at the bottom of Core, so it collapses with it.
    assert!(!co.contains(&RoomSlot::Discover));
    assert!(co.contains(&RoomSlot::Room(public_alpha)));

    // Updates is now hosted by the Directory page, not the Home rail.
    let updates_collapsed = HashSet::from([RoomSection::Updates]);
    let u = order(&updates_collapsed);
    assert!(u.contains(&RoomSlot::News));
    assert!(!u.contains(&RoomSlot::Showcase));
    assert!(!u.contains(&RoomSlot::Work));
    // Discover lives in Core, which is expanded here, so it stays present.
    assert!(u.contains(&RoomSlot::Discover));

    // DMs collapsed: the DM drops out.
    let dms_collapsed = HashSet::from([RoomSection::Dms]);
    let d = order(&dms_collapsed);
    assert!(!d.contains(&RoomSlot::Room(dm_bob.id)));
    assert!(d.contains(&RoomSlot::Room(lounge)));
}

#[test]
fn visual_order_dms_use_snapshot_activity_not_loaded_tails() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);
    let dm_alice = make_dm(me, alice);
    let dm_bob = make_dm(me, bob);
    let older = chrono::Utc::now();
    let newer = older + chrono::Duration::minutes(1);
    let loaded_newer = newer + chrono::Duration::minutes(1);

    let mut usernames = HashMap::new();
    usernames.insert(alice, "alice".to_string());
    usernames.insert(bob, "bob".to_string());

    let rooms = vec![
        (
            dm_alice.clone(),
            vec![ChatMessage {
                room_id: dm_alice.id,
                created: loaded_newer,
                updated: loaded_newer,
                ..make_msg(Uuid::from_u128(50))
            }],
        ),
        (dm_bob.clone(), Vec::new()),
    ];
    let mut room_last_message_at = HashMap::new();
    room_last_message_at.insert(dm_alice.id, Some(older));
    room_last_message_at.insert(dm_bob.id, Some(newer));

    let order = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: &rooms,
        user_id: me,
        usernames: &usernames,
        unread_counts: &HashMap::new(),
        room_last_message_at: &room_last_message_at,
        feeds_available: false,
        favorite_room_ids: &[],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &HashSet::new(),
    });
    let dm_order: Vec<_> = order
        .into_iter()
        .filter_map(|slot| match slot {
            RoomSlot::Room(room_id) => Some(room_id),
            _ => None,
        })
        .collect();

    assert_eq!(dm_order, vec![dm_bob.id, dm_alice.id]);
}

#[test]
fn visual_order_hides_dm_with_ignored_peer() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);
    let dm_alice = make_dm(me, alice);
    let dm_bob = make_dm(me, bob);

    let mut usernames = HashMap::new();
    usernames.insert(alice, "alice".to_string());
    usernames.insert(bob, "bob".to_string());

    let rooms = vec![(dm_alice.clone(), Vec::new()), (dm_bob.clone(), Vec::new())];
    let ignored = HashSet::from([bob]);

    let order = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: &rooms,
        user_id: me,
        usernames: &usernames,
        unread_counts: &HashMap::new(),
        room_last_message_at: &HashMap::new(),
        feeds_available: false,
        favorite_room_ids: &[],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &ignored,
    });

    assert!(order.contains(&RoomSlot::Room(dm_alice.id)));
    // The ignored peer's DM must not resurface in the rail.
    assert!(!order.contains(&RoomSlot::Room(dm_bob.id)));

    // Even when favorited, an ignored peer's DM stays hidden from every
    // section so it can't be jump-addressable via the favorites path.
    let favorited = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: &rooms,
        user_id: me,
        usernames: &usernames,
        unread_counts: &HashMap::new(),
        room_last_message_at: &HashMap::new(),
        feeds_available: false,
        favorite_room_ids: &[dm_bob.id],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &ignored,
    });
    assert!(!favorited.contains(&RoomSlot::Room(dm_bob.id)));
}

#[test]
fn message_is_ignored_in_covers_author_and_reply_target() {
    let ignored_user = Uuid::from_u128(2);
    let other = Uuid::from_u128(3);
    let bot = Uuid::from_u128(4);
    let ignored = HashSet::from([ignored_user]);

    // Author ignored.
    let mut by_author = make_msg(Uuid::from_u128(10));
    by_author.user_id = ignored_user;
    assert!(message_is_ignored_in(&ignored, &by_author));

    // Bot reply directed at the ignored user.
    let mut bot_reply = make_msg(Uuid::from_u128(11));
    bot_reply.user_id = bot;
    bot_reply.reply_to_user_id = Some(ignored_user);
    assert!(message_is_ignored_in(&ignored, &bot_reply));

    // Bot reply directed at someone else is kept.
    let mut other_reply = make_msg(Uuid::from_u128(12));
    other_reply.user_id = bot;
    other_reply.reply_to_user_id = Some(other);
    assert!(!message_is_ignored_in(&ignored, &other_reply));

    // Ordinary message from a non-ignored author is kept.
    let mut normal = make_msg(Uuid::from_u128(13));
    normal.user_id = other;
    assert!(!message_is_ignored_in(&ignored, &normal));
}

#[test]
fn adjacent_composer_room_skips_virtual_slots() {
    let room_a = Uuid::from_u128(1);
    let room_b = Uuid::from_u128(2);
    let room_c = Uuid::from_u128(3);
    let order = vec![
        RoomSlot::Room(room_a),
        RoomSlot::News,
        RoomSlot::Showcase,
        RoomSlot::Work,
        RoomSlot::Notifications,
        RoomSlot::Discover,
        RoomSlot::Room(room_b),
        RoomSlot::Room(room_c),
    ];

    assert_eq!(
        adjacent_composer_room(&order, Some(room_a), 1),
        Some(room_b)
    );
    assert_eq!(
        adjacent_composer_room(&order, Some(room_b), -1),
        Some(room_a)
    );
    assert_eq!(
        adjacent_composer_room(&order, Some(room_c), 1),
        Some(room_a)
    );
}

#[test]
fn adjacent_composer_room_returns_none_without_real_rooms() {
    let order = vec![
        RoomSlot::News,
        RoomSlot::Showcase,
        RoomSlot::Work,
        RoomSlot::Notifications,
        RoomSlot::Discover,
    ];
    assert_eq!(adjacent_composer_room(&order, None, 1), None);
}

#[test]
fn room_membership_command_target_ignores_stale_real_room_for_synthetic_entries() {
    let stale_room = Uuid::from_u128(1);
    let selected = SelectedRoomSlotState {
        selected_room_id: Some(stale_room),
        news_selected: true,
        ..SelectedRoomSlotState::default()
    };

    assert_eq!(room_membership_command_target(None, selected), None);
}

#[test]
fn current_slot_prefers_synthetic_entry_over_stale_room_id() {
    let stale_room = Uuid::from_u128(1);
    let selected = SelectedRoomSlotState {
        selected_room_id: Some(stale_room),
        work_selected: true,
        ..SelectedRoomSlotState::default()
    };

    assert_eq!(current_slot_from_state(selected), Some(RoomSlot::Work));
}

#[test]
fn room_membership_command_target_prefers_active_composer_room() {
    let stale_room = Uuid::from_u128(1);
    let composer_room = Uuid::from_u128(2);
    let selected = SelectedRoomSlotState {
        selected_room_id: Some(stale_room),
        news_selected: true,
        ..SelectedRoomSlotState::default()
    };

    assert_eq!(
        room_membership_command_target(Some(composer_room), selected),
        Some(composer_room)
    );
}

#[test]
fn room_slug_for_uses_explicit_room_id() {
    let lounge_id = Uuid::from_u128(11);
    let announcements_id = Uuid::from_u128(12);
    let rooms = vec![
        (
            ChatRoom {
                id: lounge_id,
                created: chrono::Utc::now(),
                updated: chrono::Utc::now(),
                kind: "lounge".to_string(),
                visibility: "public".to_string(),
                auto_join: true,
                permanent: true,
                slug: Some("lounge".to_string()),
                language_code: None,
                dm_user_a: None,
                dm_user_b: None,
            },
            vec![],
        ),
        (
            ChatRoom {
                id: announcements_id,
                created: chrono::Utc::now(),
                updated: chrono::Utc::now(),
                kind: "topic".to_string(),
                visibility: "public".to_string(),
                auto_join: true,
                permanent: true,
                slug: Some("announcements".to_string()),
                language_code: None,
                dm_user_a: None,
                dm_user_b: None,
            },
            vec![],
        ),
    ];

    assert_eq!(room_slug_for(&rooms, lounge_id), Some("lounge".to_string()));
    assert_eq!(
        room_slug_for(&rooms, announcements_id),
        Some("announcements".to_string())
    );
}

#[test]
fn room_jump_keys_continue_with_uppercase_after_digits() {
    assert_eq!(
        ROOM_JUMP_KEYS,
        b"asdfghjklqwertyuiopzxcvbnm1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    );
}

#[test]
fn resolve_room_jump_target_is_case_sensitive() {
    let room_id = Uuid::from_u128(7);
    let uppercase_room_id = Uuid::from_u128(8);
    let targets = [
        (b'a', RoomSlot::Room(room_id)),
        (b'A', RoomSlot::Room(uppercase_room_id)),
        (b's', RoomSlot::News),
        (b'd', RoomSlot::Showcase),
        (b'w', RoomSlot::Work),
        (b'f', RoomSlot::Notifications),
        (b'g', RoomSlot::Discover),
    ];

    assert_eq!(
        resolve_room_jump_target(&targets, b'A'),
        Some(RoomSlot::Room(uppercase_room_id))
    );
    assert_eq!(
        resolve_room_jump_target(&targets, b's'),
        Some(RoomSlot::News)
    );
    assert_eq!(resolve_room_jump_target(&targets, b'D'), None);
    assert_eq!(
        resolve_room_jump_target(&targets, b'w'),
        Some(RoomSlot::Work)
    );
    assert_eq!(
        resolve_room_jump_target(&targets, b'f'),
        Some(RoomSlot::Notifications)
    );
    assert_eq!(resolve_room_jump_target(&targets, b'G'), None);
    assert_eq!(resolve_room_jump_target(&targets, b'x'), None);
}

#[test]
fn parse_user_command_with_username() {
    assert_eq!(
        parse_user_command("/ignore @alice", "/ignore"),
        Some(Some("alice"))
    );
    assert_eq!(
        parse_user_command("/unignore bob", "/unignore"),
        Some(Some("bob"))
    );
}

#[test]
fn parse_user_command_lists_when_username_missing() {
    assert_eq!(parse_user_command("/ignore", "/ignore"), Some(None));
    assert_eq!(parse_user_command("/ignore   ", "/ignore"), Some(None));
    assert_eq!(parse_user_command("/ignore @", "/ignore"), Some(None));
    assert_eq!(parse_user_command("/unignore", "/unignore"), Some(None));
}

#[test]
fn parse_user_command_rejects_non_matches() {
    assert_eq!(parse_user_command("ignore alice", "/ignore"), None);
    assert_eq!(parse_user_command("/ignored alice", "/ignore"), None);
    assert_eq!(parse_user_command("/unignored alice", "/unignore"), None);
}

#[test]
fn parse_report_command_requires_enough_text() {
    assert_eq!(
        parse_report_command("/bug the door ate my hat"),
        Some((ReportKind::Bug, Some("the door ate my hat".to_string())))
    );
    assert_eq!(
        parse_report_command("  /suggest more cats in the lounge  "),
        Some((
            ReportKind::Suggestion,
            Some("more cats in the lounge".to_string())
        ))
    );
    // Bare or too-short reports show usage instead of posting.
    assert_eq!(parse_report_command("/bug"), Some((ReportKind::Bug, None)));
    assert_eq!(
        parse_report_command("/bug lol"),
        Some((ReportKind::Bug, None))
    );
    assert_eq!(
        parse_report_command("/suggest   "),
        Some((ReportKind::Suggestion, None))
    );
    // Not report commands at all.
    assert_eq!(parse_report_command("/buggy thing"), None);
    assert_eq!(parse_report_command("/suggestions here"), None);
    assert_eq!(parse_report_command("bug report"), None);
}

#[test]
fn reply_preview_text_compacts_report_cards() {
    assert_eq!(
        reply_preview_text("---BUG--- the door ate my hat"),
        "🐛 the door ate my hat"
    );
    assert_eq!(
        reply_preview_text("---SUGGESTION--- more cats\nplease"),
        "💡 more cats"
    );
}

#[test]
fn parse_public_room_with_hash() {
    assert_eq!(
        parse_room_command("/public #lobby", "/public"),
        Some("lobby")
    );
}

#[test]
fn parse_public_room_without_hash() {
    assert_eq!(
        parse_room_command("/public lobby", "/public"),
        Some("lobby")
    );
}

#[test]
fn parse_private_room_with_hash() {
    assert_eq!(
        parse_room_command("/private #hideout", "/private"),
        Some("hideout")
    );
}

#[test]
fn parse_private_room_empty() {
    assert_eq!(parse_room_command("/private ", "/private"), None);
    assert_eq!(parse_room_command("/private #", "/private"), None);
}

#[test]
fn parse_private_room_not_command() {
    assert_eq!(parse_room_command("hello", "/private"), None);
    assert_eq!(parse_room_command("/privates foo", "/private"), None);
}

#[test]
fn user_created_channel_name_length_allows_16_chars() {
    assert!(!user_created_channel_name_too_long("1234567890123456"));
}

#[test]
fn user_created_channel_name_length_rejects_more_than_16_chars() {
    assert!(user_created_channel_name_too_long("12345678901234567"));
}

#[test]
fn user_created_channel_name_length_counts_chars_not_bytes() {
    let sixteen = "界".repeat(16);
    let seventeen = "界".repeat(17);

    assert!(!user_created_channel_name_too_long(&sixteen));
    assert!(user_created_channel_name_too_long(&seventeen));
}

#[test]
fn parse_room_command_keeps_legacy_long_slugs_parseable() {
    assert_eq!(
        parse_room_command("/public #very-long-legacy-channel", "/public"),
        Some("very-long-legacy-channel")
    );
}

#[test]
fn parse_create_room_with_hash() {
    assert_eq!(
        parse_create_room_command("/create-room #announcements"),
        Some("announcements")
    );
}

#[test]
fn parse_create_room_without_hash() {
    assert_eq!(
        parse_create_room_command("/create-room announcements"),
        Some("announcements")
    );
}

#[test]
fn parse_create_room_empty() {
    assert_eq!(parse_create_room_command("/create-room "), None);
    assert_eq!(parse_create_room_command("/create-room #"), None);
}

#[test]
fn parse_create_room_not_command() {
    assert_eq!(parse_create_room_command("hello"), None);
    assert_eq!(parse_create_room_command("/create-rooms foo"), None);
}

#[test]
fn parse_delete_room_with_hash() {
    assert_eq!(
        parse_delete_room_command("/delete-room #announcements"),
        Some("announcements")
    );
}

#[test]
fn parse_delete_room_without_hash() {
    assert_eq!(
        parse_delete_room_command("/delete-room announcements"),
        Some("announcements")
    );
}

#[test]
fn parse_delete_room_empty() {
    assert_eq!(parse_delete_room_command("/delete-room "), None);
}

#[test]
fn parse_delete_room_not_command() {
    assert_eq!(parse_delete_room_command("hello"), None);
}

#[test]
fn parse_fill_room_with_hash() {
    assert_eq!(
        parse_fill_room_command("/fill-room #announcements"),
        Some("announcements")
    );
}

#[test]
fn parse_fill_room_without_hash() {
    assert_eq!(
        parse_fill_room_command("/fill-room announcements"),
        Some("announcements")
    );
}

#[test]
fn parse_fill_room_empty() {
    assert_eq!(parse_fill_room_command("/fill-room "), None);
    assert_eq!(parse_fill_room_command("/fill-room #"), None);
}

#[test]
fn parse_fill_room_not_command() {
    assert_eq!(parse_fill_room_command("hello"), None);
    assert_eq!(parse_fill_room_command("/fill-rooms foo"), None);
}

#[test]
fn parse_cup_command_matches_coffee_and_tea_case_insensitively() {
    assert_eq!(parse_cup_command("/coffee"), Some(CupKind::Coffee));
    assert_eq!(parse_cup_command("/Coffee"), Some(CupKind::Coffee));
    assert_eq!(parse_cup_command("  /COFFEE  "), Some(CupKind::Coffee));
    assert_eq!(parse_cup_command("/tea"), Some(CupKind::Tea));
    assert_eq!(parse_cup_command("/TEA"), Some(CupKind::Tea));
}

#[test]
fn parse_cup_command_rejects_arguments_and_typos() {
    // Arguments fall through so the typo handler can still flag "/coffe".
    assert_eq!(parse_cup_command("/coffee please"), None);
    assert_eq!(parse_cup_command("/tea time"), None);
    assert_eq!(parse_cup_command("/coffe"), None);
    assert_eq!(parse_cup_command("/teas"), None);
    assert_eq!(parse_cup_command("hello"), None);
    assert_eq!(parse_cup_command(""), None);
}

#[test]
fn cup_art_uses_kind_specific_silhouette() {
    let coffee = cup_art(CupKind::Coffee, 0);
    assert!(
        coffee.ends_with("c[_]"),
        "coffee should end with mug glyph, got {coffee:?}"
    );
    let tea = cup_art(CupKind::Tea, 0);
    assert!(
        tea.ends_with("\\___/"),
        "tea should end with handle-less cup, got {tea:?}"
    );
}

#[test]
fn cup_art_rotates_steam_pattern_with_variant() {
    let v0 = cup_art(CupKind::Coffee, 0);
    let v1 = cup_art(CupKind::Coffee, 1);
    let v2 = cup_art(CupKind::Coffee, 2);
    let v3 = cup_art(CupKind::Coffee, 3);
    assert_ne!(v0, v1);
    assert_ne!(v1, v2);
    assert_ne!(v2, v3);
    // CUP_VARIANT_COUNT is the period — variant 4 wraps to variant 0.
    assert_eq!(cup_art(CupKind::Coffee, 4), v0);
}

#[test]
fn unknown_slash_command_detects_typo() {
    assert_eq!(unknown_slash_command("/lsit"), Some("/lsit"));
    assert_eq!(unknown_slash_command("/lsit #lounge"), Some("/lsit"));
}

#[test]
fn unknown_slash_command_ignores_regular_messages_and_multiline_text() {
    assert_eq!(unknown_slash_command("hello"), None);
    assert_eq!(unknown_slash_command("// not a command"), None);
    assert_eq!(unknown_slash_command("/bin/ls\nstill talking"), None);
}

fn petname_request(input: &str) -> Option<PetnameRequest> {
    match parse_petname_command(input) {
        Some(PetnameParse::Request(r)) => Some(r),
        _ => None,
    }
}

#[test]
fn parse_petname_show_set_clear() {
    assert_eq!(petname_request("/petname"), Some(PetnameRequest::Show));
    assert_eq!(petname_request("/petname    "), Some(PetnameRequest::Show));
    assert_eq!(
        petname_request("/petname Whiskers"),
        Some(PetnameRequest::Set("Whiskers".to_string()))
    );
    // Inner whitespace runs collapse to a single space.
    assert_eq!(
        petname_request("/petname Sir   Hopkins"),
        Some(PetnameRequest::Set("Sir Hopkins".to_string()))
    );
    for word in ["clear", "remove", "none", "off", "CLEAR"] {
        assert_eq!(
            petname_request(&format!("/petname {word}")),
            Some(PetnameRequest::Clear),
            "{word}"
        );
    }
}

#[test]
fn parse_petname_ignores_non_petname_lines() {
    assert!(parse_petname_command("/petnames").is_none());
    assert!(parse_petname_command("/petnamer").is_none());
    assert!(parse_petname_command("rename my pet").is_none());
    assert!(parse_petname_command("/dm @alice").is_none());
}

#[test]
fn format_active_user_lines_sorts_and_shows_session_counts() {
    let friend_id = Uuid::now_v7();
    let active_users = std::sync::Arc::new(std::sync::Mutex::new(HashMap::from([
        (
            friend_id,
            ActiveUser {
                username: "zoe".to_string(),
                fingerprint: None,
                peer_ip: None,
                audio_source: late_core::models::user::AudioSource::Icecast,
                sessions: Vec::new(),
                connection_count: 2,
                last_login_at: std::time::Instant::now(),
            },
        ),
        (
            Uuid::now_v7(),
            ActiveUser {
                username: "alice".to_string(),
                fingerprint: None,
                peer_ip: None,
                audio_source: late_core::models::user::AudioSource::Icecast,
                sessions: Vec::new(),
                connection_count: 1,
                last_login_at: std::time::Instant::now(),
            },
        ),
    ])));

    assert_eq!(
        format_active_user_lines(Some(&active_users), &HashSet::new()),
        vec!["@alice".to_string(), "@zoe (2 sessions)".to_string()]
    );
    assert_eq!(
        format_active_user_lines(Some(&active_users), &HashSet::from([friend_id])),
        vec!["@alice".to_string(), "★ @zoe (2 sessions)".to_string()]
    );
}

#[test]
fn format_active_user_lines_handles_missing_registry() {
    assert_eq!(
        format_active_user_lines(None, &HashSet::new()),
        vec!["Active user list unavailable".to_string()]
    );
}

// --- adjacent_message_id (delete-and-advance) ---

fn make_msg(id: Uuid) -> ChatMessage {
    ChatMessage {
        id,
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        pinned: false,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(999),
        user_id: Uuid::from_u128(999),
        body: String::new(),
    }
}

fn make_reply_msg(id: Uuid, reply_to_message_id: Uuid) -> ChatMessage {
    ChatMessage {
        reply_to_message_id: Some(reply_to_message_id),
        ..make_msg(id)
    }
}

#[test]
fn system_line_text_requires_system_author_and_prefix() {
    let system_id = Uuid::from_u128(1);
    let mut usernames = HashMap::new();
    usernames.insert(system_id, "system".to_string());
    usernames.insert(Uuid::from_u128(3), "mira".to_string());

    let mut line = make_msg(Uuid::from_u128(10));
    line.user_id = system_id;
    line.body = "· mira sat down at poker".to_string();
    assert_eq!(
        system_line_text_in(&usernames, &line),
        Some("mira sat down at poker".to_string())
    );

    // The system author without the prefix stays a normal message...
    let mut no_prefix = make_msg(Uuid::from_u128(11));
    no_prefix.user_id = system_id;
    no_prefix.body = "hello".to_string();
    assert_eq!(system_line_text_in(&usernames, &no_prefix), None);

    // ...and so does a non-system author pasting the prefix.
    let mut spoof = make_msg(Uuid::from_u128(12));
    spoof.user_id = Uuid::from_u128(3);
    spoof.body = "· fake activity".to_string();
    assert_eq!(system_line_text_in(&usernames, &spoof), None);
}

#[test]
fn search_snippet_windows_around_match() {
    let body = format!("{}the deploy failed at midnight", "padding ".repeat(10));
    let (prefix, matched, suffix) = build_search_snippet(&body, "deploy failed");
    assert!(prefix.starts_with('…'), "long lead-in is trimmed");
    assert!(prefix.ends_with("the "));
    assert_eq!(matched, "deploy failed");
    assert_eq!(suffix, " at midnight");
}

#[test]
fn search_snippet_matches_case_insensitively_and_across_newlines() {
    let (prefix, matched, suffix) = build_search_snippet("one\nDEPLOY two", "deploy");
    assert_eq!(prefix, "one ");
    assert_eq!(matched, "DEPLOY");
    assert_eq!(suffix, " two");
}

#[test]
fn search_snippet_without_match_falls_back_to_head() {
    let (prefix, matched, suffix) = build_search_snippet("short body", "absent");
    assert_eq!(prefix, "short body");
    assert!(matched.is_empty());
    assert!(suffix.is_empty());

    let (empty_query_prefix, empty_query_match, _) = build_search_snippet("preview", "");
    assert_eq!(empty_query_prefix, "preview");
    assert!(empty_query_match.is_empty());
}

#[test]
fn search_snippet_strips_card_markers() {
    let (prefix, matched, _) = build_search_snippet(
        "---NEWS--- rust 2.0 released || summary || https://example.com",
        "rust 2.0",
    );
    assert!(!prefix.contains("---NEWS---"));
    assert_eq!(matched, "rust 2.0");

    // A fake marker that is not all-uppercase stays untouched.
    let (prefix, _, _) = build_search_snippet("---not a marker--- text", "text");
    assert!(prefix.starts_with("---not a marker---"));
}

#[test]
fn ticker_queue_dedupes_orders_newest_first_and_caps() {
    let base = chrono::Utc::now();
    let entry = |n: u128, offset_secs: i64| ActivityTickerEntry {
        id: Uuid::from_u128(n),
        text: format!("event {n}"),
        at: base + chrono::Duration::seconds(offset_secs),
    };

    let mut entries = Vec::new();
    // Tails replay out of order; the queue must still end newest-first.
    note_ticker_entry(&mut entries, entry(1, 10));
    note_ticker_entry(&mut entries, entry(2, 30));
    note_ticker_entry(&mut entries, entry(3, 20));
    assert_eq!(
        entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        vec![Uuid::from_u128(2), Uuid::from_u128(3), Uuid::from_u128(1)]
    );

    // A snapshot replaying an already-seen message is a no-op.
    note_ticker_entry(&mut entries, entry(2, 30));
    assert_eq!(entries.len(), 3);

    // Overflow drops the oldest, never the newest.
    for n in 4..=12 {
        note_ticker_entry(&mut entries, entry(n, 30 + n as i64));
    }
    assert_eq!(entries.len(), ACTIVITY_TICKER_CAP);
    assert_eq!(entries[0].id, Uuid::from_u128(12));
    assert!(!entries.iter().any(|e| e.id == Uuid::from_u128(1)));
}

#[test]
fn inline_image_url_in_body_accepts_image_url_with_query() {
    assert_eq!(
        inline_image_url_in_body("look https://example.com/image.webp?size=large"),
        Some("https://example.com/image.webp?size=large".to_string())
    );
}

#[test]
fn inline_image_request_candidates_scan_newest_messages_first() {
    let now = Instant::now();
    let mut messages: Vec<ChatMessage> = (1..=101)
        .map(|idx| make_msg(Uuid::from_u128(idx)))
        .collect();
    messages[0].body = "https://files.example.com/newest.png".to_string();

    let requests = inline_image_request_candidates(
        &messages,
        &HashSet::new(),
        &HashMap::new(),
        &HashMap::new(),
        now,
    );

    assert_eq!(
        requests,
        vec![(
            messages[0].id,
            "https://files.example.com/newest.png".to_string()
        )]
    );
}

#[test]
fn inline_image_request_candidates_respect_retry_backoff() {
    let now = Instant::now();
    let mut message = make_msg(Uuid::from_u128(1));
    message.body = "https://files.example.com/pending.png".to_string();
    let messages = vec![message.clone()];
    let mut failures = HashMap::from([(
        message.id,
        InlineImageFailure {
            attempts: 1,
            next_retry_at: now + Duration::from_secs(5),
        },
    )]);

    assert!(
        inline_image_request_candidates(
            &messages,
            &HashSet::new(),
            &HashMap::new(),
            &failures,
            now,
        )
        .is_empty()
    );

    failures.insert(
        message.id,
        InlineImageFailure {
            attempts: 1,
            next_retry_at: now - Duration::from_secs(1),
        },
    );
    assert_eq!(
        inline_image_request_candidates(
            &messages,
            &HashSet::new(),
            &HashMap::new(),
            &failures,
            now,
        ),
        vec![(
            message.id,
            "https://files.example.com/pending.png".to_string()
        )]
    );

    failures.insert(
        message.id,
        InlineImageFailure {
            attempts: INLINE_IMAGE_MAX_FAILURES,
            next_retry_at: now - Duration::from_secs(1),
        },
    );
    assert!(
        inline_image_request_candidates(
            &messages,
            &HashSet::new(),
            &HashMap::new(),
            &failures,
            now,
        )
        .is_empty()
    );
}

#[test]
fn adjacent_message_id_returns_none_for_empty_list() {
    assert_eq!(adjacent_message_id(&[], Uuid::from_u128(1)), None);
}

#[test]
fn adjacent_message_id_returns_none_when_not_in_list() {
    let msgs = vec![make_msg(Uuid::from_u128(1))];
    assert_eq!(adjacent_message_id(&msgs, Uuid::from_u128(99)), None);
}

#[test]
fn adjacent_message_id_prefers_next_index_older_message() {
    // List is newest-first: [0]=newest, [1]=middle, [2]=oldest.
    // Deleting the middle should land on the oldest (idx+1).
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let c = Uuid::from_u128(3);
    let msgs = vec![make_msg(a), make_msg(b), make_msg(c)];
    assert_eq!(adjacent_message_id(&msgs, b), Some(c));
}

#[test]
fn adjacent_message_id_falls_back_to_previous_for_last_item() {
    // Deleting the oldest (last index) should land on the previous-older
    // message (idx-1), i.e., the next-oldest remaining.
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let c = Uuid::from_u128(3);
    let msgs = vec![make_msg(a), make_msg(b), make_msg(c)];
    assert_eq!(adjacent_message_id(&msgs, c), Some(b));
}

#[test]
fn adjacent_message_id_returns_none_for_sole_item() {
    let a = Uuid::from_u128(1);
    let msgs = vec![make_msg(a)];
    assert_eq!(adjacent_message_id(&msgs, a), None);
}

#[test]
fn loaded_reply_target_id_returns_loaded_target() {
    let reply = Uuid::from_u128(1);
    let original = Uuid::from_u128(2);
    let msgs = vec![make_reply_msg(reply, original), make_msg(original)];

    assert_eq!(loaded_reply_target_id(&msgs, reply), Some(Some(original)));
}

#[test]
fn loaded_reply_target_id_returns_none_inner_when_target_not_loaded() {
    let reply = Uuid::from_u128(1);
    let original = Uuid::from_u128(2);
    let msgs = vec![make_reply_msg(reply, original)];

    assert_eq!(loaded_reply_target_id(&msgs, reply), Some(None));
}

#[test]
fn loaded_reply_target_id_rejects_non_reply_messages() {
    let message = Uuid::from_u128(1);
    let msgs = vec![make_msg(message)];

    assert_eq!(loaded_reply_target_id(&msgs, message), None);
}

// --- dm_sort_key (regression: nav order must match UI order) ---

fn make_dm(user_a: Uuid, user_b: Uuid) -> ChatRoom {
    ChatRoom {
        id: Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        kind: "dm".to_string(),
        visibility: "dm".to_string(),
        auto_join: false,
        permanent: false,
        slug: None,
        language_code: None,
        dm_user_a: Some(user_a),
        dm_user_b: Some(user_b),
    }
}

#[test]
fn dm_sort_key_resolves_other_users_name() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);

    let mut usernames = HashMap::new();
    usernames.insert(me, "me".to_string());
    usernames.insert(alice, "alice".to_string());
    usernames.insert(bob, "bob".to_string());

    let room = make_dm(me, alice);
    assert_eq!(dm_sort_key(&room, me, &usernames), "@alice");

    // Works regardless of which slot I'm in
    let room = make_dm(bob, me);
    assert_eq!(dm_sort_key(&room, me, &usernames), "@bob");
}

#[test]
fn dm_sort_key_orders_alphabetically_by_display_name() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let charlie = Uuid::from_u128(3);
    let bob = Uuid::from_u128(4);

    let mut usernames = HashMap::new();
    usernames.insert(alice, "alice".to_string());
    usernames.insert(charlie, "charlie".to_string());
    usernames.insert(bob, "bob".to_string());

    let mut dms = [make_dm(me, charlie), make_dm(me, alice), make_dm(bob, me)];
    dms.sort_by_key(|r| dm_sort_key(r, me, &usernames));

    let names: Vec<_> = dms.iter().map(|r| dm_sort_key(r, me, &usernames)).collect();
    assert_eq!(names, vec!["@alice", "@bob", "@charlie"]);
}

#[test]
fn parse_brb_bare_command() {
    assert_eq!(parse_brb_command("/brb"), Some(String::new()));
}

#[test]
fn parse_brb_with_message() {
    assert_eq!(
        parse_brb_command("/brb grabbing coffee"),
        Some("grabbing coffee".to_string())
    );
}

#[test]
fn parse_brb_trims_whitespace() {
    assert_eq!(parse_brb_command("  /brb  "), Some(String::new()));
    assert_eq!(
        parse_brb_command("/brb   lots of spaces   "),
        Some("lots of spaces".to_string())
    );
}

#[test]
fn parse_brb_rejects_non_command() {
    assert_eq!(parse_brb_command("brb"), None);
    assert_eq!(parse_brb_command("/brbx something"), None);
    assert_eq!(parse_brb_command("hello /brb"), None);
    assert_eq!(parse_brb_command(""), None);
}

#[test]
fn set_context_value_reports_only_real_changes() {
    let user_id = Uuid::from_u128(1);
    let mut map = HashMap::new();

    // Insert, same-value no-op, change, blank clears, clear of absent key.
    assert!(set_context_value(&mut map, user_id, Some("mod")));
    assert!(!set_context_value(&mut map, user_id, Some("mod")));
    assert!(set_context_value(&mut map, user_id, Some("artist")));
    assert!(set_context_value(&mut map, user_id, Some("  ")));
    assert!(map.is_empty());
    assert!(!set_context_value(&mut map, user_id, None));
}

#[test]
fn extend_changed_reports_only_real_changes() {
    let a = Uuid::from_u128(1);
    let b = Uuid::from_u128(2);
    let mut map = HashMap::from([(a, "alice".to_string())]);

    // Identical merge is a no-op; a new key or changed value reports true.
    assert!(!extend_changed(
        &mut map,
        HashMap::from([(a, "alice".to_string())])
    ));
    assert!(extend_changed(
        &mut map,
        HashMap::from([(b, "bob".to_string())])
    ));
    assert!(extend_changed(
        &mut map,
        HashMap::from([(a, "alicia".to_string())])
    ));
    assert_eq!(map.get(&a).map(String::as_str), Some("alicia"));
}

/// A ChatState wired to a real DB with inert side services, for exercising
/// the row-cache counter contract directly.
async fn counter_test_state(test_db: &late_core::test_utils::TestDb, user_id: Uuid) -> ChatState {
    let db = test_db.db.clone();
    let notifications = crate::app::chat::notifications::svc::NotificationService::new(db.clone());
    let chat = crate::app::chat::svc::ChatService::new(db.clone(), notifications.clone());
    let ai = crate::app::ai::svc::AiService::new(false, None, "test".to_string());
    let articles = crate::app::chat::news::svc::ArticleService::new(db.clone(), ai, chat.clone());
    let (notifier, _outbox) = crate::app::notify::channel();
    ChatState::new(
        ChatServices {
            chat,
            notifications,
            articles,
            feeds: crate::app::chat::feeds::svc::FeedService::new(db.clone()),
            showcases: crate::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
            work: crate::app::chat::work::svc::WorkService::new(db),
        },
        user_id,
        crate::authz::Permissions::new(false, false),
        None,
        notifier,
    )
}

async fn refresh_and_drain(state: &mut ChatState) {
    crate::test_helpers::wait_until(
        || async { state.snapshot_rx.has_changed().unwrap_or(false) },
        "chat snapshot refresh",
    )
    .await;
    state.drain_snapshot();
}

#[tokio::test]
async fn identical_snapshot_reapply_keeps_row_cache_counters_stable() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "counter_user").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "counter_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join user");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "first".to_string(),
        },
    )
    .await
    .expect("first message");

    let mut state = counter_test_state(&test_db, user.id).await;
    refresh_and_drain(&mut state).await;
    assert!(!state.rooms.is_empty(), "initial snapshot loads rooms");
    let epoch = state.context_epoch();
    let version = state.room_version(lounge.id);

    // Snapshots arrive on a fixed cadence whether or not anything changed;
    // an identical reapply must not move any counter, or every session
    // rebuilds its row caches every 10 seconds for nothing.
    state.refresh_tx.send(()).expect("force refresh");
    refresh_and_drain(&mut state).await;
    assert_eq!(state.context_epoch(), epoch);
    assert_eq!(state.room_version(lounge.id), version);
}

#[tokio::test]
async fn push_message_bumps_only_its_room_version() {
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "bump_user").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let other = ChatRoom::get_or_create_public_room(&client, "bump-other")
        .await
        .expect("other room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join lounge");
    ChatRoomMember::join(&client, other.id, user.id)
        .await
        .expect("join other");

    let mut state = counter_test_state(&test_db, user.id).await;
    refresh_and_drain(&mut state).await;
    let lounge_version = state.room_version(lounge.id);
    let other_version = state.room_version(other.id);

    let message = late_core::models::chat_message::ChatMessage {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        pinned: false,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: lounge.id,
        user_id: user.id,
        body: "hello".to_string(),
    };
    state.push_message(message.clone());
    assert_eq!(state.room_version(lounge.id), lounge_version + 1);
    assert_eq!(state.room_version(other.id), other_version);

    // Duplicate delivery dedups by id and must not invalidate the cache.
    state.push_message(message.clone());
    assert_eq!(state.room_version(lounge.id), lounge_version + 1);

    // An edit replaces in place and must repaint.
    let mut edited = message;
    edited.body = "hello, edited".to_string();
    edited.updated = Utc::now();
    state.replace_message(edited);
    assert_eq!(state.room_version(lounge.id), lounge_version + 2);
}
