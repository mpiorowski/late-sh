use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
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
                Constraint::Length(9),
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
    let mut lines: Vec<Line<'static>> = vec![Line::from("")];
    lines.extend(art.into_iter().map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
    }));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "     BBS-style persistent worlds. Browse with j/k, open with Enter.",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_game_list(frame: &mut Frame, area: Rect, selection: usize, delete_confirm: bool) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut selected_line: usize = 0;

    lines.push(Line::from(""));
    push_game_section(&mut lines, "─── Door Games ───");
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "BBS-style persistent worlds. One door is open today.",
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(""));

    push_game_entry(
        &mut lines,
        &mut selected_line,
        selection,
        DoorEntry {
            idx: DOOR_SELECTION_LATEANIA,
            name: "Lateania",
            descriptions: &[
                "Persistent shared adventure world with classes, rooms, combat, loot, and shops.",
                "by hardlygospel.github.io",
            ],
            selected_style: Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
            normal_style: Style::default().fg(theme::TEXT()),
            description_style: Style::default().fg(theme::TEXT_DIM()),
            status: vec![Span::styled(
                "Online world",
                Style::default().fg(theme::SUCCESS()),
            )],
            label_width: 18,
        },
    );
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Enter", Style::default().fg(theme::AMBER())),
        Span::styled(" open  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("j/k", Style::default().fg(theme::AMBER())),
        Span::styled(" move  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("d", Style::default().fg(theme::ERROR())),
        Span::styled(" reset", Style::default().fg(theme::TEXT_DIM())),
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

    let visible = area.height as usize;
    let third = visible / 3;
    let scroll_y = if visible >= lines.len() {
        0
    } else {
        selected_line
            .saturating_sub(third)
            .min(lines.len().saturating_sub(visible))
    };

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y as u16, 0));

    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    frame.render_widget(paragraph, layout[1]);
}

fn push_game_section(lines: &mut Vec<Line<'static>>, title: &str) {
    lines.push(Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
}

struct DoorEntry<'a> {
    idx: usize,
    name: &'a str,
    descriptions: &'a [&'a str],
    selected_style: Style,
    normal_style: Style,
    description_style: Style,
    status: Vec<Span<'static>>,
    label_width: usize,
}

fn push_game_entry(
    lines: &mut Vec<Line<'static>>,
    selected_line: &mut usize,
    selection: usize,
    entry: DoorEntry<'_>,
) {
    let is_selected = entry.idx == selection;
    if is_selected {
        *selected_line = lines.len();
    }

    let title_style = if is_selected {
        entry.selected_style
    } else {
        entry.normal_style
    };
    let mut title_line = vec![
        Span::styled(if is_selected { "> " } else { "  " }, title_style),
        Span::styled(format!("[ {} ]", entry.name), title_style),
    ];
    let padding_len = entry.label_width.saturating_sub(entry.name.len() + 4);
    title_line.push(Span::raw(" ".repeat(padding_len.max(1))));
    title_line.extend(entry.status);
    lines.push(Line::from(title_line));

    for description in entry.descriptions {
        lines.push(Line::from(vec![
            Span::raw("      "),
            Span::styled((*description).to_string(), entry.description_style),
        ]));
    }
    lines.push(Line::from(""));
}
