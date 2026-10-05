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
        laureate: false,
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
        .position(|ch| ch == '\u{1F48E}')
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

/// The stops that live inside a real surface: the music stop counts the
/// catalogue live, the lobby stop leads to the practice table, and each one's
/// breaker names its keys at the width a default terminal leaves a modal.
#[test]
fn the_surface_stops_pitch_in_a_header_that_names_their_keys() {
    let music = header_rows(
        &tour_header(Tutorial::VisitMusic, TableStop::Racked, false).unwrap(),
        72,
    );
    let total = RadioStation::enabled().count();
    assert!(
        music[1].contains(&format!("{total} stations")),
        "{music:#?}"
    );
    assert!(
        music
            .last()
            .unwrap()
            .contains("[Enter] next: the arcade ──"),
        "{music:#?}"
    );

    let lobby = header_rows(
        &tour_header(Tutorial::VisitLobby, TableStop::Racked, false).unwrap(),
        72,
    );
    assert!(
        lobby
            .last()
            .unwrap()
            .contains("[Enter] next: one shot of pool ──"),
        "{lobby:#?}"
    );

    // The table asks for the break, then for Enter onward once it is struck.
    let table = header_rows(
        &tour_header(Tutorial::VisitTable, TableStop::Racked, false).unwrap(),
        72,
    );
    assert!(
        table.last().unwrap().contains("[Enter] break ──"),
        "{table:#?}"
    );
    let struck = header_rows(
        &tour_header(Tutorial::VisitTable, TableStop::Played, false).unwrap(),
        72,
    );
    assert!(
        struck
            .last()
            .unwrap()
            .contains("[Enter] next: the games ──"),
        "{struck:#?}"
    );

    // A terminal the table does not fit: the stop says so and moves on.
    let small = header_rows(
        &tour_header(Tutorial::VisitTable, TableStop::TooSmall, false).unwrap(),
        72,
    );
    assert!(small[1].contains("needs a bigger window"), "{small:#?}");
    assert!(
        small.last().unwrap().contains("[Enter] next: the games ──"),
        "{small:#?}"
    );

    assert!(tour_header(Tutorial::VisitGames, TableStop::Racked, false).is_none());
}

/// Every page stop moves on with Enter and says so on a default terminal.
#[test]
fn a_page_stop_names_its_page_key_and_moves_on_with_enter() {
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw_tour_overlay(
                frame,
                Rect::new(1, 1, 78, 22),
                Tutorial::VisitGames,
                Screen::Games,
            )
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows: Vec<String> = (0..24)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    assert!(
        rows.iter()
            .any(|row| row.contains("the tour · [3] the games")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("[Enter] next: a taste of the dungeon")),
        "{rows:#?}"
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
                Tutorial::VisitGames,
                Screen::Games,
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
            .ends_with("[Enter] next: a taste of the dungeon ──  │"),
        "{rows:#?}"
    );
    assert!(breaker.trim_start().starts_with("│  ──"), "{rows:#?}");
}

/// The dungeon stop takes the page over the way the practice table does:
/// the header across the top, the crawl screen filling the rest, whole on a
/// default terminal. Its breaker asks for blows until the win.
#[test]
fn the_dungeon_stop_fills_the_page_under_its_header() {
    use crate::app::clubhouse::fight::{self, Fight};
    let draw = |fight: &Fight| {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let header =
                    tour_header(Tutorial::VisitDungeon, TableStop::Racked, fight.won()).unwrap();
                let page = header.draw_above(frame, Rect::new(1, 1, 78, 22));
                fight::draw(frame, page, fight, "mat");
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..24)
            .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
            .collect::<Vec<String>>()
    };
    let at = |rows: &[String], needle: &str| rows.iter().position(|row| row.contains(needle));

    let mut fight = Fight::new();
    let rows = draw(&fight);
    let breaker = at(&rows, "[Enter] fight ──").expect("the breaker asks for the fight");
    let hero = at(&rows, "mat the Slayer").expect("the panel is drawn");
    let monster = at(&rows, " fire dragon   ").expect("the monster list is drawn");
    let log = at(&rows, "A fire dragon comes into view.").expect("the log is drawn");
    assert!(
        breaker < hero && hero < monster && monster < log,
        "{rows:#?}"
    );
    // The panel takes the right of the page, the view of the level the left.
    assert_eq!(rows[hero].find("mat the Slayer"), Some(41), "{rows:#?}");
    assert!(rows[log].starts_with(" A fire dragon"), "{rows:#?}");

    while !fight.won() {
        fight.strike();
    }
    let rows = draw(&fight);
    assert!(
        at(&rows, "[Enter] next: the artboard ──").is_some(),
        "{rows:#?}"
    );
    assert!(
        at(&rows, "You kill the fire dragon!").is_some(),
        "{rows:#?}"
    );
}
