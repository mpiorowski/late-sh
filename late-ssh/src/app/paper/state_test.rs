use std::collections::HashSet;

use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::paper::{
    PaperEdition, PaperRoomPage, PaperSection, PaperSectionKind, PaperStatus,
};
use uuid::Uuid;

use super::{
    PAPER_ELSEWHERE_LIMIT, PaperAnnouncement, PaperCommand, PaperLayout, PaperLine, PaperWork,
    lay_out, parse_paper_command,
};

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
    // The operator posted twice on the covered day; both print whole,
    // oldest first, before anything graybeard wrote.
    let announcements = [
        PaperAnnouncement {
            author: "mat".to_string(),
            posted_at: Utc.with_ymd_and_hms(2026, 9, 2, 9, 5, 0).unwrap(),
            body: "maintenance tonight at 22:00 UTC\nexpect ten minutes down".to_string(),
        },
        PaperAnnouncement {
            author: "mat".to_string(),
            posted_at: Utc.with_ymd_and_hms(2026, 9, 2, 23, 40, 0).unwrap(),
            body: "back up, thanks for waiting".to_string(),
        },
    ];

    let lines = plain(&lay_out(PaperLayout {
        announcements: &announcements,
        work: None,
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
            "ANNOUNCEMENTS",
            "",
            "@mat · 09:05",
            "maintenance tonight at 22:00 UTC",
            "expect ten minutes down",
            "",
            "@mat · 23:40",
            "back up, thanks for waiting",
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
        announcements: &[],
        work: None,
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

#[test]
fn new_work_speaks_to_the_card_the_reader_has() {
    use late_core::models::work_profile::WorkStatus;

    use crate::app::jobs::state_test::posting;

    let edition = PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        rooms: Vec::new(),
        sections: Vec::new(),
    };
    let lay = |work: Option<&PaperWork>| {
        plain(&lay_out(PaperLayout {
            announcements: &[],
            work,
            edition: &edition,
            rail_order: &[],
            member_room_ids: &HashSet::new(),
            bumped_labels: &[],
        }))
    };
    let section = |line: &str| {
        vec![
            "by @graybeard · covers Wed Sep 2 (UTC) · he read it all so you would not have to"
                .to_string(),
            String::new(),
            "NEW WORK".to_string(),
            line.to_string(),
        ]
    };

    // No card: the one line that sells the card, on a quiet day too.
    let nobody = PaperWork {
        released: 11,
        card: None,
        matches: Vec::new(),
    };
    assert_eq!(
        lay(Some(&nobody)),
        section(
            "11 postings yesterday, all remote. Create a work card on page 5 to see matches here."
        )
    );
    let nobody_quiet = PaperWork {
        released: 0,
        ..nobody.clone()
    };
    assert_eq!(
        lay(Some(&nobody_quiet)),
        section("No new postings yesterday. Create a work card on page 5 to see matches here.")
    );

    // An open card with matches: the matches, then the count.
    let matched = PaperWork {
        released: 11,
        card: Some(WorkStatus::Open),
        matches: vec![
            posting("Acme", &["rust", "postgres"]),
            posting("Fastly", &["go"]),
        ],
    };
    let mut expected = section("Acme · Backend Engineer · remote · EU · rust, postgres · €80k");
    expected.push("Fastly · Backend Engineer · remote · EU · go · €80k".to_string());
    expected.push(
        "    11 postings yesterday. All postings are on page 5, / filters to your tags."
            .to_string(),
    );
    assert_eq!(lay(Some(&matched)), expected);

    // A casual card with nothing on its tags still hears the count, and
    // on a day with no release hears that too.
    let unmatched = PaperWork {
        released: 1,
        card: Some(WorkStatus::Casual),
        matches: Vec::new(),
    };
    assert_eq!(
        lay(Some(&unmatched)),
        section("1 posting yesterday, none matching your tags. All postings are on page 5.")
    );
    let casual_quiet = PaperWork {
        released: 0,
        ..unmatched
    };
    assert_eq!(
        lay(Some(&casual_quiet)),
        section("No new postings yesterday. All postings are on page 5.")
    );

    // Not looking gets a small hint, with the count when there is one.
    let not_looking = PaperWork {
        released: 11,
        card: Some(WorkStatus::NotLooking),
        matches: Vec::new(),
    };
    assert_eq!(
        lay(Some(&not_looking)),
        section("11 postings yesterday. Your work card is set to not looking (page 5).")
    );
    let not_looking_quiet = PaperWork {
        released: 0,
        ..not_looking
    };
    assert_eq!(
        lay(Some(&not_looking_quiet)),
        section("No new postings yesterday. Your work card is set to not looking (page 5).")
    );

    // The job feed switched off (or a preview): no section at all.
    assert_eq!(
        lay(None),
        vec!["by @graybeard · covers Wed Sep 2 (UTC) · he read it all so you would not have to"]
    );
}

#[test]
fn a_room_past_a_hundred_people_prints_a_capped_count() {
    let crowded = PaperRoomPage {
        member_count: 17755,
        ..page(1, "lounge", PaperStatus::Ready, 417, Some("- a line"))
    };
    let exact = PaperRoomPage {
        member_count: 100,
        ..page(2, "rust", PaperStatus::Ready, 30, Some("- a line"))
    };
    let edition = PaperEdition {
        edition: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
        rooms: vec![crowded, exact],
        sections: Vec::new(),
    };
    let rail_order = [Uuid::from_u128(1), Uuid::from_u128(2)];
    let member_room_ids: HashSet<Uuid> = rail_order.into_iter().collect();
    let lines = plain(&lay_out(PaperLayout {
        announcements: &[],
        work: None,
        edition: &edition,
        rail_order: &rail_order,
        member_room_ids: &member_room_ids,
        bumped_labels: &[],
    }));
    assert!(
        lines.contains(&"#lounge · 417 messages · 100+ people".to_string()),
        "{lines:#?}"
    );
    assert!(
        lines.contains(&"#rust · 30 messages · 100 people".to_string()),
        "{lines:#?}"
    );
}
