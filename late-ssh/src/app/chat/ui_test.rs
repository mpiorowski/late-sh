use super::*;
use chrono::Utc;
use late_core::models::chat_room::ChatRoom;
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

#[test]
fn short_user_id_returns_first_eight_chars() {
    let id = Uuid::parse_str("01234567-89ab-cdef-0123-456789abcdef").unwrap();
    assert_eq!(short_user_id(id), "01234567");
}

#[test]
fn short_user_id_handles_nil() {
    assert_eq!(short_user_id(Uuid::nil()), "00000000");
}

#[test]
fn is_bot_author_matches_all_ghost_users() {
    assert!(is_bot_author("bot"));
    assert!(is_bot_author("graybeard"));
    assert!(is_bot_author("bartender"));
    assert!(is_bot_author(" Bartender "));
    assert!(!is_bot_author("mat"));
}

#[test]
fn poll_row_widths_uses_full_label_when_space_allows() {
    let (label_width, bar_width) = poll_row_widths(64, 7, 100);

    assert_eq!(label_width, 64);
    assert_eq!(bar_width, 21);
}

#[test]
fn poll_row_widths_keeps_long_labels_beyond_old_cap() {
    let (label_width, bar_width) = poll_row_widths(80, 7, 100);

    assert_eq!(label_width, 80);
    assert_eq!(bar_width, 5);
}

#[test]
fn poll_row_widths_shrinks_labels_only_when_row_is_full() {
    let (label_width, bar_width) = poll_row_widths(80, 7, 60);

    assert_eq!(label_width, 44);
    assert_eq!(bar_width, 1);
}

#[test]
fn author_badge_suffix_keeps_badges_compact() {
    assert_eq!(
        format_author_badge_suffix(&["mod", "dev"], None, None),
        " mod dev"
    );
    assert_eq!(
        format_author_badge_suffix(&["mod"], Some("🐱"), Some("bonsai")),
        " mod bonsai 🐱"
    );
    assert_eq!(format_author_badge_suffix(&[], Some("🐱"), None), " 🐱");
    assert_eq!(
        format_author_badge_suffix(&[], None, Some("bonsai")),
        " bonsai"
    );
    assert_eq!(format_author_badge_suffix(&[], None, None), "");
}

#[test]
fn chat_composer_layout_keeps_one_blank_row_gap() {
    let area = Rect::new(0, 0, 80, 20);
    let (messages_area, composer_area) = split_chat_and_composer(area, 3);

    assert_eq!(
        composer_area.y,
        messages_area.y + messages_area.height + CHAT_COMPOSER_GAP_HEIGHT
    );
}

#[test]
fn effective_chat_scroll_keeps_selected_message_off_top_edge() {
    let scroll = effective_chat_scroll(40, 10, Some((24, 25)));
    assert_eq!(scroll, 8);
}

#[test]
fn effective_chat_scroll_keeps_selected_message_off_bottom_edge() {
    let scroll = effective_chat_scroll(40, 10, Some((29, 31)));
    assert_eq!(scroll, 3);
}

#[test]
fn chat_rows_cache_key_changes_when_theme_changes() {
    let user_id = Uuid::from_u128(2);
    let usernames = HashMap::from([(user_id, "alice".to_string())]);
    let countries = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let friend_user_ids = HashSet::new();
    let afk_user_ids = HashSet::new();
    let message_reactions = HashMap::new();
    let inline_images = HashMap::new();
    let profile_award_badges = HashMap::new();
    let drunk_levels = HashMap::new();
    let name_styles = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);

    let ctx = ChatRowsContext {
        versions: ChatRowsVersions::default(),
        current_user_id: user_id,
        afk_user_ids: &afk_user_ids,
        show_flag_fallback: false,
        usernames: &username_lookup,
        countries: &countries,
        friend_user_ids: &friend_user_ids,
        bonsai_glyphs: &bonsai_glyphs,
        chat_badges: &chat_badges,
        profile_award_badges: &profile_award_badges,
        message_reactions: &message_reactions,
        inline_images: &inline_images,
        unread_marker: None,
        drunk_levels: &drunk_levels,
        name_styles: &name_styles,
    };

    theme::set_current_by_id("late");
    let late_key = chat_rows_cache_key(&ctx, 80);
    theme::set_current_by_id("contrast");
    let contrast_key = chat_rows_cache_key(&ctx, 80);

    assert_ne!(late_key, contrast_key);
}

#[test]
fn chat_rows_cache_key_changes_with_any_version_counter() {
    // The counters are the whole invalidation contract now: a bump to the
    // room version or either context epoch, or a different rendered room,
    // must produce a different cache key so the rows rebuild.
    let user_id = Uuid::from_u128(2);
    let usernames = HashMap::from([(user_id, "alice".to_string())]);
    let countries = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let friend_user_ids = HashSet::new();
    let afk_user_ids = HashSet::new();
    let message_reactions = HashMap::new();
    let inline_images = HashMap::new();
    let profile_award_badges = HashMap::new();
    let drunk_levels = HashMap::new();
    let name_styles = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);

    let base_versions = ChatRowsVersions {
        room_id: Some(Uuid::from_u128(1)),
        room_version: 1,
        chat_ctx_epoch: 1,
        app_ctx_epoch: 1,
    };
    let ctx = |versions| ChatRowsContext {
        versions,
        current_user_id: user_id,
        afk_user_ids: &afk_user_ids,
        show_flag_fallback: false,
        usernames: &username_lookup,
        countries: &countries,
        friend_user_ids: &friend_user_ids,
        bonsai_glyphs: &bonsai_glyphs,
        chat_badges: &chat_badges,
        profile_award_badges: &profile_award_badges,
        message_reactions: &message_reactions,
        inline_images: &inline_images,
        unread_marker: None,
        drunk_levels: &drunk_levels,
        name_styles: &name_styles,
    };

    let base_key = chat_rows_cache_key(&ctx(base_versions), 80);
    let variants = [
        ChatRowsVersions {
            room_id: Some(Uuid::from_u128(9)),
            ..base_versions
        },
        ChatRowsVersions {
            room_version: 2,
            ..base_versions
        },
        ChatRowsVersions {
            chat_ctx_epoch: 2,
            ..base_versions
        },
        ChatRowsVersions {
            app_ctx_epoch: 2,
            ..base_versions
        },
    ];
    for versions in variants {
        assert_ne!(base_key, chat_rows_cache_key(&ctx(versions), 80));
    }
    assert_ne!(base_key, chat_rows_cache_key(&ctx(base_versions), 40));
}

