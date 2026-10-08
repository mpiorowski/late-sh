use chrono::Utc;
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use ratatui::text::Line;

use super::*;
use crate::app::games::{
    cards::CardSuit::{Clubs, Diamonds, Hearts, Spades},
    pool_core::cue::ShotMode,
};
use crate::app::live::{
    state::LiveStripView,
    ui::{GAP, LIVE_STRIP_HEIGHT, live_strip_compact_line, live_strip_lines},
};
use crate::app::lobby::daily::{
    briscola::{self, DailyBriscolaState},
    cribbage::{CribbageMove, DailyCribbageState},
    games::DailyGame,
    gin::DailyGinState,
    live::MatchSummary,
    live_board::CARD_WON,
    pool::{DailyPoolState, PoolAimShare},
    std_deck::{self, Card, Rank},
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

fn strip<'a>(
    item: &'a DailyMatchItem,
    aim: Option<&'a PoolAimShare>,
    finish: Option<&'a str>,
) -> LiveStripView<'a> {
    LiveStripView::Match(MatchStripView {
        view: LiveView {
            item,
            board: &item.board,
            aim,
        },
        finish,
    })
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
    let lines = live_strip_lines(
        WIDTH,
        &strip(&item, None, None),
        BACKGROUND,
    );
    let text: Vec<String> = lines.iter().map(line_text).collect();

    assert!(text[1].ends_with("eggy · weslin"), "{}", text[1]);
    assert!(text[2].ends_with("Eight-Ball · shot 0"), "{}", text[2]);
    assert!(text[3].ends_with("waiting for the break"), "{}", text[3]);
    assert!(
        text[6].ends_with("press o or click to watch"),
        "{}",
        text[6]
    );
    let key = lines[6]
        .spans
        .iter()
        .find(|span| span.content == "o")
        .expect("the key is its own span");
    assert_eq!(key.style.fg, Some(theme::AMBER_DIM()), "the key stands out");
    assert!(
        text[8].starts_with("── live ─"),
        "the rule parts the strip from the chat: {}",
        text[8]
    );

    let shot = aim();
    let aiming = live_strip_lines(
        WIDTH,
        &strip(&item, Some(&shot), None),
        BACKGROUND,
    );
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
    assert!(line_text(&done[6]).ends_with("press ctrl+g to play"));
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

#[test]
fn backgammon_words_mark_each_players_colour_and_never_touch_the_board() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut state = super::super::backgammon::DailyBackgammonState::new(eggy, weslin);
    state.white = weslin;
    state.red = eggy;
    let summary = MatchSummary::of(
        DailyGame::Backgammon,
        &serde_json::to_value(&state).unwrap(),
    )
    .unwrap();
    let item = DailyMatchItem {
        game: DailyGame::Backgammon,
        turn_user_id: Some(weslin),
        board: summary.board,
        ..pool_item()
    };

    let lines = live_strip_lines(
        WIDTH,
        &strip(&item, None, None),
        BACKGROUND,
    );
    let text: Vec<String> = lines.iter().map(line_text).collect();

    // The words start past the picture column and its gap, on every row.
    let words_at = usize::from(PICTURE_COLS + GAP);
    assert_eq!(
        &text[1].chars().skip(words_at).collect::<String>(),
        "● eggy · ● weslin"
    );
    assert_eq!(
        &text[2].chars().skip(words_at).collect::<String>(),
        "Backgammon · pips 167-167"
    );
    let marks: Vec<_> = lines[1]
        .spans
        .iter()
        .filter(|span| span.content == "● ")
        .map(|span| span.style.fg)
        .collect();
    assert_eq!(
        marks,
        vec![
            Some(crate::app::lobby::daily::live_board::BG_RED),
            Some(crate::app::lobby::daily::live_board::BG_WHITE),
        ],
        "eggy plays red, weslin white"
    );
}

