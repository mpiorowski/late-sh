use std::collections::HashSet;

use chrono::NaiveDate;
use late_core::models::paper::{
    PaperEdition, PaperRoomPage, PaperSection, PaperSectionKind, PaperStatus,
};
use uuid::Uuid;

use super::{
    PAPER_ELSEWHERE_LIMIT, PaperCommand, PaperInk, PaperLayout, PaperLine, PaperWall, lay_out,
    parse_paper_command,
};
use crate::app::artboard::gallery::ui::PaintRun;

fn page(
    id: u128,
    label: &str,
    status: PaperStatus,
    messages: i64,
    text: Option<&str>,
) -> PaperRoomPage {
    PaperRoomPage {
        room_id: Uuid::from_u128(id),
        label: label.to_string(),
        member_count: 10 + id as i64,
        kind: match label {
            "lounge" => "lounge",
            "pl" => "language",
            _ => "topic",
        }
        .to_string(),
        permanent: label == "lounge",
        status,
        message_count: messages,
        author_count: 3,
        text: text.map(str::to_string),
    }
}

fn plain(lines: &[PaperLine]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn the_paper_follows_the_rail_then_elsewhere_then_the_back_pages() {
    let edition = PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        // As the rows come back: by message count, busiest first.
        rooms: vec![
            page(
                1,
                "lounge",
                PaperStatus::Ready,
                42,
                Some("- lounge line one\n- lounge line two"),
            ),
            page(2, "rust", PaperStatus::Ready, 30, Some("- rust line")),
            page(9, "pl", PaperStatus::Ready, 28, Some("- pl line")),
            page(3, "retro", PaperStatus::Ready, 25, Some("- retro line")),
            page(4, "music", PaperStatus::Ready, 20, Some("- music line")),
            page(5, "art", PaperStatus::Ready, 12, Some("- art line")),
            page(6, "dnd", PaperStatus::Ready, 9, Some("- dnd line")),
            page(7, "quietroom", PaperStatus::Quiet, 2, None),
            page(8, "slow", PaperStatus::Printing, 7, None),
            page(10, "broken", PaperStatus::Failed, 15, None),
        ],
        sections: vec![
            PaperSection {
                section: PaperSectionKind::Outside,
                status: PaperStatus::Quiet,
                text: None,
            },
            PaperSection {
                section: PaperSectionKind::Reading,
                status: PaperStatus::Ready,
                text: Some("- someone shared a thing".to_string()),
            },
        ],
    };
    // The reader is in lounge, rust, quietroom, slow, and broken; the rail
    // puts the favorite (rust) first. Of the rooms they are not in, `dnd`
    // is bumped and `pl` is a language room.
    let member_room_ids: HashSet<Uuid> = [1u128, 2, 7, 8, 10]
        .into_iter()
        .map(Uuid::from_u128)
        .collect();
    let rail_order = [
        Uuid::from_u128(2),
        Uuid::from_u128(1),
        Uuid::from_u128(7),
        Uuid::from_u128(8),
        Uuid::from_u128(10),
    ];
    let bumped = vec!["dnd".to_string()];

    let lines = plain(&lay_out(PaperLayout {
        wall: &[],
        edition: &edition,
        rail_order: &rail_order,
        member_room_ids: &member_room_ids,
        bumped_labels: &bumped,
    }));

    assert_eq!(
        lines,
        vec![
            "by @graybeard · covers Wed Sep 2 (UTC) · he read it all so you would not have to",
            "",
            "YOUR ROOMS",
            "",
            "#rust · 30 messages · 12 people",
            "- rust line",
            "",
            "#lounge · 42 messages · 11 people",
            "- lounge line one",
            "- lounge line two",
            "",
            "ELSEWHERE ON LATE.SH",
            "",
            "#dnd · 9 messages · 16 members · bumped · /join #dnd",
            "- dnd line",
            "",
            // A language room: no `/join` hint, since `/join #pl` would
            // open a topic room named "pl" instead.
            "#pl · 28 messages · 19 members",
            "- pl line",
            "",
            "#retro · 25 messages · 13 members · /join #retro",
            "- retro line",
            "",
            "WHAT WE WERE READING",
            "- someone shared a thing",
            "",
            "quiet: #quietroom · still at the press: #slow · missed the press: #broken",
        ]
    );
    // `music` and `art` were the elsewhere rooms past the cap.
    assert_eq!(PAPER_ELSEWHERE_LIMIT, 3);
    assert!(!lines.iter().any(|line| line.contains("#music")));
    assert!(!lines.iter().any(|line| line.contains("#art")));
}

#[test]
fn a_member_room_missing_from_the_rail_still_gets_its_column() {
    let edition = PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        rooms: vec![page(1, "lounge", PaperStatus::Ready, 8, Some("- a line"))],
        sections: Vec::new(),
    };
    let member_room_ids: HashSet<Uuid> = [Uuid::from_u128(1)].into_iter().collect();
    let lines = plain(&lay_out(PaperLayout {
        wall: &[],
        edition: &edition,
        rail_order: &[],
        member_room_ids: &member_room_ids,
        bumped_labels: &[],
    }));
    assert_eq!(
        &lines[1..],
        vec![
            "",
            "YOUR ROOMS",
            "",
            "#lounge · 8 messages · 11 people",
            "- a line"
        ]
    );
}

