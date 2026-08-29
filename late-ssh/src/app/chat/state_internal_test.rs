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

/// The pot's composer boundary. Everything downstream trusts the count, so
/// this is where an unbuyable number has to die: only 1..=cap gets through,
/// and anything else is a usage banner rather than a refused transaction.
#[test]
fn parse_pot_command_only_admits_a_buyable_count() {
    use late_core::models::pot::POT_MAX_TICKETS_PER_DAY;

    assert_eq!(parse_pot_command("/pot"), Some(Some(PotCommand::Status)));
    assert_eq!(
        parse_pot_command("  /pot  "),
        Some(Some(PotCommand::Status))
    );
    assert_eq!(
        parse_pot_command("/pot buy 5"),
        Some(Some(PotCommand::Buy { count: 5 }))
    );
    assert_eq!(
        parse_pot_command(&format!("/pot buy {POT_MAX_TICKETS_PER_DAY}")),
        Some(Some(PotCommand::Buy {
            count: POT_MAX_TICKETS_PER_DAY
        }))
    );

    // Usage banner: a count nobody could buy, and a subcommand that is not
    // one. `Some(None)` is the "you meant the pot, but not like that" shape.
    for junk in [
        "/pot buy 0",
        "/pot buy -3",
        &format!("/pot buy {}", POT_MAX_TICKETS_PER_DAY + 1),
        "/pot buy all",
        "/pot buy",
        "/pot sell 3",
    ] {
        assert_eq!(parse_pot_command(junk), Some(None), "{junk}");
    }

    // Not a pot command at all: a longer command that merely starts the same
    // way must fall through to its own parser.
    assert_eq!(parse_pot_command("/potato"), None);
    assert_eq!(parse_pot_command("/pomodoro 25"), None);
    assert_eq!(parse_pot_command("hello"), None);
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

// --- parse_room_ban_command ---

/// The whole point of the duration slot: `/ban @user 7d spam` must not read
/// "7d" as the first word of the reason, and `/ban @user spamming` must not
/// lose its first word to a failed duration parse.
#[test]
fn parse_ban_splits_duration_from_reason() {
    let parsed = parse_room_ban_command("/ban @bob 7d shouting over me", "/ban")
        .expect("is a ban command")
        .expect("parses");
    assert_eq!(parsed.username, "bob");
    assert_eq!(parsed.duration, Some(chrono::Duration::days(7)));
    assert_eq!(parsed.reason, "shouting over me");

    let parsed = parse_room_ban_command("/ban bob shouting over me", "/ban")
        .expect("is a ban command")
        .expect("parses");
    assert_eq!(parsed.username, "bob");
    assert_eq!(parsed.duration, None);
    assert_eq!(parsed.reason, "shouting over me");
}

#[test]
fn parse_ban_bare_username_is_permanent_with_no_reason() {
    let parsed = parse_room_ban_command("/ban @bob", "/ban")
        .expect("is a ban command")
        .expect("parses");
    assert_eq!(parsed.username, "bob");
    assert_eq!(parsed.duration, None);
    assert_eq!(parsed.reason, "");
}

#[test]
fn parse_ban_rejects_a_missing_username_and_a_bad_duration() {
    assert!(
        parse_room_ban_command("/ban", "/ban")
            .expect("is a ban command")
            .is_err()
    );
    assert!(
        parse_room_ban_command("/ban   ", "/ban")
            .expect("is a ban command")
            .is_err()
    );
    assert!(
        parse_room_ban_command("/ban @bob -3d rude", "/ban")
            .expect("is a ban command")
            .is_err(),
        "a negative duration is a typo, not a permanent ban"
    );
}

#[test]
fn parse_ban_ignores_other_commands() {
    assert!(parse_room_ban_command("/banana split", "/ban").is_none());
    assert!(parse_room_ban_command("hello world", "/ban").is_none());
    assert!(parse_room_ban_command("/unban @bob", "/ban").is_none());
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
fn composer_cursor_visible_inverts_the_cell() {
    let mut textarea = new_chat_textarea();
    composer::set_themed_textarea_cursor_visible(&mut textarea, true);
    assert!(
        textarea
            .cursor_style()
            .add_modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
    assert_eq!(textarea.cursor_style().bg, None);
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
    assert_eq!(textarea.cursor_style().fg, Some(theme::TEXT()));
    assert!(
        textarea
            .cursor_style()
            .add_modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );

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
            topic: None,
            rules: None,
            created_by: None,
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
            cyberspace_linked: false,
            cyberspace_rooms: &[],
            cyberspace_mail: &[],
            favorite_room_ids: &[],
            collapsed_sections: &HashSet::new(),
            ignored_user_ids: &HashSet::new(),
            sticky_unread_dm: None,
            live_streams: &[],
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
fn every_section_round_trips_its_label_and_owns_a_unique_fold_key() {
    let mut shortcuts = HashSet::new();
    for section in RoomSection::ALL {
        // Clicking a header maps its text back to the section.
        assert_eq!(RoomSection::from_label(section.label()), Some(section));
        // `z` + this key folds it. A section whose key another one already
        // claimed is unreachable, and one missing from the `z` handler's own
        // key map is a section the rail draws but nothing can fold.
        assert!(
            shortcuts.insert(section.shortcut()),
            "two sections claim '{}'",
            section.shortcut() as char
        );
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
            cyberspace_linked: false,
            cyberspace_rooms: &[],
            cyberspace_mail: &[],
            favorite_room_ids: &[],
            collapsed_sections: collapsed,
            ignored_user_ids: &HashSet::new(),
            sticky_unread_dm: None,
            live_streams: &[],
        })
    };

    // Nothing collapsed: every section's rooms are present.
    let full = order(&HashSet::new());
    assert!(full.contains(&RoomSlot::Room(lounge)));
    assert!(full.contains(&RoomSlot::Room(public_alpha)));
    assert!(full.contains(&RoomSlot::Room(dm_bob.id)));

    // Channels collapsed: the channel drops out, Core and DMs stay.
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
        cyberspace_linked: false,
        cyberspace_rooms: &[],
        cyberspace_mail: &[],
        favorite_room_ids: &[],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &HashSet::new(),
        sticky_unread_dm: None,
        live_streams: &[],
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
        cyberspace_linked: false,
        cyberspace_rooms: &[],
        cyberspace_mail: &[],
        favorite_room_ids: &[],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &ignored,
        sticky_unread_dm: None,
        live_streams: &[],
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
        cyberspace_linked: false,
        cyberspace_rooms: &[],
        cyberspace_mail: &[],
        favorite_room_ids: &[dm_bob.id],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &ignored,
        sticky_unread_dm: None,
        live_streams: &[],
    });
    assert!(!favorited.contains(&RoomSlot::Room(dm_bob.id)));
}

#[test]
fn visual_order_promotes_unread_dms_above_channels() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);
    let carol = Uuid::from_u128(4);
    let lounge = Uuid::from_u128(10);
    let public_alpha = Uuid::from_u128(20);
    let dm_alice = make_dm(me, alice);
    let dm_bob = make_dm(me, bob);
    let dm_carol = make_dm(me, carol);

    let mut usernames = HashMap::new();
    usernames.insert(alice, "alice".to_string());
    usernames.insert(bob, "bob".to_string());
    usernames.insert(carol, "carol".to_string());

    let rooms = vec![
        make_room(lounge, "lounge", "public", true, Some("lounge")),
        make_room(public_alpha, "topic", "public", false, Some("alpha")),
        (dm_alice.clone(), Vec::new()),
        (dm_bob.clone(), Vec::new()),
        (dm_carol.clone(), Vec::new()),
    ];
    // Carol is favorited, so she stays in Favorites even while unread.
    let unread_counts = HashMap::from([(dm_bob.id, 3), (dm_carol.id, 1)]);

    let order = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: &rooms,
        user_id: me,
        usernames: &usernames,
        unread_counts: &unread_counts,
        room_last_message_at: &HashMap::new(),
        feeds_available: false,
        cyberspace_linked: false,
        cyberspace_rooms: &[],
        cyberspace_mail: &[],
        favorite_room_ids: &[dm_carol.id],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &HashSet::new(),
        sticky_unread_dm: None,
        live_streams: &[],
    });

    assert_eq!(
        order,
        vec![
            RoomSlot::Room(dm_carol.id),
            RoomSlot::Room(lounge),
            RoomSlot::Notifications,
            RoomSlot::News,
            RoomSlot::Discover,
            RoomSlot::Room(dm_bob.id),
            RoomSlot::Room(public_alpha),
            RoomSlot::Room(dm_alice.id),
        ]
    );
}

