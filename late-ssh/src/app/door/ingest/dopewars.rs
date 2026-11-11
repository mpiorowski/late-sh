// Pure parser for late-dopewars' append-only run log (`late-dopewars/src/
// scorelog.rs` — keep the two copies in sync). One TAB-separated line per run
// that ever reached dopewars' top-18 high-score table:
//
//   `name \t money \t dead \t ended_epoch \t sco_date`
//
// - `money` is the run's final net worth (Cash + Bank - Debt), an integer
//   that may be negative.
// - `dead` is 1 when the player's health hit zero, 0 when they walked away.
//   Neither is a win, so `DoorRunResult::WINS` never counts a dopewars run;
//   the leaderboard reads the score family only.
// - `ended_epoch` is when the host logged the run; `sco_date` is dopewars'
//   own UTC day ("%d-%m-%Y"), kept in `raw` for diagnostics.
//
// The host writes the log, so the parser never guesses: a line that is not
// exactly five fields, or whose numbers do not parse, yields `None` and the
// caller advances the cursor past it.

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::door_run::DoorRunResult;

/// The stats frame id late-dopewars streams the run log under; must match
/// `late-dopewars/src/scorelog.rs` (`RUNS_FILE`).
pub const FILE_ID: &str = "runs";

/// One finished dopewars run.
#[derive(Clone, Debug, PartialEq)]
pub struct DopewarsRun {
    pub name: String,
    pub score: i64,
    pub result: DoorRunResult,
    pub ended_at: DateTime<Utc>,
    pub raw: serde_json::Value,
}

/// Parse one run-log line. `None` for anything that is not the host's shape.
pub fn parse_runs_line(line: &str) -> Option<DopewarsRun> {
    let mut fields = line.trim_end_matches(['\r', '\n']).split('\t');
    let name = fields.next()?;
    let money = fields.next()?;
    let dead = fields.next()?;
    let ended_epoch = fields.next()?;
    let sco_date = fields.next()?;
    if name.is_empty() || fields.next().is_some() {
        return None;
    }
    let result = match dead {
        "0" => DoorRunResult::Leaving,
        "1" => DoorRunResult::Death,
        _ => return None,
    };
    Some(DopewarsRun {
        name: name.to_string(),
        score: money.parse().ok()?,
        result,
        ended_at: Utc.timestamp_opt(ended_epoch.parse().ok()?, 0).single()?,
        raw: serde_json::json!({
            "name": name,
            "money": money,
            "dead": dead == "1",
            "date": sco_date,
        }),
    })
}
