//! The week: seven days of rungs, today first, one small badge each, drawn
//! three ways. In the Arcade lobby a panel down the right where the
//! terminal has the room (seven badges stacked, the day and the board
//! beside each), else a row of badges above the daily cards, else one line
//! per day; today starred once its rung is won, tomorrow lit, today's star
//! count under it, and the rotation named under the rest. On the
//! first-visit tour's week stop: today small and starred, tomorrow large
//! with its name, the other five as names. Time-ordered, where the cards
//! stay game-ordered. Pure: the rotation and today's marks in, lines out.

use std::collections::HashMap;

use chrono::NaiveDate;
use late_core::models::{
    daily_rung::{ROTATION_LEN, Rung, STAR_GLYPH, rung_for},
    leaderboard::{DailyCompletionStatus, DailyPuzzle, Tiers},
};
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use crate::app::common::theme;

/// Days in the week: today and the six after it.
pub(crate) const DAYS: usize = 7;

/// A small badge's footprint, border included.
const BADGE_WIDTH: usize = 9;
const BADGE_ART_WIDTH: usize = BADGE_WIDTH - 2;
const BADGE_ART_ROWS: usize = 3;

/// The large badge's footprint, border included.
const LARGE_WIDTH: usize = 20;
const LARGE_ART_WIDTH: usize = LARGE_WIDTH - 2;
const LARGE_ART_ROWS: usize = 6;

/// Rows a lobby column takes: the day, the badge, the game, its board, the
/// stars.
const LOBBY_COLUMN_ROWS: usize = 3 + BADGE_ART_ROWS + 3;

/// Columns the badge row needs: seven badges and the gaps between them.
/// Narrower lobbies get the week as a list.
pub(crate) const BADGE_ROW_WIDTH: usize = DAYS * BADGE_WIDTH + (DAYS - 1);

/// Columns beside a stacked badge for its day and board.
const PANEL_TEXT_WIDTH: usize = 24;

/// The lobby panel's footprint: a badge, a gap and the text beside it,
/// and the heading, a breathing row, seven badges stacked edge to edge,
/// a breathing row and the rotation in two lines.
pub(crate) const PANEL_WIDTH: u16 = (BADGE_WIDTH + 2 + PANEL_TEXT_WIDTH) as u16;
pub(crate) const PANEL_HEIGHT: u16 = (2 + DAYS * (BADGE_ART_ROWS + 2) + 3) as u16;

/// Where a day of the week stands against today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DayKind {
    Today,
    Tomorrow,
    Ahead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WeekDay {
    pub(crate) day: NaiveDate,
    pub(crate) rung: Rung,
    pub(crate) kind: DayKind,
}

/// The seven days from today, with today's marks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Week {
    pub(crate) days: [WeekDay; DAYS],
    /// Stars on today's rung so far, the viewer's own counted: everyone
    /// whose marks today include the rung's board.
    pub(crate) today_stars: usize,
    /// The viewer has today's star.
    pub(crate) today_won: bool,
}

/// Whether this session's user has today's star, read off the leaderboard
/// snapshot's marks for the day with the session's own wins OR'd in, the
/// way the daily cards read them.
pub(crate) fn rung_won_today(
    today: NaiveDate,
    statuses: &HashMap<Uuid, DailyCompletionStatus>,
    own_session: Option<&DailyCompletionStatus>,
    own_user_id: Uuid,
) -> bool {
    let rung = rung_for(today);
    statuses.get(&own_user_id).is_some_and(|s| rung.won_in(s))
        || own_session.is_some_and(|s| rung.won_in(s))
}

impl Week {
    /// The week from `today`, read the way `rung_won_today` reads it, for
    /// everyone on the snapshot.
    pub(crate) fn today(
        today: NaiveDate,
        statuses: &HashMap<Uuid, DailyCompletionStatus>,
        own_session: Option<&DailyCompletionStatus>,
        own_user_id: Uuid,
    ) -> Self {
        let rung = rung_for(today);
        let starred = |status: &DailyCompletionStatus| rung.won_in(status);
        let own_in_snapshot = statuses.get(&own_user_id).is_some_and(starred);
        let today_won = rung_won_today(today, statuses, own_session, own_user_id);
        let session_only = usize::from(today_won && !own_in_snapshot);
        let today_stars = statuses.values().filter(|s| starred(s)).count() + session_only;
        let days = std::array::from_fn(|offset| {
            let day = today + chrono::Duration::days(offset as i64);
            let kind = match offset {
                0 => DayKind::Today,
                1 => DayKind::Tomorrow,
                _ => DayKind::Ahead,
            };
            WeekDay {
                day,
                rung: rung_for(day),
                kind,
            }
        });
        Self {
            days,
            today_stars,
            today_won,
        }
    }

