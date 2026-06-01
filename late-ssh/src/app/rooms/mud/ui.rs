// Rendering for Lateania. Reads the cached per-session snapshot and paints a
// two-column view: the scrolling adventure log on the left, a character/room
// panel on the right. Lock-free; never awaits or touches a service mutex.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    common::theme,
    rooms::mud::{
        state::State,
        svc::{LogKind, PlayerView},
    },
};
use crate::usernames::UsernameLookup;

const SIDE_WIDE: u16 = 30;
const SIDE_NARROW: u16 = 24;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, usernames: &UsernameLookup<'_>) {
    let view = state.view();

    if !view.joined {
        let lines = vec![
            Line::from(Span::styled(
                "Entering Lateania...",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "World by Tasmania - thanks to late.sh and its contributors.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    if area.width < 46 || area.height < 8 {
        draw_compact(frame, area, &view);
        return;
    }

    let side_w = if area.width >= 78 {
        SIDE_WIDE
    } else {
        SIDE_NARROW
    };
    let columns =
        Layout::horizontal([Constraint::Min(24), Constraint::Length(side_w)]).split(area);
    draw_log(frame, columns[0], &view);
    draw_side(frame, columns[1], state, &view, usernames);
}

fn draw_compact(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            view.room_name.clone(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  hp {}/{}", view.hp, view.max_hp),
            Style::default().fg(hp_color(view.hp, view.max_hp)),
        ),
    ])];
    let tail = log_tail(view, area.height.saturating_sub(1) as usize);
    for line in tail {
        lines.push(log_line(line.0, line.1));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_log(frame: &mut Frame, area: Rect, view: &PlayerView) {
    let capacity = area.height as usize;
    let tail = log_tail(view, capacity);
    let lines: Vec<Line> = tail
        .into_iter()
        .map(|(kind, text)| log_line(kind, text))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_side(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    view: &PlayerView,
    usernames: &UsernameLookup<'_>,
) {
    let _ = state;
    let mut lines = Vec::new();

    lines.push(section("Adventurer"));
    lines.push(stat_line("Level", view.level.to_string()));
    lines.push(Line::from(vec![
        Span::styled("  HP   ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!("{}/{}", view.hp, view.max_hp),
            Style::default()
                .fg(hp_color(view.hp, view.max_hp))
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(stat_line("XP", view.xp.to_string()));
    lines.push(Line::raw(""));

    lines.push(section("Here"));
    lines.push(Line::from(Span::styled(
        format!("  {}", view.zone),
        Style::default().fg(theme::TEXT()),
    )));
    let exits = if view.exits.is_empty() {
        "none".to_string()
    } else {
        view.exits
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    lines.push(Line::from(vec![
        Span::styled("  exits ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(exits, Style::default().fg(theme::AMBER_DIM())),
    ]));

    if !view.mobs.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Foes"));
        for mob in &view.mobs {
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", mob.name), Style::default().fg(theme::ERROR())),
                Span::styled(
                    format!("{}/{}", mob.hp, mob.max_hp),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ]));
        }
    }

    if !view.occupants.is_empty() {
        lines.push(Line::raw(""));
        lines.push(section("Adventurers here"));
        for occ in &view.occupants {
            let name = usernames
                .get(&occ.user_id)
                .cloned()
                .unwrap_or_else(|| "adventurer".to_string());
            let marker = if occ.in_combat { " (fighting)" } else { "" };
            lines.push(Line::from(Span::styled(
                format!("  {name}{marker}"),
                Style::default().fg(theme::SUCCESS()),
            )));
        }
    }

    lines.push(Line::raw(""));
    lines.push(section("Commands"));
    if view.respawning {
        lines.push(Line::from(Span::styled(
            "  recovering...",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    } else if view.in_combat_with.is_some() {
        lines.push(hint("space/x", "strike"));
        lines.push(hint("z", "flee"));
    } else {
        lines.push(hint("w a s d", "move"));
        lines.push(hint("arrows", "move"));
        lines.push(hint("space/x", "attack"));
        lines.push(hint("o", "look"));
    }
    lines.push(hint("q / Esc", "leave"));

    frame.render_widget(Paragraph::new(lines), area);
}

fn log_tail(view: &PlayerView, capacity: usize) -> Vec<(LogKind, String)> {
    if capacity == 0 {
        return Vec::new();
    }
    let start = view.log.len().saturating_sub(capacity);
    view.log[start..]
        .iter()
        .map(|line| (line.kind, line.text.clone()))
        .collect()
}

fn log_line(kind: LogKind, text: String) -> Line<'static> {
    let color = match kind {
        LogKind::Normal => theme::TEXT(),
        LogKind::Combat => theme::ERROR(),
        LogKind::System => theme::AMBER_DIM(),
        LogKind::Say => theme::CHAT_BODY(),
    };
    Line::from(Span::styled(text, Style::default().fg(color)))
}

fn section(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(" - ", Style::default().fg(theme::BORDER())),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn stat_line(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {label:<5}"), Style::default().fg(theme::TEXT_DIM())),
        Span::styled(value, Style::default().fg(theme::TEXT_BRIGHT())),
    ])
}

fn hint(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {key}"), Style::default().fg(theme::AMBER_DIM())),
        Span::styled(format!("  {label}"), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn hp_color(hp: i32, max_hp: i32) -> ratatui::style::Color {
    if max_hp <= 0 {
        return theme::TEXT_DIM();
    }
    let pct = (hp * 100) / max_hp;
    if pct <= 25 {
        theme::ERROR()
    } else if pct <= 60 {
        theme::AMBER()
    } else {
        theme::SUCCESS()
    }
}