#[test]
fn visual_order_holds_the_dm_being_read_in_the_unread_group() {
    let me = Uuid::from_u128(1);
    let bob = Uuid::from_u128(3);
    let public_alpha = Uuid::from_u128(20);
    let dm_bob = make_dm(me, bob);

    let mut usernames = HashMap::new();
    usernames.insert(bob, "bob".to_string());

    let rooms = vec![
        make_room(public_alpha, "topic", "public", false, Some("alpha")),
        (dm_bob.clone(), Vec::new()),
    ];
    let order = |sticky: Option<Uuid>| {
        visual_order_for_rooms(RoomVisualOrderInput {
            rooms: &rooms,
            user_id: me,
            usernames: &usernames,
            // Opening the DM zeroes its count, which is exactly the moment the
            // row must not move.
            unread_counts: &HashMap::from([(dm_bob.id, 0)]),
            room_last_message_at: &HashMap::new(),
            feeds_available: false,
            cyberspace_linked: false,
            cyberspace_rooms: &[],
            cyberspace_mail: &[],
            favorite_room_ids: &[],
            collapsed_sections: &HashSet::new(),
            ignored_user_ids: &HashSet::new(),
            sticky_unread_dm: sticky,
            live_streams: &[],
        })
    };

    let dm_index = |order: &[RoomSlot]| {
        order
            .iter()
            .position(|slot| *slot == RoomSlot::Room(dm_bob.id))
            .expect("dm present")
    };
    let channel_index = |order: &[RoomSlot]| {
        order
            .iter()
            .position(|slot| *slot == RoomSlot::Room(public_alpha))
            .expect("channel present")
    };

    let reading = order(Some(dm_bob.id));
    assert!(dm_index(&reading) < channel_index(&reading));

    // Reading any other room releases it, and the DM drops back down.
    let left = order(None);
    assert!(dm_index(&left) > channel_index(&left));
}

#[test]
fn visual_order_keeps_promoted_unread_dms_when_the_dms_section_is_collapsed() {
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let bob = Uuid::from_u128(3);
    let dm_alice = make_dm(me, alice);
    let dm_bob = make_dm(me, bob);

    let mut usernames = HashMap::new();
    usernames.insert(alice, "alice".to_string());
    usernames.insert(bob, "bob".to_string());

    let rooms = vec![(dm_alice.clone(), Vec::new()), (dm_bob.clone(), Vec::new())];

    let order = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: &rooms,
        user_id: me,
        usernames: &usernames,
        unread_counts: &HashMap::from([(dm_bob.id, 2)]),
        room_last_message_at: &HashMap::new(),
        feeds_available: false,
        cyberspace_linked: false,
        cyberspace_rooms: &[],
        cyberspace_mail: &[],
        favorite_room_ids: &[],
        collapsed_sections: &HashSet::from([RoomSection::Dms]),
        ignored_user_ids: &HashSet::new(),
        sticky_unread_dm: None,
        live_streams: &[],
    });

    // Collapsing DMs folds away the read ones only; an unread DM lives in its
    // own group and stays reachable.
    assert!(order.contains(&RoomSlot::Room(dm_bob.id)));
    assert!(!order.contains(&RoomSlot::Room(dm_alice.id)));
}

#[test]
fn sticky_unread_dm_holds_the_open_dm_until_another_room_is_read() {
    let dm = Uuid::from_u128(1);
    let other_dm = Uuid::from_u128(2);
    let channel = Uuid::from_u128(3);

    // Opening an unread DM claims the slot.
    let sticky = next_sticky_unread_dm(NextStickyUnreadDm {
        current: None,
        room_id: dm,
        is_dm: true,
        unread: true,
    });
    assert_eq!(sticky, Some(dm));

    // A message landing while it is open re-marks it read; it must stay.
    assert_eq!(
        next_sticky_unread_dm(NextStickyUnreadDm {
            current: sticky,
            room_id: dm,
            is_dm: true,
            unread: false,
        }),
        Some(dm)
    );

    // Reading a channel or an already-read DM releases it.
    assert_eq!(
        next_sticky_unread_dm(NextStickyUnreadDm {
            current: sticky,
            room_id: channel,
            is_dm: false,
            unread: true,
        }),
        None
    );
    assert_eq!(
        next_sticky_unread_dm(NextStickyUnreadDm {
            current: sticky,
            room_id: other_dm,
            is_dm: true,
            unread: false,
        }),
        None
    );

    // Moving straight into another unread DM hands the slot over.
    assert_eq!(
        next_sticky_unread_dm(NextStickyUnreadDm {
            current: sticky,
            room_id: other_dm,
            is_dm: true,
            unread: true,
        }),
        Some(other_dm)
    );
}

