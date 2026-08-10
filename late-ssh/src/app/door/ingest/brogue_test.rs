use chrono::{TimeZone, Utc};
use late_core::models::door_run::DoorRunResult;

use super::brogue::{BrogueLine, parse_run_history_line, playname_from_file};

// Shaped like real 1.15.1 lines (field order from platformdependent.c
// saveRunHistory: seed, epoch, result, killedBy, score, gold, lumenstones,
// deepestLevel, turns).
const DEATH_LINE: &str = "8697033734589\t1754560000\tDied\tpink jelly\t1520\t1020\t0\t8\t2341";
const ESCAPE_LINE: &str = "1234567890\t1754560000\tEscaped\t-\t4870\t4870\t0\t26\t18023";
const MASTERY_LINE: &str = "99\t1754560000\tMastered\t-\t18420\t9420\t3\t40\t31007";

#[test]
fn parses_a_death_line() {
    let BrogueLine::Run(run) = parse_run_history_line(DEATH_LINE).expect("death line parses")
    else {
        panic!("death line is a run");
    };
    assert_eq!(run.result, DoorRunResult::Death);
    assert_eq!(run.score, Some(1520));
    assert_eq!(run.depth, Some(8));
    assert_eq!(run.turns, Some(2341));
    assert_eq!(run.killed_by, "pink jelly");
    assert_eq!(run.ended_at, Utc.timestamp_opt(1754560000, 0).unwrap());
    // The full line rides along for boards not invented yet.
    assert_eq!(run.raw["seed"], "8697033734589");
    assert_eq!(run.raw["gold"], "1020");
    assert_eq!(run.raw["lumenstones"], "0");
}

#[test]
fn maps_the_victory_pair() {
    let BrogueLine::Run(escape) = parse_run_history_line(ESCAPE_LINE).expect("escape parses")
    else {
        panic!("escape line is a run");
    };
    assert_eq!(escape.result, DoorRunResult::Win);
    assert_eq!(escape.killed_by, "-");

    let BrogueLine::Run(mastery) = parse_run_history_line(MASTERY_LINE).expect("mastery parses")
    else {
        panic!("mastery line is a run");
    };
    assert_eq!(mastery.result, DoorRunResult::Mastery);
    assert_eq!(mastery.raw["lumenstones"], "3");
}

#[test]
fn parses_a_quit_and_a_custom_phrase_killer() {
    let BrogueLine::Run(quit) =
        parse_run_history_line("5\t1754560000\tQuit\t-\t100\t100\t0\t3\t500").expect("quit parses")
    else {
        panic!("quit line is a run");
    };
    assert_eq!(quit.result, DoorRunResult::Quit);

    // Custom-phrasing deaths store the whole capitalized phrase, spaces and
    // all, in the killer column.
    let BrogueLine::Run(run) =
        parse_run_history_line("7\t1754560000\tDied\tStarved to death\t80\t80\t0\t5\t900")
            .expect("phrase killer parses")
    else {
        panic!("phrase line is a run");
    };
    assert_eq!(run.killed_by, "Starved to death");
}

#[test]
fn reset_marker_is_not_a_run() {
    // saveResetRun's exact shape: seed 0, "Reset", "-", zeros.
    let line = "0\t1754560000\tReset\t-\t0\t0\t0\t0\t0";
    assert_eq!(parse_run_history_line(line), Some(BrogueLine::Reset));
}

#[test]
fn tolerates_extra_trailing_fields() {
    // A future CE appending a column must not break ingestion of the first
    // nine (upstream's own sscanf reader ignores trailing text too).
    let line = format!("{DEATH_LINE}\tfuture");
    let BrogueLine::Run(run) = parse_run_history_line(&line).expect("line parses") else {
        panic!("line is a run");
    };
    assert_eq!(run.score, Some(1520));
}

#[test]
fn rejects_lines_missing_the_core_shape() {
    // Truncated mid-line (host never frames these, stay defensive).
    assert!(parse_run_history_line("123\t1754560000\tDied\tjackal").is_none());
    // Non-numeric seed anchor.
    assert!(parse_run_history_line("x\t1754560000\tDied\tjackal\t1\t1\t0\t2\t3").is_none());
    // Garbage stamp.
    assert!(parse_run_history_line("1\tnotatime\tDied\tjackal\t1\t1\t0\t2\t3").is_none());
    // Unknown result word.
    assert!(parse_run_history_line("1\t1754560000\tVanished\t-\t1\t1\t0\t2\t3").is_none());
    // Empty / junk lines.
    assert!(parse_run_history_line("").is_none());
    assert!(parse_run_history_line("no tabs here").is_none());
}

#[test]
fn recovers_the_playname_from_the_file_id() {
    assert_eq!(
        playname_from_file("players/GleamingUnicycle/BrogueRunHistory.txt"),
        Some("GleamingUnicycle")
    );
    assert_eq!(
        playname_from_file("players/late_stats/BrogueRunHistory.txt"),
        Some("late_stats")
    );
}

#[test]
fn rejects_file_ids_outside_the_contract() {
    // Variant files are never streamed, but stay defensive.
    assert_eq!(
        playname_from_file("players/mat/RapidBrogueRunHistory.txt"),
        None
    );
    assert_eq!(playname_from_file("players//BrogueRunHistory.txt"), None);
    assert_eq!(playname_from_file("players/a/b/BrogueRunHistory.txt"), None);
    assert_eq!(playname_from_file("BrogueRunHistory.txt"), None);
    assert_eq!(playname_from_file("logfile"), None);
}