#[test]
fn unread_boundary_ignores_read_and_own_messages() {
    let room_id = Uuid::from_u128(1);
    let current_user_id = Uuid::from_u128(2);
    let other_user_id = Uuid::from_u128(3);
    let marker = Utc::now();
    let make_message = |user_id, created| ChatMessage {
        id: Uuid::now_v7(),
        created,
        updated: created,
        pinned: false,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id,
        user_id,
        body: "hello".to_string(),
    };

    assert!(is_unread_boundary_message(
        Some(marker),
        &make_message(other_user_id, marker + chrono::Duration::seconds(1)),
        current_user_id
    ));
    assert!(!is_unread_boundary_message(
        Some(marker),
        &make_message(current_user_id, marker + chrono::Duration::seconds(1)),
        current_user_id
    ));
    assert!(!is_unread_boundary_message(
        Some(marker),
        &make_message(other_user_id, marker - chrono::Duration::seconds(1)),
        current_user_id
    ));
    assert!(!is_unread_boundary_message(
        None,
        &make_message(other_user_id, marker + chrono::Duration::seconds(1)),
        current_user_id
    ));
}

fn composer_view<'a>(textarea: &'a TextArea<'static>) -> ComposerBlockView<'a> {
    ComposerBlockView {
        composer: textarea,
        composing: true,
        selected_message: false,
        selected_image_message: false,
        selected_news_message: false,
        reaction_picker_active: false,
        reply_author: None,
        is_editing: false,
        mention_active: false,
        mention_matches: &[],
        mention_selected: 0,
        keep_composer_focused: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn chat_view<'a>(
    rows_cache: &'a mut ChatRowsCache,
    rooms: &'a [(ChatRoom, Vec<ChatMessage>)],
    selected_room_id: Option<Uuid>,
    usernames: &'a UsernameLookup<'a>,
    countries: &'a HashMap<Uuid, String>,
    message_reactions: &'a HashMap<Uuid, Vec<ChatMessageReactionSummary>>,
    unread_counts: &'a HashMap<Uuid, i64>,
    bonsai_glyphs: &'a HashMap<Uuid, String>,
    chat_badges: &'a HashMap<Uuid, String>,
    profile_award_badges: &'a HashMap<Uuid, String>,
    composer: &'a TextArea<'static>,
    news_composer: &'a TextArea<'static>,
) -> ChatRenderInput<'a> {
    static INLINE_IMAGES: OnceLock<HashMap<Uuid, InlineImagePreview>> = OnceLock::new();
    static FRIEND_USER_IDS: OnceLock<HashSet<Uuid>> = OnceLock::new();
    static AFK_USER_IDS: OnceLock<HashSet<Uuid>> = OnceLock::new();
    static IGNORED_USER_IDS: OnceLock<HashSet<Uuid>> = OnceLock::new();
    static VOICE_SNAPSHOT: OnceLock<crate::app::voice::svc::VoiceSnapshot> = OnceLock::new();
    static VOICE_CHANNELS: OnceLock<HashMap<Uuid, late_core::models::voice_channel::VoiceChannel>> =
        OnceLock::new();
    static COLLAPSED_SECTIONS: OnceLock<HashSet<RoomSection>> = OnceLock::new();
    static ACTIVE_ROOM_EFFECTS: OnceLock<HashMap<Uuid, Vec<ActiveChatRoomEffect>>> =
        OnceLock::new();
    static ROOM_LAST_MESSAGE_AT: OnceLock<HashMap<Uuid, Option<DateTime<Utc>>>> = OnceLock::new();
    static ROOM_UNREAD_MARKERS: OnceLock<HashMap<Uuid, Option<DateTime<Utc>>>> = OnceLock::new();
    static DRUNK_LEVELS: OnceLock<HashMap<Uuid, u8>> = OnceLock::new();
    static NAME_STYLES: OnceLock<HashMap<Uuid, NameStyle>> = OnceLock::new();
    static ROOM_VERSIONS: OnceLock<HashMap<Uuid, u64>> = OnceLock::new();

    ChatRenderInput {
        pet_strip: None,
        activity_ticker: &[],
        feeds_selected: false,
        feeds_processing: false,
        feeds_unread_count: 0,
        feeds_view: crate::app::chat::feeds::ui::FeedListView {
            entries: &[],
            selected_index: 0,
            has_feeds: false,
            marker_read_at: None,
        },
        news_selected: false,
        news_unread_count: 0,
        news_view: crate::app::chat::news::ui::ArticleListView {
            articles: &[],
            selected_index: 0,
            marker_read_at: None,
            mine_only: false,
        },
        discover_selected: false,
        discover_view: crate::app::chat::discover::ui::DiscoverListView {
            items: Vec::new(),
            selected_index: 0,
            loading: false,
            filtering: false,
            query: "",
        },
        rows_cache,
        room_versions: ROOM_VERSIONS.get_or_init(HashMap::new),
        chat_ctx_epoch: 0,
        app_ctx_epoch: 0,
        chat_rooms: rooms,
        overlay: None,
        image_modal: None,
        usernames,
        countries,
        friend_user_ids: FRIEND_USER_IDS.get_or_init(HashSet::new),
        message_reactions,
        inline_images: INLINE_IMAGES.get_or_init(HashMap::new),
        room_unread_markers: ROOM_UNREAD_MARKERS.get_or_init(HashMap::new),
        unread_counts,
        room_last_message_at: ROOM_LAST_MESSAGE_AT.get_or_init(HashMap::new),
        favorite_room_ids: &[],
        active_room_effects: ACTIVE_ROOM_EFFECTS.get_or_init(HashMap::new),
        active_poll: None,
        collapsed_sections: COLLAPSED_SECTIONS.get_or_init(HashSet::new),
        selected_room_id,
        room_jump_active: false,
        room_section_prefix_armed: false,
        selected_message_id: None,
        selected_image_message: false,
        selected_news_message: false,
        reaction_picker_active: false,
        highlighted_message_id: None,
        composer,
        composing: false,
        current_user_id: Uuid::nil(),
        afk_user_ids: AFK_USER_IDS.get_or_init(HashSet::new),
        ignored_user_ids: IGNORED_USER_IDS.get_or_init(HashSet::new),
        show_flag_fallback: false,
        cursor_visible: false,
        mention_matches: &[],
        mention_selected: 0,
        mention_active: false,
        reply_author: None,
        is_editing: false,
        bonsai_glyphs,
        chat_badges,
        profile_award_badges,
        drunk_levels: DRUNK_LEVELS.get_or_init(HashMap::new),
        name_styles: NAME_STYLES.get_or_init(HashMap::new),
        news_composer,
        news_composing: false,
        news_processing: false,
        notifications_selected: false,
        notifications_unread_count: 0,
        notifications_view: crate::app::chat::notifications::ui::NotificationListView {
            items: &[],
            selected_index: 0,
            marker_read_at: None,
        },
        voice_channels_by_room_id: VOICE_CHANNELS.get_or_init(HashMap::new),
        voice_snapshot: VOICE_SNAPSHOT.get_or_init(Default::default),
        voice_paired_cli_supports_voice: false,
        showcase_selected: false,
        showcase_unread_count: 0,
        showcase_view: crate::app::chat::showcase::ui::ShowcaseListView {
            items: &[],
            selected_index: 0,
            current_user_id: Uuid::nil(),
            is_admin: false,
            marker_read_at: None,
            mine_only: false,
        },
        showcase_state: None,
        showcase_composing: false,
        work_selected: false,
        work_unread_count: 0,
        work_view: crate::app::chat::work::ui::WorkListView {
            items: &[],
            selected_index: 0,
            current_user_id: Uuid::nil(),
            is_admin: false,
            marker_read_at: None,
            profile_base_url: "http://localhost:3000",
            mine_only: false,
        },
        work_state: None,
        work_composing: false,
        keep_composer_focused: false,
        composer_rect_slot: None,
        composer_viewport_top_slot: None,
        chat_hit_slot: None,
    }
}

