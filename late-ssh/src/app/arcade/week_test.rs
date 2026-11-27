use std::collections::HashSet;

use late_core::models::leaderboard::DailyPuzzle;
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use super::*;
use crate::app::hub::dailies::state::RungStanding;

fn day(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

fn plain(lines: &[Line<'_>]) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

/// Every badge is its stated size, so seven of them line up and every
/// frame closes.
#[test]
fn every_badge_is_its_size() {
    for puzzle in DailyPuzzle::ALL.iter().copied() {
        for row in small_art(puzzle) {
            assert_eq!(row.width(), BADGE_ART_WIDTH, "{puzzle:?} small {row:?}");
        }
        assert!(short_name(puzzle).width() <= BADGE_WIDTH, "{puzzle:?}");
    }
}

/// The week runs from today: seven distinct games, today and tomorrow
/// named, and the star count is the snapshot's stars with the viewer's
/// own OR'd in once.
#[test]
fn the_week_is_seven_days_from_today_and_counts_todays_stars() {
    let monday = day(2026, 11, 9);
    let me = Uuid::from_u128(1);
    let alice = Uuid::from_u128(2);
    let starred = HashSet::from([alice]);

    let week = Week::today(monday, &starred, RungStanding::Open, me);
    assert_eq!(
        week.days.map(|d| d.kind),
        [
            DayKind::Today,
            DayKind::Tomorrow,
            DayKind::Ahead,
            DayKind::Ahead,
            DayKind::Ahead,
            DayKind::Ahead,
            DayKind::Ahead,
        ]
    );
    assert_eq!(week.days[0].day, monday);
    assert_eq!(week.days[6].day, day(2026, 11, 15));
    let mut games: Vec<DailyPuzzle> = week.days.iter().map(|d| d.rung.puzzle).collect();
    games.dedup();
    assert_eq!(games.len(), DAYS, "a week never repeats a game");
    assert_eq!((week.today_stars, week.today_won), (1, false));

    // The viewer's own star counts once, before and after the snapshot
    // catches up with the hub.
    let week = Week::today(monday, &starred, RungStanding::Done, me);
    assert_eq!((week.today_stars, week.today_won), (2, true));
    let mut caught_up = starred.clone();
    caught_up.insert(me);
    let week = Week::today(monday, &caught_up, RungStanding::Done, me);
    assert_eq!((week.today_stars, week.today_won), (2, true));
}

/// The lobby row: seven columns the same width, today starred once won,
/// its stars under it, and the rotation line under the row. Narrow
/// lobbies get the same days as a list.
#[test]
fn the_lobby_row_lines_up_seven_badges_and_narrows_to_a_list() {
    let monday = day(2026, 11, 9);
    let me = Uuid::from_u128(1);
    let week = Week::today(monday, &HashSet::new(), RungStanding::Done, me);

    let rows = plain(&week.lobby_lines(80));
    assert_eq!(rows.len(), LOBBY_COLUMN_ROWS + 1, "{rows:#?}");
    assert_eq!(
        rows[0],
        " ✦ today  tomorrow   Wed 11    Thu 12    Fri 13    Sat 14    Sun 15",
    );
    assert_eq!(
        rows[1],
        "┌───────┐ ┏━━━━━━━┓ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐ ┌───────┐",
    );
    assert_eq!(
        rows[2],
        "│ 5│ │3 │ ┃ ╶─■─╴ ┃ │ □■□□■ │ │ 1 ▓ 1 │ │  │1 2 │ │ ▓k▓ ▓ │ │┌─┐ ┌─┐│"
    );
    assert_eq!(
        rows[5],
        "└───────┘ ┗━━━━━━━┛ └───────┘ └───────┘ └───────┘ └───────┘ └───────┘",
    );
    assert_eq!(
        rows[6],
        " sudoku    netwalk   le word    mines   nonogram    chess   solitaire",
    );
    assert_eq!(
        rows[7],
        " medium     hard               medium    medium     hard     draw-1",
    );
    assert_eq!(rows[8], "   1 ✦");
    assert_eq!(rows[9], "  11 boards, one a day, all new at 00:00 UTC.");
    for row in &rows[..LOBBY_COLUMN_ROWS] {
        assert!(row.width() <= BADGE_ROW_WIDTH, "{row:?}");
    }

    let list = plain(&week.lobby_lines(60));
    assert_eq!(list.len(), DAYS + 1, "{list:#?}");
    assert_eq!(list[0], "  ✦ today  medium Sudoku           1 ✦");
    assert_eq!(list[1], "  tomorrow hard Netwalk");
    assert_eq!(list[6], "  Sun 15   draw-1 Solitaire");
    assert_eq!(list[7], "  11 boards, one a day, all new at 00:00 UTC.");
}

/// The lobby's panel: seven badges stacked edge to edge, the day and the
/// board beside each, today starred and counted, tomorrow heavy with when
/// it comes up, the rotation under them; its stated size exactly.
#[test]
fn the_panel_stacks_the_week_with_the_day_and_board_beside_each_badge() {
    let monday = day(2026, 11, 9);
    let me = Uuid::from_u128(1);
    let week = Week::today(monday, &HashSet::new(), RungStanding::Done, me);
    let rows = plain(&week.panel_lines());
    assert_eq!(rows.len(), usize::from(PANEL_HEIGHT), "{rows:#?}");
    assert_eq!(rows[0], "─── The Week ───");
    assert_eq!(rows[1], "");
    assert_eq!(rows[2], "┌───────┐  ✦ today · Mon 9");
    assert_eq!(rows[3], "│ 5│ │3 │  medium Sudoku");
    assert_eq!(rows[4], "│ ─┼─┼─ │  1 ✦");
    assert_eq!(rows[6], "└───────┘");
    assert_eq!(rows[7], "┏━━━━━━━┓  tomorrow · Tue 10");
    assert_eq!(rows[8], "┃ ╶─■─╴ ┃  hard Netwalk");
    assert_eq!(rows[9], "┃   │   ┃  up at 00:00 UTC");
    assert_eq!(rows[12], "┌───────┐  Wed 11");
    assert_eq!(rows[13], "│ □■□□■ │  Le Word");
    assert_eq!(rows[32], "┌───────┐  Sun 15");
    assert_eq!(rows[33], "│┌─┐ ┌─┐│  draw-1 Solitaire");
    assert_eq!(rows[36], "└───────┘");
    assert_eq!(rows[37], "");
    assert_eq!(rows[38], "11 boards, one a day,");
    assert_eq!(rows[39], "all new at 00:00 UTC.");
    for row in &rows {
        assert!(row.width() <= usize::from(PANEL_WIDTH), "{row:?}");
    }
}

/// The tour's box: today's and tomorrow's badges at the lobby's size, the
/// pitch beside them on the art rows, where the rest is on the names' row,
/// and nothing else.
#[test]
fn the_tour_shows_today_and_tomorrow_as_badges_with_the_pitch_beside() {
    let monday = day(2026, 11, 9);
    let week = Week::today(
        monday,
        &HashSet::new(),
        RungStanding::Open,
        Uuid::from_u128(1),
    );
    let rows = plain(&week.tour_lines());
    assert_eq!(rows.len(), LOBBY_COLUMN_ROWS, "{rows:#?}");
    let netwalk = small_art(DailyPuzzle::Netwalk);
    assert_eq!(
        rows[0],
        format!(
            "{}   {}",
            centered("today", BADGE_WIDTH),
            centered("tomorrow", BADGE_WIDTH)
        )
        .trim_end()
    );
    assert_eq!(rows[1], "┌───────┐   ┏━━━━━━━┓");
    assert_eq!(
        rows[2],
        format!(
            "│ 5│ │3 │   ┃{}┃   up at 00:00 UTC: your first star.",
            netwalk[0]
        )
    );
    assert!(
        rows[3].ends_with("┃   one board a day, the same for everyone."),
        "{rows:#?}"
    );
    assert!(
        rows[4].ends_with("┃   win it on its day for the star ✦"),
        "{rows:#?}"
    );
    assert_eq!(rows[5], "└───────┘   ┗━━━━━━━┛");
    assert_eq!(
        rows[6],
        format!(
            "{}   {}   the days after sit in the Arcade lobby, on The Week.",
            centered("sudoku", BADGE_WIDTH),
            centered("netwalk", BADGE_WIDTH)
        )
    );
    assert_eq!(
        rows[7],
        format!(
            "{}   {}",
            centered("medium", BADGE_WIDTH),
            centered("hard", BADGE_WIDTH)
        )
        .trim_end()
    );
    assert_eq!(rows[8], "   0 ✦");
    assert!(
        !rows.iter().any(|row| row.contains("and then")),
        "{rows:#?}"
    );
}
