use super::dopewars::parse_runs_line;
use late_core::models::door_run::DoorRunResult;

#[test]
fn parses_a_surviving_run() {
    let run = parse_runs_line("Wormsong\t1234567\t0\t1769947200\t01-02-2026").expect("parse");
    assert_eq!(run.name, "Wormsong");
    assert_eq!(run.score, 1_234_567);
    assert_eq!(run.result, DoorRunResult::Leaving);
    assert_eq!(run.ended_at.timestamp(), 1_769_947_200);
    assert_eq!(run.raw["date"], "01-02-2026");
    assert_eq!(run.raw["dead"], false);
}

#[test]
fn parses_a_death_and_a_negative_worth() {
    let run = parse_runs_line("Wormsong\t-500\t1\t1769947200\t01-02-2026").expect("parse");
    assert_eq!(run.score, -500);
    assert_eq!(run.result, DoorRunResult::Death);
    assert_eq!(run.raw["dead"], true);
}

#[test]
fn rejects_malformed_lines() {
    assert!(parse_runs_line("").is_none());
    assert!(
        parse_runs_line("Wormsong\t100\t0\t1769947200").is_none(),
        "missing date"
    );
    assert!(
        parse_runs_line("Wormsong\t100\t0\t1769947200\t01-02-2026\textra").is_none(),
        "extra field"
    );
    assert!(parse_runs_line("Wormsong\tlots\t0\t1769947200\t01-02-2026").is_none());
    assert!(parse_runs_line("Wormsong\t100\t2\t1769947200\t01-02-2026").is_none());
    assert!(parse_runs_line("\t100\t0\t1769947200\t01-02-2026").is_none());
}
