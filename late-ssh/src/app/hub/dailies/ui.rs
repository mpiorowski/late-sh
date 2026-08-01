//! The quest surface. Quests are arcade-only by owner decision, so they
//! render as a compact strip at the top of The Arcade lobby instead of a
//! Hub tab: see the quest, launch the game, no modal in between.
//!
//! Layout (rows are dynamic, `arcade_strip_height` must match `strip_lines`;
//! the strip shares the lobby's 4-column left padding so it reads as one
//! screen with the game list below it):
//!
//! ```text
//!
//! ─── Quests ───  streak ▰▰▰▱▱▱ 3 days · next daily +300 bonus
//!
//!   Daily 0/2 · +525 chips to earn · resets 00:00 UTC
//!     ○ Win easy Sudoku            easy     +150
//!     ○ Score 10,000 in Snake      medium   +375  ▰▱▱▱▱▱▱▱ 4,000/10,000
//!
//!   Weekly 0/1 · +750 chips to earn · resets in 4d
//!     ○ Win draw-3 Solitaire       hard     +750
//! ```

use chrono::{NaiveDate, Utc};
use late_core::models::quest::{DailyQuestStreakSnapshot, MAX_DAILY_QUEST_STREAK_BONUS_LEVEL};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::{primitives::thousands, theme};

use super::{
    state::QuestState,
    svc::{QuestItem, QuestSnapshot},
};

/// The streak meter has one slot per day until the bonus caps: day 1 starts
/// the streak at bonus level 0, so the cap lands on day `MAX + 1`.
const STREAK_BAR_SLOTS: i32 = MAX_DAILY_QUEST_STREAK_BONUS_LEVEL + 1;

/// Item progress bars only fit comfortably on wider terminals; below this the
/// bar is dropped and only the numeric progress remains.
const ITEM_BAR_MIN_WIDTH: usize = 70;
const ITEM_BAR_SLOTS: usize = 8;

pub(crate) fn arcade_strip_height(state: &QuestState) -> u16 {
    strip_height_for(state.snapshot(), state.is_loaded())
}

/// Content rows plus one blank separator before the lobby content. Each
/// section brings its own blank line above it, matching the airy rhythm of
/// the game list below the strip.
fn strip_height_for(snapshot: &QuestSnapshot, loaded: bool) -> u16 {
    let mut rows: usize = 2;
    if loaded {
        if !snapshot.daily.is_empty() {
            rows += 2 + snapshot.daily.len();
        }
        if !snapshot.weekly.is_empty() {
            rows += 2 + snapshot.weekly.len();
        }
    }
    (rows + 1) as u16
}

pub(crate) fn draw_arcade_strip(frame: &mut Frame, area: Rect, state: &QuestState) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // Same 4-column left padding as the lobby's game list, so the strip
    // lines up with the section headings below it.
    let columns = Layout::horizontal([Constraint::Length(4), Constraint::Min(0)]).split(area);
    let content = columns[1];
    let today = Utc::now().date_naive();
    let lines = strip_lines(
        state.snapshot(),
        state.is_loaded(),
        today,
        content.width as usize,
    );
    frame.render_widget(Paragraph::new(lines), content);
}

fn strip_lines(
    snapshot: &QuestSnapshot,
    loaded: bool,
    today: NaiveDate,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(""), heading_line(&snapshot.daily_streak, loaded)];
    if !loaded {
        return lines;
    }
    if !snapshot.daily.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_line("Daily", &snapshot.daily, "resets 00:00 UTC"));
        for item in &snapshot.daily {
            lines.push(item_line(item, width));
        }
    }
    if !snapshot.weekly.is_empty() {
        let note = weekly_reset_note(&snapshot.weekly, today);
        lines.push(Line::from(""));
        lines.push(section_line("Weekly", &snapshot.weekly, &note));
        for item in &snapshot.weekly {
            lines.push(item_line(item, width));
        }
    }
    lines
}

