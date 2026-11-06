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

fn header_rows(header: &TourHeader, width: u16) -> Vec<String> {
    let height = header.rows();
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| header.draw(frame, Rect::new(0, 0, width, height)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// The board stop writes its pitch across the top of the Arcade: it asks
/// for the board's keys while the board is open, says so and moves on
/// where the board does not fit, and once the board is solved names
/// tomorrow and the way out.
#[test]
fn the_board_stop_pitches_in_a_header_that_names_its_keys() {
    let open = header_rows(
        &tour_header(Tutorial::VisitBoard, TableStop::Racked, BoardStop::Open).unwrap(),
        100,
    );
    assert!(open[0].contains("the tour · today's board"), "{open:#?}");
    assert!(
        open[1].contains("everyone here gets this same board today"),
        "{open:#?}"
    );
    assert!(
        open.last()
            .unwrap()
            .contains("[hjkl/arrows] move · [Space] turn · [f] lock ──"),
        "{open:#?}"
    );

    let small = header_rows(
        &tour_header(Tutorial::VisitBoard, TableStop::Racked, BoardStop::TooSmall).unwrap(),
        100,
    );
    assert!(small[1].contains("needs a bigger window"), "{small:#?}");
    assert!(
        small.last().unwrap().contains("[Enter] settle in ──"),
        "{small:#?}"
    );

    let solved = header_rows(
        &tour_header(
            Tutorial::VisitBoard,
            TableStop::Racked,
            BoardStop::Solved { hours_left: 7 },
        )
        .unwrap(),
        100,
    );
    assert!(
        solved[1].contains("connected. that was day one."),
        "{solved:#?}"
    );
    assert!(
        solved[2].contains("the next board is up at 00:00 UTC, in 7 h. the paper is at the door."),
        "{solved:#?}"
    );
    assert!(
        solved[4].contains("[q] the lobby · [o] who else solved it · [?] the guide"),
        "{solved:#?}"
    );
    assert!(
        solved.last().unwrap().contains("[Enter] settle in ──"),
        "{solved:#?}"
    );

    for stage in [
        Tutorial::Off,
        Tutorial::Pending,
        Tutorial::Welcome,
        Tutorial::VisitChat,
        Tutorial::Done,
    ] {
        assert!(
            tour_header(stage, TableStop::Racked, BoardStop::Open).is_none(),
            "{stage:?}"
        );
    }
}

/// The table stop asks for the break, then for Enter onward once it is
/// struck; a terminal the table does not fit is told so and moves on.
#[test]
fn the_table_stop_pitches_in_a_header_that_names_its_keys() {
    let header = |table: TableStop| {
        header_rows(
            &tour_header(Tutorial::VisitTable, table, BoardStop::Open).unwrap(),
            72,
        )
    };
    let racked = header(TableStop::Racked);
    assert!(
        racked[0].contains("the tour · one shot of pool"),
        "{racked:#?}"
    );
    assert!(racked[1].contains("take the break"), "{racked:#?}");
    assert!(
        racked.last().unwrap().contains("[Enter] break ──"),
        "{racked:#?}"
    );
    let struck = header(TableStop::Played);
    assert!(
        struck
            .last()
            .unwrap()
            .contains("[Enter] next: today's board ──"),
        "{struck:#?}"
    );
    let small = header(TableStop::TooSmall);
    assert!(small[1].contains("needs a bigger window"), "{small:#?}");
    assert!(
        small
            .last()
            .unwrap()
            .contains("[Enter] next: today's board ──"),
        "{small:#?}"
    );
}

/// The board stop stands on what the frame can show: solved is solved
/// whatever the terminal, an unsolved board has to fit under the header,
/// and the send-off counts whole hours up to the next 00:00 UTC.
#[test]
fn the_board_stop_follows_the_board_and_the_terminal() {
    let noon = chrono::DateTime::parse_from_rfc3339("2026-11-06T12:10:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let default_terminal = Rect::new(1, 1, 78, 22);
    assert_eq!(board_stop(default_terminal, false, noon), BoardStop::Open);
    assert_eq!(
        board_stop(default_terminal, true, noon),
        BoardStop::Solved { hours_left: 12 }
    );
    // Too short for the header and a five-tile board under it.
    let tiny = Rect::new(1, 1, 78, 9);
    assert_eq!(board_stop(tiny, false, noon), BoardStop::TooSmall);
    assert_eq!(
        board_stop(tiny, true, noon),
        BoardStop::Solved { hours_left: 12 }
    );

    let late = chrono::DateTime::parse_from_rfc3339("2026-11-06T23:30:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(
        board_stop(default_terminal, true, late),
        BoardStop::Solved { hours_left: 1 }
    );
    let midnight = chrono::DateTime::parse_from_rfc3339("2026-11-06T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(
        board_stop(default_terminal, true, midnight),
        BoardStop::Solved { hours_left: 24 }
    );
}

/// The chat stop names its page key, moves on with Enter and says so on a
/// default terminal, and draws only over its own page.
#[test]
fn the_chat_stop_names_its_page_key_and_moves_on_with_enter() {
    let draw = |screen: Screen| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                draw_tour_overlay(frame, Rect::new(1, 1, 78, 22), Tutorial::VisitChat, screen)
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
            .collect::<Vec<String>>()
    };
    let rows = draw(Screen::Dashboard);
    assert!(
        rows.iter().any(|row| row.contains("the tour · [1] home")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("every name in it is a real person")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("[Enter] next: one shot of pool")),
        "{rows:#?}"
    );
    let elsewhere = draw(Screen::Arcade);
    assert!(
        elsewhere.iter().all(|row| row.trim().is_empty()),
        "{elsewhere:#?}"
    );
}

/// Boxed or hosted, a stop reads the same: the keys close the breaker flush
/// right, two columns in from the frame.
#[test]
fn a_boxed_stop_ends_on_the_same_breaker_as_a_hosted_one() {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw_tour_overlay(
                frame,
                Rect::new(1, 1, 78, 22),
                Tutorial::VisitChat,
                Screen::Dashboard,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..24)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    let breaker = rows
        .iter()
        .find(|row| row.contains("[Enter]"))
        .expect("the stop names its key");
    assert!(
        breaker
            .trim_end()
            .ends_with("[Enter] next: one shot of pool ──  │"),
        "{rows:#?}"
    );
    assert!(breaker.trim_start().starts_with("│  ──"), "{rows:#?}");
}
