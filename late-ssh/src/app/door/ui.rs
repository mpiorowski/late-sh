use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{common::theme, state::DOOR_SELECTION_LATEANIA};
use crate::usernames::UsernameLookup;

pub struct DoorHubView<'a> {
    pub game_selection: usize,
    pub delete_confirm: bool,
    pub lateania_state: Option<&'a super::lateania::state::State>,
    pub usernames: &'a UsernameLookup<'a>,
}

pub fn draw_door_hub(frame: &mut Frame, area: Rect, view: &DoorHubView<'_>) {
    if let Some(state) = view.lateania_state {
        super::lateania::ui::draw_page(frame, area, state, view.usernames);
        return;
    }

    if area.height < 8 || area.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Door Games"), area);
        return;
    }

    let show_header = area.height >= 18;
    let layout = if show_header {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0)])
            .split(area)
    };

    if show_header {
        draw_header(frame, layout[0]);
        draw_game_list(frame, layout[2], view.game_selection, view.delete_confirm);
    } else {
        draw_game_list(frame, layout[0], view.game_selection, view.delete_confirm);
    }
}

fn draw_header(frame: &mut Frame, area: Rect) {
    let art = [
        r#"     ██████╗  ██████╗  ██████╗ ██████╗ ███████╗"#,
        r#"     ██╔══██╗██╔═══██╗██╔═══██╗██╔══██╗██╔════╝"#,
        r#"     ██║  ██║██║   ██║██║   ██║██████╔╝███████╗"#,
        r#"     ██║  ██║██║   ██║██║   ██║██╔══██╗╚════██║"#,
        r#"     ██████╔╝╚██████╔╝╚██████╔╝██║  ██║███████║"#,
        r#"     ╚═════╝  ╚═════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝"#,
    ];
    let mut lines: Vec<Line<'static>> = art
        .into_iter()
        .map(|line| {
            Line::from(Span::styled(
                line,
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "     BBS-style persistent worlds. Browse with j/k, open with Enter.",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_game_list(frame: &mut Frame, area: Rect, selection: usize, delete_confirm: bool) {
    let mut lines = Vec::new();
    push_section(&mut lines, "--- Door Games ---");
    lines.push(Line::from(""));
    push_game_entry(
        &mut lines,
        selection,
        DOOR_SELECTION_LATEANIA,
        "Lateania",
        "Persistent shared adventure world with classes, rooms, combat, loot, and shops.",
        "Online world",
    );
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Enter", Style::default().fg(theme::AMBER())),
        Span::styled(" open  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("j/k", Style::default().fg(theme::AMBER())),
        Span::styled(" move  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("d", Style::default().fg(theme::ERROR())),
        Span::styled(" reset  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("?", Style::default().fg(theme::AMBER())),
        Span::styled(" guide", Style::default().fg(theme::TEXT_DIM())),
    ]));
    if delete_confirm && selection == DOOR_SELECTION_LATEANIA {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Delete your Lateania character?",
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Enter/Y", Style::default().fg(theme::ERROR())),
            Span::styled(" confirm  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("N/Esc", Style::default().fg(theme::AMBER())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn push_section(lines: &mut Vec<Line<'static>>, title: &'static str) {
    lines.push(Line::from(Span::styled(
        title,
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )));
}

fn push_game_entry(
    lines: &mut Vec<Line<'static>>,
    selection: usize,
    idx: usize,
    name: &'static str,
    description: &'static str,
    status: &'static str,
) {
    let selected = selection == idx;
    let marker = if selected { ">" } else { " " };
    let title_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    lines.push(Line::from(vec![
        Span::styled(format!(" {marker} "), Style::default().fg(theme::AMBER())),
        Span::styled(format!("{name:<16}"), title_style),
        Span::styled(status, Style::default().fg(theme::SUCCESS())),
    ]));
    lines.push(Line::from(vec![
        Span::raw("    "),
        Span::styled(description, Style::default().fg(theme::TEXT_DIM())),
    ]));
}
