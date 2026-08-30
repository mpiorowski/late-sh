// Pure parsers for NetHack's log files, verified against the pinned NetHack
// 5.0.0 source (src/topten.c `writexlentry`, src/files.c `livelog_add`):
//
// - Both files are `key=value` fields joined by TAB (`XLOG_SEP`/`LLOG_SEP`).
//   Values never contain tabs (naming/killer text is sanitized upstream), so
//   a plain split is exact.
// - `xlogfile` gets one line per finished game, written unconditionally
//   (XLOGFILE is defined in 5.0.0's config.h; asserted fail-closed in
//   docker/doors/nethack.Dockerfile). Fields this module reads: `name`,
//   `endtime` (unix epoch, `timet_to_seconds`), `death` (how it ended:
//   `ascended`/`escaped`/`quit` or the killer text), `points`, `maxlvl`
//   (deepest dungeon level reached — already the run maximum, so the dive
//   board needs no milestone union like DCSS's `absdepth`), `deathlev` (the
//   level the game ended on, for the death feed line), `turns`, `achieve`
//   (hex bitmask; value N sets bit N-1, so ACH_AMUL=6 is 0x20), and `flags`
//   (hex; bit 0 wizard, bit 1 explore — those games still write a line and
//   must be filtered by the caller).
// - `livelog` gets live mid-run lines (the LIVELOG compile chain plus the
//   sysconf `LIVELOG=0x0002` LL_ACHIEVE mask, both handled in the
//   Dockerfile): `name`, `curtime` (unix epoch), `message`. The one tracked
//   message is the Amulet pickup, matched verbatim against `achieve_msg[]`
//   in src/insight.c. Livelog lines carry no wizard/explore flag; the build
//   blanks sysconf `EXPLORERS` so non-scoring games cannot reach this file's
//   badge path.
//
// Parsers never touch identity or the DB: they turn one line into data, the
// ingest service decides what to do with it. Unknown fields ride along into
// `raw`; a line missing what a fact row needs parses to `None` and is skipped
// (the cursor still advances past it).

use chrono::{DateTime, TimeZone, Utc};
use late_core::models::door_milestone::DoorMilestoneKind;
use late_core::models::door_run::DoorRunResult;

/// The livelog message for the Amulet pickup, verbatim from `achieve_msg[]`
/// (src/insight.c, ACH_AMUL). Re-verify on any NetHack version bump.
const AMULET_MESSAGE: &str = "acquired The Amulet of Yendor";

/// `flags` bits for wizard (debug) and explore (discover) games. Both modes
/// still write an xlogfile line; neither may reach boards or badges.
const FLAG_WIZARD: u64 = 1 << 0;
const FLAG_EXPLORE: u64 = 1 << 1;

/// One finished game from the xlogfile.
#[derive(Clone, Debug, PartialEq)]
pub struct NethackRun {
    /// The playname (the account's arcade handle, or a legacy/reserved name
    /// the service will skip). Identity mapping is the caller's job.
    pub name: String,
    pub ended_at: DateTime<Utc>,
    pub result: DoorRunResult,
    pub score: Option<i64>,
    /// Deepest dungeon level reached (`maxlvl`).
    pub depth: Option<i32>,
    pub turns: Option<i64>,
    /// How the game ended, for the death feed line, e.g.
    /// "killed by a soldier ant".
    pub death: String,
    /// The dungeon level the game ended on (`deathlev`).
    pub death_level: Option<i32>,
    /// Wizard- or explore-mode game (`flags`). The caller must not attribute
    /// these: no fact row, no badge, no feed event.
    pub cheat_mode: bool,
    /// Every parsed field, for `door_runs.raw`.
    pub raw: serde_json::Value,
}

/// One livelog line. `kind` is `None` for valid lines whose message this pipe
/// does not track (the caller advances the cursor and persists nothing).
#[derive(Clone, Debug, PartialEq)]
pub struct NethackMilestone {
    pub name: String,
    pub occurred_at: DateTime<Utc>,
    pub kind: Option<DoorMilestoneKind>,
    /// The human-readable achievement text, e.g. "acquired The Amulet of
    /// Yendor".
    pub text: String,
    pub raw: serde_json::Value,
}

/// Split one tab-separated line into (key, value) pairs, preserving order.
/// A field without `=` is dropped; values keep any further `=` (killer names
/// have theirs rewritten to `_` upstream, but stay defensive).
fn parse_fields(line: &str) -> Vec<(String, String)> {
    line.split('\t')
        .filter_map(|field| {
            let (key, value) = field.split_once('=')?;
            Some((key.to_string(), value.to_string()))
        })
        .collect()
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

/// Parse an epoch-seconds stamp (`timet_to_seconds` output).
fn parse_epoch(stamp: &str) -> Option<DateTime<Utc>> {
    Utc.timestamp_opt(stamp.parse().ok()?, 0).single()
}

/// Parse NetHack's `0x…` hex bitmask fields (`achieve`, `flags`, `conduct`).
fn parse_hex(value: &str) -> Option<u64> {
    u64::from_str_radix(value.strip_prefix("0x")?, 16).ok()
}

/// How a game ended, from the `death` field. The three non-death ends are
/// exact literals from end.c's `deaths[]` table; upstream's own reader
/// (topten.c) prefix-matches them, so this does too. Everything else —
/// killer text, "panic", "trickery" — is a death.
fn run_result(death: &str) -> DoorRunResult {
    if death.starts_with("ascended") {
        DoorRunResult::Win
    } else if death.starts_with("quit") {
        DoorRunResult::Quit
    } else if death.starts_with("escaped") {
        DoorRunResult::Leaving
    } else {
        DoorRunResult::Death
    }
}

/// Parse one xlogfile line into a finished run. `None` when the line lacks
/// the identity/time/result core a fact row needs.
pub fn parse_xlogfile_line(line: &str) -> Option<NethackRun> {
    let fields = parse_fields(line);
    let name = field(&fields, "name")?.to_string();
    if name.is_empty() {
        return None;
    }
    let ended_at = parse_epoch(field(&fields, "endtime")?)?;
    let death = field(&fields, "death")?.to_string();
    let flags = field(&fields, "flags").and_then(parse_hex).unwrap_or(0);
    Some(NethackRun {
        ended_at,
        result: run_result(&death),
        score: field(&fields, "points").and_then(|v| v.parse().ok()),
        depth: field(&fields, "maxlvl").and_then(|v| v.parse().ok()),
        turns: field(&fields, "turns").and_then(|v| v.parse().ok()),
        death_level: field(&fields, "deathlev").and_then(|v| v.parse().ok()),
        cheat_mode: flags & (FLAG_WIZARD | FLAG_EXPLORE) != 0,
        raw: raw_json(&fields),
        death,
        name,
    })
}

/// Parse one livelog line. `None` when the identity/time/message core is
/// missing; a valid line with an untracked message parses with `kind: None`.
pub fn parse_livelog_line(line: &str) -> Option<NethackMilestone> {
    let fields = parse_fields(line);
    let name = field(&fields, "name")?.to_string();
    if name.is_empty() {
        return None;
    }
    let occurred_at = parse_epoch(field(&fields, "curtime")?)?;
    let text = field(&fields, "message")?.to_string();
    let kind = (text == AMULET_MESSAGE).then_some(DoorMilestoneKind::Amulet);
    Some(NethackMilestone {
        occurred_at,
        kind,
        text,
        raw: raw_json(&fields),
        name,
    })
}
