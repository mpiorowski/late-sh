use chrono::{TimeZone, Utc};
use late_core::models::deadchannel_spawn::{
    LoungeDay, Spawn, SpawnHit, SpawnKind, SpawnOutcome, Window,
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use super::{
    AFTERMATH_MS, ComposerText, Due, LoungeCommand, composer_text, due, hits_needed, is_stale,
    lifetime_secs, minute_of_day, parse_lounge_command, roll_dice, shows,
};

fn day() -> LoungeDay {
    LoungeDay {
        day: Utc
            .with_ymd_and_hms(2026, 10, 14, 0, 0, 0)
            .unwrap()
            .date_naive(),
        flicker_minutes: [120, 600, 1000],
        howler_minute: 700,
        flickers_spent: [false; 3],
        howler_spent: false,
        last_spawn_at: None,
    }
}

fn at(hour: u32, minute: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, 0).unwrap()
}

fn hit(name: &str) -> SpawnHit {
    SpawnHit {
        user_id: Uuid::now_v7(),
        username: name.to_string(),
        at_ms: 0,
    }
}

fn spawn(kind: SpawnKind, hits: Vec<SpawnHit>) -> Spawn {
    Spawn {
        id: Uuid::now_v7(),
        kind,
        hits_needed: hits_needed(kind),
        hits,
        spawned_at_ms: 1_000,
        expires_at_ms: 1_000 + lifetime_secs(kind) * 1_000,
        ended_at_ms: None,
        outcome: None,
    }
}

#[test]
fn the_dice_land_inside_their_windows() {
    let mut rng = StdRng::seed_from_u64(7);
    for _ in 0..200 {
        let dice = roll_dice(&mut rng);
        assert!((0..480).contains(&dice.flicker_minutes[0]), "{dice:?}");
        assert!((480..960).contains(&dice.flicker_minutes[1]), "{dice:?}");
        assert!((960..1440).contains(&dice.flicker_minutes[2]), "{dice:?}");
        assert!((0..1440).contains(&dice.howler_minute), "{dice:?}");
    }
}

#[test]
fn a_flicker_is_due_from_its_minute_while_a_runner_looks_and_its_window_is_open() {
    let day = day();
    // Before its minute: nothing.
    assert_eq!(due(&day, at(1, 59), 3, false), Due::Nothing);
    // Its minute, nobody looking: it waits.
    assert_eq!(due(&day, at(2, 0), 0, false), Due::Nothing);
    // Its minute, one runner: the early window's flicker.
    assert_eq!(due(&day, at(2, 0), 1, false), Due::Flicker(Window::Early));
    // Hours later in the same window, still unspent: still due.
    assert_eq!(due(&day, at(7, 59), 1, false), Due::Flicker(Window::Early));
    // The next window: the early one is lost, the mid one not yet due.
    assert_eq!(due(&day, at(8, 0), 1, false), Due::Nothing);
    assert_eq!(due(&day, at(10, 0), 1, false), Due::Flicker(Window::Mid));
    // Spent: nothing.
    let mut spent = day.clone();
    spent.flickers_spent[1] = true;
    assert_eq!(due(&spent, at(10, 0), 1, false), Due::Nothing);
    // A live spawn blocks everything.
    assert_eq!(due(&day, at(10, 0), 3, true), Due::Nothing);
}

#[test]
fn the_howler_needs_two_runners_and_waits_behind_a_due_flicker() {
    let day = day();
    // 11:40 is past the howler's minute (700) and the mid flicker's (600):
    // the flicker goes first.
    assert_eq!(due(&day, at(11, 40), 2, false), Due::Flicker(Window::Mid));
    let mut spent = day.clone();
    spent.flickers_spent[1] = true;
    assert_eq!(due(&spent, at(11, 40), 1, false), Due::Nothing);
    assert_eq!(due(&spent, at(11, 40), 2, false), Due::Howler);
    // Any time later in the day, as long as it is unspent, and always
    // behind the window's own flicker.
    assert_eq!(
        due(&spent, at(23, 59), 2, false),
        Due::Flicker(Window::Late)
    );
    spent.flickers_spent[2] = true;
    assert_eq!(due(&spent, at(23, 59), 2, false), Due::Howler);
    spent.howler_spent = true;
    assert_eq!(due(&spent, at(23, 59), 5, false), Due::Nothing);
}

