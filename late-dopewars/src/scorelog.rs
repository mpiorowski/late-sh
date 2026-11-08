// Append-only log of every run that ever reached dopewars' high-score table.
//
// Upstream rewrites its score file in place (`HighScoreWrite` in
// src/serverside.c: ftruncate + rewind + the whole table again), so the
// binary `-f` file holds only the current top `NUMHISCORE` (18) and forgets
// every run that dropped off it. The leaderboard needs the history, so after
// every game (and once at startup, to backfill a PVC's existing file) we
// diff the table against `<score_file>.runs.log` — one TAB-separated line per
// run ever seen — and append only the entries not already logged.
//
// File format, from `HighScoreTypeWrite` (dopewars 1.6.2, src/serverside.c:
// 1770): the NUL-terminated header "DOPEWARS SCORES V.1", then two arrays of
// 18 entries (antique mode first — always empty here — then the regular
// table). Each entry is a NUL-terminated name, a NUL-terminated date
// ("%d-%m-%Y", UTC), a NUL-terminated price (`pricetostr`: ASCII digits with
// an optional leading '-', "" for zero), and one byte `dead` (non-zero =
// died). An unused slot is written with empty strings and dead = 0.
//
// `runs.log` line: `<name>\t<money>\t<dead:0|1>\t<ended_epoch>\t<sco_date>`.
//
// shortcut: the table keeps only the best 18 runs, so a game that never made
// the table is never logged. Logging every game needs an upstream change;
// upgrade when dopewars grows a per-run log.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `HighScoreWriteHeader`: "DOPEWARS SCORES V." + version 1, NUL-terminated.
const SCORE_HEADER: &[u8] = b"DOPEWARS SCORES V.1\0";

/// Two `NUMHISCORE` (18) arrays: antique mode, then the regular table.
const MAX_RECORDS: usize = 36;

/// The frame id late-ssh's stats session uses for this file.
pub(crate) const RUNS_FILE: &str = "runs";

/// One score-file entry: a run's player, final net worth, death flag, and the
/// UTC day dopewars stamped it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ScoEntry {
    pub(crate) name: String,
    pub(crate) money: i64,
    pub(crate) dead: bool,
    pub(crate) date: String,
}

/// The startup backfill and a game's post-exit sync can race; one writer at a
/// time keeps the diff honest.
static SYNC: Mutex<()> = Mutex::new(());

/// Parse a whole score file. A missing header yields nothing; a truncated
/// tail (a killed game mid-write) yields the complete records before it.
pub(crate) fn parse_sco(bytes: &[u8]) -> Vec<ScoEntry> {
    let Some(mut pos) = bytes
        .starts_with(SCORE_HEADER)
        .then_some(SCORE_HEADER.len())
    else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for _ in 0..MAX_RECORDS {
        let Some((name, next)) = read_cstr(bytes, pos) else {
            break;
        };
        let Some((date, next)) = read_cstr(bytes, next) else {
            break;
        };
        let Some((money, next)) = read_cstr(bytes, next) else {
            break;
        };
        let Some(&dead) = bytes.get(next) else {
            break;
        };
        pos = next + 1;
        // An unused slot is an empty name with empty date and price.
        if name.is_empty() {
            continue;
        }
        let Ok(money) = parse_money(&money) else {
            tracing::warn!(name = %name, money = %money, "unparseable dopewars score price; skipping");
            continue;
        };
        entries.push(ScoEntry {
            name,
            money,
            dead: dead != 0,
            date,
        });
    }
    entries
}

/// The append-only run log that lives beside the score file
/// (`dopewars.sco` -> `dopewars.sco.runs.log`).
pub(crate) fn runs_log_path(score_file: &str) -> PathBuf {
    PathBuf::from(format!("{score_file}.{RUNS_FILE}.log"))
}