#[test]
fn pick_title_that_fits_selects_longest_tier_that_fits() {
    let tiers = ["aaaaaa", "bbbb", "cc", ""];
    // block_width = N, available for title = N - 2.
    assert_eq!(pick_title_that_fits(8, &tiers), "aaaaaa");
    assert_eq!(pick_title_that_fits(7, &tiers), "bbbb");
    assert_eq!(pick_title_that_fits(5, &tiers), "cc");
    assert_eq!(pick_title_that_fits(3, &tiers), "");
}

#[test]
fn pick_title_that_fits_uses_display_width_not_byte_length() {
    // ⏎ is 3 bytes but 1 display column.
    let tiers = ["⏎⏎⏎⏎", ""];
    assert_eq!(pick_title_that_fits(6, &tiers), "⏎⏎⏎⏎");
}

#[test]
fn composer_viewport_top_scrolls_to_keep_cursor_visible() {
    assert_eq!(next_composer_viewport_top(Some(0), 0, 4), 0);
    assert_eq!(next_composer_viewport_top(Some(0), 3, 4), 0);
    assert_eq!(next_composer_viewport_top(Some(0), 4, 4), 1);
    assert_eq!(next_composer_viewport_top(Some(6), 4, 4), 4);
}

#[test]
fn composer_viewport_top_treats_zero_height_as_one_row() {
    assert_eq!(next_composer_viewport_top(Some(0), 2, 0), 2);
}

#[test]
fn composer_viewport_top_keeps_prev_top_when_cursor_moves_up_within_view() {
    // Regression: a 10-row draft in a 5-row viewport scrolls to top 5
    // while typing. Pressing Up to row 6 must keep top 5, like the
    // widget's own viewport, not re-anchor to 6 - 5 + 1 = 2. This relies
    // on the slot persisting across frames so prev_top is real.
    assert_eq!(next_composer_viewport_top(Some(5), 9, 5), 5);
    assert_eq!(next_composer_viewport_top(Some(5), 6, 5), 5);
    // Only a fresh slot (first ever render) bottom-anchors at the cursor.
    assert_eq!(next_composer_viewport_top(None, 6, 5), 2);
}

#[test]
fn composer_title_collapses_across_block_widths() {
    let ta = TextArea::default();
    let view = composer_view(&ta);
    let full = " Compose (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ";
    let long = " (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) ";
    let short = " (⏎ send, Alt+S stay, Alt+⏎/Ctrl+J newline, Esc cancel) ";
    let compact = " Compose (Enter send, Esc cancel) ";
    let minimal = " (⏎ send, Esc cancel) ";
    let cancel = " (Esc cancel) ";
    let esc = " Esc ";
    let need = |title: &str| (UnicodeWidthStr::width(title) + 2) as u16;
    let titled = |title: &str| format!("──{title}");

    assert_eq!(composer_title(&view, need(full)), titled(full));
    assert_eq!(composer_title(&view, need(full) - 1), titled(long));

    assert_eq!(composer_title(&view, need(long)), titled(long));
    assert_eq!(composer_title(&view, need(long) - 1), titled(short));

    assert_eq!(composer_title(&view, need(short)), titled(short));
    assert_eq!(composer_title(&view, need(short) - 1), titled(compact));

    assert_eq!(composer_title(&view, need(compact)), titled(compact));
    assert_eq!(composer_title(&view, need(compact) - 1), titled(minimal));

    assert_eq!(composer_title(&view, need(minimal)), titled(minimal));
    assert_eq!(composer_title(&view, need(minimal) - 1), titled(cancel));

    assert_eq!(composer_title(&view, need(cancel)), titled(cancel));
    assert_eq!(composer_title(&view, need(cancel) - 1), titled(esc));

    assert_eq!(composer_title(&view, need(esc)), titled(esc));
    assert_eq!(composer_title(&view, need(esc) - 1), "");
}

#[test]
fn composer_title_with_keep_composer_focused_drops_alt_s_copy() {
    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.keep_composer_focused = true;
    let full = composer_title(&view, 100);
    assert!(
        full.contains("send & stay"),
        "expected 'send & stay' copy, got {full:?}"
    );
    assert!(
        !full.contains("Alt+S"),
        "expected Alt+S to be removed, got {full:?}"
    );

    view.reply_author = Some("alice");
    let reply = composer_title(&view, 100);
    assert!(
        reply.contains("send & stay"),
        "expected reply copy to mention 'send & stay', got {reply:?}"
    );
    assert!(!reply.contains("Alt+S"));

    view.reply_author = None;
    view.is_editing = true;
    let edit = composer_title(&view, 100);
    assert!(
        edit.contains("save & stay"),
        "expected edit copy to mention 'save & stay', got {edit:?}"
    );
    assert!(!edit.contains("Alt+S"));
}

#[test]
fn visible_rows_paint_background_for_selected_highlighted_message() {
    let message_id = Uuid::now_v7();
    let mut cache = ChatRowsCache {
        all_rows: vec![
            Line::from(Span::raw("alice")),
            Line::from(Span::raw("hello")),
        ],
        ..Default::default()
    };
    cache.selected_ranges.insert(message_id, (1, 2));
    cache.highlighted_ranges.insert(message_id, (0, 2));

    let visible = visible_chat_rows(&cache, Some(message_id), Some(message_id), 4);
    assert_eq!(
        visible.lines.len(),
        visible.hits.len(),
        "visible_chat_rows must return lines and hits of identical length"
    );
    assert!(
        visible
            .lines
            .iter()
            .flat_map(|row| row.spans.iter())
            .any(|span| span.style.bg == Some(theme::BG_SELECTION())),
        "expected selected highlighted message to receive background"
    );
}