#[test]
fn visual_order_never_promotes_an_ignored_peers_unread_dm() {
    let me = Uuid::from_u128(1);
    let bob = Uuid::from_u128(3);
    let dm_bob = make_dm(me, bob);
    let usernames = HashMap::new();
    let rooms = vec![(dm_bob.clone(), Vec::new())];

    let order = visual_order_for_rooms(RoomVisualOrderInput {
        rooms: &rooms,
        user_id: me,
        usernames: &usernames,
        unread_counts: &HashMap::from([(dm_bob.id, 5)]),
        room_last_message_at: &HashMap::new(),
        feeds_available: false,
        cyberspace_linked: false,
        cyberspace_rooms: &[],
        cyberspace_mail: &[],
        favorite_room_ids: &[],
        collapsed_sections: &HashSet::new(),
        ignored_user_ids: &HashSet::from([bob]),
        sticky_unread_dm: None,
        live_streams: &[],
    });

    assert!(!order.contains(&RoomSlot::Room(dm_bob.id)));
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
                topic: None,
                rules: None,
                created_by: None,
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
                topic: None,
                rules: None,
                created_by: None,
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
fn join_opens_a_public_room_like_public_does() {
    assert_eq!(parse_public_room_command("/join #lobby"), Some("lobby"));
    assert_eq!(parse_public_room_command("/join lobby"), Some("lobby"));
    assert_eq!(parse_public_room_command("/public #lobby"), Some("lobby"));

    // A bare alias is not a room command, so it falls through to the unknown
    // command banner the same way a bare `/public` does.
    assert_eq!(parse_public_room_command("/join "), None);
    assert_eq!(parse_public_room_command("/join #"), None);
    assert_eq!(parse_public_room_command("/joins lobby"), None);
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
        topic: None,
        rules: None,
        created_by: None,
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
fn counter_test_state(test_db: &late_core::test_utils::TestDb, user_id: Uuid) -> ChatState {
    chat_state_with_cyberspace(test_db, user_id).0
}

/// Same wiring, returning the cyberspace service handle so a test can play
/// the part of another session of the same linked account.
fn chat_state_with_cyberspace(
    test_db: &late_core::test_utils::TestDb,
    user_id: Uuid,
) -> (
    ChatState,
    crate::app::chat::cyberspace::svc::CyberspaceService,
) {
    let db = test_db.db.clone();
    let notifications = crate::app::chat::notifications::svc::NotificationService::new(db.clone());
    let chat = crate::app::chat::svc::ChatService::new(db.clone(), notifications.clone());
    let ai = crate::app::ai::svc::AiService::new(false, None);
    let translation = crate::app::ai::translate::TranslationService::new(db.clone(), ai.clone());
    let summary = crate::app::ai::summary::SummaryService::new(db.clone(), ai.clone());
    let articles = crate::app::chat::news::svc::ArticleService::new(db.clone(), ai, chat.clone());
    let (notifier, _outbox) = crate::app::notify::channel();
    // Dead base URL: state logic under test never talks to the network.
    let cyberspace = crate::app::chat::cyberspace::svc::CyberspaceService::new(
        db.clone(),
        "http://127.0.0.1:1".to_string(),
    );
    let state = ChatState::new(
        ChatServices {
            chat,
            translation,
            summary,
            notifications,
            articles,
            feeds: crate::app::chat::feeds::svc::FeedService::new(db.clone()),
            showcases: crate::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
            work: crate::app::chat::work::svc::WorkService::new(db),
            cyberspace: cyberspace.clone(),
        },
        ChatSession {
            user_id,
            username: "internal-test-user".to_string(),
            permissions: crate::authz::Permissions::new(false, false),
            device_left_at: None,
        },
        None,
        notifier,
        crate::app::ai::ladder::MentionLadders::new(),
        None,
    );
    (state, cyberspace)
}

async fn wait_for_snapshot(state: &mut ChatState) {
    crate::test_helpers::wait_until(
        || async { state.snapshot_rx.has_changed().unwrap_or(false) },
        "chat snapshot refresh",
    )
    .await;
}

/// Pump full ChatState ticks until `ready` holds, the way the app tick loop
/// does every frame. Sub-pane events (cyberspace among them) drain inside
/// `tick`, not `drain_events`, so `drain_events_until` cannot serve here.
async fn tick_until(state: &mut ChatState, label: &str, ready: impl Fn(&ChatState) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        state.tick();
        if ready(state) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for condition: {label}");
}

#[tokio::test]
async fn remote_pin_changes_re_derive_the_rail_room_selection() {
    use late_core::models::cyberspace_account::CyberspaceAccount;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "circ_reconcile").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    CyberspaceAccount::set_circ_rooms(&client, user.id, &["alpha".to_string(), "beta".to_string()])
        .await
        .expect("pin rooms");

    let (mut state, cyberspace) = chat_state_with_cyberspace(&test_db, user.id);
    tick_until(&mut state, "pinned rooms load", |state| {
        state.cyberspace.pinned_rooms().len() == 2
    })
    .await;

    state.select_cyberspace_room(1);
    assert_eq!(state.cyberspace.open_circ_slug(), Some("beta"));
    assert_eq!(state.cyberspace_room_selected, Some(1));

    // Another session of the same account pins a room in front: beta moves
    // to index 2, and the rail cursor must follow the room, not the slot.
    cyberspace.set_circ_pinned_task(
        user.id,
        vec!["zeta".to_string(), "alpha".to_string(), "beta".to_string()],
    );
    tick_until(&mut state, "pinned list grows", |state| {
        state.cyberspace.pinned_rooms().len() == 3
    })
    .await;
    assert_eq!(
        state.cyberspace_room_selected,
        Some(2),
        "the selection follows the open room's slug through a reorder"
    );
    assert_eq!(
        state.cyberspace.open_circ_slug(),
        Some("beta"),
        "the open room itself rides out the reorder"
    );

    // Another session unpins the open room: the rail can no longer name it,
    // so the session leaves the room and lands back on the pane.
    cyberspace.set_circ_pinned_task(user.id, vec!["alpha".to_string()]);
    tick_until(&mut state, "pinned list shrinks", |state| {
        state.cyberspace.pinned_rooms().len() == 1
    })
    .await;
    assert_eq!(state.cyberspace_room_selected, None);
    assert_eq!(
        state.cyberspace.open_circ_slug(),
        None,
        "an unpinned room cannot keep its stream and heartbeat"
    );
    assert!(
        state.cyberspace_selected,
        "the user lands on the cyberspace pane, not in limbo"
    );
}

#[tokio::test]
async fn a_room_hop_keeps_the_recorded_return_row() {
    use late_core::models::cyberspace_account::CyberspaceAccount;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "circ_return_row").await;
    CyberspaceAccount::upsert_for_user(&client, user.id, "uid-1", "odd", "refresh-1")
        .await
        .expect("link");
    CyberspaceAccount::set_circ_rooms(&client, user.id, &["alpha".to_string(), "beta".to_string()])
        .await
        .expect("pin rooms");

    let (mut state, _cyberspace) = chat_state_with_cyberspace(&test_db, user.id);
    tick_until(&mut state, "pinned rooms load", |state| {
        state.cyberspace.pinned_rooms().len() == 2
    })
    .await;

    // A mention jump: standing on the notifications row, walk into a room.
    state.select_cyberspace_notifications();
    state.select_cyberspace_room(0);
    // Hop to the second room through the rail. The user never stood on a
    // pane row in between, so the recorded origin must survive the hop.
    state.select_cyberspace_room(1);

    state.select_cyberspace_return_row();
    assert!(
        state.cyberspace_notifications_selected,
        "a room-to-room hop must not reset the recorded return row to the feed"
    );
}

/// Pump the chat event stream until `ready` holds, the way the app tick loop
/// drains events every frame. `wait_until` cannot serve here: its predicate
/// borrows immutably, and draining needs `&mut ChatState`.
async fn drain_events_until(
    state: &mut ChatState,
    label: &str,
    ready: impl Fn(&ChatState) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        state.drain_events();
        if ready(state) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for condition: {label}");
}

/// Regression: `sync_selection` runs on every snapshot apply and used to
/// reset any selection that was not a chat-list room — which bounced a
/// freshly opened stream room (`kind='game'`) straight back to the lounge.
#[tokio::test]
async fn sync_selection_keeps_a_selected_stream_room() {
    use late_core::models::chat_room::ChatRoom;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "stream_viewer").await;
    let streamer = late_core::test_utils::create_test_user(&test_db.db, "stream_owner").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let stream_room = ChatRoom::get_or_create_stream_room(&client, "stream_owner", streamer.id)
        .await
        .expect("stream room");

    let mut state = counter_test_state(&test_db, user.id);
    state.rooms = vec![
        (lounge.clone(), Vec::new()),
        (stream_room.clone(), Vec::new()),
    ];
    state.live_streams = vec![crate::app::stream::registry::LiveStreamView {
        user_id: streamer.id,
        username: "stream_owner".to_string(),
        title: "show".to_string(),
        room_id: stream_room.id,
        voice_channel_id: Uuid::now_v7(),
        stream_id: "stream-id".to_string(),
        live: true,
        watching: 0,
        watch_url: String::new(),
    }];

    // Selected stream room survives a selection sync, member or not.
    state.selected_room_id = Some(stream_room.id);
    state.sync_selection();
    assert_eq!(state.selected_room_id, Some(stream_room.id));
    state.rooms = vec![(lounge.clone(), Vec::new())];
    state.sync_selection();
    assert_eq!(state.selected_room_id, Some(stream_room.id));

    // Once the stream is gone the selection falls back to a list room.
    state.live_streams.clear();
    state.sync_selection();
    assert_eq!(state.selected_room_id, Some(lounge.id));
}

#[tokio::test]
async fn snapshot_and_message_updates_preserve_row_cache_contract() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = late_core::test_utils::create_test_user(&test_db.db, "counter_user").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "counter_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    let other = ChatRoom::get_or_create_public_room(&client, "counter-other")
        .await
        .expect("other room");
    ChatRoomMember::join(&client, lounge.id, user.id)
        .await
        .expect("join user");
    ChatRoomMember::join(&client, other.id, user.id)
        .await
        .expect("join other");
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

    let mut state = counter_test_state(&test_db, user.id);
    wait_for_snapshot(&mut state).await;
    assert!(state.drain_snapshot(), "first snapshot populates state");
    assert!(!state.rooms.is_empty(), "initial snapshot loads rooms");
    let epoch = state.context_epoch();
    let version = state.room_version(lounge.id);
    let other_version = state.room_version(other.id);

    // Snapshots arrive on a fixed cadence whether or not anything changed;
    // an identical reapply must report clean and leave every counter stable,
    // or every session rebuilds its row caches every 10 seconds for nothing.
    state.refresh_tx.send(()).expect("force refresh");
    wait_for_snapshot(&mut state).await;
    assert!(
        !state.drain_snapshot(),
        "identical snapshot reapply reports clean"
    );
    assert_eq!(state.context_epoch(), epoch);
    assert_eq!(state.room_version(lounge.id), version);
    assert_eq!(state.room_version(other.id), other_version);

    // A snapshot carrying a new message must still dirty the frame.
    ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "second".to_string(),
        },
    )
    .await
    .expect("second message");
    state.refresh_tx.send(()).expect("force refresh");
    wait_for_snapshot(&mut state).await;
    assert!(
        state.drain_snapshot(),
        "snapshot with a new message reports changed"
    );
    let lounge_version = state.room_version(lounge.id);
    let other_version = state.room_version(other.id);

    let message = late_core::models::chat_message::ChatMessage {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
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

#[tokio::test]
async fn stale_snapshot_does_not_roll_back_a_newer_ignore_list() {
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = late_core::test_utils::create_test_user(&test_db.db, "stale_ignore_viewer").await;
    let target = late_core::test_utils::create_test_user(&test_db.db, "stale_ignore_target").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, target.id)
        .await
        .expect("join target");

    let mut state = counter_test_state(&test_db, viewer.id);
    wait_for_snapshot(&mut state).await;
    state.drain_snapshot();

    // A snapshot whose read ran before the ignore was written, held back
    // undrained: every live session has one of these in flight, and a slow
    // host delivers it after the ignore lands.
    state.refresh_tx.send(()).expect("force refresh");
    crate::test_helpers::wait_until(
        || async { state.snapshot_rx.has_changed().unwrap_or(false) },
        "pre-ignore chat snapshot",
    )
    .await;

    state
        .service
        .ignore_user_task(viewer.id, target.username.clone());
    let target_id = target.id;
    drain_events_until(&mut state, "ignore list updated", |state| {
        state.ignored_user_ids().contains(&target_id)
    })
    .await;

    // The stale read is older than the write it would overwrite, so it must
    // not un-ignore the target and let their next message through.
    state.drain_snapshot();
    assert!(
        state.ignored_user_ids().contains(&target_id),
        "stale snapshot must not roll back the ignore list"
    );
}

