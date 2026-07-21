use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use late_core::models::quest::MAX_DAILY_QUEST_STREAK_BONUS_LEVEL;

use crate::app::common::theme;

use super::{
    state::QuestState,
    svc::{QuestItem, QuestSnapshot, daily_streak_bonus_label},
};

const STREAK_PROGRESS_BAR_MAX_WIDTH: usize = 42;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &QuestState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // hint
        Constraint::Length(1), // breathing before streaks
        Constraint::Length(1), // streak heading
        Constraint::Length(1), // daily streak label
        Constraint::Length(1), // daily streak progress
        Constraint::Length(1), // breathing
        Constraint::Min(12),   // quests
        Constraint::Length(1), // footer
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Quests")), sections[0]);
    frame.render_widget(Paragraph::new(summary_line(state.snapshot())), sections[1]);
    frame.render_widget(Paragraph::new(section_heading("Streaks")), sections[3]);
    frame.render_widget(
        Paragraph::new(daily_streak_label_line(
            state.snapshot(),
            sections[4].width as usize,
        )),
        sections[4],
    );
    draw_daily_streak_progress(frame, sections[5], state.snapshot());
    draw_quests(frame, sections[7], state.snapshot());
    draw_footer(frame, sections[8], state);
}

fn draw_quests(frame: &mut Frame, area: Rect, snapshot: &QuestSnapshot) {
    let rows = Layout::vertical([
        Constraint::Length(9), // two daily quests
        Constraint::Length(1), // divider breathing
        Constraint::Min(5),    // weekly
    ])
    .split(area);

    draw_group(frame, rows[0], "Today", &snapshot.daily);
    draw_group(frame, rows[2], "This week", &snapshot.weekly);
}

fn draw_group(frame: &mut Frame, area: Rect, title: &str, items: &[QuestItem]) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let mut lines = Vec::with_capacity(area.height as usize);
    lines.push(section_heading(title));

    if items.is_empty() {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "no quests assigned yet",
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
    } else {
        for item in items {
            if lines.len() + 3 > area.height as usize {
                break;
            }
            lines.extend(item_lines(item, area.width as usize));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn item_lines(item: &QuestItem, width: usize) -> Vec<Line<'static>> {
    let done = item.completed();
    let title_style = if done {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    };
    let status = if done { "done" } else { "open" };
    let status_style = if done {
        Style::default().fg(theme::SUCCESS())
    } else {
        Style::default().fg(theme::AMBER_DIM())
    };
    let progress = format!("{}/{}", item.visible_progress(), item.target);
    let reward = if item.reward_chips > 0 {
        format!("+{} chips", item.reward_chips)
    } else {
        "no chip reward".to_string()
    };
    let meta = format!(
        "{} / {} / {} / {}",
        item.difficulty, item.domain, progress, reward
    );
    let reset = format!("resets {}", item.period_end);
    vec![
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("{:<4}", status), status_style),
            Span::styled(" ", Style::default()),
            Span::styled(truncate(&item.title, width.saturating_sub(9)), title_style),
        ]),
        Line::from(vec![
            Span::raw("       "),
            Span::styled(
                truncate(&item.description, width.saturating_sub(7)),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::from(vec![
            Span::raw("       "),
            Span::styled(meta, Style::default().fg(theme::AMBER_DIM())),
            Span::styled("  ", Style::default()),
            Span::styled(reset, Style::default().fg(theme::TEXT_FAINT())),
        ]),
    ]
}

fn summary_line(snapshot: &QuestSnapshot) -> Line<'static> {
    let daily_done = snapshot
        .daily
        .iter()
        .filter(|item| item.completed())
        .count();
    let weekly_done = snapshot
        .weekly
        .iter()
        .filter(|item| item.completed())
        .count();
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("daily {daily_done}/{}  ", snapshot.daily.len()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            format!("weekly {weekly_done}/{}  ", snapshot.weekly.len()),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            "2 daily quests and 1 weekly quest are drawn globally on UTC boundaries",
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn daily_streak_label_line(snapshot: &QuestSnapshot, width: usize) -> Line<'static> {
    let streak = &snapshot.daily_streak;
    let done_today = snapshot.daily.iter().any(QuestItem::completed);
    let status = if done_today {
        "today banked"
    } else {
        "finish any daily quest"
    };
    let current_bonus = format!("+{} chips", streak.current_bonus_chips);
    let next_bonus = if streak.next_bonus_chips > 0 {
        format!("+{} chips", streak.next_bonus_chips)
    } else {
        "+0 chips".to_string()
    };
    let text = format!(
        "daily streak {} day{} / level {}/{} / current {} / next {} / {}",
        streak.consecutive_days,
        if streak.consecutive_days == 1 {
            ""
        } else {
            "s"
        },
        streak.bonus_level,
        MAX_DAILY_QUEST_STREAK_BONUS_LEVEL,
        current_bonus,
        next_bonus,
        status
    );
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            truncate(&text, width.saturating_sub(2)),
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ])
}

fn draw_daily_streak_progress(frame: &mut Frame, area: Rect, snapshot: &QuestSnapshot) {
    if area.width == 0 {
        return;
    }
    let progress = snapshot.daily_streak.bonus_level;
    let target = MAX_DAILY_QUEST_STREAK_BONUS_LEVEL;
    let progress_text = format!("{progress}/{target} {}", daily_streak_bonus_label(progress));
    let bar_w = (area.width as usize)
        .saturating_sub(progress_text.chars().count() + 3)
        .min(STREAK_PROGRESS_BAR_MAX_WIDTH);
    let filled = if target <= 0 {
        0
    } else {
        (bar_w * progress.max(0) as usize / target as usize).min(bar_w)
    };
    let empty = bar_w.saturating_sub(filled);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("█".repeat(filled), Style::default().fg(theme::SUCCESS())),
            Span::styled("░".repeat(empty), Style::default().fg(theme::BORDER_DIM())),
            Span::raw(" "),
            Span::styled(progress_text, Style::default().fg(theme::TEXT_DIM())),
        ])),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &QuestState) {
    let text = Style::default().fg(theme::TEXT_DIM());
    let loaded = if state.is_loaded() {
        "loaded"
    } else {
        "loading"
    };
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled(loaded, Style::default().fg(theme::TEXT_FAINT())),
        Span::styled("  rewards pay automatically on completion", text),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  -- ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" --", dim),
    ])
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