#[test]
fn composer_title_reply_state_degrades_through_name_only_and_label() {
    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.reply_author = Some("alice");
    assert_eq!(
        composer_title(&view, 100),
        "── Reply to @alice (Enter send, Alt+S stay, Alt+Enter/Ctrl+J newline, Esc cancel) "
    );
    // Far too narrow for even the shortest reply form → drops to " Reply ".
    // " Reply " = 7 cols → needs block_w ≥ 9.
    assert_eq!(composer_title(&view, 10), "── Reply ");
    assert_eq!(composer_title(&view, 9), "── Reply ");
    // " Esc " = 5 cols → needs block_w ≥ 7.
    assert_eq!(composer_title(&view, 8), "── Esc ");
    assert_eq!(composer_title(&view, 7), "── Esc ");
    assert_eq!(composer_title(&view, 6), "");
}

#[test]
fn composer_title_when_not_composing_shows_press_i_prompt() {
    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.composing = false;
    assert_eq!(composer_title(&view, 30), "── Compose (press i) ");
    assert_eq!(composer_title(&view, 13), "── (press i) ");
    // " i " = 3 cols → needs block_w ≥ 5.
    assert_eq!(composer_title(&view, 5), "── i ");
    assert_eq!(composer_title(&view, 4), "");
}

#[test]
fn composer_title_never_truncates_across_block_widths() {
    use ratatui::{Terminal, backend::TestBackend};
    // Render the composer block at every block width where a non-empty
    // title is expected (≥7 for the " Esc " fallback). At each width,
    // confirm the picked title survives intact in the top border row.
    let ta = TextArea::default();
    let view = composer_view(&ta);
    for block_w in 7u16..=120 {
        let backend = TestBackend::new(block_w, 3);
        let mut terminal = Terminal::new(backend).expect("term");
        let expected_title = composer_title(&view, block_w);
        terminal
            .draw(|f| draw_composer_block(f, Rect::new(0, 0, block_w, 3), &view))
            .unwrap();
        let buf = terminal.backend().buffer();
        let row: String = (0..block_w)
            .map(|x| buf[(x, 0)].symbol().to_string())
            .collect();
        assert!(
            row.contains(&expected_title),
            "title {expected_title:?} truncated at block_w={block_w}: rendered {row:?}",
        );
    }
}

#[test]
fn reaction_picker_placeholder_uses_one_line() {
    let lines = reaction_picker_placeholder_lines(Style::default(), usize::MAX);
    assert_eq!(lines.len(), 1);

    let rendered: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(
        rendered,
        "1 👍  2 🧡  3 😂  4 👀  5 🔥  6 🙌  7 🚀  8 🤔  9 💩  0 icon  f list"
    );
}

#[test]
fn reaction_picker_placeholder_wraps_at_narrow_width() {
    let lines = reaction_picker_placeholder_lines(Style::default(), 48);
    assert_eq!(lines.len(), 2);
    let rendered: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert_eq!(
        rendered,
        vec![
            "1 👍  2 🧡  3 😂  4 👀  5 🔥  6 🙌  7 🚀  8 🤔",
            "9 💩  0 icon  f list",
        ]
    );
}

#[test]
fn chat_composer_placeholder_counts_wrapped_reaction_picker_lines() {
    let ta = TextArea::default();
    let lines = chat_composer_placeholder_lines(&ta, false, true, 48);
    assert_eq!(lines, 2);
}

#[test]
fn reaction_picker_placeholder_keeps_custom_zero_choice_at_mid_width() {
    let lines = reaction_picker_placeholder_lines(Style::default(), 50);
    let rendered: String = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect();

    assert!(
        rendered.contains("0 icon"),
        "custom icon reaction choice missing from {rendered:?}",
    );
}

#[test]
fn draw_composer_block_renders_reaction_picker_in_placeholder() {
    use ratatui::{Terminal, backend::TestBackend};

    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.reaction_picker_active = true;
    view.composing = false;
    view.selected_message = true;

    let backend = TestBackend::new(96, 3);
    let mut terminal = Terminal::new(backend).expect("term");

    terminal
        .draw(|f| draw_composer_block(f, Rect::new(0, 0, 96, 3), &view))
        .unwrap();

    let buf = terminal.backend().buffer();
    let row_1: String = (0..96).map(|x| buf[(x, 1)].symbol().to_string()).collect();
    assert!(
        row_1.contains("1 👍"),
        "reaction choices missing from {row_1:?}",
    );
    assert!(
        row_1.contains("1 👍   2 🧡"),
        "reaction choices should preserve two separator spaces plus wide emoji padding: {row_1:?}",
    );
    assert!(
        row_1.contains("8 🤔"),
        "extended reaction choices missing from {row_1:?}",
    );
    assert!(
        row_1.contains("9 💩"),
        "ninth reaction choice missing from {row_1:?}",
    );
    assert!(
        row_1.contains("0 icon"),
        "custom icon reaction choice missing from {row_1:?}",
    );
    assert!(
        row_1.contains("f list"),
        "reaction owner hint missing from {row_1:?}",
    );
    assert!(
        row_1.contains("0 icon  f list"),
        "reaction owner hint should preserve separator spacing after custom icon choice: {row_1:?}",
    );
}

#[test]
fn empty_composer_placeholder_is_dim_while_composing() {
    use ratatui::{Terminal, backend::TestBackend};

    let ta = TextArea::default();
    let view = composer_view(&ta);
    let placeholder = empty_composer_placeholder(&view, 20);
    let width = 20u16;
    let backend = TestBackend::new(width, 1);
    let mut terminal = Terminal::new(backend).expect("term");

    terminal
        .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered: String = (0..17).map(|x| buf[(x, 0)].symbol()).collect();
    assert_eq!(rendered, "Type a message...");
    assert_eq!(buf[(0, 0)].fg, theme::BG_CANVAS());
    assert_eq!(buf[(0, 0)].bg, theme::TEXT_DIM());
    assert_eq!(buf[(1, 0)].fg, theme::TEXT_DIM());
}

#[test]
fn empty_composer_placeholder_uses_hint_text_when_not_composing() {
    use ratatui::{Terminal, backend::TestBackend};

    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.composing = false;

    let expected =
        "Type a message · j/k select · Ctrl+] icon picker · or just ask @bot about anything";
    let width = expected.chars().count() as u16;
    let placeholder = empty_composer_placeholder(&view, width as usize);
    let backend = TestBackend::new(width, 1);
    let mut terminal = Terminal::new(backend).expect("term");

    terminal
        .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();
    assert_eq!(rendered, expected);
    assert_eq!(buf[(0, 0)].fg, theme::TEXT_DIM());
}