#[test]
fn the_minute_of_the_day_is_utc() {
    assert_eq!(minute_of_day(at(0, 0)), 0);
    assert_eq!(minute_of_day(at(11, 40)), 700);
    assert_eq!(minute_of_day(at(23, 59)), 1439);
}

#[test]
fn only_a_bare_hit_is_the_command() {
    assert_eq!(parse_lounge_command("/hit"), Some(LoungeCommand::Hit));
    assert_eq!(parse_lounge_command("  /hit  "), Some(LoungeCommand::Hit));
    assert_eq!(parse_lounge_command("/hit it"), None);
    assert_eq!(parse_lounge_command("/hits"), None);
    assert_eq!(parse_lounge_command("hit"), None);
}

#[test]
fn a_spawn_shows_while_live_and_for_the_aftermath() {
    let live = spawn(SpawnKind::Flicker, Vec::new());
    assert!(shows(&live, live.expires_at_ms - 1));
    assert!(!shows(&live, live.expires_at_ms));
    let mut down = live.clone();
    down.ended_at_ms = Some(5_000);
    down.outcome = Some(SpawnOutcome::Slain);
    assert!(shows(&down, 5_000 + AFTERMATH_MS - 1));
    assert!(!shows(&down, 5_000 + AFTERMATH_MS));
}

fn said(text: &str, hit_key: bool) -> ComposerText {
    ComposerText {
        text: text.to_string(),
        hit_key,
    }
}

#[test]
fn the_composer_reads_the_spawn_to_the_viewer() {
    let viewer = Uuid::now_v7();
    let flicker = spawn(SpawnKind::Flicker, Vec::new());
    assert_eq!(
        composer_text(&flicker, viewer),
        said("a flicker is in the channel.", true)
    );

    let fresh = spawn(SpawnKind::Howler, Vec::new());
    assert_eq!(
        composer_text(&fresh, viewer),
        said("a howler is in the channel. 3 hits to put it down.", true)
    );

    let mine = SpawnHit {
        user_id: viewer,
        username: "tom".to_string(),
        at_ms: 0,
    };
    let hit_once = spawn(SpawnKind::Howler, vec![mine.clone()]);
    // The viewer hit it: no key offered.
    assert_eq!(
        composer_text(&hit_once, viewer),
        said("a howler is in the channel. tom hit it. 2 to go.", false)
    );
    assert_eq!(
        composer_text(&hit_once, Uuid::now_v7()),
        said("a howler is in the channel. tom hit it. 2 to go.", true)
    );

    let mut down = spawn(SpawnKind::Howler, vec![mine, hit("dax"), hit("mira")]);
    down.ended_at_ms = Some(2_000);
    down.outcome = Some(SpawnOutcome::Slain);
    assert_eq!(
        composer_text(&down, viewer),
        said(
            "the howler is down. tom, dax and mira put it down. a crystal each.",
            false
        )
    );

    let mut slipped = spawn(SpawnKind::Howler, Vec::new());
    slipped.ended_at_ms = Some(2_000);
    slipped.outcome = Some(SpawnOutcome::Slipped);
    assert_eq!(
        composer_text(&slipped, viewer),
        said("the howler slipped away.", false)
    );
}

#[test]
fn an_older_payload_of_the_held_spawn_is_stale() {
    let one_hit = spawn(SpawnKind::Howler, vec![hit("tom")]);
    let mut two_hits = one_hit.clone();
    two_hits.hits.push(hit("dax"));
    let mut down = two_hits.clone();
    down.hits.push(hit("mira"));
    down.ended_at_ms = Some(3_000);
    down.outcome = Some(SpawnOutcome::Slain);

    // Nothing held, or another spawn held: every payload is news.
    assert!(!is_stale(None, &one_hit));
    assert!(!is_stale(
        Some(&spawn(SpawnKind::Flicker, Vec::new())),
        &one_hit
    ));
    // The same spawn moving forward: news.
    assert!(!is_stale(Some(&one_hit), &two_hits));
    assert!(!is_stale(Some(&two_hits), &down));
    assert!(!is_stale(Some(&two_hits), &two_hits));
    // The hits that arrived after the kill, or fewer hits than held: stale.
    assert!(is_stale(Some(&down), &two_hits));
    assert!(is_stale(Some(&two_hits), &one_hit));
}
