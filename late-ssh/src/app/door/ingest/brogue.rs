// Pure parser for Brogue CE's per-player run history file, verified against
// the pinned 1.15.1 source (src/platform/platformdependent.c
// `saveRunHistory`/`saveResetRun`, src/brogue/RogueMain.c call sites):
//
// - One line per finished game, TAB-separated positional fields (no keys):
//   `seed \t epoch \t result \t killedBy \t score \t gold \t lumenstones \t
//   deepestLevel \t turns`. `result` is `Died`/`Quit`/`Escaped`/`Mastered`,
//   plus the `Reset` marker `saveResetRun` appends when the player resets
//   their in-game stats (seed 0, everything else zeroed) — a marker, not a
//   game, parsed as [`BrogueLine::Reset`] so the caller can advance the
//   cursor without a warning.
// - `killedBy` is `-` for every non-death end. For deaths it is either a
//   bare lowercase monster name ("jackal") or a capitalized custom phrase
//   ("Starved to death"), exactly as passed to `gameOver`; the feed line
//   formatter in svc.rs branches on that case.
// - The file identifies the variant, not the line: `setRunHistoryFilename`
//   prefixes the variant name, so standard games land in
//   `BrogueRunHistory.txt` and Rapid/Bullet in their own files. The host
//   streams only the standard file (variant games do not count on the
//   boards, owner decision 2026-08-08), so no per-line variant filter is
//   needed here.
// - The line carries NO player name. Identity is the per-player directory
//   the file lives in; [`playname_from_file`] recovers it from the frame's
//   file id (`players/<handle>/BrogueRunHistory.txt`).
// - Upstream self-polices cheat modes: Easy and Wizard games never write a
//   run-history line (the victory path needed our
//   scripts/brogue_victory_log.patch to get that right; see the Dockerfile).
//
// Parsers never touch identity or the DB: they turn one line into data, the
// ingest service decides what to do with it. A line missing the core a fact
// row needs parses to `None` and is skipped (the cursor still advances).

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::door_run::DoorRunResult;

/// The host-side frame id shape this door's identity rides on; must match
/// `late-brogue/src/stats.rs` (`PLAYERS_DIR`/`RUN_HISTORY_FILE`).
const FILE_PREFIX: &str = "players/";
const FILE_SUFFIX: &str = "/BrogueRunHistory.txt";

/// One finished game from a player's run history.
#[derive(Clone, Debug, PartialEq)]
pub struct BrogueRun {
    pub ended_at: DateTime<Utc>,
    pub result: DoorRunResult,
    pub score: Option<i64>,
    /// Deepest depth reached (`deepestLevel`), the run maximum like
    /// NetHack's `maxlvl`, so the dive board needs no milestone union.
    pub depth: Option<i32>,
    pub turns: Option<i64>,
    /// The raw killer column: `-` for non-deaths, else a monster name or a
    /// capitalized phrase (see the module comment).
    pub killed_by: String,
    /// Every parsed field, for `door_runs.raw`.
    pub raw: serde_json::Value,
}

/// One parsed run-history line: a game, or the stats-reset marker.
#[derive(Clone, Debug, PartialEq)]
pub enum BrogueLine {
    Run(BrogueRun),
    /// `saveResetRun`'s marker line. Not a game; nothing to persist.
    Reset,
}

/// The player handle behind a stats frame's file id, or `None` for a file
/// this pipe does not recognize (the host only ever streams the shape below).
pub fn playname_from_file(file: &str) -> Option<&str> {
    let name = file.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    (!name.is_empty() && !name.contains('/')).then_some(name)
}

/// How a game ended, from the `result` column. The four values are the exact
/// literals `saveRunHistory` is called with; `Escaped` is the ordinary win
/// and `Mastered` the super-victory (out with the Birthright of Yendor).
fn run_result(result: &str) -> Option<DoorRunResult> {
    match result {
        "Died" => Some(DoorRunResult::Death),
        "Quit" => Some(DoorRunResult::Quit),
        "Escaped" => Some(DoorRunResult::Win),
        "Mastered" => Some(DoorRunResult::Mastery),
        _ => None,
    }
}

fn raw_json(keys: &[&str], values: &[&str]) -> serde_json::Value {
    serde_json::Value::Object(
        keys.iter()
            .zip(values)
            .map(|(k, v)| {
                (
                    (*k).to_string(),
                    serde_json::Value::String((*v).to_string()),
                )
            })
            .collect(),
    )
}

/// Parse one run-history line. `None` when the line is not the 9-field shape
/// `saveRunHistory` writes or its time/result core does not parse; extra
/// trailing fields from a future CE version are tolerated and ignored, like
/// upstream's own `sscanf` reader.
pub fn parse_run_history_line(line: &str) -> Option<BrogueLine> {
    const KEYS: [&str; 9] = [
        "seed",
        "time",
        "result",
        "killed_by",
        "score",
        "gold",
        "lumenstones",
        "deepest_level",
        "turns",
    ];
    let values: Vec<&str> = line.split('\t').collect();
    if values.len() < KEYS.len() {
        return None;
    }
    let [
        seed,
        time,
        result,
        killed_by,
        score,
        _gold,
        _lumenstones,
        deepest_level,
        turns,
    ] = values[..KEYS.len()]
    else {
        unreachable!("slice of KEYS.len() matches the 9-element pattern");
    };
    // Seed must be numeric: it anchors the shape (a stray tab inside a killer
    // phrase can never produce a numeric first column followed by a numeric
    // epoch, because upstream writes killer text without tabs).
    seed.parse::<u64>().ok()?;
    if result == "Reset" {
        return Some(BrogueLine::Reset);
    }
    let ended_at = Utc.timestamp_opt(time.parse().ok()?, 0).single()?;
    Some(BrogueLine::Run(BrogueRun {
        ended_at,
        result: run_result(result)?,
        score: score.parse().ok(),
        depth: deepest_level.parse().ok(),
        turns: turns.parse().ok(),
        killed_by: killed_by.to_string(),
        raw: raw_json(&KEYS, &values[..KEYS.len()]),
    }))
}