#[test]
fn empty_composer_placeholder_contextualizes_selected_news_message() {
    use ratatui::{Terminal, backend::TestBackend};

    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.composing = false;
    view.selected_message = true;
    view.selected_news_message = true;

    let expected =
        "f react · r reply · e edit · d delete · p profile · c copy · Enter view/copy link";
    let width = expected.chars().count() as u16;
    let placeholder = empty_composer_placeholder(&view, width as usize);
    let backend = TestBackend::new(width, 1);
    let mut terminal = Terminal::new(backend).expect("term");

    terminal
        .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();
    assert_eq!(rendered, expected);
}

#[test]
fn empty_composer_placeholder_contextualizes_selected_image_message() {
    use ratatui::{Terminal, backend::TestBackend};

    let ta = TextArea::default();
    let mut view = composer_view(&ta);
    view.composing = false;
    view.selected_message = true;
    view.selected_image_message = true;

    let expected = "f react · r reply · e edit · d delete · p profile · c copy · Enter view image";
    let width = expected.chars().count() as u16;
    let placeholder = empty_composer_placeholder(&view, width as usize);
    let backend = TestBackend::new(width, 1);
    let mut terminal = Terminal::new(backend).expect("term");

    terminal
        .draw(|f| f.render_widget(placeholder, Rect::new(0, 0, width, 1)))
        .unwrap();

    let buf = terminal.backend().buffer();
    let rendered: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();
    assert_eq!(rendered, expected);
}

#[test]
fn rooms_scroll_keeps_selection_near_center() {
    // height=9 -> anchor row = 4, leaving context above and below.
    assert_eq!(rooms_scroll_for_selection(20, 9, Some(4)), 0);
    assert_eq!(rooms_scroll_for_selection(20, 9, Some(7)), 3);
    // Selections near the end clamp to max_scroll = total - height.
    assert_eq!(rooms_scroll_for_selection(20, 9, Some(19)), 11);
}

#[test]
fn rooms_scroll_with_no_selection_does_not_scroll() {
    assert_eq!(rooms_scroll_for_selection(50, 10, None), 0);
}

#[test]
fn rooms_scroll_when_content_fits_returns_zero() {
    assert_eq!(rooms_scroll_for_selection(5, 10, Some(4)), 0);
}

#[test]
fn room_jump_prefix_shows_jump_key_when_active() {
    assert_eq!(room_jump_prefix(Some(b'a'), true, false), "[a] ");
}

#[test]
fn room_jump_prefix_shows_selected_marker_when_inactive() {
    assert_eq!(room_jump_prefix(None, false, true), "> ");
    assert_eq!(room_jump_prefix(None, false, false), "  ");
}

#[test]
fn room_list_rows_display_lounge() {
    let lounge = ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "lounge".to_string(),
        visibility: "public".to_string(),
        auto_join: true,
        slug: Some("lounge".to_string()),
        permanent: true,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rooms = vec![(lounge.clone(), Vec::new())];
    let mut rows_cache = ChatRowsCache::default();
    let usernames = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);
    let countries = HashMap::new();
    let message_reactions = HashMap::new();
    let unread_counts = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let composer = TextArea::default();
    let profile_award_badges = HashMap::new();
    let news_composer = TextArea::default();
    let view = chat_view(
        &mut rows_cache,
        &rooms,
        Some(lounge.id),
        &username_lookup,
        &countries,
        &message_reactions,
        &unread_counts,
        &bonsai_glyphs,
        &chat_badges,
        &profile_award_badges,
        &composer,
        &news_composer,
    );

    let room_list_view = room_list_view_from_render_input(&view);
    let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));
    let rendered = room_rows
        .lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>();

    assert!(
        rendered.iter().any(|line| line.contains("lounge")),
        "expected room list to show lounge: {rendered:?}"
    );
}

#[test]
fn room_list_rows_keep_directory_surfaces_out_of_home() {
    let rooms = Vec::new();
    let mut rows_cache = ChatRowsCache::default();
    let usernames = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);
    let countries = HashMap::new();
    let message_reactions = HashMap::new();
    let unread_counts = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let composer = TextArea::default();
    let profile_award_badges = HashMap::new();
    let news_composer = TextArea::default();
    let view = chat_view(
        &mut rows_cache,
        &rooms,
        None,
        &username_lookup,
        &countries,
        &message_reactions,
        &unread_counts,
        &bonsai_glyphs,
        &chat_badges,
        &profile_award_badges,
        &composer,
        &news_composer,
    );

    let room_list_view = room_list_view_from_render_input(&view);
    let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));
    let hit_slots: Vec<_> = room_rows.hit_slots.into_iter().flatten().collect();

    assert_eq!(
        hit_slots,
        vec![RoomSlot::Notifications, RoomSlot::News, RoomSlot::Discover,]
    );
}

