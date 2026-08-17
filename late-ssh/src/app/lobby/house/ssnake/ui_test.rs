use super::*;
use crate::app::lobby::house::ssnake::levels::open_test_arena;

use std::sync::Arc;

fn empty_player() -> SsnakePlayerSnapshot {
    SsnakePlayerSnapshot {
        body: Vec::new(),
        motion: Motion::Idle,
        chips: 0,
        last_chip: None,
        seated: false,
    }
}

fn snapshot_with_level(level: SsnakeLevel) -> SsnakeSnapshot {
    SsnakeSnapshot {
        room_id: Uuid::nil(),
        seats: [None; MAX_SEATS],
        seat_limit: 2,
        level: Some(Arc::new(level)),
        players: [
            SsnakePlayerSnapshot {
                body: vec![Pos { x: 2, y: 2 }, Pos { x: 3, y: 2 }],
                motion: Motion::Moving(crate::app::lobby::house::ssnake::state::Direction::Left),
                chips: 120,
                last_chip: Some(SsnakeChipKind::Food),
                seated: true,
            },
            SsnakePlayerSnapshot {
                body: vec![Pos { x: 5, y: 5 }],
                motion: Motion::Idle,
                chips: 0,
                last_chip: None,
                seated: true,
            },
            empty_player(),
            empty_player(),
            empty_player(),
        ],
        point: Some(Pos { x: 7, y: 7 }),
        bonus_food: false,
        food_wall_edges: 0,
        points_left: 5,
        phase: SsnakePhase::Running,
        moving_snakes: 1,
        freeze_millis_left: 0,
        skip_votes: [false; MAX_SEATS],
        skip_cooldown_millis_left: 0,
        status_message: "test".to_string(),
        speed_label: "classic".to_string(),
        tick_count: 1,
    }
}

#[test]
fn board_lines_cover_full_arena_width() {
    let level = open_test_arena(30, 21);
    let snapshot = snapshot_with_level(level.clone());
    let lines = board_lines(&snapshot, &level, 1);
    assert_eq!(lines.len(), 11, "21 rows pack into 11 half-block lines");
    for line in &lines {
        let width: usize = line
            .spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        assert_eq!(width, level.width);
    }
}

#[test]
fn zoomed_board_doubles_every_cell() {
    let level = open_test_arena(30, 21);
    let snapshot = snapshot_with_level(level.clone());
    let lines = board_lines(&snapshot, &level, 2);
    assert_eq!(lines.len(), 21, "42 virtual rows pack into 21 lines");
    for line in &lines {
        let width: usize = line
            .spans
            .iter()
            .map(|span| span.content.chars().count())
            .sum();
        assert_eq!(width, level.width * 2);
    }
    // Each terminal line covers exactly one arena row at 2x, so the
    // half-block fg and bg agree everywhere; the green head at (2, 2)
    // spans virtual columns 4-5 on line 2.
    let line = &lines[2];
    let mut x = 0usize;
    let mut found = false;
    for span in &line.spans {
        let len = span.content.chars().count();
        if x <= 4 && 4 < x + len {
            assert_eq!(span.style.fg, Some(GREEN_HEAD));
            assert_eq!(span.style.bg, Some(GREEN_HEAD));
            found = true;
        }
        x += len;
    }
    assert!(found, "head span missing on the zoomed line");
}

#[test]
fn zoom_asks_for_taller_pane_only_when_it_fits() {
    let level = open_test_arena(30, 21);
    let wide = Rect::new(0, 0, 120, 50);
    let narrow = Rect::new(0, 0, 80, 50);
    let short = Rect::new(0, 0, 120, 30);
    assert!(zoom_eligible(&level, wide));
    assert!(!zoom_eligible(&level, narrow), "2x + sidebar needs 90 cols");
    assert!(!zoom_eligible(&level, short), "chat must keep its floor");
}

