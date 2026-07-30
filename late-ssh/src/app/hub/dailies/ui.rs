//! The quest surface. Quests are arcade-only by owner decision, so they
//! render as a compact strip at the top of The Arcade lobby instead of a
//! Hub tab: see the quest, launch the game, no modal in between.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

use super::{
    state::QuestState,
    svc::{QuestItem, QuestSnapshot},
};

/// Rows the strip needs: heading/summary, two dailies, one weekly, one
/// blank separator before the lobby content.
pub(crate) const ARCADE_STRIP_HEIGHT: u16 = 5;

pub(crate) fn draw_arcade_strip(frame: &mut Frame, area: Rect, state: &QuestState) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let snapshot = state.snapshot();
    let mut lines = Vec::with_capacity(area.height as usize);
    lines.push(heading_line(snapshot, state.is_loaded()));
    for item in &snapshot.daily {
        lines.push(item_line(item, None, area.width as usize));
    }
    for item in &snapshot.weekly {
        lines.push(item_line(item, Some("weekly"), area.width as usize));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn heading_line(snapshot: &QuestSnapshot, loaded: bool) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT_DIM());

    let mut spans = vec![
        Span::styled("─── ", dim),
        Span::styled("Quests", accent),
        Span::styled(" ───", dim),
    ];
    if !loaded {
        spans.push(Span::styled("  loading", Style::default().fg(theme::TEXT_FAINT())));
        return Line::from(spans);
    }

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
    let streak = &snapshot.daily_streak;
    let mut summary = format!(
        "  daily {daily_done}/{} · weekly {weekly_done}/{}",
        snapshot.daily.len(),
        snapshot.weekly.len(),
    );
    summary.push_str(&format!(
        " · streak {} day{}",
        streak.consecutive_days,
        if streak.consecutive_days == 1 { "" } else { "s" },
    ));
    if streak.next_bonus_chips > 0 {
        summary.push_str(&format!(" · next +{} chips", streak.next_bonus_chips));
    }
    spans.push(Span::styled(summary, text));
    Line::from(spans)
}

fn item_line(item: &QuestItem, tag: Option<&'static str>, width: usize) -> Line<'static> {
    let done = item.completed();
    let (status, status_style) = if done {
        ("done", Style::default().fg(theme::SUCCESS()))
    } else {
        ("open", Style::default().fg(theme::AMBER_DIM()))
    };
    let title_style = if done {
        Style::default().fg(theme::TEXT_DIM())
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };

    let progress = format!("{}/{}", item.visible_progress(), item.target);
    let mut meta = format!("  {progress}");
    if item.reward_chips > 0 {
        meta.push_str(&format!(" · +{} chips", item.reward_chips));
    }
    if let Some(tag) = tag {
        meta.push_str(&format!(" · {tag}"));
    }

    let title_budget = width.saturating_sub(8 + meta.chars().count());
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{status:<4}"), status_style),
        Span::raw("  "),
        Span::styled(truncate(&item.title, title_budget), title_style),
        Span::styled(meta, Style::default().fg(theme::AMBER_DIM())),
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