#[test]
fn cozy_room_rail_places_voice_news_and_feeds_below_mentions_with_jump_keys() {
    let lounge = ChatRoom {
        id: Uuid::from_u128(1),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "lounge".to_string(),
        visibility: "public".to_string(),
        auto_join: true,
        slug: Some("lounge".to_string()),
        permanent: true,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rust = ChatRoom {
        id: Uuid::from_u128(2),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "topic".to_string(),
        visibility: "public".to_string(),
        auto_join: false,
        slug: Some("rust".to_string()),
        permanent: false,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rooms = vec![(lounge.clone(), Vec::new()), (rust.clone(), Vec::new())];
    let mut rows_cache = ChatRowsCache::default();
    let usernames = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);
    let countries = HashMap::new();
    let message_reactions = HashMap::new();
    let unread_counts = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let composer = TextArea::default();
    let profile_award_badges = HashMap::new();
    let news_composer = TextArea::default();
    let mut view = chat_view(
        &mut rows_cache,
        &rooms,
        None,
        &username_lookup,
        &countries,
        &message_reactions,
        &unread_counts,
        &bonsai_glyphs,
        &chat_badges,
        &profile_award_badges,
        &composer,
        &news_composer,
    );
    view.feeds_view.has_feeds = true;
    view.room_jump_active = true;

    let room_list_view = room_list_view_from_render_input(&view);
    let room_rows = build_cozy_room_rail_rows(&room_list_view, 40);
    let keyed_slots: Vec<_> = room_rows
        .lines
        .iter()
        .zip(room_rows.hit_slots.iter())
        .filter_map(|(line, slot)| slot.map(|slot| (slot, line_text(line))))
        .collect();

    assert_eq!(
        &keyed_slots[..6],
        &[
            (RoomSlot::Room(lounge.id), "a lounge".to_string()),
            (RoomSlot::Notifications, "s mentions".to_string()),
            (RoomSlot::News, "d news".to_string()),
            (RoomSlot::Feeds, "f rss".to_string()),
            // Discover ("+ browse rooms") is the last entry in Core, so the
            // topic rooms below it start one jump key later.
            (RoomSlot::Discover, "g + browse rooms".to_string()),
            (RoomSlot::Room(rust.id), "h rust".to_string()),
        ]
    );
}

#[test]
fn cozy_room_rail_shows_section_keys_when_fold_prefix_is_armed() {
    let lounge = ChatRoom {
        id: Uuid::from_u128(1),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "lounge".to_string(),
        visibility: "public".to_string(),
        auto_join: true,
        slug: Some("lounge".to_string()),
        permanent: true,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rust = ChatRoom {
        id: Uuid::from_u128(2),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "topic".to_string(),
        visibility: "public".to_string(),
        auto_join: false,
        slug: Some("rust".to_string()),
        permanent: false,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let dm = ChatRoom {
        id: Uuid::from_u128(3),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "dm".to_string(),
        visibility: "private".to_string(),
        auto_join: false,
        slug: None,
        permanent: false,
        language_code: None,
        dm_user_a: Some(Uuid::nil()),
        dm_user_b: Some(Uuid::from_u128(4)),
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rooms = vec![
        (lounge.clone(), Vec::new()),
        (rust.clone(), Vec::new()),
        (dm, Vec::new()),
    ];
    let favorite_room_ids = vec![lounge.id];
    let mut rows_cache = ChatRowsCache::default();
    let usernames = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);
    let countries = HashMap::new();
    let message_reactions = HashMap::new();
    let unread_counts = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let composer = TextArea::default();
    let profile_award_badges = HashMap::new();
    let news_composer = TextArea::default();
    let mut view = chat_view(
        &mut rows_cache,
        &rooms,
        None,
        &username_lookup,
        &countries,
        &message_reactions,
        &unread_counts,
        &bonsai_glyphs,
        &chat_badges,
        &profile_award_badges,
        &composer,
        &news_composer,
    );
    view.favorite_room_ids = &favorite_room_ids;
    view.room_section_prefix_armed = true;

    let room_list_view = room_list_view_from_render_input(&view);
    let room_rows = build_cozy_room_rail_rows(&room_list_view, 40);
    let rendered = room_rows.lines.iter().map(line_text).collect::<Vec<_>>();

    for expected in [
        "[f] - favorites",
        "[o] - core",
        "[c] - channels",
        "[d] - dms",
    ] {
        assert!(
            rendered.iter().any(|line| line == expected),
            "expected {expected:?} in {rendered:?}"
        );
    }
}

#[test]
fn room_section_header_parser_ignores_fold_key_hints() {
    assert_eq!(strip_room_section_header_prefix("[o] - core"), "core");
    assert_eq!(strip_room_section_header_prefix("- [o] core"), "core");
    assert_eq!(strip_room_section_header_prefix("+ dms"), "dms");
}

#[test]
fn room_list_rows_skip_game_rooms() {
    let lounge = ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "lounge".to_string(),
        visibility: "public".to_string(),
        auto_join: true,
        slug: Some("lounge".to_string()),
        permanent: true,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let game = ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "game".to_string(),
        visibility: "public".to_string(),
        auto_join: false,
        slug: Some("bj-abc123".to_string()),
        permanent: false,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rooms = vec![(lounge.clone(), Vec::new()), (game.clone(), Vec::new())];
    let mut rows_cache = ChatRowsCache::default();
    let usernames = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);
    let countries = HashMap::new();
    let message_reactions = HashMap::new();
    let unread_counts = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let composer = TextArea::default();
    let profile_award_badges = HashMap::new();
    let news_composer = TextArea::default();
    let view = chat_view(
        &mut rows_cache,
        &rooms,
        Some(lounge.id),
        &username_lookup,
        &countries,
        &message_reactions,
        &unread_counts,
        &bonsai_glyphs,
        &chat_badges,
        &profile_award_badges,
        &composer,
        &news_composer,
    );

    let room_list_view = room_list_view_from_render_input(&view);
    let room_rows = build_room_list_rows(&room_list_view, Rect::new(0, 0, 40, 20));

    assert!(!room_rows.hit_slots.contains(&Some(RoomSlot::Room(game.id))));
}

#[test]
fn room_list_hit_test_maps_public_room_row_to_room_slot() {
    let lounge = ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "lounge".to_string(),
        visibility: "public".to_string(),
        auto_join: true,
        slug: Some("lounge".to_string()),
        permanent: true,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rust = ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "topic".to_string(),
        visibility: "public".to_string(),
        auto_join: false,
        slug: Some("rust".to_string()),
        permanent: false,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    };
    let rooms = vec![(lounge.clone(), Vec::new()), (rust.clone(), Vec::new())];
    let mut rows_cache = ChatRowsCache::default();
    let usernames = HashMap::new();
    let username_lookup = UsernameLookup::new(&usernames, None);
    let countries = HashMap::new();
    let message_reactions = HashMap::new();
    let unread_counts = HashMap::new();
    let bonsai_glyphs = HashMap::new();
    let chat_badges = HashMap::new();
    let composer = TextArea::default();
    let profile_award_badges = HashMap::new();
    let news_composer = TextArea::default();
    let view = chat_view(
        &mut rows_cache,
        &rooms,
        Some(lounge.id),
        &username_lookup,
        &countries,
        &message_reactions,
        &unread_counts,
        &bonsai_glyphs,
        &chat_badges,
        &profile_award_badges,
        &composer,
        &news_composer,
    );

    let area = Rect::new(1, 1, 74, 30);
    let rooms_area = room_list_area(area, chat_selection_mode(&view, area));
    let room_list_view = room_list_view_from_render_input(&view);
    let inner = room_rail_inner_area(rooms_area);
    let hint_rows = build_rail_nav_hint_lines().len() as u16;
    let footer_reserve = hint_rows + 2;
    let list_area = if inner.height > footer_reserve + 2 {
        Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_reserve)]).split(inner)[0]
    } else {
        inner
    };
    let room_rows = build_cozy_room_rail_rows(&room_list_view, rooms_area.width.saturating_sub(2));
    let rust_row = room_rows
        .hit_slots
        .iter()
        .position(|slot| *slot == Some(RoomSlot::Room(rust.id)))
        .expect("rust room row");

    assert_eq!(
        room_list_hit_test(
            rooms_area,
            &room_list_view,
            list_area.x,
            list_area.y + rust_row as u16
        ),
        Some(RoomSlot::Room(rust.id))
    );
    assert_eq!(
        room_list_hit_test(rooms_area, &room_list_view, list_area.x, list_area.y),
        None
    );
    assert!(room_list_panel_contains(
        rooms_area,
        &room_list_view,
        rooms_area.x,
        rooms_area.y
    ));
    assert!(!room_list_panel_contains(
        rooms_area,
        &room_list_view,
        rooms_area.right(),
        rooms_area.y
    ));
}