/// Append this score file's not-yet-logged entries to `runs_log`. Returns how
/// many lines were appended.
///
/// A log that did not exist yet (first deploy, rebuilt PVC) is a cold start:
/// the dated history the score file still holds is stamped at noon UTC of its
/// own day, so old runs land in the right month. Later discoveries are
/// stamped `now_epoch` — they come from the game that just ended.
pub(crate) fn sync_from_sco(
    score_file: &Path,
    runs_log: &Path,
    now_epoch: i64,
) -> anyhow::Result<usize> {
    let _guard = SYNC.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let logged = read_runs_log(runs_log)?;
    let cold_start = logged.is_empty();
    let bytes = match fs::read(score_file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };

    // Multiset difference: an entry still in the table that was already
    // logged (same player, money, death flag and date) must not be appended
    // twice, however many times the table is rewritten.
    let mut remaining = logged;
    let mut out = String::new();
    let mut appended = 0;
    for entry in parse_sco(&bytes) {
        let key = (
            entry.name.clone(),
            entry.money,
            entry.dead,
            entry.date.clone(),
        );
        match remaining.get_mut(&key) {
            Some(count) if *count > 0 => *count -= 1,
            _ => {
                let ended = match cold_start {
                    true => date_noon_epoch(&entry.date).unwrap_or(now_epoch),
                    false => now_epoch,
                };
                out.push_str(&format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    entry.name,
                    entry.money,
                    u8::from(entry.dead),
                    ended,
                    entry.date
                ));
                appended += 1;
            }
        }
    }
    if appended > 0 {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(runs_log)?;
        file.write_all(out.as_bytes())?;
    }
    Ok(appended)
}

type LoggedKey = (String, i64, bool, String);

/// Count the run log's lines by identity. Malformed lines are skipped rather
/// than fatal: the log is a cache of history, not a source of record.
fn read_runs_log(runs_log: &Path) -> anyhow::Result<HashMap<LoggedKey, usize>> {
    let text = match fs::read_to_string(runs_log) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e.into()),
    };
    let mut counts: HashMap<LoggedKey, usize> = HashMap::new();
    for line in text.lines() {
        let mut fields = line.split('\t');
        let (Some(name), Some(money), Some(dead), Some(_ended), Some(date)) = (
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
            fields.next(),
        ) else {
            continue;
        };
        let (Ok(money), Ok(dead)) = (money.parse::<i64>(), dead.parse::<u8>()) else {
            continue;
        };
        *counts
            .entry((name.to_string(), money, dead != 0, date.to_string()))
            .or_default() += 1;
    }
    Ok(counts)
}

fn read_cstr(bytes: &[u8], pos: usize) -> Option<(String, usize)> {
    let end = pos + bytes.get(pos..)?.iter().position(|&b| b == 0)?;
    Some((
        String::from_utf8_lossy(&bytes[pos..end]).into_owned(),
        end + 1,
    ))
}

/// `pricetostr` writes "" for zero; every other value is plain ASCII digits
/// with an optional leading '-'.
fn parse_money(text: &str) -> Result<i64, std::num::ParseIntError> {
    match text.is_empty() {
        true => Ok(0),
        false => text.parse(),
    }
}

/// Noon UTC of a dopewars `"%d-%m-%Y"` date, so a backfilled run lands in
/// the right month. `None` for anything else.
fn date_noon_epoch(date: &str) -> Option<i64> {
    let mut parts = date.split('-');
    let day: i32 = parts.next()?.parse().ok()?;
    let month: i32 = parts.next()?.parse().ok()?;
    let year: i32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let mut tm: nix::libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_mday = day;
    tm.tm_mon = month - 1;
    tm.tm_year = year - 1900;
    tm.tm_hour = 12;
    let epoch = unsafe { nix::libc::timegm(&mut tm) };
    (epoch != -1).then_some(epoch)
}

#[cfg(test)]
#[path = "scorelog_test.rs"]
mod scorelog_test;
