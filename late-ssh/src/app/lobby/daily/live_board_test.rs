use chrono::Utc;
use uuid::Uuid;

use super::*;
use crate::app::games::pool_core::{cue::ShotMode, rules::PoolRules};
use crate::app::lobby::daily::{
    games::DailyGame,
    live::MatchSummary,
    pool::{DailyPoolState, default_table},
    svc::DailyMatchItem,
};

const WIDTH: u16 = 21;
const BACKGROUND: Rgb = [0, 0, 0];

fn line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

fn pool_item() -> DailyMatchItem {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let state =
        serde_json::to_value(DailyPoolState::new(PoolRules::EightBall, eggy, weslin)).unwrap();
    let summary = MatchSummary::of(DailyGame::EightBall, &state).unwrap();
    DailyMatchItem {
        id: Uuid::from_u128(9),
        game: DailyGame::EightBall,
        challenger_id: eggy,
        challenger_username: Some("eggy".to_string()),
        opponent_id: weslin,
        opponent_username: Some("weslin".to_string()),
        white_id: None,
        black_id: None,
        turn_user_id: Some(eggy),
        turn_deadline_at: None,
        move_count: 0,
        updated: Utc::now(),
        board: summary.board,
    }
}

fn aim() -> PoolAimShare {
    PoolAimShare {
        azimuth: std::f64::consts::PI,
        tip: [0.0, 0.0],
        pull: 0.5,
        mode: ShotMode::Aim,
        place: None,
        called_pocket: None,
    }
}

fn has_pixel(lines: &[Line<'_>], colour: Rgb) -> bool {
    let want = Color::Rgb(colour[0], colour[1], colour[2]);
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .any(|span| span.style.fg == Some(want) || span.style.bg == Some(want))
}

#[test]
fn a_fresh_aim_draws_the_stick() {
    let item = pool_item();
    let board = item.board.clone();

    let still = board_lines(
        WIDTH,
        &LiveView {
            item: &item,
            board: &board,
            aim: None,
        },
        BACKGROUND,
    );
    assert_eq!(still.len(), BOARD_ROWS as usize);
    assert!(!has_pixel(&still, CUE_STICK));

    let shot = aim();
    let aiming = board_lines(
        WIDTH,
        &LiveView {
            item: &item,
            board: &board,
            aim: Some(&shot),
        },
        BACKGROUND,
    );
    assert!(
        has_pixel(&aiming, CUE_STICK),
        "the stick shows while they aim"
    );
}

#[test]
fn the_cloth_and_its_rail_fit_the_board_for_every_table() {
    let px_h = BOARD_ROWS * 2;
    for rules in [
        PoolRules::EightBall,
        PoolRules::NineBall,
        PoolRules::Snooker,
    ] {
        let cloth = fit_cloth(default_table(rules), WIDTH, px_h);
        assert!(
            cloth.x0 - 1 >= 0 && cloth.y0 - 1 >= 0,
            "{rules:?}: {cloth:?}"
        );
        assert!(cloth.x0 + cloth.w < WIDTH as i32, "{rules:?}: {cloth:?}");
        assert!(cloth.y0 + cloth.h < px_h as i32, "{rules:?}: {cloth:?}");
        // Wider than tall, as a table is.
        assert!(cloth.w > cloth.h, "{rules:?}: {cloth:?}");
    }
}

#[test]
fn the_one_row_form_names_the_game_and_the_players_or_the_shooter() {
    let item = pool_item();
    let board = item.board.clone();
    let still = LiveView {
        item: &item,
        board: &board,
        aim: None,
    };
    assert_eq!(
        line_text(&live_compact_line(WIDTH, &still)),
        "8ball eggy v weslin"
    );

    let shot = aim();
    let aiming = LiveView {
        item: &item,
        board: &board,
        aim: Some(&shot),
    };
    // eggy is on the move, so eggy is the one holding the cue.
    assert_eq!(
        line_text(&live_compact_line(WIDTH, &aiming)),
        "eggy is aiming"
    );
}
