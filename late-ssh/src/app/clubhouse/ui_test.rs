use super::*;

#[test]
fn truncate_name_keeps_short_names_and_cuts_long_ones() {
    assert_eq!(truncate_name("alice"), "alice");
    assert_eq!(truncate_name("exactly-10"), "exactly-10");
    assert_eq!(truncate_name("much-too-long-name"), "much-too-…");
}

#[test]
fn single_width_folds_wide_and_zero_width_glyphs() {
    // ASCII and box-drawing art survive untouched.
    assert_eq!(to_single_width("hello ·│─"), "hello ·│─");
    // Emoji (width 2) and combining marks (width 0) become one cell each,
    // so the char count matches the rendered cell count.
    let folded = to_single_width("a🎉b");
    assert_eq!(folded, "a·b");
    assert_eq!(folded.chars().count(), 3);
    // Wide names collapse before truncation, so the length math is honest:
    // 12 double-width chars fold to 12 cells, cut to 9 plus an ellipsis.
    assert_eq!(truncate_name("你你你你你你你你你你你你"), "·········…");
}

#[test]
fn camera_centers_small_maps_and_clamps_large_ones() {
    // Viewport wider than the map: origin pinned to 0 (padding centers).
    assert_eq!(camera_origin(10, 300, 200), 0);
    // Player near the left edge: no negative origin.
    assert_eq!(camera_origin(2, 40, 200), 0);
    // Player mid-map: centered on the player.
    assert_eq!(camera_origin(100, 40, 200), 80);
    // Player near the right edge: clamped to the map end.
    assert_eq!(camera_origin(199, 40, 200), 160);
}

#[test]
fn labels_clamp_inside_the_walls() {
    let mut cells: Cells =
        vec![vec![(' ', Style::default()); usize::from(map::MAP_W)]; usize::from(map::MAP_H)];
    put_label(&mut cells, 1, 5, "longishname", Style::default());
    assert_eq!(cells[5][1].0, 'l');
    put_label(
        &mut cells,
        map::MAP_W - 2,
        6,
        "longishname",
        Style::default(),
    );
    let end: String = cells[6].iter().map(|(ch, _)| *ch).collect();
    assert!(end.trim_end().ends_with("longishname"));
}

#[test]
fn bubble_text_drops_reply_quotes_and_flattens_lines() {
    assert_eq!(
        bubble_text("> @alice: earlier\nthanks a lot"),
        "thanks a lot"
    );
    assert_eq!(bubble_text("two\nlines  here"), "two lines here");
}

#[test]
fn wrap_bubble_wraps_and_ellipsizes() {
    let (lines, truncated) = wrap_bubble("hello there".to_string(), 28, 3);
    assert_eq!(lines, vec!["hello there"]);
    assert!(!truncated);

    let long = "one two three four five six seven eight nine ten eleven twelve \
                thirteen fourteen fifteen sixteen seventeen"
        .to_string();
    let (lines, truncated) = wrap_bubble(long, 12, 3);
    assert_eq!(lines.len(), 3);
    assert!(truncated);
    assert!(lines.iter().all(|l| l.chars().count() <= 12));
    assert!(lines.last().unwrap().ends_with('…'));

    assert!(wrap_bubble("   ".to_string(), 10, 3).0.is_empty());
}

#[test]
fn bubbles_widen_before_they_truncate() {
    // Fits at the cozy tier: stays narrow.
    let lines = wrap_bubble_fitting("a short one".to_string());
    assert_eq!(lines, vec!["a short one"]);

    // Too long for 28x3 but fits wider: widens instead of cutting. This
    // is the bartender-answer case.
    let mid = "the arcade cabinet is page 2, the heavy door is page 3, \
               the big table is page 4, and the easel is page 5"
        .to_string();
    let lines = wrap_bubble_fitting(mid.clone());
    assert!(lines.len() <= BUBBLE_MAX_LINES);
    assert!(!lines.last().unwrap().ends_with('…'), "widening failed");
    assert_eq!(lines.join(" "), mid);

    // Genuinely huge: widest tier plus ellipsis.
    let huge = "word ".repeat(80);
    let lines = wrap_bubble_fitting(huge);
    assert_eq!(lines.len(), BUBBLE_MAX_LINES);
    assert!(lines.last().unwrap().ends_with('…'));
}