// ── Mouse hit-test (author header segments) ──────────────────

#[test]
fn header_segments_bare_username_only() {
    let (prefix, segs) =
        build_author_prefix_and_segments(false, "alice", &[], None, None, None, None);
    assert_eq!(prefix, "alice");
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].target, HeaderTarget::Profile);
    // column 0 is pad, prefix begins at 1.
    assert_eq!(segs[0].start_col, 1);
    assert_eq!(segs[0].end_col, 1 + 5); // "alice"
}

#[test]
fn build_author_prefix_matches_legacy_formatter_across_combinations() {
    // The legacy `format_author_badge_suffix` is kept under #[cfg(test)]
    // precisely to pin this byte-identity invariant: whatever pieces
    // the production builder emits must concatenate to exactly the
    // same prefix string the legacy `format!(...)` block produced.
    let assert_matches =
        |is_friend: bool, author: &str, sp: &[&str], cb: Option<&str>, bg: Option<&str>| {
            let suffix = format_author_badge_suffix(sp, cb, bg);
            let legacy = if is_friend {
                format!("{FRIEND_BADGE} {author}{suffix}")
            } else {
                format!("{author}{suffix}")
            };
            let (built, _) =
                build_author_prefix_and_segments(is_friend, author, sp, cb, bg, None, None);
            assert_eq!(
                built, legacy,
                "case {is_friend} {author:?} {sp:?} {cb:?} {bg:?}"
            );
        };
    assert_matches(false, "alice", &[], None, None);
    assert_matches(true, "alice", &[], None, None);
    assert_matches(false, "alice", &["mod", "dev"], None, None);
    assert_matches(false, "alice", &[], Some("🐱"), None);
    assert_matches(false, "alice", &[], None, Some("🌱"));
    assert_matches(false, "alice", &[], Some("🐱"), Some("🌱"));
    assert_matches(true, "alice", &["mod"], Some("🐱"), Some("🌱"));
}

#[test]
fn header_segments_full_label_orders_special_bonsai_store() {
    // alice ★ + author + " mod bonsai 🐱"
    // (special "mod", bonsai "bonsai", store "🐱")
    let (prefix, segs) = build_author_prefix_and_segments(
        true,
        "alice",
        &["mod"],
        Some("🐱"),
        Some("bonsai"),
        None,
        None,
    );
    // Sanity: the legacy formatter produces the same suffix shape.
    let legacy = format!(
        "{FRIEND_BADGE} alice{}",
        format_author_badge_suffix(&["mod"], Some("🐱"), Some("bonsai"))
    );
    assert_eq!(prefix, legacy);

    // Profile-classified segments: friend badge, author, "mod", "bonsai".
    let profiles: Vec<_> = segs
        .iter()
        .filter(|s| s.target == HeaderTarget::Profile)
        .collect();
    assert_eq!(profiles.len(), 4);

    // Exactly one StoreBadge segment, sitting after "bonsai".
    let stores: Vec<_> = segs
        .iter()
        .filter(|s| s.target == HeaderTarget::StoreBadge)
        .collect();
    assert_eq!(stores.len(), 1);
    let store = stores[0];
    // The store segment's start col must equal the prefix-relative
    // offset of the chat-badge emoji (column 0 is the pad cell).
    let expected_store_offset = 1
        + UnicodeWidthStr::width(FRIEND_BADGE) as u16
        + 1
        + UnicodeWidthStr::width("alice") as u16
        + 1
        + UnicodeWidthStr::width("mod") as u16
        + UnicodeWidthStr::width(AUTHOR_BADGE_SEPARATOR) as u16
        + UnicodeWidthStr::width("bonsai") as u16
        + UnicodeWidthStr::width(AUTHOR_BADGE_SEPARATOR) as u16;
    assert_eq!(store.start_col, expected_store_offset);
    assert_eq!(
        store.end_col,
        expected_store_offset + UnicodeWidthStr::width("🐱") as u16
    );
}

#[test]
fn header_segments_skip_empty_badges() {
    // Empty special/store/bonsai entries should be dropped — they
    // would render as zero-width but a hit-test range of (col, col)
    // would never match anything, so don't emit them.
    let (_prefix, segs) = build_author_prefix_and_segments(
        false,
        "alice",
        &["", "mod"],
        Some(""),
        Some(""),
        None,
        None,
    );
    // 1 author + 1 special "mod" = 2 segments. No store, no bonsai.
    assert_eq!(segs.len(), 2);
    assert!(segs.iter().all(|s| s.target == HeaderTarget::Profile));
    assert!(segs.iter().any(|s| s.end_col - s.start_col == 3)); // "mod"
}

#[test]
fn header_segments_bonsai_then_store_without_specials() {
    let (_prefix, segs) =
        build_author_prefix_and_segments(false, "bob", &[], Some("🐱"), Some("🌱"), None, None);
    // author (Profile), bonsai (Profile), store (StoreBadge).
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[0].target, HeaderTarget::Profile);
    assert_eq!(segs[1].target, HeaderTarget::Profile);
    assert_eq!(segs[2].target, HeaderTarget::StoreBadge);
    // Bonsai and store are separated by `AUTHOR_BADGE_SEPARATOR`, so their
    // ranges must not abut.
    assert!(segs[2].start_col > segs[1].end_col);
}

#[test]
fn header_segments_put_monthly_awards_after_author() {
    let (prefix, segs) = build_author_prefix_and_segments(
        false,
        "alice",
        &["mod"],
        Some("shop"),
        Some("bonsai"),
        Some("AW1 CHIP2 SN3"),
        None,
    );

    assert_eq!(prefix, "alice [AW1 CHIP2 SN3] mod bonsai shop");
    assert_eq!(segs.len(), 5);
    assert_eq!(segs[0].target, HeaderTarget::Profile);
    assert_eq!(segs[1].target, HeaderTarget::Profile);
    assert_eq!(segs[2].target, HeaderTarget::Profile);
    assert_eq!(segs[3].target, HeaderTarget::Profile);
    assert_eq!(segs[4].target, HeaderTarget::StoreBadge);

    let expected_awards_offset = 1 + UnicodeWidthStr::width("alice ") as u16;
    assert_eq!(segs[1].start_col, expected_awards_offset);
    assert_eq!(
        segs[1].end_col,
        expected_awards_offset + UnicodeWidthStr::width("[AW1 CHIP2 SN3]") as u16
    );
}

