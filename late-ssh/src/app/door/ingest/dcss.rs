// Pure parsers for crawl's xlog files, verified against the pinned DCSS
// 0.34.1 source (source/hiscores.cc):
//
// - A line is `key=value` fields joined by `:`; a literal `:` inside a value
//   is escaped as `::` (`_xlog_escape`), so `place=D:5` lands as `place=D::5`.
// - `logfile` gets one line per finished game (`logfile_new_entry`, called
//   unconditionally in end.cc for non-wizard/non-explore games). Fields this
//   module reads: `name`, `sc` (score), `absdepth` (deepest absolute depth),
//   `turn`, `ktyp` (how it ended; `winning` = escaped with the Orb), `end`
//   (end-of-game stamp), `tmsg` (preformatted death one-liner, present via
//   the DGL_EXTENDED_LOGFILES build define), `place`.
// - `milestones` gets live mid-run lines (`mark_milestone`, present via the
//   DGL_MILESTONES build define): base fields plus `time`, `type`, and
//   `milestone` text. Types this pipe tracks: `orb` (Orb pickup), `rune`,
//   `br.enter`.
// - Timestamps (`make_date_string` in tags.cc) are `%Y%m%d%H%M%S` plus a
//   DST letter, with the MONTH WRITTEN 0-BASED (raw `tm_mon`, the classic
//   xlogfile quirk). UTC via the TIME_FN=gmtime build define.
//
// Parsers never touch identity or the DB: they turn one line into data, the
// ingest service decides what to do with it. Unknown fields ride along into
// `raw`; a line missing what a fact row needs parses to `None` and is skipped
// (the cursor still advances past it).

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::door_milestone::DoorMilestoneKind;
use late_core::models::door_run::DoorRunResult;

/// One finished game from the logfile.
#[derive(Clone, Debug, PartialEq)]
pub struct DcssRun {
    /// The playname (the account's arcade handle, or a legacy/reserved name
    /// the service will skip). Identity mapping is the caller's job.
    pub name: String,
    pub ended_at: DateTime<Utc>,
    pub result: DoorRunResult,
    pub score: Option<i64>,
    pub depth: Option<i32>,
    pub turns: Option<i64>,
    /// The preformatted one-line death message (`tmsg`), e.g.
    /// "slain by an orc warrior".
    pub death_message: Option<String>,
    /// Where the game ended, unescaped (e.g. "D:5").
    pub place: Option<String>,
    /// Every parsed field, for `door_runs.raw`.
    pub raw: serde_json::Value,
}

/// One milestone line. `kind` is `None` for valid lines whose type this pipe
/// does not track (the caller advances the cursor and persists nothing).
#[derive(Clone, Debug, PartialEq)]
pub struct DcssMilestone {
    pub name: String,
    pub occurred_at: DateTime<Utc>,
    pub kind: Option<DoorMilestoneKind>,
    /// The human-readable milestone text, e.g. "found the Orb of Zot!".
    pub text: String,
    pub raw: serde_json::Value,
}

/// Split one xlog line into (key, value) pairs, handling the `::` escape.
/// Field order is preserved; a field without `=` is dropped, same as crawl's
/// own reader (`xlog_fields::init`).
fn parse_xlog_fields(line: &str) -> Vec<(String, String)> {
    let bytes = line.as_bytes();
    let mut fields = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i <= bytes.len() {
        let at_separator = i < bytes.len() && bytes[i] == b':';
        if at_separator && i + 1 < bytes.len() && bytes[i + 1] == b':' {
            i += 2; // escaped colon, part of the value
            continue;
        }
        if at_separator || i == bytes.len() {
            let field = &line[start..i];
            if let Some((key, value)) = field.split_once('=') {
                fields.push((key.to_string(), value.replace("::", ":")));
            }
            start = i + 1;
        }
        i += 1;
    }
    fields
}

/// Parse crawl's `make_date_string` stamp: `%Y%m%d%H%M%S` + `S`/`D`, month
/// 0-based (raw `tm_mon`). UTC by the TIME_FN=gmtime build define.
fn parse_xlog_time(stamp: &str) -> Option<DateTime<Utc>> {
    if stamp.len() < 14 || !stamp.as_bytes()[..14].iter().all(u8::is_ascii_digit) {
        return None;
    }
    let num = |range: std::ops::Range<usize>| stamp[range].parse::<u32>().ok();
    let year = stamp[0..4].parse::<i32>().ok()?;
    let month0 = num(4..6)?;
    let day = num(6..8)?;
    let hour = num(8..10)?;
    let minute = num(10..12)?;
    let second = num(12..14)?;
    Utc.with_ymd_and_hms(year, month0 + 1, day, hour, minute, second)
        .single()
}

fn raw_json(fields: &[(String, String)]) -> serde_json::Value {
    serde_json::Value::Object(
        fields
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect(),
    )
}

fn field<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

/// How a game ended, from `ktyp`. The value set is crawl's
/// `kill_method_names` table: three non-death ends, everything else a death.
fn run_result(ktyp: &str) -> DoorRunResult {
    match ktyp {
        "winning" => DoorRunResult::Win,
        "quitting" => DoorRunResult::Quit,
        "leaving" => DoorRunResult::Leaving,
        _ => DoorRunResult::Death,
    }
}

/// Parse one logfile line into a finished run. `None` when the line lacks
/// the identity/time/result core a fact row needs.
pub fn parse_logfile_line(line: &str) -> Option<DcssRun> {
    let fields = parse_xlog_fields(line);
    let name = field(&fields, "name")?.to_string();
    if name.is_empty() {
        return None;
    }
    let ended_at = parse_xlog_time(field(&fields, "end")?)?;
    let result = run_result(field(&fields, "ktyp")?);
    Some(DcssRun {
        ended_at,
        result,
        score: field(&fields, "sc").and_then(|v| v.parse().ok()),
        depth: field(&fields, "absdepth").and_then(|v| v.parse().ok()),
        turns: field(&fields, "turn").and_then(|v| v.parse().ok()),
        death_message: field(&fields, "tmsg").map(str::to_string),
        place: field(&fields, "place").map(str::to_string),
        raw: raw_json(&fields),
        name,
    })
}

/// Parse one milestones line. `None` when the identity/time/type core is
/// missing; a valid line with an untracked type parses with `kind: None`.
pub fn parse_milestone_line(line: &str) -> Option<DcssMilestone> {
    let fields = parse_xlog_fields(line);
    let name = field(&fields, "name")?.to_string();
    if name.is_empty() {
        return None;
    }
    let occurred_at = parse_xlog_time(field(&fields, "time")?)?;
    let kind = match field(&fields, "type")? {
        "orb" => Some(DoorMilestoneKind::Orb),
        "rune" => Some(DoorMilestoneKind::Rune),
        "br.enter" => Some(DoorMilestoneKind::BranchEnter),
        _ => None,
    };
    Some(DcssMilestone {
        occurred_at,
        kind,
        text: field(&fields, "milestone").unwrap_or_default().to_string(),
        raw: raw_json(&fields),
        name,
    })
}
