use chrono::{TimeZone, Utc};
use late_core::models::door_milestone::DoorMilestoneKind;
use late_core::models::door_run::DoorRunResult;

use super::nethack::{parse_livelog_line, parse_xlogfile_line};

// Shaped like a real 5.0.0 line (field order from topten.c writexlentry;
// TAB-separated, epoch-seconds endtime). flags=0x4 is the never-requested-
// bones bit, a normal game.
const DEATH_LINE: &str = "version=5.0.0\tpoints=12345\tdeathdnum=0\tdeathlev=6\tmaxlvl=8\thp=-2\tmaxhp=45\tdeaths=1\tdeathdate=20260807\tbirthdate=20260806\tuid=1000\trole=Val\trace=Dwa\tgender=Fem\talign=Law\tname=GleamingUnicycle\tdeath=killed by a soldier ant\tconduct=0x1000\tturns=23456\tachieve=0x800\tachieveX=entered_gnomish_mines\tconductX=artifacts\trealtime=3600\tstarttime=1754550000\tendtime=1754560000\tgender0=Fem\talign0=Law\tflags=0x4\tgold=120\twish_cnt=0\tarti_wish_cnt=0\tbones=1\trerolls=0";

const ASCENSION_LINE: &str = "version=5.0.0\tpoints=3654321\tdeathdnum=7\tdeathlev=-5\tmaxlvl=53\thp=88\tmaxhp=102\tdeaths=0\tdeathdate=20260807\tbirthdate=20260701\tuid=1000\trole=Wiz\trace=Elf\tgender=Mal\talign=Cha\tname=Wormsong\tdeath=ascended\tconduct=0x0\tturns=81234\tachieve=0x1ff\tachieveX=ascended,obtained_the_amulet_of_yendor\tconductX=\trealtime=90000\tstarttime=1754000000\tendtime=1754560000\tgender0=Mal\talign0=Cha\tflags=0x0\tgold=15000\twish_cnt=3\tarti_wish_cnt=1\tbones=0\trerolls=0";

#[test]
fn parses_a_death_line() {
    let run = parse_xlogfile_line(DEATH_LINE).expect("death line parses");
    assert_eq!(run.name, "GleamingUnicycle");
    assert_eq!(run.result, DoorRunResult::Death);
    assert_eq!(run.score, Some(12345));
    assert_eq!(run.depth, Some(8));
    assert_eq!(run.turns, Some(23456));
    assert_eq!(run.death, "killed by a soldier ant");
    assert_eq!(run.death_level, Some(6));
    assert!(!run.cheat_mode);
    assert_eq!(run.ended_at, Utc.timestamp_opt(1754560000, 0).unwrap());
    // The full line rides along for boards not invented yet.
    assert_eq!(run.raw["role"], "Val");
    assert_eq!(run.raw["conductX"], "artifacts");
}

#[test]
fn parses_an_ascension() {
    let run = parse_xlogfile_line(ASCENSION_LINE).expect("ascension parses");
    assert_eq!(run.result, DoorRunResult::Win);
    assert_eq!(run.score, Some(3654321));
    assert!(!run.cheat_mode);
}

#[test]
fn maps_non_death_ends() {
    for (death, expected) in [
        ("quit", DoorRunResult::Quit),
        ("escaped", DoorRunResult::Leaving),
        ("panic", DoorRunResult::Death),
        ("petrified by a cockatrice", DoorRunResult::Death),
    ] {
        let line = format!("name=X\tdeath={death}\tendtime=1754560000");
        let run = parse_xlogfile_line(&line).expect("line parses");
        assert_eq!(run.result, expected, "death={death}");
    }
}

#[test]
fn flags_wizard_and_explore_games_as_cheat_mode() {
    for flags in ["0x1", "0x2", "0x3", "0x6"] {
        let line = format!("name=X\tdeath=ascended\tendtime=1754560000\tflags={flags}");
        let run = parse_xlogfile_line(&line).expect("line parses");
        assert!(run.cheat_mode, "flags={flags}");
    }
}

#[test]
fn amulet_bit_alone_does_not_win() {
    // Died carrying the Amulet: the achieve bit is set but death is a death,
    // and the bit itself is not read (the livelog pickup line is the only
    // Amulet source).
    let line = "name=X\tdeath=killed by Death\tendtime=1754560000\tachieve=0x20";
    let run = parse_xlogfile_line(line).expect("line parses");
    assert_eq!(run.result, DoorRunResult::Death);
}

#[test]
fn legacy_and_reserved_names_still_parse() {
    // Identity policy (skipping late/late_* and dead handles) is the
    // service's job; the parser hands the name through untouched.
    let line = "name=late_a1b2c3\tdeath=quit\tendtime=1754560000";
    let run = parse_xlogfile_line(line).expect("line parses");
    assert_eq!(run.name, "late_a1b2c3");
}

#[test]
fn rejects_lines_missing_the_core_fields() {
    // Truncated mid-line: no end stamp.
    assert!(parse_xlogfile_line("version=5.0.0\tname=X\tpoints=12").is_none());
    // No name.
    assert!(parse_xlogfile_line("death=quit\tendtime=1754560000").is_none());
    assert!(parse_xlogfile_line("name=\tdeath=quit\tendtime=1754560000").is_none());
    // Garbage stamp.
    assert!(parse_xlogfile_line("name=X\tdeath=quit\tendtime=notatime").is_none());
    // Empty / junk lines.
    assert!(parse_xlogfile_line("").is_none());
    assert!(parse_xlogfile_line("no fields here").is_none());
}

#[test]
fn parses_the_amulet_livelog_line() {
    let line = "lltype=2\tname=Wormsong\trole=Wiz\trace=Elf\tgender=Mal\talign=Cha\tturns=65432\tstarttime=1754000000\tcurtime=1754540000\tmessage=acquired The Amulet of Yendor";
    let milestone = parse_livelog_line(line).expect("amulet line parses");
    assert_eq!(milestone.name, "Wormsong");
    assert_eq!(milestone.kind, Some(DoorMilestoneKind::Amulet));
    assert_eq!(milestone.text, "acquired The Amulet of Yendor");
    assert_eq!(
        milestone.occurred_at,
        Utc.timestamp_opt(1754540000, 0).unwrap()
    );
}

#[test]
fn untracked_livelog_messages_parse_without_a_kind() {
    for message in [
        "entered Gehennom",
        "performed the invocation",
        // A prefix or embedding of the amulet message must not match.
        "acquired The Amulet of Yendor replica",
    ] {
        let line = format!("lltype=2\tname=X\tcurtime=1754540000\tmessage={message}");
        let milestone = parse_livelog_line(&line).expect("line parses");
        assert_eq!(milestone.kind, None, "message={message}");
    }
}

#[test]
fn rejects_livelog_lines_missing_the_core_fields() {
    assert!(parse_livelog_line("name=X\tmessage=acquired The Amulet of Yendor").is_none());
    assert!(parse_livelog_line("curtime=1754540000\tmessage=whatever").is_none());
    assert!(parse_livelog_line("lltype=2\tname=X\tcurtime=1754540000").is_none());
}