#[test]
fn header_segments_split_chat_flag_from_regular_badge() {
    let chat_badges = [
        (HeaderTarget::StoreBadge, "🐱"),
        (HeaderTarget::StoreFlag, "US"),
    ];
    let (prefix, segs, author_range) = build_author_prefix_and_segments_with_chat_badges(
        false,
        "bob",
        &[],
        &chat_badges,
        None,
        None,
        None,
    );
    assert_eq!(prefix, "bob 🐱 US");
    assert_eq!(author_range, (0, 3));
    assert_eq!(segs.len(), 3);
    assert_eq!(segs[0].target, HeaderTarget::Profile);
    assert_eq!(segs[1].target, HeaderTarget::StoreBadge);
    assert_eq!(segs[2].target, HeaderTarget::StoreFlag);
}

#[test]
fn header_prefix_orders_all_badge_classes() {
    let chat_badges = [
        (HeaderTarget::StoreBadge, "badge"),
        (HeaderTarget::StoreFlag, "flag"),
    ];
    let (prefix, _segs, _author_range) = build_author_prefix_and_segments_with_chat_badges(
        false,
        "alice",
        &["mod", "developer", "artist"],
        &chat_badges,
        Some("bonsai"),
        Some("AW1 CHIP2"),
        Some("brb"),
    );

    assert_eq!(
        prefix,
        "alice [AW1 CHIP2] mod developer artist bonsai badge flag brb"
    );
}

#[test]
fn chat_badge_display_parts_put_store_badge_before_flag() {
    let parts = chat_badge_display_parts("🇺🇸 🐱", false);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].0, HeaderTarget::StoreBadge);
    assert_eq!(parts[0].1, "🐱");
    assert_eq!(parts[1].0, HeaderTarget::StoreFlag);
    assert_eq!(parts[1].1, "🇺🇸");

    let fallback_parts = chat_badge_display_parts("🇺🇸 🐱", true);
    assert_eq!(fallback_parts[0].0, HeaderTarget::StoreBadge);
    assert_eq!(fallback_parts[0].1, "🐱");
    assert_eq!(fallback_parts[1].0, HeaderTarget::StoreFlag);
    assert_eq!(fallback_parts[1].1, "US");
}

#[test]
fn visible_chat_rows_pads_top_with_none_hits() {
    // Three rows of content into a viewport of height 5 ⇒ two
    // leading padding rows whose hit kind must be `None`.
    let message_id = Uuid::now_v7();
    let cache = ChatRowsCache {
        all_rows: vec![
            Line::from(Span::raw("alice")),
            Line::from(Span::raw("hello")),
            Line::from(Span::raw("world")),
        ],
        row_message: vec![Some(message_id), Some(message_id), Some(message_id)],
        row_kind: vec![RowKindLite::Header, RowKindLite::Body, RowKindLite::Body],
        header_segments: {
            let mut m = HashMap::new();
            m.insert(
                message_id,
                vec![HeaderSegment {
                    start_col: 1,
                    end_col: 6,
                    target: HeaderTarget::Profile,
                }],
            );
            m
        },
        ..Default::default()
    };

    let visible = visible_chat_rows(&cache, None, None, 5);
    assert_eq!(visible.lines.len(), 5);
    assert_eq!(visible.hits.len(), 5);
    // Top two are padding.
    assert!(matches!(visible.hits[0].kind, ChatRowKind::None));
    assert!(visible.hits[0].message_id.is_none());
    assert!(matches!(visible.hits[1].kind, ChatRowKind::None));
    // Then header, body, body.
    assert!(matches!(visible.hits[2].kind, ChatRowKind::Header(_)));
    assert_eq!(visible.hits[2].message_id, Some(message_id));
    assert!(matches!(visible.hits[3].kind, ChatRowKind::Body));
    assert!(matches!(visible.hits[4].kind, ChatRowKind::Body));
}

fn room_with_info(title: Option<&str>, about: Option<&str>, rules: Option<&str>) -> ChatRoom {
    ChatRoom {
        id: Uuid::now_v7(),
        created: Utc::now(),
        updated: Utc::now(),
        kind: "topic".to_string(),
        visibility: "public".to_string(),
        auto_join: false,
        slug: Some("book-club".to_string()),
        permanent: false,
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: title.map(str::to_string),
        about: about.map(str::to_string),
        rules: rules.map(str::to_string),
        created_by: None,
    }
}

fn row_text(buf: &ratatui::buffer::Buffer, y: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf[(x, y)].symbol().to_string())
        .collect()
}

#[test]
fn room_info_header_pins_name_and_about_and_shrinks_the_message_area() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let room = room_with_info(Some("Book Club"), Some("We read sci-fi"), Some("Be kind"));
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).expect("term");
    let area = Rect::new(0, 0, 40, 20);
    let mut remaining = area;
    terminal
        .draw(|f| remaining = super::draw_room_info_header(f, area, &room))
        .unwrap();

    // Name + about + separator = 3 rows carved off the top.
    assert_eq!(remaining.y, 3);
    assert_eq!(remaining.height, 17);

    let buf = terminal.backend().buffer();
    assert!(row_text(buf, 0, 40).contains("Book Club"));
    assert!(row_text(buf, 0, 40).contains("/rules"));
    assert!(row_text(buf, 1, 40).contains("We read sci-fi"));
}

#[test]
fn room_info_header_is_absent_when_the_room_has_no_info() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let room = room_with_info(None, None, None);
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).expect("term");
    let area = Rect::new(0, 0, 40, 20);
    let mut remaining = area;
    terminal
        .draw(|f| remaining = super::draw_room_info_header(f, area, &room))
        .unwrap();
    assert_eq!(remaining, area, "no header should be drawn");
}

#[test]
fn room_info_header_falls_back_to_slug_when_unnamed_but_has_about() {
    use ratatui::{Terminal, backend::TestBackend, layout::Rect};
    let room = room_with_info(None, Some("A cozy corner"), None);
    let mut terminal = Terminal::new(TestBackend::new(40, 20)).expect("term");
    let area = Rect::new(0, 0, 40, 20);
    let mut remaining = area;
    terminal
        .draw(|f| remaining = super::draw_room_info_header(f, area, &room))
        .unwrap();
    assert_eq!(remaining.y, 3);
    let buf = terminal.backend().buffer();
    assert!(row_text(buf, 0, 40).contains("#book-club"));
    // No rules -> no /rules hint.
    assert!(!row_text(buf, 0, 40).contains("/rules"));
}