#[test]
fn gin_says_how_the_last_hand_ended_once_the_next_is_dealt() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut rng = StdRng::seed_from_u64(3);
    let mut state = DailyGinState::new(eggy, weslin, &mut rng);
    // eggy deals, so weslin plays first, one draw from a knock on an ace.
    state.seats = [eggy, weslin];
    let c = Card::new;
    let mut deal = vec![
        // weslin: three melds and the ace.
        c(Rank::Three, Hearts),
        c(Rank::Four, Hearts),
        c(Rank::Five, Hearts),
        c(Rank::Eight, Spades),
        c(Rank::Eight, Diamonds),
        c(Rank::Eight, Clubs),
        c(Rank::Jack, Clubs),
        c(Rank::Queen, Clubs),
        c(Rank::King, Clubs),
        c(Rank::Ace, Diamonds),
        // eggy: 6♥ and 7♥ lay off on the run, 59 stay in hand.
        c(Rank::Six, Hearts),
        c(Rank::Seven, Hearts),
        c(Rank::Nine, Spades),
        c(Rank::Nine, Diamonds),
        c(Rank::Two, Diamonds),
        c(Rank::Four, Clubs),
        c(Rank::Five, Clubs),
        c(Rank::Ten, Diamonds),
        c(Rank::King, Diamonds),
        c(Rank::Queen, Hearts),
        // The upcard, then the top of the stock.
        c(Rank::Three, Spades),
        c(Rank::Two, Clubs),
    ];
    let rest: Vec<Card> = std_deck::fresh_deck()
        .into_iter()
        .filter(|card| !deal.contains(card))
        .collect();
    deal.extend(rest);
    state.deals = vec![deal];
    state.apply_move(GinMove::Draw(Pile::Stock)).unwrap();
    state
        .apply_move(GinMove::Discard {
            card: c(Rank::Two, Clubs),
            knock: true,
        })
        .unwrap();
    // The service deals the next hand in the same write as the knock.
    state.deal_next(&mut rng).unwrap();
    let summary =
        MatchSummary::of(DailyGame::GinRummy, &serde_json::to_value(&state).unwrap()).unwrap();
    let item = DailyMatchItem {
        game: DailyGame::GinRummy,
        turn_user_id: Some(eggy),
        move_count: summary.move_count,
        board: summary.board,
        ..pool_item()
    };

    let lines = live_strip_lines(
        WIDTH,
        &strip(&item, None, None),
        BACKGROUND,
    );
    let text: Vec<String> = lines.iter().map(line_text).collect();

    let words_at = usize::from(PICTURE_COLS + GAP);
    assert_eq!(
        &text[2].chars().skip(words_at).collect::<String>(),
        "Gin Rummy · 0-58 · hand 2"
    );
    assert_eq!(
        &text[3].chars().skip(words_at).collect::<String>(),
        "weslin knocked · +58"
    );
}

// ── The card games, challenger in seat 1 ──────────────────────
//
// The coin flip put eggy, the challenger, in seat 1. The picture still
// draws eggy on top and every pair still reads eggy first.

/// A match as the snapshot carries it, read off `state` like `publish` does.
fn card_item(game: DailyGame, state: &impl serde::Serialize, turn: Uuid) -> DailyMatchItem {
    let summary = MatchSummary::of(game, &serde_json::to_value(state).unwrap()).unwrap();
    DailyMatchItem {
        game,
        turn_user_id: Some(turn),
        move_count: summary.move_count,
        board: summary.board,
        ..pool_item()
    }
}

/// One strip row: the picture in its column, the words past the gap.
fn row(picture: &str, words: &str) -> String {
    let column = usize::from(PICTURE_COLS + GAP);
    format!("{picture:<column$}{words}").trim_end().to_string()
}

/// The strip's picture rows as text, without the rule under them.
fn strip_rows(lines: &[Line<'_>]) -> Vec<String> {
    lines[..usize::from(PICTURE_ROWS)]
        .iter()
        .map(|line| line_text(line).trim_end().to_string())
        .collect()
}

#[test]
fn briscola_shows_the_trick_the_follower_took_with_the_challenger_on_top() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let heart = |rank| briscola::Card { suit: Hearts, rank };
    let (lead, answer, trump) = (
        heart(briscola::Rank::King),
        heart(briscola::Rank::Ace),
        heart(briscola::Rank::Six),
    );
    let mut state = DailyBriscolaState::new(eggy, weslin);
    state.seats = [weslin, eggy];
    // The deck is the two opening hands, then the trump: weslin holds the
    // king, eggy the ace that takes it, and the six turns up.
    for (index, card) in [
        (0, lead),
        (briscola::HAND, answer),
        (2 * briscola::HAND, trump),
    ] {
        let at = state
            .deck
            .iter()
            .position(|held| *held == card)
            .expect("a whole deck holds every card");
        state.deck.swap(index, at);
    }
    state.apply_play(lead).unwrap();
    state.apply_play(answer).unwrap();
    let item = card_item(DailyGame::Briscola, &state, eggy);

    let lines = live_strip_lines(
        WIDTH,
        &strip(&item, None, None),
        BACKGROUND,
    );

    assert_eq!(
        strip_rows(&lines),
        vec![
            row("      ░░ ░░ ░░", ""),
            row("", "eggy · weslin"),
            row("╭─╭───╮  ╭───╮ ╭───╮", "Briscola · 15-0"),
            row("│░│6 ♥│  │K ♥│ │A ♥│", "eggy played A♥"),
            row("╰─╰───╯  ╰───╯ ╰───╯", ""),
            row("32 left", ""),
            row("", "press o or click to watch"),
            row("      ░░ ░░ ░░", ""),
        ]
    );
    // The trump, the lead, the answer: only the card that took it is gold.
    let gold: Vec<bool> = lines[2]
        .spans
        .iter()
        .filter(|span| span.content == "╭───╮")
        .map(|span| span.style.fg == Some(CARD_WON))
        .collect();
    assert_eq!(gold, vec![false, false, true]);
}