#[test]
fn parse_pair_command_accepts_directed_form() {
    assert_eq!(
        parse_pair_command("/pair @alice"),
        Some(Some(PairRequest::Directed("alice".to_string())))
    );
}

#[test]
fn parse_pair_command_rejects_bare_and_malformed_forms() {
    assert_eq!(parse_pair_command("/pair"), Some(None), "no target");
    assert_eq!(parse_pair_command("/pair @"), Some(None), "empty username");
    assert_eq!(parse_pair_command("/pair alice"), Some(None), "missing @");
    assert_eq!(
        parse_pair_command("/pair @alice extra"),
        Some(None),
        "trailing token"
    );
}

#[test]
fn parse_cyberspace_command_reads_the_subcommands_and_leaves_neighbours_alone() {
    assert_eq!(
        parse_cyberspace_command("/cs"),
        Some(CyberspaceCommand::Open)
    );
    assert_eq!(
        parse_cyberspace_command("/cyberspace"),
        Some(CyberspaceCommand::Open)
    );
    assert_eq!(
        parse_cyberspace_command("  /cs post  "),
        Some(CyberspaceCommand::Post)
    );
    assert_eq!(
        parse_cyberspace_command("/cs link"),
        Some(CyberspaceCommand::Link)
    );
    assert_eq!(
        parse_cyberspace_command("/cs unlink"),
        Some(CyberspaceCommand::Unlink)
    );
    // A typo is still a cyberspace command, so it gets the usage banner
    // rather than being posted to the room as a message.
    assert_eq!(
        parse_cyberspace_command("/cs psot"),
        Some(CyberspaceCommand::Invalid)
    );
    // A longer command that merely starts with the prefix is not ours.
    assert_eq!(parse_cyberspace_command("/csomething"), None);
    assert_eq!(parse_cyberspace_command("/cyberspaces"), None);
    assert_eq!(parse_cyberspace_command("look at /cs"), None);
}

#[test]
fn parse_pair_command_ignores_unrelated_input() {
    assert_eq!(parse_pair_command("/pairing @alice"), None);
    assert_eq!(parse_pair_command("hello /pair @alice"), None);
    assert_eq!(parse_pair_command("/challenge @alice"), None);
}

fn pomodoro_start(minutes: u32, label: &str) -> Option<PomodoroParse> {
    Some(PomodoroParse::Request(PomodoroRequest::Start {
        minutes,
        label: label.to_string(),
    }))
}

#[test]
fn parse_pomodoro_command_defaults_duration_and_label() {
    assert_eq!(
        parse_pomodoro_command("/pomodoro"),
        pomodoro_start(POMODORO_DEFAULT_MINUTES, POMODORO_DEFAULT_LABEL)
    );
    assert_eq!(
        parse_pomodoro_command("  /pomodoro   "),
        pomodoro_start(POMODORO_DEFAULT_MINUTES, POMODORO_DEFAULT_LABEL),
        "surrounding whitespace is not a label"
    );
}

#[test]
fn parse_pomodoro_command_reads_leading_minutes_then_label() {
    assert_eq!(
        parse_pomodoro_command("/pomodoro 50"),
        pomodoro_start(50, POMODORO_DEFAULT_LABEL)
    );
    assert_eq!(
        parse_pomodoro_command("/pomodoro 50 deep   work"),
        pomodoro_start(50, "deep work"),
        "label whitespace collapses"
    );
    // No leading integer means the whole rest is the label, so a plain
    // `/pomodoro <thing>` still starts the default block.
    assert_eq!(
        parse_pomodoro_command("/pomodoro deep work"),
        pomodoro_start(POMODORO_DEFAULT_MINUTES, "deep work")
    );
    assert_eq!(
        parse_pomodoro_command("/pomodoro 5k run"),
        pomodoro_start(POMODORO_DEFAULT_MINUTES, "5k run"),
        "a digit-prefixed word is not a duration"
    );
}

#[test]
fn parse_pomodoro_command_sanitizes_and_caps_the_label() {
    let long = "x".repeat(POMODORO_LABEL_MAX_COLS + 10);
    assert_eq!(
        parse_pomodoro_command(&format!("/pomodoro {long}")),
        pomodoro_start(
            POMODORO_DEFAULT_MINUTES,
            &"x".repeat(POMODORO_LABEL_MAX_COLS)
        )
    );
    // The cap is display cells, so a double-width label stops at half the
    // char count rather than twice the border budget.
    assert_eq!(
        parse_pomodoro_command(&format!("/pomodoro {}", "深".repeat(20))),
        pomodoro_start(
            POMODORO_DEFAULT_MINUTES,
            &"深".repeat(POMODORO_LABEL_MAX_COLS / 2)
        )
    );
    // The label reaches a desktop notification and the top border, so control
    // characters never survive parsing.
    assert_eq!(
        parse_pomodoro_command("/pomodoro focus\u{1b}]777;notify"),
        pomodoro_start(POMODORO_DEFAULT_MINUTES, "focus]777;notify")
    );
}

#[test]
fn parse_pomodoro_command_stops_a_running_timer() {
    assert_eq!(
        parse_pomodoro_command("/pomodoro stop"),
        Some(PomodoroParse::Request(PomodoroRequest::Stop))
    );
    assert_eq!(
        parse_pomodoro_command("/pomodoro STOP"),
        Some(PomodoroParse::Request(PomodoroRequest::Stop))
    );
    assert_eq!(
        parse_pomodoro_command("/pomodoro stop now"),
        Some(PomodoroParse::Invalid),
        "stop takes no arguments"
    );
}

#[test]
fn parse_pomodoro_command_rejects_out_of_range_durations() {
    assert_eq!(
        parse_pomodoro_command("/pomodoro 0"),
        Some(PomodoroParse::Invalid),
        "zero"
    );
    assert_eq!(
        parse_pomodoro_command(&format!("/pomodoro {}", POMODORO_MAX_MINUTES + 1)),
        Some(PomodoroParse::Invalid),
        "over the cap"
    );
    assert_eq!(
        parse_pomodoro_command("/pomodoro 99999999999999999999"),
        Some(PomodoroParse::Invalid),
        "digit run too long for u32"
    );
}

#[test]
fn parse_pomodoro_command_ignores_unrelated_input() {
    assert_eq!(parse_pomodoro_command("/pomodoros"), None);
    assert_eq!(parse_pomodoro_command("hello /pomodoro"), None);
    assert_eq!(parse_pomodoro_command("/poll"), None);
}

#[test]
fn format_cooldown_rounds_minutes_up() {
    assert_eq!(format_cooldown(Duration::from_secs(45)), "45s");
    assert_eq!(format_cooldown(Duration::from_millis(200)), "1s");
    assert_eq!(format_cooldown(Duration::from_secs(60)), "1 min");
    assert_eq!(format_cooldown(Duration::from_secs(90)), "2 min");
    assert_eq!(format_cooldown(Duration::from_secs(300)), "5 min");
}