#[test]
fn fresh_bubbles_take_the_newest_message_per_author_from_a_newest_first_tail() {
    let now = chrono::Utc::now();
    let msg = |n: u128, author: u128, secs_ago: i64, body: &str| ChatMessage {
        id: Uuid::from_u128(n),
        created: now - chrono::Duration::seconds(secs_ago),
        updated: now - chrono::Duration::seconds(secs_ago),
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(99),
        user_id: Uuid::from_u128(author),
        body: body.to_string(),
    };
    // Newest-first, like ChatState room tails.
    let tail = vec![
        msg(1, 1, 2, "newest from alice"),
        msg(2, 2, 4, "from bob"),
        msg(3, 1, 6, "older from alice"),
        msg(4, 3, 60, "stale from carol"),
        msg(5, 4, 3, "unreachable behind the stale break"),
    ];
    let picked: Vec<&str> = fresh_bubble_messages(&tail, now)
        .iter()
        .map(|m| m.body.as_str())
        .collect();
    assert_eq!(picked, vec!["newest from alice", "from bob"]);
}

#[test]
fn bubble_boxes_stay_inside_the_map() {
    let mut cells: Cells =
        vec![vec![(' ', Style::default()); usize::from(map::MAP_W)]; usize::from(map::MAP_H)];
    // Anchored right at the top wall: flips below instead of clipping.
    draw_bubble_box(&mut cells, 5, 1, &["hi".to_string()]);
    let top_row: String = cells[0].iter().map(|(ch, _)| *ch).collect();
    assert!(top_row.trim().is_empty(), "bubble drew over the top wall");
    // Anchored mid-room: the border lands above the anchor.
    draw_bubble_box(&mut cells, 90, 20, &["hello".to_string()]);
    assert_eq!(cells[18][86].0, '╭');
}

/// The crown on the floor is painted the same amber it wears in chat. It
/// sits at index `name_len`, exactly where the name's color effect stops,
/// so without its own rule it would fall into the dim label color.
#[test]
fn the_crown_glyph_on_the_floor_is_painted_amber_not_dim() {
    theme::set_current_by_id("late");
    let mut cells: Cells =
        vec![vec![(' ', Style::default()); usize::from(map::MAP_W)]; usize::from(map::MAP_H)];
    let flair = ResolvedName {
        style: None,
        title: Some("the night clerk".to_string()),
        crown: true,
        milestone: None,
    };
    let dim = Style::default().fg(ratatui::style::Color::DarkGray);
    draw_presence(
        &mut cells,
        Placement::Walking(30, 12),
        'o',
        Style::default(),
        "bob",
        dim,
        Some(&flair),
        0,
    );

    let row = &cells[9];
    let text: String = row.iter().map(|(ch, _)| *ch).collect();
    let crown_at = text
        .chars()
        .position(|ch| ch == '\u{1F451}')
        .unwrap_or_else(|| panic!("no crown on the floor label: {text:?}"));
    assert_eq!(row[crown_at - 2].0, 'b');
    assert_eq!(
        row[crown_at - 2].1,
        dim,
        "a name without an effect stays dim"
    );
    assert_eq!(row[crown_at - 1].0, ' ', "one space between name and crown");
    assert_eq!(
        row[crown_at].1.fg,
        Some(theme::AMBER_GLOW()),
        "the crown must be amber, as in chat"
    );
    // The emoji is two columns wide: it owns the next cell too, so the
    // title starts one cell later than a char count would put it and the
    // rest of the row stays aligned to the walls.
    assert_eq!(row[crown_at + 1].0, WIDE_TAIL);
    assert_eq!(row[crown_at + 2].0, ',');
    assert_eq!(row[crown_at + 2].1, dim, "the title after it stays dim");
}
