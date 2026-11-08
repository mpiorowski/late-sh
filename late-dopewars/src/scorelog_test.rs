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

    fn score(&self) -> PathBuf {
        self.dir.join("dopewars.sco")
    }

    fn runs(&self) -> PathBuf {
        runs_log_path(&self.score().to_string_lossy())
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
    write_score(
        &fixture.score(),
        &[
            record("Alice", "01-02-2026", "1234567", 1),
            record("Bob", "02-02-2026", "-500", 0),
            record("Zero", "03-02-2026", "", 0),
        ],
    );
    assert_eq!(
        parse_sco(&fs::read(fixture.score()).unwrap()),
        vec![
            entry("Alice", 1_234_567, true, "01-02-2026"),
            entry("Bob", -500, false, "02-02-2026"),
            entry("Zero", 0, false, "03-02-2026"),
        ]
    );
}

#[test]
fn parse_rejects_a_missing_header() {
    let fixture = Fixture::new("noheader");
    fs::write(fixture.score(), b"not a score file").unwrap();
    assert!(parse_sco(&fs::read(fixture.score()).unwrap()).is_empty());
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
fn sync_appends_once_and_is_idempotent() {
    let fixture = Fixture::new("sync");
    write_score(
        &fixture.score(),
        &[
            record("Alice", "01-02-2026", "1234567", 0),
            record("Bob", "02-02-2026", "-500", 1),
        ],
    );
    assert_eq!(
        sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap(),
        2
    );
    assert_eq!(
        sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap(),
        0,
        "a rewrite of the same table appends nothing"
    );
    let lines = log_lines(&fixture.runs());
    assert_eq!(lines.len(), 2);
    let first: Vec<&str> = lines[0].split('\t').collect();
    assert_eq!(first[0], "Alice");
    assert_eq!(&first[1..3], ["1234567", "0"]);
    assert_eq!(first[4], "01-02-2026");
    let second: Vec<&str> = lines[1].split('\t').collect();
    assert_eq!(&second[0..3], ["Bob", "-500", "1"]);
}

#[test]
fn only_the_new_entry_is_appended() {
    let fixture = Fixture::new("delta");
    write_score(
        &fixture.score(),
        &[record("Alice", "01-02-2026", "1000", 0)],
    );
    sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap();

    write_score(
        &fixture.score(),
        &[
            record("Alice", "01-02-2026", "1000", 0),
            record("Carol", "02-02-2026", "2000", 1),
        ],
    );
    assert_eq!(
        sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap(),
        1
    );
    let lines = log_lines(&fixture.runs());
    assert_eq!(lines.len(), 2);
    assert!(lines[1].starts_with("Carol\t2000\t1\t"), "{:?}", lines[1]);
}

#[test]
fn cold_start_uses_the_score_date_and_later_syncs_use_now() {
    let fixture = Fixture::new("cold");
    write_score(
        &fixture.score(),
        &[record("Alice", "15-03-2026", "1000", 0)],
    );
    sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap();
    let backfilled = log_lines(&fixture.runs()).remove(0);
    let ended: i64 = backfilled.split('\t').nth(3).unwrap().parse().unwrap();
    assert_eq!(ended, date_noon_epoch("15-03-2026").unwrap());

    write_score(
        &fixture.score(),
        &[
            record("Alice", "15-03-2026", "1000", 0),
            record("Bob", "16-03-2026", "2000", 0),
        ],
    );
    sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap();
    let live = log_lines(&fixture.runs()).remove(1);
    let ended: i64 = live.split('\t').nth(3).unwrap().parse().unwrap();
    assert_eq!(ended, NOW);
}

#[test]
fn sync_without_a_score_file_appends_nothing() {
    let fixture = Fixture::new("missing");
    assert_eq!(
        sync_from_sco(&fixture.score(), &fixture.runs(), NOW).unwrap(),
        0
    );
    assert!(!fixture.runs().exists());
}