#[tokio::test]
async fn bot_cooldown_banner_warns_only_for_the_hot_bot_and_room() {
    use crate::app::ai::ladder::LadderBot;

    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "ladder_banner_user").await;
    let state = counter_test_state(&test_db, user.id);
    let room = Uuid::now_v7();
    let other_room = Uuid::now_v7();

    // Nothing answered yet: a mention warns nobody.
    assert!(state.bot_cooldown_banner(room, "@bot hello").is_none());

    // The ghost loop answers once; the ladder is now hot in this room.
    state
        .mention_ladders
        .check_and_step(LadderBot::Bot, user.id, room);

    let banner = state
        .bot_cooldown_banner(room, "hey @bot still there?")
        .expect("hot ladder warns");
    assert!(
        banner.message.contains("@bot is cooling down"),
        "unexpected banner text: {}",
        banner.message
    );

    // A different bot, a plain message, and another room all stay quiet.
    assert!(
        state
            .bot_cooldown_banner(room, "@bartender a pint")
            .is_none()
    );
    assert!(state.bot_cooldown_banner(room, "no bots here").is_none());
    assert!(
        state
            .bot_cooldown_banner(other_room, "@bot hello")
            .is_none()
    );
}

/// Load a room's messages the way entering the room does. Snapshots carry
/// rooms with EMPTY message vectors; the messages arrive on the room-tail
/// event, so any test that needs a concrete message must pull the tail.
async fn load_room_tail(state: &mut ChatState, room_id: Uuid, message_id: Uuid) {
    wait_for_snapshot(state).await;
    state.drain_snapshot();
    state.request_room_tail(room_id);
    drain_events_until(state, "room tail loads the message", |state| {
        state.rooms.iter().any(|(room, messages)| {
            room.id == room_id && messages.iter().any(|m| m.id == message_id)
        })
    })
    .await;
}

/// Pump translation results the way the app tick loop does. Mirrors
/// `drain_events_until`, but for the translation channel.
async fn drain_translations_until(
    state: &mut ChatState,
    label: &str,
    ready: impl Fn(&ChatState) -> bool,
) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        state.drain_translation_events();
        if ready(state) {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    panic!("timed out waiting for condition: {label}");
}

#[tokio::test]
async fn pressing_t_shows_a_translation_then_collapses_and_reopens_it() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;
    use late_core::models::message_translation::{
        CachedTranslation, MessageTranslation, TranslateLang,
    };

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = late_core::test_utils::create_test_user(&test_db.db, "translate_viewer").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "translate_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");

    let foreign = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "你好，我刚发现这个地方".to_string(),
        },
    )
    .await
    .expect("foreign message");
    let english = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "what a cozy little place".to_string(),
        },
    )
    .await
    .expect("english message");
    // Seeded the way another viewer's earlier call would: the cache is what
    // makes a translation free for everyone who comes after the first. The
    // English message got a same-language verdict from that call, also
    // cached, so nobody pays to learn it again.
    MessageTranslation::upsert_if_current(
        &client,
        foreign.id,
        TranslateLang::En,
        "你好，我刚发现这个地方",
        &CachedTranslation::Translated("hello, i just found this place".to_string()),
        false,
    )
    .await
    .expect("seed cache");
    MessageTranslation::upsert_if_current(
        &client,
        english.id,
        TranslateLang::En,
        "what a cozy little place",
        &CachedTranslation::SameLanguage,
        false,
    )
    .await
    .expect("seed same-language cache");

    let mut state = counter_test_state(&test_db, viewer.id);
    load_room_tail(&mut state, lounge.id, foreign.id).await;

    // `t` on a foreign-script message asks for a translation and shows the
    // pending marker until the result lands.
    state.selected_message_id = Some(foreign.id);
    let version_before = state.room_version(lounge.id);
    assert!(
        state
            .toggle_translation_selected_in_room(lounge.id)
            .is_none(),
        "a translatable message banners nothing"
    );
    assert_eq!(
        state.translations.get(&foreign.id),
        Some(&TranslationDisplay::Pending)
    );
    assert!(
        state.room_version(lounge.id) > version_before,
        "the pending marker changes the painted rows, so the cache must rebuild"
    );

    drain_translations_until(&mut state, "cached translation arrives", |state| {
        matches!(
            state.translations.get(&foreign.id),
            Some(TranslationDisplay::Ready(_))
        )
    })
    .await;
    assert_eq!(
        state.translations.get(&foreign.id),
        Some(&TranslationDisplay::Ready(
            "hello, i just found this place".to_string()
        ))
    );
    assert!(!state.translation_hidden.contains(&foreign.id));

    // A second `t` collapses it, a third brings it back, and the text is
    // never re-fetched.
    state.toggle_translation_selected_in_room(lounge.id);
    assert!(state.translation_hidden.contains(&foreign.id));
    state.toggle_translation_selected_in_room(lounge.id);
    assert!(!state.translation_hidden.contains(&foreign.id));
    assert_eq!(
        state.translations.get(&foreign.id),
        Some(&TranslationDisplay::Ready(
            "hello, i just found this place".to_string()
        ))
    );

    // `t` on a message already in the viewer's language: the request goes
    // out (the script check can't clear English for an English target), the
    // cached same-language verdict comes back, nothing renders, and a
    // second `t` explains instead of collapsing a line that isn't there.
    state.selected_message_id = Some(english.id);
    assert!(
        state
            .toggle_translation_selected_in_room(lounge.id)
            .is_none(),
        "the request itself banners nothing"
    );
    drain_translations_until(&mut state, "same-language verdict arrives", |state| {
        matches!(
            state.translations.get(&english.id),
            Some(TranslationDisplay::SameLanguage)
        )
    })
    .await;
    let banner = state
        .toggle_translation_selected_in_room(lounge.id)
        .expect("same-language message banners");
    assert!(
        banner.message.contains("Already written in English"),
        "unexpected banner text: {}",
        banner.message
    );
}

#[tokio::test]
async fn auto_mode_requests_fire_without_a_pending_placeholder() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;
    use late_core::models::message_translation::TranslateLang;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = late_core::test_utils::create_test_user(&test_db.db, "auto_viewer").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "auto_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");
    let seed = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "seed".to_string(),
        },
    )
    .await
    .expect("seed message");

    // Inline harness keeping the service handles: the live auto path only
    // runs for events arriving on the state's own service channel.
    let db = test_db.db.clone();
    let notifications = crate::app::chat::notifications::svc::NotificationService::new(db.clone());
    let chat = crate::app::chat::svc::ChatService::new(db.clone(), notifications.clone());
    let ai = crate::app::ai::svc::AiService::new(false, None);
    let translation = crate::app::ai::translate::TranslationService::new(db.clone(), ai.clone());
    let summary = crate::app::ai::summary::SummaryService::new(db.clone(), ai.clone());
    let mut translation_events = translation.subscribe();
    let articles = crate::app::chat::news::svc::ArticleService::new(db.clone(), ai, chat.clone());
    let (notifier, _outbox) = crate::app::notify::channel();
    let mut state = ChatState::new(
        ChatServices {
            chat: chat.clone(),
            translation,
            summary,
            notifications,
            articles,
            feeds: crate::app::chat::feeds::svc::FeedService::new(db.clone()),
            showcases: crate::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
            work: crate::app::chat::work::svc::WorkService::new(db.clone()),
            cyberspace: crate::app::chat::cyberspace::svc::CyberspaceService::new(
                db,
                "http://127.0.0.1:1".to_string(),
            ),
        },
        ChatSession {
            user_id: viewer.id,
            username: viewer.username.clone(),
            permissions: crate::authz::Permissions::new(false, false),
            device_left_at: None,
        },
        None,
        notifier,
        crate::app::ai::ladder::MentionLadders::new(),
        None,
    );
    load_room_tail(&mut state, lounge.id, seed.id).await;
    state.set_visible_room_id(Some(lounge.id));
    state.set_translate_settings(TranslateLang::En, true);

    chat.send_message_task(
        author.id,
        lounge.id,
        None,
        "bonjour tout le monde".to_string(),
        Uuid::now_v7(),
        false,
    );
    drain_events_until(&mut state, "live message arrives", |state| {
        state.rooms.iter().any(|(room, messages)| {
            room.id == lounge.id && messages.iter().any(|m| m.body.contains("bonjour"))
        })
    })
    .await;
    let message_id = state
        .rooms
        .iter()
        .find(|(room, _)| room.id == lounge.id)
        .and_then(|(_, messages)| messages.iter().find(|m| m.body.contains("bonjour")))
        .map(|m| m.id)
        .expect("live message loaded");

    // The request went out (AI is off, so it resolves Failed)...
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), translation_events.recv())
        .await
        .expect("translation event timeout")
        .expect("translation channel open");
    assert_eq!(event.message_id, message_id);
    // ...but nothing went on screen for it: the "translating…" placeholder
    // is manual-only (`t`), so auto mode never flashes a line under a
    // message that then vanishes on a same-language verdict.
    assert!(
        !state.translations.contains_key(&message_id),
        "auto-fired request must not render a pending placeholder"
    );
}

