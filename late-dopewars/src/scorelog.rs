// Append-only log of every finished dopewars game, the leaderboard's source.
//
// Upstream keeps no per-run log: a finished game only lands in its binary
// `-f` high-score table, which holds the top `NUMHISCORE` (18) and is
// rewritten in place (`HighScoreWrite` in src/serverside.c: ftruncate +
// rewind + the whole table again). A game enters the table when it beats a
// held entry or finds an empty slot (`SendHighScores`, serverside.c:2159), so
// a table shared by everyone logs only games that beat its 18th place: one
// player with 18 big runs shuts every other player out of the board.
//
// So every game session gets its own table, empty at the start
// (`SessionTable`, the file in the session's scratch `HOME`): every game it
// plays finds an empty slot, until a session past 18 games pushes its own
// lowest off. The host syncs the table on a timer and at the session's end,
// appending to `<score_file>.runs.log` (one TAB-separated line per run) the
// entries this session has not logged yet. Nothing writes the configured
// `score_file` itself; it only names the run log.
//
// File format, from `HighScoreTypeWrite` (dopewars 1.6.2, src/serverside.c:
// 1770): the NUL-terminated header "DOPEWARS SCORES V.1", then two arrays of
// 18 entries (antique mode first, always empty here, then the regular
// table). Each entry is a NUL-terminated name, a NUL-terminated date
// ("%d-%m-%Y", UTC), a NUL-terminated price (`pricetostr`: ASCII digits with
// an optional leading '-', "" for zero), and one byte `dead` (non-zero =
// died). An unused slot is written with empty strings and dead = 0.
//
// `runs.log` line: `<name>\t<money>\t<dead:0|1>\t<ended_epoch>\t<sco_date>`.
//
// shortcut: a session past 18 games can push its own lowest off before a
// sync sees it, and a host killed mid-session loses what finished since the
// last timer tick. Logging every game exactly needs an upstream per-run log.

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

/// Concurrent sessions append to the one run log; one writer at a time keeps
/// each sync's lines together.
static APPEND: Mutex<()> = Mutex::new(());

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

/// One game session's own high-score table, and the entries of it already
/// appended to the run log (see the header).
pub(crate) struct SessionTable {
    score_file: PathBuf,
    /// Counted, not a set: two games of one session can end on the same
    /// money the same day, and both are runs.
    logged: HashMap<ScoEntry, usize>,
}

impl SessionTable {
    /// The table in the session's scratch `HOME`. The game creates the file
    /// on launch (`OpenHighScoreFile`, serverside.c:1982), so it starts empty.
    pub(crate) fn new(home: &Path) -> Self {
        Self {
            score_file: home.join("dopewars.sco"),
            logged: HashMap::new(),
        }
    }

    /// The path the game gets as `-f`.
    pub(crate) fn score_file(&self) -> &Path {
        &self.score_file
    }

    /// Append the table's entries this session has not logged yet to
    /// `runs_log`. Returns how many lines were appended. A failed append
    /// logs nothing as done, so the next sync retries it.
    pub(crate) fn sync(&mut self, runs_log: &Path, now_epoch: i64) -> anyhow::Result<usize> {
        let entries = match fs::read(&self.score_file) {
            Ok(bytes) => parse_sco(&bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };

        // Multiset difference: an entry still in the table that this session
        // already logged must not be appended twice, however many times the
        // table is rewritten.
        let mut remaining = self.logged.clone();
        let mut out = String::new();
        let mut fresh = Vec::new();
        for entry in entries {
            match remaining.get_mut(&entry) {
                Some(count) if *count > 0 => *count -= 1,
                Some(_) | None => {
                    out.push_str(&format!(
                        "{}\t{}\t{}\t{}\t{}\n",
                        entry.name,
                        entry.money,
                        u8::from(entry.dead),
                        ended_epoch(&entry.date, now_epoch),
                        entry.date
                    ));
                    fresh.push(entry);
                }
            }
        }
        if fresh.is_empty() {
            return Ok(0);
        }
        append(runs_log, &out)?;
        let appended = fresh.len();
        for entry in fresh {
            *self.logged.entry(entry).or_default() += 1;
        }
        Ok(appended)
    }
}

fn append(path: &Path, lines: &str) -> anyhow::Result<()> {
    let _guard = APPEND
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(lines.as_bytes())?;
    Ok(())
}

/// When a newly seen run ended: now, the game having just finished, unless
/// the table dates it to another UTC day (a game that ended before midnight
/// and was synced after it, maybe in another month). Then it is noon of the
/// day the table recorded, so the run lands on the board of the month it was
/// played.
fn ended_epoch(date: &str, now_epoch: i64) -> i64 {
    const HALF_DAY: i64 = 12 * 60 * 60;
    match date_noon_epoch(date) {
        Some(noon) if !(-HALF_DAY..HALF_DAY).contains(&(now_epoch - noon)) => noon,
        Some(_) | None => now_epoch,
    }
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

/// Noon UTC of a dopewars `"%d-%m-%Y"` date. `None` for anything else.
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
