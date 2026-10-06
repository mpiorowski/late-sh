use chrono::Utc;
use cozy_chess::Board;
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::*;
use crate::app::games::{
    chess_core::rules::random_chess960_board,
    pool_core::{cue::ShotMode, rules::PoolRules},
};
use crate::app::live::ui::{PICTURE_COLS, PICTURE_ROWS};
use crate::app::lobby::daily::{
    cribbage::DailyCribbageState,
    games::DailyGame,
    gin::DailyGinState,
    live::MatchSummary,
    pool::{DailyPoolState, default_table},
    svc::{DailyChessState, DailyMatchItem},
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
        assert!(cloth.x0 > 0 && cloth.y0 > 0, "{rules:?}: {cloth:?}");
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

fn item_of(game: DailyGame, state: serde_json::Value) -> DailyMatchItem {
    let summary = MatchSummary::of(game, &state).unwrap();
    DailyMatchItem {
        game,
        move_count: summary.move_count,
        board: summary.board,
        ..pool_item()
    }
}

fn picture(item: &DailyMatchItem) -> Vec<Line<'static>> {
    board_lines(
        WIDTH,
        &LiveView {
            item,
            board: &item.board,
            aim: None,
        },
        BACKGROUND,
    )
}

/// A roster game's state as its claim builds it. Exhaustive, so a new game
/// has to be drawn here before the fit test below compiles.
fn opening_state(game: DailyGame) -> serde_json::Value {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut rng = StdRng::seed_from_u64(7);
    match game {
        DailyGame::Chess => {
            serde_json::to_value(DailyChessState::new(eggy, weslin, &Board::default()))
        }
        DailyGame::Chess960 => {
            serde_json::to_value(DailyChessState::new(eggy, weslin, &random_chess960_board()))
        }
        DailyGame::Battleship => {
            serde_json::to_value(battleship::DailyBattleshipState::new(eggy, weslin))
        }
        DailyGame::ConnectFour => {
            serde_json::to_value(connect4::DailyConnect4State::new(eggy, weslin))
        }
        DailyGame::Reversi => serde_json::to_value(reversi::DailyReversiState::new(eggy, weslin)),
        DailyGame::Checkers => {
            serde_json::to_value(checkers::DailyCheckersState::new(eggy, weslin))
        }
        DailyGame::Backgammon => {
            serde_json::to_value(backgammon::DailyBackgammonState::new(eggy, weslin))
        }
        DailyGame::Briscola => {
            serde_json::to_value(briscola::DailyBriscolaState::new(eggy, weslin))
        }
        DailyGame::Cribbage => {
            serde_json::to_value(DailyCribbageState::new(eggy, weslin, &mut rng))
        }
        DailyGame::GinRummy => serde_json::to_value(DailyGinState::new(eggy, weslin, &mut rng)),
        DailyGame::EightBall => {
            serde_json::to_value(DailyPoolState::new(PoolRules::EightBall, eggy, weslin))
        }
        DailyGame::NineBall => {
            serde_json::to_value(DailyPoolState::new(PoolRules::NineBall, eggy, weslin))
        }
        DailyGame::Snooker => {
            serde_json::to_value(DailyPoolState::new(PoolRules::Snooker, eggy, weslin))
        }
    }
    .unwrap()
}

#[test]
fn every_games_picture_fits_the_strips_column() {
    for game in DailyGame::ALL {
        let item = item_of(game, opening_state(game));
        let lines = board_lines(
            PICTURE_COLS,
            &LiveView {
                item: &item,
                board: &item.board,
                aim: None,
            },
            BACKGROUND,
        );
        assert!(
            lines.len() <= PICTURE_ROWS as usize,
            "{game:?} is {} rows tall",
            lines.len()
        );
        for line in &lines {
            assert!(
                line.width() <= usize::from(PICTURE_COLS),
                "{game:?} runs past its column: {:?}",
                line_text(line)
            );
        }
    }
}

#[test]
fn backgammon_paints_the_opening_position_with_the_roll_on_the_movers_side() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut state = backgammon::DailyBackgammonState::new(eggy, weslin);
    state.white = eggy;
    state.red = weslin;
    state.next_roll = Some([6, 2]);
    let item = item_of(DailyGame::Backgammon, serde_json::to_value(&state).unwrap());

    let text: Vec<String> = picture(&item).iter().map(line_text).collect();

    // White's seat: the 13 to 24 points across the top, 12 to 1 along the
    // bottom. Five-stacks show their count in the innermost row, empty
    // points taper to a half-block tip, white rolls on its side (row 4).
    assert_eq!(
        text,
        vec![
            "   ●   ●  ●    ●   ",
            "   ●   ●  ●    ●   ",
            "   5▀▀▀●▀ 5▀▀▀▀▀   ",
            "                   ",
            "    6  2           ",
            "   5▄▄▄●▄ 5▄▄▄▄▄   ",
            "   ●   ●  ●    ●   ",
            "   ●   ●  ●    ●   ",
        ]
    );
}

#[test]
fn battleship_charts_the_shots_and_never_a_ship() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut state = battleship::DailyBattleshipState::new(eggy, weslin);
    let ship: Vec<usize> = state.side(1).ships[0]
        .cells
        .iter()
        .map(|&cell| cell as usize)
        .collect();
    let water = (0..battleship::CELLS)
        .find(|cell| {
            !state
                .side(1)
                .ships
                .iter()
                .any(|ship| ship.cells.contains(&(*cell as u8)))
        })
        .unwrap();
    // eggy (side 0, the challenger) finds the ship, then misses.
    let at = Utc::now();
    state.apply_shot(0, ship[0], at).unwrap();
    state
        .apply_shot(0, water, at + chrono::Duration::seconds(1))
        .unwrap();
    let item = item_of(DailyGame::Battleship, serde_json::to_value(&state).unwrap());

    let lines = picture(&item);
    // The right-hand sea is weslin's waters, charted by eggy's shots.
    let colour = |cell: usize| -> Color {
        let (row, col) = (cell / battleship::GRID, cell % battleship::GRID);
        let line = &lines[1 + row / 2];
        let mut x = 0;
        let span = line
            .spans
            .iter()
            .find(|span| {
                let here = x;
                x += span.width();
                here == battleship::GRID + 1 + col && span.width() == 1
            })
            .expect("a cell under every column");
        if row % 2 == 0 {
            span.style.fg.unwrap()
        } else {
            span.style.bg.unwrap()
        }
    };

    assert_eq!(colour(ship[0]), HIT);
    assert_eq!(colour(water), LAST_SHOT, "the newest shot stands out");
    for &unshot in &ship[1..] {
        assert!(
            [SEA_A, SEA_B].contains(&colour(unshot)),
            "an unshot ship cell is only sea"
        );
    }
}