fn heading_line(streak: &DailyQuestStreakSnapshot, loaded: bool) -> Line<'static> {
    // Same style as the game list's section headings ("─── Score Games ───"),
    // so the strip reads as the first section of the lobby.
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);

    let mut spans = vec![Span::styled("─── Quests ───", accent)];
    if !loaded {
        spans.push(Span::styled(
            "  loading",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        return Line::from(spans);
    }

    spans.push(Span::styled(
        "  streak ",
        Style::default().fg(theme::TEXT_DIM()),
    ));
    let filled = streak.consecutive_days.clamp(0, STREAK_BAR_SLOTS) as usize;
    spans.extend(bar_spans(filled, STREAK_BAR_SLOTS as usize));
    spans.push(Span::raw(" "));

    if streak.consecutive_days == 0 {
        spans.push(Span::styled(
            "no streak · win any daily quest to start one",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        return Line::from(spans);
    }

    let days = streak.consecutive_days;
    spans.push(Span::styled(
        format!("{days} day{}", if days == 1 { "" } else { "s" }),
        Style::default().fg(theme::TEXT_BRIGHT()),
    ));
    spans.push(Span::styled(" · ", Style::default().fg(theme::AMBER_DIM())));
    if filled as i32 >= STREAK_BAR_SLOTS {
        spans.push(Span::styled(
            format!("max bonus +{}/day", thousands(streak.current_bonus_chips)),
            Style::default().fg(theme::AMBER()),
        ));
    } else {
        spans.push(Span::styled(
            format!("next daily +{} bonus", thousands(streak.next_bonus_chips)),
            Style::default().fg(theme::AMBER()),
        ));
    }
    Line::from(spans)
}

fn section_line(label: &'static str, items: &[QuestItem], reset_note: &str) -> Line<'static> {
    let done = items.iter().filter(|item| item.completed()).count();
    let open_chips: i64 = items
        .iter()
        .filter(|item| !item.completed())
        .map(|item| item.reward_chips)
        .sum();

    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            label,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {done}/{}", items.len()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(" · ", Style::default().fg(theme::AMBER_DIM())),
    ];
    if open_chips > 0 {
        spans.push(Span::styled(
            format!("+{} chips to earn", thousands(open_chips)),
            Style::default().fg(theme::AMBER()),
        ));
    } else {
        spans.push(Span::styled(
            "all clear ✓",
            Style::default().fg(theme::SUCCESS()),
        ));
    }
    spans.push(Span::styled(" · ", Style::default().fg(theme::AMBER_DIM())));
    spans.push(Span::styled(
        reset_note.to_string(),
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    Line::from(spans)
}

fn weekly_reset_note(items: &[QuestItem], today: NaiveDate) -> String {
    // `period_end` is the exclusive end of the assignment window, so the
    // remaining days are a plain date difference.
    let days_left = items
        .iter()
        .map(|item| (item.period_end - today).num_days())
        .min()
        .unwrap_or(0);
    if days_left <= 0 {
        "resets soon".to_string()
    } else {
        format!("resets in {days_left}d")
    }
}

fn item_line(item: &QuestItem, width: usize) -> Line<'static> {
    let done = item.completed();
    let (mark, mark_style) = if done {
        ("✓", Style::default().fg(theme::SUCCESS()))
    } else {
        ("○", Style::default().fg(theme::AMBER_DIM()))
    };
    let title_style = if done {
        Style::default().fg(theme::TEXT_DIM())
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let difficulty_style = if done {
        Style::default().fg(theme::TEXT_FAINT())
    } else {
        difficulty_color(&item.difficulty)
    };
    let reward_style = if done {
        Style::default().fg(theme::TEXT_FAINT())
    } else {
        Style::default().fg(theme::AMBER())
    };

    // Columns: indent(4) mark(2) title gap(2) difficulty(7) gap(1) reward(6),
    // then the optional progress tail for multi-step quests.
    let show_bar = width >= ITEM_BAR_MIN_WIDTH;
    let tail_budget = if item.target > 1 {
        2 + if show_bar { ITEM_BAR_SLOTS + 1 } else { 0 }
            + format!(
                "{}/{}",
                thousands(i64::from(item.visible_progress())),
                thousands(i64::from(item.target)),
            )
            .chars()
            .count()
    } else {
        0
    };
    let title_budget = width
        .saturating_sub(4 + 2 + 2 + 7 + 1 + 6 + tail_budget)
        .clamp(12, 30);

    let mut spans = vec![
        Span::raw("    "),
        Span::styled(format!("{mark} "), mark_style),
        Span::styled(
            format!("{:<title_budget$}", truncate(&item.title, title_budget)),
            title_style,
        ),
        Span::raw("  "),
        Span::styled(format!("{:<7}", item.difficulty), difficulty_style),
        Span::raw(" "),
        Span::styled(
            format!("{:>6}", format!("+{}", thousands(item.reward_chips))),
            reward_style,
        ),
    ];

    if item.target > 1 && !done {
        spans.push(Span::raw("  "));
        if show_bar {
            let filled =
                (item.visible_progress() as usize * ITEM_BAR_SLOTS) / (item.target.max(1) as usize);
            spans.extend(bar_spans(filled, ITEM_BAR_SLOTS));
            spans.push(Span::raw(" "));
        }
        spans.push(Span::styled(
            format!(
                "{}/{}",
                thousands(i64::from(item.visible_progress())),
                thousands(i64::from(item.target)),
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    Line::from(spans)
}

fn bar_spans(filled: usize, slots: usize) -> Vec<Span<'static>> {
    let filled = filled.min(slots);
    vec![
        Span::styled("▰".repeat(filled), Style::default().fg(theme::AMBER())),
        Span::styled(
            "▱".repeat(slots - filled),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ]
}

fn difficulty_color(difficulty: &str) -> Style {
    match difficulty {
        "easy" => Style::default().fg(theme::SUCCESS()),
        "medium" => Style::default().fg(theme::AMBER()),
        "hard" => Style::default().fg(theme::ERROR()),
        _ => Style::default().fg(theme::TEXT_DIM()),
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 3 {
        return value.chars().take(max_chars).collect();
    }
    let mut out: String = value.chars().take(max_chars - 3).collect();
    out.push_str("...");
    out
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
