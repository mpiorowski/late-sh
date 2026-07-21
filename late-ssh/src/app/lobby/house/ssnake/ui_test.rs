use super::*;
use crate::app::lobby::house::ssnake::levels::open_test_arena;
use crate::app::lobby::house::ssnake::svc::SsnakePlayerSnapshot;
use std::sync::Arc;

fn empty_player() -> SsnakePlayerSnapshot {
    SsnakePlayerSnapshot {
        body: Vec::new(),
        motion: Motion::Idle,
        lives: 0,
        score: 0,
        eliminated: false,
        in_round: false,
    }
}

fn snapshot_with_level(level: SsnakeLevel) -> SsnakeSnapshot {
    SsnakeSnapshot {
        room_id: Uuid::nil(),
        seats: [None; MAX_SEATS],
        seat_limit: 2,
        level: Some(Arc::new(level)),
        arena_choice: "random arena".to_string(),
        players: [
            SsnakePlayerSnapshot {
                body: vec![Pos { x: 2, y: 2 }, Pos { x: 3, y: 2 }],
                motion: Motion::Moving(crate::app::lobby::house::ssnake::state::Direction::Left),
                lives: 3,
                score: 0,
                eliminated: false,
                in_round: true,
            },
            SsnakePlayerSnapshot {
                body: vec![Pos { x: 5, y: 5 }],
                motion: Motion::Idle,
                lives: 3,
                score: 0,
                eliminated: false,
                in_round: true,
            },
            empty_player(),
            empty_player(),
        ],
        point: Some(Pos { x: 7, y: 7 }),
        life_point: false,
        points_left: 5,
        phase: SsnakePhase::Running,
        outcome: None,
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
fn hearts_show_remaining_and_lost_lives() {
    // Test arena starts with 3 lives; 2 left = 2 filled + 1 hollow.
    let snapshot = snapshot_with_level(open_test_arena(30, 20));
    let spans = heart_spans(2, &snapshot);
    assert_eq!(spans[0].content, "♥ ♥ ");
    assert_eq!(spans[1].content, "♡ ");

    // Extra lives from life points never render negative hollow hearts.
    let spans = heart_spans(5, &snapshot);
    assert_eq!(spans[0].content, "♥ ♥ ♥ ♥ ♥ ");
    assert_eq!(spans[1].content, "");

    // Absurd totals collapse to a count.
    let spans = heart_spans(12, &snapshot);
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].content, "♥ x12");
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