#[test]
fn earnings_line_signs_the_tally_and_names_the_motion() {
    let snapshot = snapshot_with_level(open_test_arena(30, 20));

    // "pending", not "chips": none of it is spendable until the player
    // stands up and the seat is banked.
    let moving = earnings_line(&snapshot.players[0], false);
    assert_eq!(moving.spans[0].content, "  +120 pending  ");
    assert_eq!(moving.spans[1].content, "moving");

    // A parked snake is called out: it pays nobody, including itself.
    let parked = earnings_line(&snapshot.players[1], false);
    assert_eq!(parked.spans[0].content, "  +0 pending  ");
    assert_eq!(parked.spans[1].content, "parked");

    let broke = earnings_line(
        &SsnakePlayerSnapshot {
            chips: -30,
            motion: Motion::Dying,
            ..snapshot.players[0].clone()
        },
        false,
    );
    assert_eq!(broke.spans[0].content, "  -30 pending  ");
    assert_eq!(broke.spans[1].content, "crashed");
}

#[test]
fn payout_labels_track_the_moving_snake_count() {
    let mut snapshot = snapshot_with_level(open_test_arena(30, 20));
    assert_eq!(snapshot.food_chips(), SSNAKE_FOOD_CHIPS);
    snapshot.moving_snakes = 3;
    assert_eq!(snapshot.food_chips(), SSNAKE_FOOD_CHIPS * 3);
    assert_eq!(snapshot.clear_chips(), SSNAKE_CLEAR_CHIPS * 3);
    // An empty board still advertises the lone rate, never zero.
    snapshot.moving_snakes = 0;
    assert_eq!(snapshot.food_chips(), SSNAKE_FOOD_CHIPS);
}

#[test]
fn food_breakdown_lists_only_the_terms_in_play() {
    let mut snapshot = snapshot_with_level(open_test_arena(30, 20));
    assert_eq!(
        food_breakdown(&snapshot),
        format!("  {SSNAKE_FOOD_CHIPS}"),
        "open floor, no pink, one snake: nothing to explain"
    );

    snapshot.food_wall_edges = 2;
    snapshot.bonus_food = true;
    snapshot.moving_snakes = 4;
    assert_eq!(
        food_breakdown(&snapshot),
        format!(
            "  {SSNAKE_FOOD_CHIPS} +{} edge ×{SSNAKE_BONUS_FOOD_MULTIPLIER} pink ×4 moving",
            SSNAKE_EDGE_BONUS_CHIPS * 2
        )
    );
}

