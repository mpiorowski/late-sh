use super::*;

const NOW: i64 = 1_700_000_000;

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "late-dopewars-scorelog-{tag}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    /// The shared run log every session appends to.
    fn runs(&self) -> PathBuf {
        runs_log_path(&self.dir.join("dopewars.sco").to_string_lossy())
    }

    /// A game session's own table, in its own scratch `HOME`.
    fn session(&self, home: &str) -> SessionTable {
        let home = self.dir.join(home);
        fs::create_dir_all(&home).unwrap();
        SessionTable::new(&home)
    }

    fn sync(&self, table: &mut SessionTable, now_epoch: i64) -> usize {
        table.sync(&self.runs(), now_epoch).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn record(name: &str, date: &str, money: &str, dead: u8) -> Vec<u8> {
    let mut out = Vec::new();
    for field in [name, date, money] {
        out.extend_from_slice(field.as_bytes());
        out.push(0);
    }
    out.push(dead);
    out
}

/// Write a score file exactly as upstream does: header, the records, then
/// empty slots out to the two full 18-entry arrays.
fn write_score(path: &Path, records: &[Vec<u8>]) {
    let mut bytes = SCORE_HEADER.to_vec();
    for rec in records {
        bytes.extend_from_slice(rec);
    }
    let empty = record("", "", "", 0);
    for _ in records.len()..MAX_RECORDS {
        bytes.extend_from_slice(&empty);
    }
    fs::write(path, bytes).unwrap();
}

fn log_lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

fn entry(name: &str, money: i64, dead: bool, date: &str) -> ScoEntry {
    ScoEntry {
        name: name.to_string(),
        money,
        dead,
        date: date.to_string(),
    }
}

#[test]
fn parse_reads_entries_and_skips_empty_slots() {
    let fixture = Fixture::new("parse");
    let score = fixture.dir.join("dopewars.sco");
    write_score(
        &score,
        &[
            record("Alice", "01-02-2026", "1234567", 1),
            record("Bob", "02-02-2026", "-500", 0),
            record("Zero", "03-02-2026", "", 0),
        ],
    );
    assert_eq!(
        parse_sco(&fs::read(&score).unwrap()),
        vec![
            entry("Alice", 1_234_567, true, "01-02-2026"),
            entry("Bob", -500, false, "02-02-2026"),
            entry("Zero", 0, false, "03-02-2026"),
        ]
    );
}

#[test]
fn parse_rejects_a_missing_header() {
    assert!(parse_sco(b"not a score file").is_empty());
}

#[test]
fn parse_ignores_a_truncated_tail() {
    let mut bytes = SCORE_HEADER.to_vec();
    bytes.extend_from_slice(&record("Alice", "01-02-2026", "1000", 0));
    bytes.extend_from_slice(b"Bob\0partial");
    assert_eq!(
        parse_sco(&bytes),
        vec![entry("Alice", 1000, false, "01-02-2026")]
    );
}

#[test]
fn a_session_logs_each_game_once_however_often_it_syncs() {
    let fixture = Fixture::new("sync");
    let mut table = fixture.session("home");
    write_score(
        table.score_file(),
        &[record("Alice", "14-11-2023", "1234567", 0)],
    );
    assert_eq!(fixture.sync(&mut table, NOW), 1);
    assert_eq!(
        fixture.sync(&mut table, NOW),
        0,
        "a rewrite of the same table appends nothing"
    );

    // "Play again?": a second game, ranked above the first.
    write_score(
        table.score_file(),
        &[
            record("Alice", "14-11-2023", "9000000", 1),
            record("Alice", "14-11-2023", "1234567", 0),
        ],
    );
    assert_eq!(fixture.sync(&mut table, NOW), 1);
    assert_eq!(
        log_lines(&fixture.runs()),
        [
            format!("Alice\t1234567\t0\t{NOW}\t14-11-2023"),
            format!("Alice\t9000000\t1\t{NOW}\t14-11-2023"),
        ]
    );
}

/// The reported bug: one player's 18 big runs filled the shared table, and a
/// smaller run by anyone else never made it, so it never reached the board.
/// Each session's table starts empty, so another player's game lands however
/// far below the big runs it is.
#[test]
fn a_small_run_is_logged_beside_another_players_full_table() {
    let fixture = Fixture::new("crowded");
    let mut grinder = fixture.session("grinder");
    let big: Vec<Vec<u8>> = (0..18)
        .map(|i| record("Grinder", "14-11-2023", &(90_000_000 - i).to_string(), 0))
        .collect();
    write_score(grinder.score_file(), &big);
    assert_eq!(fixture.sync(&mut grinder, NOW), 18);

    let mut newcomer = fixture.session("newcomer");
    write_score(
        newcomer.score_file(),
        &[record("Newcomer", "14-11-2023", "-500", 1)],
    );
    assert_eq!(fixture.sync(&mut newcomer, NOW), 1);
    assert_eq!(
        log_lines(&fixture.runs()).last().unwrap(),
        &format!("Newcomer\t-500\t1\t{NOW}\t14-11-2023")
    );
}

/// Sessions only skip what they logged themselves: the same player ending
/// two sessions on the same money the same day played two runs.
#[test]
fn identical_runs_in_two_sessions_are_both_logged() {
    let fixture = Fixture::new("twins");
    for home in ["first", "second"] {
        let mut table = fixture.session(home);
        write_score(
            table.score_file(),
            &[record("Alice", "14-11-2023", "1000", 0)],
        );
        assert_eq!(fixture.sync(&mut table, NOW), 1);
    }
    assert_eq!(log_lines(&fixture.runs()).len(), 2);
}

/// `NOW` is 14-11-2023 UTC. A run the table dates to that day ended now; one
/// it dates to another day was synced after midnight and keeps its own day,
/// so it lands on the board of the month it was played.
#[test]
fn a_run_found_on_a_later_day_keeps_the_day_it_was_played() {
    let fixture = Fixture::new("late");
    let mut table = fixture.session("home");
    write_score(
        table.score_file(),
        &[
            record("Alice", "14-11-2023", "1000", 0),
            record("Bob", "31-10-2023", "2000", 0),
        ],
    );
    assert_eq!(fixture.sync(&mut table, NOW), 2);
    let ended: Vec<i64> = log_lines(&fixture.runs())
        .iter()
        .map(|line| line.split('\t').nth(3).unwrap().parse().unwrap())
        .collect();
    assert_eq!(ended, [NOW, date_noon_epoch("31-10-2023").unwrap()]);
}

/// A session that never finished a game leaves no table and logs nothing.
#[test]
fn a_session_without_a_table_appends_nothing() {
    let fixture = Fixture::new("missing");
    let mut table = fixture.session("home");
    assert_eq!(fixture.sync(&mut table, NOW), 0);
    assert!(!fixture.runs().exists());
}