    /// The lobby's week, for a list `width` columns wide: the badge row
    /// where it fits, one line per day where it does not, and the rotation
    /// line under either.
    pub(crate) fn lobby_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = if usize::from(width) >= BADGE_ROW_WIDTH {
            self.badge_rows()
        } else {
            self.list_rows()
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(rotation_sentence(), Style::default().fg(theme::TEXT_DIM())),
        ]));
        lines
    }

    /// The lobby's panel: seven badges stacked, the day, the board and
    /// (today) the stars beside each, the rotation under them. Exactly
    /// [`PANEL_HEIGHT`] lines, none wider than [`PANEL_WIDTH`].
    pub(crate) fn panel_lines(&self) -> Vec<Line<'static>> {
        let mut lines = vec![
            Line::from(Span::styled(
                "─── The Week ───",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
        ];
        for day in &self.days {
            let (frame, ink, label) = palette(day.kind);
            let beside = [
                Some((self.panel_day_label(day), label)),
                Some((day.rung.label(), ink)),
                self.panel_note(day).map(|note| (note, label)),
                None,
                None,
            ];
            let badge =
                strokes(day.kind).badge(&small_art(day.rung.puzzle), BADGE_ART_WIDTH, frame, ink);
            for (badge, beside) in badge.into_iter().zip(beside) {
                let mut spans = vec![badge, Span::raw("  ")];
                if let Some((text, style)) = beside {
                    spans.push(Span::styled(text, style));
                }
                lines.push(Line::from(spans));
            }
        }
        let dim = Style::default().fg(theme::TEXT_DIM());
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("{ROTATION_LEN} boards, one a day,"),
            dim,
        )));
        lines.push(Line::from(Span::styled("all new at 00:00 UTC.", dim)));
        lines
    }

    /// Seven badges side by side, each with its day above and its game,
    /// board and (today) stars below.
    fn badge_rows(&self) -> Vec<Line<'static>> {
        let mut rows: Vec<Vec<Span<'static>>> = vec![Vec::new(); LOBBY_COLUMN_ROWS];
        for (i, day) in self.days.iter().enumerate() {
            for (row, span) in rows.iter_mut().zip(self.lobby_column(day)) {
                if i > 0 {
                    row.push(Span::raw(" "));
                }
                row.push(span);
            }
        }
        rows.into_iter().map(Line::from).collect()
    }

    /// One day per line: the day, the board, and today's stars.
    fn list_rows(&self) -> Vec<Line<'static>> {
        self.days
            .iter()
            .map(|day| {
                let (_, ink, label) = palette(day.kind);
                let mut spans = vec![
                    Span::styled(format!("  {:<9}", self.day_label(day)), label),
                    Span::styled(format!("{:<24}", day.rung.label()), ink),
                ];
                if let Some(stars) = self.stars_label(day) {
                    spans.push(Span::styled(stars, label));
                }
                Line::from(spans)
            })
            .collect()
    }

    /// A day's lobby column, every span exactly [`BADGE_WIDTH`] cells.
    fn lobby_column(&self, day: &WeekDay) -> Vec<Span<'static>> {
        let (frame, ink, label) = palette(day.kind);
        let mut column = Vec::with_capacity(LOBBY_COLUMN_ROWS);
        column.push(Span::styled(
            centered(&self.day_label(day), BADGE_WIDTH),
            label,
        ));
        column.extend(strokes(day.kind).badge(
            &small_art(day.rung.puzzle),
            BADGE_ART_WIDTH,
            frame,
            ink,
        ));
        column.push(Span::styled(
            centered(short_name(day.rung.puzzle), BADGE_WIDTH),
            ink,
        ));
        column.push(Span::styled(
            centered(board_name(day.rung), BADGE_WIDTH),
            ink,
        ));
        column.push(Span::styled(
            centered(&self.stars_label(day).unwrap_or_default(), BADGE_WIDTH),
            label,
        ));
        column
    }

    /// The tour's week stop: today small, tomorrow large, and beside them
    /// what the rung is in three short lines. The days after are not here:
    /// the lobby under the box carries the whole week.
    pub(crate) fn tour_lines(&self) -> Vec<Line<'static>> {
        const GAP: &str = "   ";
        let today = &self.days[0];
        let tomorrow = self.days[1];
        let text = Style::default().fg(theme::TEXT());
        let dim = Style::default().fg(theme::TEXT_DIM());
        let (frame, ink, label) = palette(DayKind::Tomorrow);

        let mut large = Vec::with_capacity(LARGE_ART_ROWS + 4);
        large.push(Span::styled(centered("tomorrow", LARGE_WIDTH), label));
        large.extend(strokes(DayKind::Tomorrow).badge(
            &large_art(tomorrow.rung.puzzle),
            LARGE_ART_WIDTH,
            frame,
            ink,
        ));
        large.push(Span::styled(
            centered(&tomorrow.rung.label(), LARGE_WIDTH),
            label,
        ));

        // Beside the art, from its second row: the pitch, a breath, the
        // rule, a breath, where the rest of the week is.
        let mut beside: Vec<Span<'static>> = vec![Span::raw(""); large.len()];
        beside[2] = Span::styled(
            "up at 00:00 UTC: your first star.",
            text.add_modifier(Modifier::BOLD),
        );
        beside[4] = Span::styled("one board a day, the same for everyone.", text);
        beside[5] = Span::styled(format!("win it on its day for the star {STAR_GLYPH}"), text);
        beside[7] = Span::styled("the days after sit in the Arcade lobby, on The Week.", dim);

        let mut small = self.lobby_column(today);
        small.resize(large.len(), Span::raw(" ".repeat(BADGE_WIDTH)));

        small
            .into_iter()
            .zip(large)
            .zip(beside)
            .map(|((small, large), beside)| {
                Line::from(vec![small, Span::raw(GAP), large, Span::raw(GAP), beside])
            })
            .collect()
    }

    fn day_label(&self, day: &WeekDay) -> String {
        match day.kind {
            DayKind::Today if self.today_won => format!("{STAR_GLYPH} today"),
            DayKind::Today => "today".to_string(),
            DayKind::Tomorrow => "tomorrow".to_string(),
            DayKind::Ahead => day.day.format("%a %-d").to_string(),
        }
    }

    /// The panel's day: today and tomorrow named with their date, the rest
    /// by date alone.
    fn panel_day_label(&self, day: &WeekDay) -> String {
        let date = day.day.format("%a %-d");
        match day.kind {
            DayKind::Today | DayKind::Tomorrow => format!("{} · {date}", self.day_label(day)),
            DayKind::Ahead => date.to_string(),
        }
    }

    /// The panel's third line beside a badge: today's stars, when
    /// tomorrow's board comes up, nothing for the days after.
    fn panel_note(&self, day: &WeekDay) -> Option<String> {
        match day.kind {
            DayKind::Today => self.stars_label(day),
            DayKind::Tomorrow => Some("up at 00:00 UTC".to_string()),
            DayKind::Ahead => None,
        }
    }

    /// Today's star count; the other days have none yet.
    fn stars_label(&self, day: &WeekDay) -> Option<String> {
        match day.kind {
            DayKind::Today => Some(format!("{} {STAR_GLYPH}", self.today_stars)),
            DayKind::Tomorrow | DayKind::Ahead => None,
        }
    }
}