/// Flatten a rendered line back to plain text for assertions.
fn text_of(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

#[test]
fn the_control_block_lists_every_key_seated_or_not() {
    for seated in [false, true] {
        let rendered: Vec<String> = control_block(seated).iter().map(text_of).collect();
        let joined = rendered.join("\n");
        // The same rows in the same order either way, so the block never
        // reshuffles under someone reading it.
        assert!(joined.contains("s / space"), "sit key missing: {joined}");
        assert!(joined.contains("arrows/wasd"), "steer keys missing");
        assert!(joined.contains("vote skip"), "skip-vote key missing");
        assert!(joined.contains("stand up"), "stand-up key missing");
        assert!(joined.contains("leave table"), "leave key missing");
        assert_eq!(rendered.len(), 8, "header, five keys, two note lines");
    }
}

#[test]
fn sidebar_lines_never_wrap() {
    // `sidebar_height` counts lines to size the pane, so a line wide enough
    // to wrap would silently cost a row and push the bottom off screen.
    for seated in [false, true] {
        for line in control_block(seated) {
            let width = text_of(&line).chars().count();
            assert!(
                width <= SIDEBAR_TEXT_WIDTH,
                "{width} cols: {:?}",
                text_of(&line)
            );
        }
    }
}

#[test]
fn long_names_are_cut_rather_than_wrapped() {
    assert_eq!(fit("bob", 10), "bob");
    assert_eq!(fit("0123456789", 10), "0123456789");
    assert_eq!(fit("01234567890", 10), "012345678…");
}

#[test]
fn the_title_prices_the_food_on_the_board_not_an_average() {
    let mut snapshot = snapshot_with_level(open_test_arena(30, 20));
    snapshot.points_left = 10;
    assert_eq!(
        status_text(&snapshot),
        format!("10 food left · this one: {SSNAKE_FOOD_CHIPS} chips"),
        "every food is priced on its own walls, so 'each' would be wrong"
    );
}

#[test]
fn skip_status_shows_the_tally_then_the_cooldown() {
    let mut snapshot = snapshot_with_level(open_test_arena(30, 20));
    assert!(
        skip_status(&snapshot).is_none(),
        "an empty table has nobody to reach consensus with"
    );

    snapshot.seats[0] = Some(Uuid::now_v7());
    snapshot.seats[1] = Some(Uuid::now_v7());
    assert_eq!(
        text_of(&skip_status(&snapshot).unwrap()).trim_end(),
        "Skip vote  0/2"
    );

    snapshot.skip_votes[0] = true;
    assert_eq!(
        text_of(&skip_status(&snapshot).unwrap()).trim_end(),
        "Skip vote  1/2"
    );

    // Once it fires, the tally gives way to the wait.
    snapshot.skip_votes = [false; MAX_SEATS];
    snapshot.skip_cooldown_millis_left = 41_200;
    assert_eq!(
        text_of(&skip_status(&snapshot).unwrap()).trim_end(),
        "Skip vote  in 42s"
    );
}

#[test]
fn controls_come_before_anything_that_could_be_clipped() {
    // The sidebar is an unscrolled paragraph: keys have to land inside the
    // first screenful or a newcomer never sees them.
    let lines = control_block(false);
    assert!(
        matches!(text_of(&lines[0]).as_str(), "Controls"),
        "the block leads with its own header"
    );
}

#[test]
fn only_the_last_food_pulses() {
    let mut snapshot = snapshot_with_level(open_test_arena(30, 20));

    // Plain and pink both hold a steady colour across the blink phase, so
    // the two bonuses can never be confused for one another.
    for tick in [0, 1] {
        snapshot.tick_count = tick;

        snapshot.bonus_food = false;
        assert_eq!(food_color(&snapshot), POINT, "plain food stays gold");

        snapshot.bonus_food = true;
        assert_eq!(food_color(&snapshot), BONUS_FOOD, "pink does not blink");
    }

    // The lap-ending food outranks pink and is the only thing that pulses.
    snapshot.points_left = 1;
    snapshot.tick_count = 0;
    assert_eq!(food_color(&snapshot), FINAL_FOOD);
    snapshot.tick_count = 1;
    assert_eq!(food_color(&snapshot), FINAL_FOOD_BLINK);
}

#[test]
fn the_hold_counts_down_on_the_board() {
    let mut snapshot = snapshot_with_level(open_test_arena(30, 20));
    // One clean second per step across the whole freeze window.
    for (millis, expected) in [
        (4_000, "3"),
        (3_500, "3"),
        (3_000, "2"),
        (2_001, "2"),
        (2_000, "1"),
        (1_001, "1"),
        (1_000, "GO"),
        (1, "GO"),
    ] {
        snapshot.freeze_millis_left = millis;
        assert_eq!(countdown_overlay(&snapshot).0, expected, "at {millis}ms");
    }

    snapshot.freeze_millis_left = 2_400;
    assert!(snapshot.is_frozen());
    assert_eq!(
        status_text(&snapshot),
        "New arena — get ready",
        "the number lives on the board, not in the title"
    );

    snapshot.freeze_millis_left = 0;
    assert!(!snapshot.is_frozen());
    assert!(status_text(&snapshot).contains("food left"));
}

#[test]
fn cell_colors_layer_snakes_over_floor() {
    let level = open_test_arena(30, 20);
    let snapshot = snapshot_with_level(level.clone());
    let colors = cell_colors(&snapshot, &level);
    assert_eq!(colors[2 * level.width + 2], GREEN_HEAD);
    assert_eq!(colors[2 * level.width + 3], GREEN_BODY);
    assert_eq!(colors[7 * level.width + 7], POINT);
    assert_eq!(colors[0], WALL);
    assert_eq!(colors[level.width + 1], ARENA_BG);
}
