use chrono::{TimeZone, Utc};
use late_core::models::door_milestone::DoorMilestoneKind;
use late_core::models::door_run::DoorRunResult;

use super::dcss::{parse_logfile_line, parse_milestone_line};

// Shaped like a real 0.34.1 line (field order from set_base_xlog_fields +
// set_score_fields). Note the xlog stamp quirk: the month is written 0-based
// (raw tm_mon), so "07" in the stamp is August.
const DEATH_LINE: &str = "v=0.34.1:vlong=0.34.1:lv=0.1:name=GleamingUnicycle:race=Minotaur:cls=Berserker:char=MiBe:xl=14:sk=Axes:sklev=14:title=Cleaver:place=D::10:br=D:lvl=10:absdepth=10:hp=-3:mhp=98:god=Trog:start=20260630120000S:dur=4321:turn=23456:kills=345:gold=512:goldfound=700:goldspent=188:sc=54321:ktyp=mon:killer=an orc warrior:dam=12:kaux=:end=20260706221530S:seed=123456789:tmsg=slain by an orc warrior";

const WIN_LINE: &str = "v=0.34.1:lv=0.1:name=Wormsong:race=Spriggan:cls=Enchanter:char=SpEn:xl=27:place=D::0:absdepth=0:urune=3:nrune=3:turn=91234:sc=2345678:ktyp=winning:end=20260706230000S:tmsg=escaped with the Orb and 3 runes!";

#[test]
fn parses_a_death_line() {
    let run = parse_logfile_line(DEATH_LINE).expect("death line parses");
    assert_eq!(run.name, "GleamingUnicycle");
    assert_eq!(run.result, DoorRunResult::Death);
    assert_eq!(run.score, Some(54321));
    assert_eq!(run.depth, Some(10));
    assert_eq!(run.turns, Some(23456));
    assert_eq!(
        run.death_message.as_deref(),
        Some("slain by an orc warrior")
    );
    // The `::` escape in place unescapes to a literal colon.
    assert_eq!(run.place.as_deref(), Some("D:10"));
    // Stamp month is 0-based: 07 -> August.
    assert_eq!(
        run.ended_at,
        Utc.with_ymd_and_hms(2026, 8, 6, 22, 15, 30).unwrap()
    );
    // The full line rides along for boards not invented yet.
    assert_eq!(run.raw["killer"], "an orc warrior");
    assert_eq!(run.raw["god"], "Trog");
}

#[test]
fn parses_a_win_line() {
    let run = parse_logfile_line(WIN_LINE).expect("win line parses");
    assert_eq!(run.result, DoorRunResult::Win);
    assert_eq!(run.score, Some(2345678));
}

#[test]
fn maps_non_death_ktyps() {
    for (ktyp, expected) in [
        ("quitting", DoorRunResult::Quit),
        ("leaving", DoorRunResult::Leaving),
        ("beam", DoorRunResult::Death),
        ("starvation", DoorRunResult::Death),
    ] {
        let line = format!("name=X:ktyp={ktyp}:end=20260006120000S");
        let run = parse_logfile_line(&line).expect("line parses");
        assert_eq!(run.result, expected, "ktyp={ktyp}");
    }
}

#[test]
fn legacy_and_reserved_names_still_parse() {
    // Identity policy (skipping late/late_* and dead handles) is the
    // service's job; the parser hands the name through untouched.
    let line = "name=late_a1b2c3:ktyp=mon:end=20260006120000S";
    let run = parse_logfile_line(line).expect("line parses");
    assert_eq!(run.name, "late_a1b2c3");
}

#[test]
fn rejects_lines_missing_the_core_fields() {
    // Truncated mid-line: no end stamp.
    assert!(parse_logfile_line("v=0.34.1:name=X:sc=123:ktyp=mon").is_none());
    // No name.
    assert!(parse_logfile_line("ktyp=mon:end=20260006120000S").is_none());
    assert!(parse_logfile_line("name=:ktyp=mon:end=20260006120000S").is_none());
    // Garbage stamp.
    assert!(parse_logfile_line("name=X:ktyp=mon:end=notatime").is_none());
    // Empty / junk lines.
    assert!(parse_logfile_line("").is_none());
    assert!(parse_logfile_line("no fields here").is_none());
}

#[test]
fn parses_the_orb_milestone() {
    // Shaped like a real 0.34.1 milestone line: mark_milestone writes the
    // same base xlog fields as the logfile (including absdepth) plus the
    // milestone trio.
    let line = "v=0.34.1:name=Wormsong:race=Spriggan:cls=Enchanter:xl=25:place=Zot::5:br=Zot:lvl=5:absdepth=27:oplace=Zot::5:time=20260706215500S:type=orb:milestone=found the Orb of Zot!";
    let milestone = parse_milestone_line(line).expect("orb line parses");
    assert_eq!(milestone.name, "Wormsong");
    assert_eq!(milestone.kind, Some(DoorMilestoneKind::Orb));
    assert_eq!(milestone.text, "found the Orb of Zot!");
    assert_eq!(
        milestone.occurred_at,
        Utc.with_ymd_and_hms(2026, 8, 6, 21, 55, 0).unwrap()
    );
    // The Deepest Dive board unions `raw->>'absdepth'` from milestone rows
    // (a winner's logfile line ends at the surface), so the parser carrying
    // this field through into `raw` is load-bearing, not decoration.
    assert_eq!(milestone.raw["absdepth"], "27");
}

#[test]
fn parses_rune_and_branch_entry_kinds() {
    let rune = parse_milestone_line(
        "name=X:time=20260006120000S:type=rune:milestone=found a slimy rune of Zot.",
    )
    .expect("rune line parses");
    assert_eq!(rune.kind, Some(DoorMilestoneKind::Rune));

    let branch = parse_milestone_line(
        "name=X:time=20260006120000S:type=br.enter:milestone=entered the Lair of Beasts.:oplace=D::8",
    )
    .expect("branch line parses");
    assert_eq!(branch.kind, Some(DoorMilestoneKind::BranchEnter));
}

#[test]
fn untracked_milestone_types_parse_without_a_kind() {
    let line =
        "name=X:time=20260006120000S:type=god.worship:milestone=became a worshipper of Trog.";
    let milestone = parse_milestone_line(line).expect("line parses");
    assert_eq!(milestone.kind, None);
}

#[test]
fn rejects_milestones_missing_the_core_fields() {
    assert!(parse_milestone_line("name=X:type=orb:milestone=found the Orb of Zot!").is_none());
    assert!(parse_milestone_line("time=20260006120000S:type=orb").is_none());
}