/// `11 boards, one a day, all new at 00:00 UTC.`
fn rotation_sentence() -> String {
    format!("{ROTATION_LEN} boards, one a day, all new at 00:00 UTC.")
}

/// A day's colours: its border, its art and text, its label.
fn palette(kind: DayKind) -> (Style, Style, Style) {
    match kind {
        DayKind::Today => (
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
            Style::default().fg(theme::AMBER()),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        DayKind::Tomorrow => (
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
            Style::default().fg(theme::TEXT_BRIGHT()),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        DayKind::Ahead => (
            Style::default().fg(theme::TEXT_DIM()),
            Style::default().fg(theme::TEXT_DIM()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    }
}

/// A badge's border: tomorrow heavy, every other day light.
struct Strokes {
    corners: [char; 4],
    horizontal: char,
    side: char,
}

impl Strokes {
    /// `art` boxed, one span a row: the top rule in `frame`, each art row
    /// between the sides in `ink`, the bottom rule. `inner` is the art's
    /// width in cells.
    fn badge(&self, art: &[&str], inner: usize, frame: Style, ink: Style) -> Vec<Span<'static>> {
        let rule = self.horizontal.to_string().repeat(inner);
        let [top_left, top_right, bottom_left, bottom_right] = self.corners;
        let mut badge = vec![Span::styled(format!("{top_left}{rule}{top_right}"), frame)];
        badge.extend(
            art.iter()
                .map(|row| Span::styled(format!("{}{row}{}", self.side, self.side), ink)),
        );
        badge.push(Span::styled(
            format!("{bottom_left}{rule}{bottom_right}"),
            frame,
        ));
        badge
    }
}

fn strokes(kind: DayKind) -> Strokes {
    match kind {
        DayKind::Tomorrow => Strokes {
            corners: ['┏', '┓', '┗', '┛'],
            horizontal: '━',
            side: '┃',
        },
        DayKind::Today | DayKind::Ahead => Strokes {
            corners: ['┌', '┐', '└', '┘'],
            horizontal: '─',
            side: '│',
        },
    }
}

/// `text` centred in `width` cells, padded with spaces.
fn centered(text: &str, width: usize) -> String {
    let text_width = text.width();
    let left = width.saturating_sub(text_width) / 2;
    let right = width.saturating_sub(text_width + left);
    format!("{}{text}{}", " ".repeat(left), " ".repeat(right))
}

/// The game under its badge, at most [`BADGE_WIDTH`] cells.
fn short_name(puzzle: DailyPuzzle) -> &'static str {
    match puzzle {
        DailyPuzzle::LeWord => "le word",
        DailyPuzzle::RubiksCube => "cube",
        DailyPuzzle::SlidingPuzzle => "slider",
        DailyPuzzle::Sudoku => "sudoku",
        DailyPuzzle::Nonogram => "nonogram",
        DailyPuzzle::Minesweeper => "mines",
        DailyPuzzle::Solitaire => "solitaire",
        DailyPuzzle::Netwalk => "netwalk",
        DailyPuzzle::ChessPuzzle => "chess",
        DailyPuzzle::WordWheel => "wheel",
        DailyPuzzle::Quartets => "quartets",
    }
}

/// The rung's board under the game: its difficulty key, or nothing for a
/// one-board daily.
fn board_name(rung: Rung) -> &'static str {
    match rung.puzzle.tiers() {
        Tiers::Single(_) => "",
        Tiers::Keyed(_) => rung.difficulty_key,
    }
}

/// A game's small badge: [`BADGE_ART_ROWS`] rows of [`BADGE_ART_WIDTH`]
/// cells, single-width glyphs only.
fn small_art(puzzle: DailyPuzzle) -> [&'static str; BADGE_ART_ROWS] {
    match puzzle {
        DailyPuzzle::Sudoku => [" 5│ │3 ", " ─┼─┼─ ", "  │7│  "],
        DailyPuzzle::Netwalk => [" ╶─■─╴ ", "   │   ", " ╶─┴─╴ "],
        DailyPuzzle::LeWord => [" □■□□■ ", " LATER ", " ■■■■■ "],
        DailyPuzzle::Minesweeper => [" 1 ▓ 1 ", " ▓ * ▓ ", " 1 ▓ 1 "],
        DailyPuzzle::Nonogram => ["  │1 2 ", " 2│█ █ ", " 1│ █  "],
        DailyPuzzle::ChessPuzzle => [" ▓k▓ ▓ ", "  ▓ ▓N ", " ▓ ▓ ▓ "],
        DailyPuzzle::Solitaire => ["┌─┐ ┌─┐", "│A│ │K│", "└─┘ └─┘"],
        DailyPuzzle::WordWheel => ["  A R  ", " T(E)L ", "  S N  "],
        DailyPuzzle::SlidingPuzzle => [" 1 2 3 ", " 4   6 ", " 7 5 8 "],
        DailyPuzzle::Quartets => ["▪ ▪ ▪ ▪", "▫ ▫ ▫ ▫", "▪ ▪ ▪ ▪"],
        DailyPuzzle::RubiksCube => ["┌─┬─┬─┐", "├─┼─┼─┤", "└─┴─┴─┘"],
    }
}

/// A game's large badge, for the tour's tomorrow: [`LARGE_ART_ROWS`] rows
/// of [`LARGE_ART_WIDTH`] cells.
fn large_art(puzzle: DailyPuzzle) -> [&'static str; LARGE_ART_ROWS] {
    match puzzle {
        DailyPuzzle::Sudoku => [
            "  5 3   │ 7   │   ",
            "  6     │ 1 9 │ 5 ",
            " ───────┼─────┼───",
            "  8     │     │  6",
            "  4     │ 8   │  3",
            " ───────┼─────┼───",
        ],
        DailyPuzzle::Netwalk => [
            "  ┌───┬───■───┐   ",
            "  │   │   │   │   ",
            "  ├───┘   └───┤   ",
            "  │           │   ",
            "  ╵   ┌───┐   ╵   ",
            "      ╵   ╵       ",
        ],
        DailyPuzzle::LeWord => [
            "                  ",
            "    C R A N E     ",
            "    □ ■ □ □ ■     ",
            "    L A T E R     ",
            "    ■ ■ ■ ■ ■     ",
            "                  ",
        ],
        DailyPuzzle::Minesweeper => [
            "  1 1 1 ▓ ▓ ▓ ▓   ",
            "  1 * 1 ▓ ▓ ▓ ▓   ",
            "  1 1 1 2 ▓ ▓ ▓   ",
            "  ▓ ▓ 1 * 2 ▓ ▓   ",
            "  ▓ ▓ 1 1 1 ▓ ▓   ",
            "  ▓ ▓ ▓ ▓ ▓ ▓ ▓   ",
        ],
        DailyPuzzle::Nonogram => [
            "       │1 3 1 2   ",
            "   1 1 │█   █     ",
            "     3 │█ █ █     ",
            "   1 1 │  █   █   ",
            "     2 │  █ █     ",
            "     1 │      █   ",
        ],
        DailyPuzzle::ChessPuzzle => [
            "   ▓ ░ ▓ ░ ▓ ░    ",
            "   ░ k ░ ▓ ░ ▓    ",
            "   ▓ ░ ▓ N ▓ ░    ",
            "   ░ ▓ ░ ▓ ░ ▓    ",
            "   ▓ ░ ▓ ░ K ░    ",
            "   ░ ▓ ░ ▓ ░ ▓    ",
        ],
        DailyPuzzle::Solitaire => [
            "  ┌──┐┌──┐┌──┐    ",
            "  │A♠││K♥││Q♣│    ",
            "  └──┘└──┘└──┘    ",
            "   ┌──┐  ┌──┐     ",
            "   │J♦│  │10│     ",
            "   └──┘  └──┘     ",
        ],
        DailyPuzzle::WordWheel => [
            "        A         ",
            "     T     R      ",
            "    L  (E)  S     ",
            "     N     I      ",
            "        G         ",
            "                  ",
        ],
        DailyPuzzle::SlidingPuzzle => [
            "  ┌──┬──┬──┐      ",
            "  │ 1│ 2│ 3│      ",
            "  ├──┼──┼──┤      ",
            "  │ 4│  │ 6│      ",
            "  ├──┼──┼──┤      ",
            "  │ 7│ 5│ 8│      ",
        ],
        DailyPuzzle::Quartets => [
            "                  ",
            "   ■ ■ ■ ■        ",
            "   ▪ ▪ ▪ ▪        ",
            "   □ □ □ □        ",
            "   ▫ ▫ ▫ ▫        ",
            "                  ",
        ],
        // One face, closed on every side: six rows leave no room for the
        // inner rules, so the columns alone draw the grid.
        DailyPuzzle::RubiksCube => [
            "                  ",
            "   ┌──┬──┬──┐     ",
            "   │▓▓│░░│▓▓│     ",
            "   │░░│▓▓│░░│     ",
            "   │▓▓│░░│▓▓│     ",
            "   └──┴──┴──┘     ",
        ],
    }
}

#[cfg(test)]
#[path = "week_test.rs"]
mod week_test;
