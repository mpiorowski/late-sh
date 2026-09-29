use chrono::Utc;
use uuid::Uuid;

use super::*;
use crate::app::games::pool_core::{cue::ShotMode, rules::PoolRules};
use crate::app::lobby::daily::{
    games::DailyGame,
    live::MatchSummary,
    pool::{DailyPoolState, PoolAimShare},
    svc::DailyMatchItem,
};

const WIDTH: u16 = 80;
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
        board: Some(summary.board),
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

fn strip<'a>(
    item: &'a DailyMatchItem,
    aim: Option<&'a PoolAimShare>,
    finish: Option<&'a str>,
) -> LiveStripView<'a> {
    LiveStripView {
        view: LiveView {
            item,
            board: item.board.as_ref().unwrap(),
            aim,
        },
        finish,
    }
}

#[test]
fn strip_height_is_fixed_whatever_it_shows() {
    let item = pool_item();
    let shot = aim();
    for strip in [
        strip(&item, None, None),
        strip(&item, Some(&shot), None),
        strip(&item, None, Some("weslin won · eight ball · +400 chips")),
    ] {
        assert_eq!(
            live_strip_lines(WIDTH, &strip, BACKGROUND).len(),
            LIVE_STRIP_HEIGHT as usize
        );
    }
}

#[test]
fn the_words_sit_beside_the_board() {
    let item = pool_item();
    let lines = live_strip_lines(WIDTH, &strip(&item, None, None), BACKGROUND);
    let text: Vec<String> = lines.iter().map(line_text).collect();

    assert!(text[1].ends_with("eggy · weslin"), "{}", text[1]);
    assert!(text[2].ends_with("Eight-Ball · shot 0"), "{}", text[2]);
    assert!(text[3].ends_with("waiting for the break"), "{}", text[3]);
    assert!(text[6].ends_with("o or click to watch"), "{}", text[6]);
    assert!(
        text[8].starts_with("── live ─"),
        "the rule parts the strip from the chat: {}",
        text[8]
    );

    let shot = aim();
    let aiming = live_strip_lines(WIDTH, &strip(&item, Some(&shot), None), BACKGROUND);
    assert!(
        line_text(&aiming[3]).ends_with("eggy is lining up a shot"),
        "{}",
        line_text(&aiming[3])
    );

    let done = live_strip_lines(
        WIDTH,
        &strip(&item, None, Some("weslin won · eight ball · +400 chips")),
        BACKGROUND,
    );
    assert!(line_text(&done[3]).ends_with("weslin won · eight ball · +400 chips"));
    assert!(line_text(&done[6]).ends_with("ctrl+g to play"));
}

#[test]
fn the_card_picks_the_form_and_keeps_rows_for_the_messages() {
    let card = |width, height| Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let (size, strip, rest) = fit_live_strip(card(80, 30)).unwrap();
    assert_eq!(size, StripSize::Full);
    assert_eq!(strip.height, LIVE_STRIP_HEIGHT);
    assert_eq!(rest.height, 30 - LIVE_STRIP_HEIGHT);

    let (size, strip, _) = fit_live_strip(card(80, 14)).unwrap();
    assert_eq!(
        size,
        StripSize::Compact,
        "a short card gets the one-row form"
    );
    assert_eq!(strip.height, LIVE_STRIP_COMPACT_HEIGHT);

    let (size, _, _) = fit_live_strip(card(40, 30)).unwrap();
    assert_eq!(size, StripSize::Compact, "a narrow card too");

    assert!(fit_live_strip(card(80, 4)).is_none(), "no room at all");
}

#[test]
fn the_compact_line_names_the_game_and_the_players() {
    let item = pool_item();
    let live = line_text(&live_strip_compact_line(WIDTH, &strip(&item, None, None)));
    assert!(live.starts_with("── live "), "{live}");
    assert!(live.ends_with("8ball eggy v weslin"), "{live}");

    let done = line_text(&live_strip_compact_line(
        WIDTH,
        &strip(&item, None, Some("a draw")),
    ));
    assert!(done.ends_with("a draw"), "{done}");
}