#[test]
fn paper_commands_parse_and_everything_else_falls_through() {
    assert_eq!(
        parse_paper_command("/paper"),
        Some(Some(PaperCommand::Open))
    );
    assert_eq!(
        parse_paper_command("  /paper  "),
        Some(Some(PaperCommand::Open))
    );
    assert_eq!(
        parse_paper_command("/paper on"),
        Some(Some(PaperCommand::On))
    );
    assert_eq!(
        parse_paper_command("/paper off"),
        Some(Some(PaperCommand::Off))
    );
    assert_eq!(
        parse_paper_command("/paper outside on"),
        Some(Some(PaperCommand::OutsideOn))
    );
    assert_eq!(
        parse_paper_command("/paper outside off"),
        Some(Some(PaperCommand::OutsideOff))
    );
    assert_eq!(
        parse_paper_command("/paper print"),
        Some(Some(PaperCommand::Print))
    );
    assert_eq!(
        parse_paper_command("/paper preview"),
        Some(Some(PaperCommand::Preview))
    );
    assert_eq!(
        parse_paper_command("/paper reset"),
        Some(Some(PaperCommand::Reset))
    );
    // Junk after the command is a usage banner, not a chat line.
    assert_eq!(parse_paper_command("/paper yesterday"), Some(None));
    // Not the command at all: posts as text.
    assert_eq!(parse_paper_command("/papers"), None);
    assert_eq!(parse_paper_command("paper"), None);

    assert!(!PaperCommand::Open.admin_only());
    assert!(PaperCommand::Off.admin_only());
    assert!(PaperCommand::Preview.admin_only());
}

/// A wall piece `height` lines tall, its first line painted red and the
/// rest plain.
fn wall_piece(title: &str, applause: i64, height: usize) -> PaperWall {
    let red = dartboard_core::RgbColor::new(255, 0, 0);
    let lines = (0..height)
        .map(|row| {
            vec![PaintRun {
                text: format!("row {row}"),
                fg: if row == 0 { Some(red) } else { None },
            }]
        })
        .collect();
    PaperWall {
        title: title.to_string(),
        username: "painter".to_string(),
        applause,
        lines,
    }
}

#[test]
fn the_wall_prints_the_pieces_in_colour_within_its_line_budget() {
    let edition = PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        rooms: Vec::new(),
        sections: Vec::new(),
    };
    // 40 + 2 and 10 + 2 lines fit the budget of 60; the third piece's 10
    // + 2 would pass it and stays on page 4.
    let wall = [
        wall_piece("tall", 5, 40),
        wall_piece("small", 2, 10),
        wall_piece("late", 1, 10),
    ];
    let laid = lay_out(PaperLayout {
        wall: &wall,
        edition: &edition,
        rail_order: &[],
        member_room_ids: &HashSet::new(),
        bumped_labels: &[],
    });
    let lines = plain(&laid);
    let start = lines
        .iter()
        .position(|line| line == "ON THE WALL")
        .expect("the wall heading");
    let titles: Vec<&String> = lines[start..]
        .iter()
        .filter(|line| line.starts_with('"'))
        .collect();
    assert_eq!(
        titles,
        vec![
            "\"tall\" by @painter, hung yesterday, 5 applause so far.",
            "\"small\" by @painter, hung yesterday, 2 applause so far."
        ]
    );
    assert_eq!(
        lines.last().map(String::as_str),
        Some("    the whole wall hangs on page 4, the Artboard gallery")
    );
    // The painted glyphs keep their colour; the rest print as body text.
    let first_row = &laid[start + 3];
    assert_eq!(
        first_row
            .iter()
            .map(|span| (span.text.as_str(), span.ink))
            .collect::<Vec<_>>(),
        vec![
            ("    ", PaperInk::Body),
            (
                "row 0",
                PaperInk::Paint(dartboard_core::RgbColor::new(255, 0, 0))
            )
        ]
    );
    assert_eq!(
        laid[start + 4]
            .iter()
            .map(|span| (span.text.as_str(), span.ink))
            .collect::<Vec<_>>(),
        vec![("    ", PaperInk::Body), ("row 1", PaperInk::Body)]
    );
}

#[test]
fn an_empty_wall_prints_no_column() {
    let edition = PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        rooms: Vec::new(),
        sections: Vec::new(),
    };
    let lines = plain(&lay_out(PaperLayout {
        wall: &[],
        edition: &edition,
        rail_order: &[],
        member_room_ids: &HashSet::new(),
        bumped_labels: &[],
    }));
    assert!(!lines.iter().any(|line| line == "ON THE WALL"));
}