#[tokio::test]
async fn author_shared_translations_show_without_auto_mode_or_t() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;
    use late_core::models::message_translation::{
        CachedTranslation, MessageTranslation, TranslateLang,
    };

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = late_core::test_utils::create_test_user(&test_db.db, "shared_viewer").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "shared_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");

    // Three cached rows, one per display rule: the author's shared message
    // (shows to everyone), another author message a reader once translated
    // privately (stays private), and the viewer's own shared message (the
    // author never sees their own text echoed back translated).
    let shared = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "bonjour tout le monde".to_string(),
        },
    )
    .await
    .expect("shared message");
    let private = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "salut la compagnie".to_string(),
        },
    )
    .await
    .expect("private message");
    let own = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: viewer.id,
            body: "je vous salue bien".to_string(),
        },
    )
    .await
    .expect("own message");
    MessageTranslation::upsert_if_current(
        &client,
        shared.id,
        TranslateLang::En,
        "bonjour tout le monde",
        &CachedTranslation::Translated("hello everyone".to_string()),
        true,
    )
    .await
    .expect("seed shared row");
    MessageTranslation::upsert_if_current(
        &client,
        private.id,
        TranslateLang::En,
        "salut la compagnie",
        &CachedTranslation::Translated("hi folks".to_string()),
        false,
    )
    .await
    .expect("seed private row");
    MessageTranslation::upsert_if_current(
        &client,
        own.id,
        TranslateLang::En,
        "je vous salue bien",
        &CachedTranslation::Translated("i salute you".to_string()),
        true,
    )
    .await
    .expect("seed own shared row");

    // Inline harness keeping a witness receiver: the test waits until every
    // broadcast event exists before draining, so the whole-map assertion
    // below judges all three rules at once instead of racing the sweep.
    let db = test_db.db.clone();
    let notifications = crate::app::chat::notifications::svc::NotificationService::new(db.clone());
    let chat = crate::app::chat::svc::ChatService::new(db.clone(), notifications.clone());
    let ai = crate::app::ai::svc::AiService::new(false, None);
    let translation = crate::app::ai::translate::TranslationService::new(db.clone(), ai.clone());
    let summary = crate::app::ai::summary::SummaryService::new(db.clone(), ai.clone());
    let mut translation_events = translation.subscribe();
    let articles = crate::app::chat::news::svc::ArticleService::new(db.clone(), ai, chat.clone());
    let (notifier, _outbox) = crate::app::notify::channel();
    let mut state = ChatState::new(
        ChatServices {
            chat,
            translation: translation.clone(),
            summary,
            notifications,
            articles,
            feeds: crate::app::chat::feeds::svc::FeedService::new(db.clone()),
            showcases: crate::app::chat::showcase::svc::ShowcaseService::new(db.clone()),
            work: crate::app::chat::work::svc::WorkService::new(db.clone()),
            cyberspace: crate::app::chat::cyberspace::svc::CyberspaceService::new(
                db,
                "http://127.0.0.1:1".to_string(),
            ),
        },
        ChatSession {
            user_id: viewer.id,
            username: viewer.username.clone(),
            permissions: crate::authz::Permissions::new(false, false),
            device_left_at: None,
        },
        None,
        notifier,
        crate::app::ai::ladder::MentionLadders::new(),
        None,
    );
    load_room_tail(&mut state, lounge.id, own.id).await;

    // No auto mode, no `t`. Making the room visible runs the sweep over the
    // two messages by others; the viewer's own message is swept out, so its
    // event is forced through the service directly to pin the drain's
    // own-message guard too.
    assert!(!state.auto_translate);
    state.set_visible_room_id(Some(lounge.id));
    translation.load_cached(lounge.id, vec![own.id], TranslateLang::En);
    for _ in 0..3 {
        tokio::time::timeout(std::time::Duration::from_secs(5), translation_events.recv())
            .await
            .expect("translation event timeout")
            .expect("translation channel open");
    }

    state.drain_translation_events();
    assert_eq!(
        state.translations,
        std::collections::HashMap::from([(
            shared.id,
            TranslationDisplay::Ready("hello everyone".to_string())
        )]),
        "only the author-shared message by someone else displays"
    );
}

#[tokio::test]
async fn over_cap_foreign_message_banners_too_long_not_already_readable() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = late_core::test_utils::create_test_user(&test_db.db, "toolong_viewer").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "toolong_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");

    // Genuinely foreign script, but past TRANSLATE_MAX_BODY_CHARS (chat
    // bodies go to 2000). "Already readable" would be a lie here.
    let long = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "字".repeat(1_600),
        },
    )
    .await
    .expect("long message");

    let mut state = counter_test_state(&test_db, viewer.id);
    load_room_tail(&mut state, lounge.id, long.id).await;
    state.selected_message_id = Some(long.id);
    let banner = state
        .toggle_translation_selected_in_room(lounge.id)
        .expect("over-cap message banners");
    assert!(
        banner.message.contains("too long"),
        "unexpected banner text: {}",
        banner.message
    );
    assert!(!state.translations.contains_key(&long.id));
}

#[tokio::test]
async fn changing_the_target_language_drops_translations_for_the_old_one() {
    use late_core::models::chat_message::{ChatMessage, ChatMessageParams};
    use late_core::models::chat_room::ChatRoom;
    use late_core::models::chat_room_member::ChatRoomMember;
    use late_core::models::message_translation::{
        CachedTranslation, MessageTranslation, TranslateLang,
    };

    let test_db = crate::test_helpers::new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let viewer = late_core::test_utils::create_test_user(&test_db.db, "retarget_viewer").await;
    let author = late_core::test_utils::create_test_user(&test_db.db, "retarget_author").await;
    let lounge = ChatRoom::ensure_lounge(&client).await.expect("lounge");
    ChatRoomMember::join(&client, lounge.id, viewer.id)
        .await
        .expect("join viewer");
    ChatRoomMember::join(&client, lounge.id, author.id)
        .await
        .expect("join author");
    let message = ChatMessage::create(
        &client,
        ChatMessageParams {
            room_id: lounge.id,
            user_id: author.id,
            body: "你好，我刚发现这个地方".to_string(),
        },
    )
    .await
    .expect("message");
    MessageTranslation::upsert_if_current(
        &client,
        message.id,
        TranslateLang::En,
        "你好，我刚发现这个地方",
        &CachedTranslation::Translated("hello there".to_string()),
        false,
    )
    .await
    .expect("seed cache");

    let mut state = counter_test_state(&test_db, viewer.id);
    load_room_tail(&mut state, lounge.id, message.id).await;
    state.selected_message_id = Some(message.id);
    state.toggle_translation_selected_in_room(lounge.id);
    drain_translations_until(&mut state, "english translation arrives", |state| {
        matches!(
            state.translations.get(&message.id),
            Some(TranslationDisplay::Ready(_))
        )
    })
    .await;

    // Switching target language: everything stored described the old
    // language, so none of it may survive the switch.
    assert!(state.set_translate_settings(TranslateLang::Ko, false));
    assert!(state.translations.is_empty());
    assert!(state.translation_hidden.is_empty());

    // A late English result for the pre-switch request must not paint over
    // the new target's view.
    state.drain_translation_events();
    assert!(state.translations.is_empty());
}

