use std::collections::HashMap;

use late_core::models::deadchannel_spawn::{Spawn, SpawnHit, SpawnKind, SpawnOutcome};
use rand::SeedableRng;
use rand::rngs::StdRng;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use super::super::data::{EMPTY_SEAT, GHOST_PORTRAIT, face};
use super::super::state::{AFTERMATH_MS, STRIKE_MS};
use super::{SCENE_HEIGHT, scene};
use crate::app::deadchannel::runner::state::{Look, PORTRAIT_WIDTH};
use crate::app::deadchannel::runner::svc::RunnerEntry;

fn howler(hits: Vec<SpawnHit>) -> Spawn {
    Spawn {
        id: Uuid::now_v7(),
        kind: SpawnKind::Howler,
        hits_needed: 3,
        hits,
        spawned_at_ms: 1_000,
        expires_at_ms: 181_000,
        ended_at_ms: None,
        outcome: None,
    }
}

fn hit(name: &str, at_ms: i64) -> SpawnHit {
    SpawnHit {
        user_id: Uuid::now_v7(),
        username: name.to_string(),
        at_ms,
    }
}

fn entry(seed: u64) -> RunnerEntry {
    RunnerEntry {
        look: Look::random(1, &mut StdRng::seed_from_u64(seed)),
        level: 1,
        peak_level: 1,
        marks: 0,
    }
}

fn text_of(line: &ratatui::text::Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

#[test]
fn every_face_and_seat_row_is_its_width() {
    for kind in [SpawnKind::Flicker, SpawnKind::Howler] {
        for row in face(kind) {
            // Nine cells: the face is placed by width, never reshaped.
            assert_eq!(UnicodeWidthStr::width(row), 9, "{kind:?} {row:?}");
        }
    }
    for row in EMPTY_SEAT.iter().chain(GHOST_PORTRAIT.iter()) {
        assert_eq!(UnicodeWidthStr::width(*row), PORTRAIT_WIDTH, "{row:?}");
    }
}

#[test]
fn the_scene_seats_the_hitters_and_offers_the_key() {
    let viewer = Uuid::now_v7();
    let tom = hit("tom", 10_000);
    let looks = HashMap::from([(tom.user_id, entry(1))]);
    let spawn = howler(vec![tom]);
    // Long after the hit: no strike on the lane.
    let drawn = scene(&spawn, None, viewer, &looks, 10_000 + STRIKE_MS);
    assert_eq!(drawn.rows.len(), SCENE_HEIGHT as usize);
    let rows: Vec<String> = drawn.rows.iter().map(text_of).collect();
    let portrait = looks[&spawn.hits[0].user_id].look.rows();
    // Tom's portrait in the first seat, two empty seats, a quiet lane,
    // the face, the words.
    assert_eq!(
        rows[0],
        format!(
            " {}  {}  {}         {}   a howler is in the channel.",
            portrait[0].piece.row,
            EMPTY_SEAT[0],
            EMPTY_SEAT[0],
            face(SpawnKind::Howler)[0]
        )
    );
    assert_eq!(
        rows[1],
        format!(
            " {}  {}  {}         {}   tom hit it. 2 to go.",
            portrait[1].piece.row,
            EMPTY_SEAT[1],
            EMPTY_SEAT[1],
            face(SpawnKind::Howler)[1]
        )
    );
    assert_eq!(
        rows[2],
        format!(
            " {}  {}  {}         {}   /hit  2:46 left",
            portrait[2].piece.row,
            EMPTY_SEAT[2],
            EMPTY_SEAT[2],
            face(SpawnKind::Howler)[2]
        )
    );

    // The viewer hit it: the key goes, the countdown stays, their own
    // word stands in for the detail.
    let mut mine = spawn.clone();
    mine.hits[0].user_id = viewer;
    let drawn = scene(
        &mine,
        Some("your tire iron hits the howler. 2 to go."),
        viewer,
        &looks,
        10_000 + STRIKE_MS,
    );
    let rows: Vec<String> = drawn.rows.iter().map(text_of).collect();
    assert!(
        rows[0].starts_with(&format!(" {}", GHOST_PORTRAIT[0])),
        "{}",
        rows[0]
    );
    assert!(
        rows[1].ends_with("your tire iron hits the howler. 2 to go."),
        "{}",
        rows[1]
    );
    assert!(rows[2].ends_with("   2:46 left"), "{}", rows[2]);
    assert!(!rows[2].contains("/hit"), "{}", rows[2]);
}

#[test]
fn a_hit_sends_a_strike_across_the_lane_for_five_seconds() {
    let viewer = Uuid::now_v7();
    let spawn = howler(vec![hit("tom", 10_000)]);
    let looks = HashMap::new();
    let lane = |now_ms: i64| -> String {
        let drawn = scene(&spawn, None, viewer, &looks, now_ms);
        let row = text_of(&drawn.rows[1]);
        // The lane sits after the three seats: " " + 3 × 5 + 2 × 2 + 2.
        row.chars().skip(1 + 15 + 4 + 2).take(5).collect()
    };
    assert_eq!(lane(10_000), "━╸   ");
    assert_eq!(lane(10_000 + 132), " ━╸  ");
    assert_eq!(lane(10_000 + 4 * 132), "    ━");
    assert_eq!(lane(10_000 + 5 * 132), "━╸   ");
    assert_eq!(lane(10_000 + STRIKE_MS), "     ");
}

#[test]
fn the_face_dissolves_through_the_aftermath() {
    let viewer = Uuid::now_v7();
    let mut down = howler(vec![hit("tom", 0), hit("dax", 0), hit("mira", 0)]);
    down.ended_at_ms = Some(20_000);
    down.outcome = Some(SpawnOutcome::Slain);
    let looks = HashMap::new();
    let face_cells = |now_ms: i64| -> usize {
        let drawn = scene(&down, None, viewer, &looks, now_ms);
        drawn
            .rows
            .iter()
            // The face is the tenth span: the pad, three seats and their
            // gaps, the lane and its gap.
            .map(|row| row.spans[9].content.chars().filter(|c| *c != ' ').count())
            .sum()
    };
    let whole = face_cells(20_000);
    let half = face_cells(20_000 + AFTERMATH_MS / 2);
    let gone = face_cells(20_000 + AFTERMATH_MS);
    assert!(whole > half && half > gone, "{whole} {half} {gone}");
    assert_eq!(gone, 0);
    let rows: Vec<String> = scene(&down, None, viewer, &looks, 20_000)
        .rows
        .iter()
        .map(text_of)
        .collect();
    assert!(rows[0].ends_with("the howler is down."), "{}", rows[0]);
    assert!(
        rows[1].ends_with("tom, dax and mira put it down. a crystal each."),
        "{}",
        rows[1]
    );
    // Nothing to press and nothing to count down: the key row is bare.
    assert!(
        rows[2].ends_with(&format!("{}   ", face(SpawnKind::Howler)[2])),
        "{}",
        rows[2]
    );
}