#[test]
fn gin_shows_the_pile_and_each_hand_with_the_challenger_on_top() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut state = DailyGinState::new(eggy, weslin, &mut StdRng::seed_from_u64(3));
    // weslin deals, so eggy plays first. A deck in order turns up 8♦ and
    // opens the stock with 9♦.
    state.seats = [weslin, eggy];
    state.deals = vec![std_deck::fresh_deck()];
    state.apply_move(GinMove::Draw(Pile::Stock)).unwrap();
    state
        .apply_move(GinMove::Discard {
            card: Card::new(Rank::Nine, Diamonds),
            knock: false,
        })
        .unwrap();
    state.apply_move(GinMove::Draw(Pile::Stock)).unwrap();
    let item = card_item(DailyGame::GinRummy, &state, weslin);

    let lines = live_strip_lines(
        WIDTH,
        &strip(&item, None, None),
        BACKGROUND,
    );

    // eggy holds ten on top, weslin eleven below with the card just drawn.
    assert_eq!(
        strip_rows(&lines),
        vec![
            row("     ▏▏▏▏▏▏▏▏▏▏", ""),
            row("", "eggy · weslin"),
            row("   ╭───╮   ╭─╭───╮", "Gin Rummy · 0-0 · hand 1"),
            row("   │29 │   │ │9 ♦│", "weslin drew from the stock"),
            row("   ╰───╯   ╰─╰───╯", ""),
            row("   stock      pile", ""),
            row("", "press o or click to watch"),
            row("     ▏▏▏▏▏▏▏▏▏▏▏", ""),
        ]
    );
}

#[test]
fn cribbage_pegs_each_lane_with_the_challenger_on_top() {
    let (eggy, weslin) = (Uuid::from_u128(1), Uuid::from_u128(2));
    let mut state = DailyCribbageState::new(eggy, weslin, &mut StdRng::seed_from_u64(3));
    // weslin deals, so eggy is pone: pone's six, the dealer's six, the
    // starter, then the rest of the deck.
    state.seats = [weslin, eggy];
    let c = Card::new;
    let mut deal = vec![
        c(Rank::Five, Hearts),
        c(Rank::Five, Diamonds),
        c(Rank::Ten, Clubs),
        c(Rank::Four, Spades),
        c(Rank::King, Spades),
        c(Rank::Queen, Spades),
        c(Rank::Five, Clubs),
        c(Rank::Six, Hearts),
        c(Rank::Three, Diamonds),
        c(Rank::Two, Clubs),
        c(Rank::King, Hearts),
        c(Rank::Queen, Hearts),
        c(Rank::Nine, Spades),
    ];
    let rest: Vec<Card> = std_deck::fresh_deck()
        .into_iter()
        .filter(|card| !deal.contains(card))
        .collect();
    deal.extend(rest);
    state.deals = vec![deal];
    for played in [
        CribbageMove::Discard([c(Rank::King, Spades), c(Rank::Queen, Spades)]),
        CribbageMove::Discard([c(Rank::King, Hearts), c(Rank::Queen, Hearts)]),
        CribbageMove::Play(c(Rank::Five, Hearts)),
        // weslin pairs it for 2, then eggy makes fifteen for 2 and pair
        // royal for 6: eggy's back peg on 2, front peg on 8.
        CribbageMove::Play(c(Rank::Five, Clubs)),
        CribbageMove::Play(c(Rank::Five, Diamonds)),
    ] {
        state.apply_move(played).unwrap();
    }
    let item = card_item(DailyGame::Cribbage, &state, weslin);

    let lines = live_strip_lines(
        WIDTH,
        &strip(&item, None, None),
        BACKGROUND,
    );

    let empty = "·".repeat(20);
    let home = format!("{empty}○");
    assert_eq!(
        strip_rows(&lines),
        vec![
            row("·●·····●············", ""),
            row("·●··················", "● eggy · ● weslin"),
            row("", "Cribbage · 8-2 · hand 1"),
            row(&empty, "weslin to move"),
            row(&empty, ""),
            row("", ""),
            row(&home, "press o or click to watch"),
            row(&home, ""),
        ]
    );
}