#[test]
fn the_cyberspace_section_carries_the_pane_the_pinned_rooms_and_c_mail() {
    let me = Uuid::from_u128(1);
    let lounge = Uuid::from_u128(10);
    let usernames: HashMap<Uuid, String> = HashMap::new();
    let rooms = vec![make_room(lounge, "lounge", "public", true, Some("lounge"))];
    let pinned = vec!["general".to_string(), "tech".to_string()];
    let mail = vec![CmailThread {
        id: "conv-1".to_string(),
        username: "alice".to_string(),
    }];

    let order_for = |linked: bool, collapsed: &HashSet<RoomSection>| {
        visual_order_for_rooms(RoomVisualOrderInput {
            rooms: &rooms,
            user_id: me,
            usernames: &usernames,
            unread_counts: &HashMap::new(),
            room_last_message_at: &HashMap::new(),
            feeds_available: false,
            cyberspace_linked: linked,
            cyberspace_rooms: &pinned,
            cyberspace_mail: &mail,
            favorite_room_ids: &[],
            collapsed_sections: collapsed,
            ignored_user_ids: &HashSet::new(),
            sticky_unread_dm: None,
            live_streams: &[],
        })
    };

    // Linked: feeds, then notifications, then the pinned rooms, then the
    // pinned conversations, all under one section. Notifications is its own
    // row rather than a view inside the pane, so the rail highlight and the
    // pane can never disagree about which of the two you are reading.
    assert_eq!(
        order_for(true, &HashSet::new()),
        vec![
            RoomSlot::Room(lounge),
            RoomSlot::Notifications,
            RoomSlot::News,
            RoomSlot::Discover,
            RoomSlot::Cyberspace,
            RoomSlot::CyberspaceNotifications,
            RoomSlot::CyberspaceRoom(0),
            RoomSlot::CyberspaceRoom(1),
            RoomSlot::CyberspaceMail(0),
        ]
    );

    // Unlinked: no section at all, however many rooms a stale list holds.
    // A row the rail cannot draw is a slot the user can land on but never see.
    assert_eq!(
        order_for(false, &HashSet::new()),
        vec![
            RoomSlot::Room(lounge),
            RoomSlot::Notifications,
            RoomSlot::News,
            RoomSlot::Discover,
        ]
    );

    // Collapsed: the header stays, its rooms leave navigation with it.
    let collapsed = HashSet::from([RoomSection::Cyberspace]);
    assert_eq!(
        order_for(true, &collapsed),
        vec![
            RoomSlot::Room(lounge),
            RoomSlot::Notifications,
            RoomSlot::News,
            RoomSlot::Discover,
        ]
    );
}

#[test]
fn parse_golive_routes_console_obs_and_stop() {
    assert_eq!(
        parse_golive_command("/golive"),
        Some(GoLiveCommand::Start { title: None })
    );
    assert_eq!(
        parse_golive_command("/golive fixing the render loop"),
        Some(GoLiveCommand::Start {
            title: Some("fixing the render loop".to_string())
        })
    );
    assert_eq!(
        parse_golive_command("/golive stop"),
        Some(GoLiveCommand::Stop)
    );
    assert_eq!(
        parse_golive_command("/golive obs"),
        Some(GoLiveCommand::StartObs { title: None })
    );
    assert_eq!(
        parse_golive_command("/golive obs speedrun night"),
        Some(GoLiveCommand::StartObs {
            title: Some("speedrun night".to_string())
        })
    );
    // Not the command at all: no space boundary after /golive.
    assert_eq!(parse_golive_command("/golivenow"), None);
    assert_eq!(parse_golive_command("hello"), None);
}

#[test]
fn parse_golive_clamps_titles_at_the_boundary() {
    let long = "x".repeat(GOLIVE_TITLE_MAX_CHARS + 20);
    match parse_golive_command(&format!("/golive obs {long}")) {
        Some(GoLiveCommand::StartObs { title: Some(title) }) => {
            assert_eq!(title.chars().count(), GOLIVE_TITLE_MAX_CHARS);
        }
        other => panic!("expected clamped obs title, got {other:?}"),
    }
}

#[test]
fn parse_summary_arg_names_every_outcome() {
    use crate::app::ai::summary::SUMMARY_MAX_WINDOW_HOURS;

    // Bare `/summary` (the argument is whatever followed the command, so a
    // trailing space is the same thing).
    assert_eq!(parse_summary_arg(""), SummaryArg::CatchUp);
    assert_eq!(parse_summary_arg("   "), SummaryArg::CatchUp);

    // Both units, and the boundary that is still allowed.
    assert_eq!(
        parse_summary_arg(" 6h"),
        SummaryArg::Window(chrono::Duration::hours(6))
    );
    assert_eq!(
        parse_summary_arg(" 90m"),
        SummaryArg::Window(chrono::Duration::minutes(90))
    );
    assert_eq!(
        parse_summary_arg(" 6H"),
        SummaryArg::Window(chrono::Duration::hours(6))
    );
    assert_eq!(
        parse_summary_arg(&format!(" {SUMMARY_MAX_WINDOW_HOURS}h")),
        SummaryArg::Window(chrono::Duration::hours(SUMMARY_MAX_WINDOW_HOURS))
    );

    // Past the max: refused, never quietly clamped, so a summary is never
    // narrower than the window it was asked for.
    assert_eq!(
        parse_summary_arg(&format!(" {}h", SUMMARY_MAX_WINDOW_HOURS + 1)),
        SummaryArg::TooLong
    );
    assert_eq!(parse_summary_arg(" 4000m"), SummaryArg::TooLong);
    // Big enough to overflow a naive hours-to-minutes multiply.
    assert_eq!(parse_summary_arg(" 4000000000h"), SummaryArg::TooLong);

    // Empty windows.
    assert_eq!(parse_summary_arg(" 0h"), SummaryArg::TooShort);
    assert_eq!(parse_summary_arg(" 0m"), SummaryArg::TooShort);

    // Junk, and the near-misses that must not be guessed at: a unitless
    // number, a negative, a decimal, and a wrong unit.
    for junk in [
        " 6",
        " -6h",
        " 1.5h",
        " 6d",
        " h",
        " six hours",
        " 6 h",
        " 6hh",
    ] {
        assert_eq!(parse_summary_arg(junk), SummaryArg::Unparseable, "{junk:?}");
    }
}

/// Minimal room for the `/summary` command gate; the branch reads only
/// `visibility` (and `id` for the request).
fn summary_room(visibility: &str) -> ChatRoom {
    ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "topic".to_string(),
        visibility: visibility.to_string(),
        auto_join: false,
        permanent: false,
        slug: None,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        topic: None,
        rules: None,
        created_by: None,
    }
}

/// The AFK line marks a discontinuity in attention, so what it must get
/// right is *where* the silence started, and that it stays put once placed.
#[tokio::test]
async fn the_afk_line_lands_where_the_silence_started_and_stays_there() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "afk_place").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let room = summary_room("public");
    let room_id = room.id;
    state.visible_room_id = Some(room_id);
    state.rooms.push((room, Vec::new()));

    // Under the threshold nothing happens: a pause is not an absence.
    assert!(!state.sync_afk_line(super::AFK_LINE_IDLE - Duration::from_secs(1)));
    assert_eq!(state.afk_lines.get(&room_id), None);

    // Over it, the line goes where the keyboard went quiet, not where the
    // session noticed. Those are the same instant only by accident.
    let before = Utc::now();
    assert!(state.sync_afk_line(super::AFK_LINE_IDLE));
    let placed = *state.afk_lines.get(&room_id).expect("line placed");
    let expected = before - chrono::Duration::from_std(super::AFK_LINE_IDLE).unwrap();
    assert!(
        (placed - expected).num_seconds().abs() <= 1,
        "line at {placed}, expected about {expected}"
    );

    // Staying away longer does not drag the line forward: it says when you
    // left, and four hours later you still left when you left.
    assert!(!state.sync_afk_line(Duration::from_secs(4 * 60 * 60)));
    assert_eq!(*state.afk_lines.get(&room_id).expect("line kept"), placed);
}

/// The rule in one sentence: from when you went quiet until you speak again.
/// Speaking is the clear, and only your own voice counts.
#[tokio::test]
async fn speaking_in_the_room_clears_its_line_but_being_spoken_to_does_not() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "afk_speak").await;
    let other = late_core::test_utils::create_test_user(&test_db.db, "afk_other").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let room = summary_room("public");
    let room_id = room.id;
    state.visible_room_id = Some(room_id);
    state.rooms.push((room, Vec::new()));
    assert!(state.sync_afk_line(super::AFK_LINE_IDLE));

    let message = |author: Uuid, body: &str| late_core::models::chat_message::ChatMessage {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id,
        user_id: author,
        body: body.to_string(),
    };

    // The backlog piling up under the line is the line doing its job.
    state.push_message(message(other.id, "while you were out"));
    assert!(state.afk_lines.contains_key(&room_id));

    // Your own message ends the silence the line was marking.
    state.push_message(message(user.id, "back"));
    assert_eq!(state.afk_lines.get(&room_id), None);
}

