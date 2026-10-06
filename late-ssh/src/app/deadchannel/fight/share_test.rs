use chrono::NaiveDate;
use uuid::Uuid;

use super::road_card;
use crate::app::arcade::share::{ShareFormat, puzzle_number, render};
use crate::app::deadchannel::fight::road::{Mark, Trace};
use crate::app::deadchannel::fight::state::Sheet;

fn sheet() -> Sheet {
    let mut sheet = Sheet::fresh(
        Uuid::nil(),
        NaiveDate::from_ymd_opt(2026, 9, 24).expect("a day"),
    );
    sheet.level = 7;
    sheet.signal = 12;
    sheet
}

fn step(lane: u8, mark: Mark) -> Trace {
    Trace { lane, mark }
}

/// The day's card: the lane walked, lit by how each step went, on a road
/// everybody had. Whole, as it is pasted.
#[test]
fn a_walked_road_is_a_card() {
    let mut walked = sheet();
    walked.rations_left = 0;
    walked.kills_today = 4;
    walked.road.path = vec![
        step(1, Mark::Won),
        step(0, Mark::Cached),
        step(0, Mark::Won),
        step(1, Mark::Mended),
        step(2, Mark::BrightWon),
        step(2, Mark::Cleared),
        step(1, Mark::Ran),
        step(1, Mark::Cached),
        step(1, Mark::Mended),
        step(0, Mark::Won),
    ];
    let number = puzzle_number(walked.day);

    let card = road_card(&walked).expect("a walked road has a card");

    assert_eq!(
        render(&card, ShareFormat::Ascii),
        format!(
            "late.sh the road #{number} · walked\n\
             .-#......#\n\
             #..@..o-@.\n\
             ....+@....\n\
             lv 7 · 4 glyphs · signal 12/70\n\
             ssh late.sh"
        )
    );
    assert!(render(&card, ShareFormat::Emoji).contains("⬛⬜🟩⬛⬛⬛⬛⬛⬛🟩"));
}

/// Falling is a card too, and a better one: the step it happened on is
/// marked and named, and the road past it stays dark.
#[test]
fn a_fall_is_a_card_and_a_road_still_being_walked_is_not() {
    let mut fell = sheet();
    fell.signal = 0;
    fell.rations_left = 7;
    fell.kills_today = 1;
    fell.road.path = vec![
        step(1, Mark::Won),
        step(1, Mark::Cached),
        step(2, Mark::Fell),
    ];
    let card = road_card(&fell).expect("a fall has a card");
    let text = render(&card, ShareFormat::Ascii);
    assert!(text.starts_with("late.sh the road #"), "{text}");
    assert!(text.contains("· fell at 3\n"), "{text}");
    assert!(text.contains("\n#-........\n..*.......\n"), "{text}");
    assert!(text.contains("lv 7 · 1 glyph · signal 0/70"), "{text}");

    // Steps still to take, or a fight still on: no card yet.
    let mut walking = sheet();
    walking.rations_left = 7;
    walking.road.path = fell.road.path[..2].to_vec();
    assert_eq!(road_card(&walking), None);
    // And a runner who dropped before ever stepping has no road to show.
    let mut nothing = sheet();
    nothing.signal = 0;
    assert_eq!(road_card(&nothing), None);
}