/// A room you are not looking at was never being attended, so it collects
/// nothing; its rail badge already says what is waiting there.
#[tokio::test]
async fn only_the_room_on_screen_collects_a_line() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "afk_scope").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let watched = summary_room("public");
    let watched_id = watched.id;
    let background = summary_room("public");
    let background_id = background.id;
    state.visible_room_id = Some(watched_id);
    state.rooms.push((watched, Vec::new()));
    state.rooms.push((background, Vec::new()));

    assert!(state.sync_afk_line(super::AFK_LINE_IDLE));

    assert!(state.afk_lines.contains_key(&watched_id));
    assert_eq!(state.afk_lines.get(&background_id), None);
}

/// Both catch-up surfaces read the line and neither spends it. `/history`
/// is how you go and look, and the anchor has to survive the looking;
/// `/summary` tells you what is below the line, it does not move where the
/// line is. Only speaking does that.
#[tokio::test]
async fn neither_a_summary_nor_history_spends_the_line() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "afk_catchup").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let room = summary_room("public");
    let room_id = room.id;
    state.visible_room_id = Some(room_id);
    state.selected_room_id = Some(room_id);
    state.rooms.push((room, Vec::new()));
    assert!(state.sync_afk_line(super::AFK_LINE_IDLE));
    let placed = *state.afk_lines.get(&room_id).expect("line placed");

    state.composer.insert_str("/history");
    state.submit_composer(false, false);
    assert_eq!(state.afk_lines.get(&room_id), Some(&placed));

    state.composer.insert_str("/summary");
    let banner = state.submit_composer(false, false).expect("banner");
    assert_eq!(banner.message, "Summarizing…");
    assert_eq!(state.afk_lines.get(&room_id), Some(&placed));
}

/// The two marks answer different questions and never feed each other: a
/// bare `/summary` reads from when you last left the app on this device,
/// whatever the room's AFK line says, and a device with no mark gets the
/// default rather than the line.
#[test]
fn a_bare_summary_reads_the_device_mark_and_never_the_afk_line() {
    let left_at = Utc::now() - chrono::Duration::hours(9);
    assert_eq!(
        super::catch_up_window(Some(left_at)),
        SummaryWindow::SinceLeftApp(left_at)
    );
    assert_eq!(super::catch_up_window(None), SummaryWindow::Default);
}

#[tokio::test]
async fn summary_command_refuses_non_public_rooms() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "sum_cmd_priv").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let room = summary_room("private");
    state.visible_room_id = Some(room.id);
    state.rooms.push((room, Vec::new()));

    state.composer.insert_str("/summary");
    let banner = state.submit_composer(false, false).expect("banner");

    assert_eq!(banner.message, "Summaries cover public rooms only");
}

#[tokio::test]
async fn summary_command_requests_the_visible_public_room() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "sum_cmd_pub").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let room = summary_room("public");
    let room_id = room.id;
    state.visible_room_id = Some(room_id);
    state.rooms.push((room, Vec::new()));
    let mut events = state.summary_service.subscribe();

    state.composer.insert_str("/summary");
    let banner = state.submit_composer(false, false).expect("banner");
    assert_eq!(banner.message, "Summarizing…");

    // AI is disabled in this wiring, so the issued request answers
    // Unavailable; its arrival is the proof a request went out for this
    // user and room.
    let event = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
        .await
        .expect("event within timeout")
        .expect("channel open");
    assert_eq!(event.user_id, user.id);
    assert_eq!(event.room_id, room_id);
    assert!(matches!(event.outcome, SummaryOutcome::Unavailable));
}

#[tokio::test]
async fn summary_command_refuses_a_malformed_window_without_requesting() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "sum_cmd_bad").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let room = summary_room("public");
    state.visible_room_id = Some(room.id);
    state.rooms.push((room, Vec::new()));
    let mut events = state.summary_service.subscribe();

    state.composer.insert_str("/summary 6");
    let banner = state.submit_composer(false, false).expect("banner");

    // The banner teaches the format, and nothing was spent: a typo must not
    // fall back to the default window and answer the wrong question.
    assert_eq!(banner.message, "Use /summary, or a window like /summary 6h");
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn a_ready_summary_waits_for_an_open_overlay_instead_of_clobbering_it() {
    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "sum_overlay").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    state.overlay = Some(Overlay::new("Rules", vec!["be kind".to_string()]));

    state.summary_service.emit_for_test(SummaryEvent {
        user_id: user.id,
        room_id: Uuid::now_v7(),
        room_label: "#lounge".to_string(),
        outcome: SummaryOutcome::Ready {
            text: "- alice shipped the thing".to_string(),
            message_count: 3,
            since: Utc::now() - chrono::Duration::hours(2),
            basis: SummaryBasis::Explicit,
            capped: false,
            truncated: false,
        },
    });
    let tick = state.tick();

    // The overlay the user is reading stays; a banner says the summary
    // waits for the surface.
    assert_eq!(
        state.overlay.as_ref().map(|o| o.title.as_str()),
        Some("Rules")
    );
    assert_eq!(
        tick.banner.expect("banner").message,
        "Summary ready, close the open panel to view"
    );

    // Closing it hands the surface to the waiting summary, and the tick
    // reports the change so the frame redraws.
    state.overlay = None;
    let tick = state.tick();
    assert!(tick.changed);
    assert_eq!(
        state.overlay.as_ref().map(|o| o.title.as_str()),
        Some("#lounge catch-up")
    );
}

/// The catch-up head is the one absolute time in the overlay, so it is
/// written on the reader's clock when the account has a zone.
#[tokio::test]
async fn a_ready_summary_dates_its_window_in_the_viewers_timezone() {
    use chrono::TimeZone;

    let test_db = crate::test_helpers::new_test_db().await;
    let user = late_core::test_utils::create_test_user(&test_db.db, "sum_tz").await;
    let mut state = chat_state_with_cyberspace(&test_db, user.id).0;
    let since = Utc
        .with_ymd_and_hms(2026, 8, 28, 14, 30, 0)
        .single()
        .unwrap();
    let emit = |state: &mut ChatState| {
        state.summary_service.emit_for_test(SummaryEvent {
            user_id: user.id,
            room_id: Uuid::now_v7(),
            room_label: "#lounge".to_string(),
            outcome: SummaryOutcome::Ready {
                text: "- alice shipped the thing".to_string(),
                message_count: 3,
                since,
                basis: SummaryBasis::Explicit,
                capped: false,
                truncated: false,
            },
        });
        state.tick();
    };

    state.set_viewer_tz(Some(chrono_tz::Europe::Warsaw));
    emit(&mut state);
    assert_eq!(
        state.overlay.as_ref().expect("overlay").lines[0],
        "3 messages since Aug 28 16:30 CEST"
    );

    // No account zone: the window stays UTC, and says so.
    state.overlay = None;
    state.set_viewer_tz(None);
    emit(&mut state);
    assert_eq!(
        state.overlay.as_ref().expect("overlay").lines[0],
        "3 messages since Aug 28 14:30 UTC"
    );
}

#[test]
fn selection_scroll_steps_within_measured_overflow_and_reports_edges() {
    let scroll = SelectionScroll::default();
    // No measurement yet (or a selection that fits): every step falls
    // through to selection movement.
    assert!(!scroll.step(1));
    assert!(!scroll.step(-1));

    scroll.overflow.set(3);
    assert!(scroll.step(1));
    assert!(scroll.step(1));
    assert!(scroll.step(1));
    assert_eq!(scroll.rows.get(), 3);
    // The bottom edge is reached: the next step falls through.
    assert!(!scroll.step(1));

    assert!(scroll.step(-1));
    assert_eq!(scroll.rows.get(), 2);

    scroll.reset();
    assert_eq!(scroll.rows.get(), 0);
    assert!(!scroll.step(1));
}
