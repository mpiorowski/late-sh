use late_core::models::quest::RewardTemplateAdminRow;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::{
    common::theme,
    hub::admin::state::{AdminCategory, AdminField, AdminState},
};

pub fn draw(frame: &mut Frame, area: Rect, state: &AdminState, is_admin: bool) {
    if !is_admin {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "Admin access required",
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            area,
        );
        return;
    }

    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Admin")), sections[0]);
    draw_categories(frame, sections[1], state);
    draw_status(frame, sections[2], state);
    draw_body(frame, sections[3], state);
    draw_footer(frame, sections[4], state);
}

fn draw_categories(frame: &mut Frame, area: Rect, state: &AdminState) {
    let mut spans = vec![Span::raw("  ")];
    for (index, category) in AdminCategory::ALL.iter().copied().enumerate() {
        let selected = index == state.selected_category_index();
        let style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", category.label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_status(frame: &mut Frame, area: Rect, state: &AdminState) {
    let status = if state.is_loading() {
        "loading"
    } else if state.is_saving() {
        "saving"
    } else if state.is_editing() {
        "editing"
    } else if state.is_dirty() {
        "dirty"
    } else {
        "clean"
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{} templates", state.templates().len()),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::raw("  "),
            Span::styled(status, status_style(status)),
        ])),
        area,
    );
}

fn draw_body(frame: &mut Frame, area: Rect, state: &AdminState) {
    let columns =
        Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)]).split(area);
    draw_template_list(frame, columns[0], state);
    draw_detail(frame, columns[1], state);
}

fn draw_template_list(frame: &mut Frame, area: Rect, state: &AdminState) {
    let rows = state.visible_templates();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "no matching templates",
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ])),
            area,
        );
        return;
    }

    let height = area.height.max(1) as usize;
    let start = visible_window_start(state.selected_index(), rows.len(), height);
    let lines = rows
        .iter()
        .enumerate()
        .skip(start)
        .take(height)
        .map(|(index, row)| template_row(index == state.selected_index(), row))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_detail(frame: &mut Frame, area: Rect, state: &AdminState) {
    let Some(row) = state.selected_template() else {
        return;
    };
    let Some(draft) = state.draft() else {
        return;
    };

    let mut lines = vec![
        section_heading(&row.title),
        Line::from(vec![
            Span::raw("  key    "),
            Span::styled(row.key.clone(), Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(vec![
            Span::raw("  kind   "),
            Span::styled(row.kind.clone(), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("  {}", row.claim_policy),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]),
        Line::from(vec![
            Span::raw("  scope  "),
            Span::styled(scope_label(row), Style::default().fg(theme::TEXT_DIM())),
        ]),
        Line::from(""),
    ];

    for (index, field) in AdminField::ALL.iter().copied().enumerate() {
        let selected = index == state.selected_field_index();
        let value = if state.is_editing() && selected {
            state.edit_buffer().to_string()
        } else {
            field_value(field, draft)
        };
        lines.push(field_line(
            field,
            selected,
            &value,
            state.is_editing() && selected,
        ));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  params "),
        Span::styled(
            row.params.to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]));
    if let Some(seconds) = row.cooldown_seconds {
        lines.push(Line::from(vec![
            Span::raw("  cd     "),
            Span::styled(
                format!("{seconds}s"),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &AdminState) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    let spans = if state.is_editing() {
        vec![
            Span::raw("  "),
            Span::styled("Enter", key),
            Span::styled(" accept  ", text),
            Span::styled("Esc", key),
            Span::styled(" cancel  ", text),
            Span::styled("Backspace", key),
            Span::styled(" delete", text),
        ]
    } else {
        vec![
            Span::raw("  "),
            Span::styled("j/k", key),
            Span::styled(" rows  ", text),
            Span::styled("h/l", key),
            Span::styled(" field  ", text),
            Span::styled("[/]", key),
            Span::styled(" group  ", text),
            Span::styled("Enter/e", key),
            Span::styled(" edit  ", text),
            Span::styled("+/-", key),
            Span::styled(" adjust  ", text),
            Span::styled("s", key),
            Span::styled(" save  ", text),
            Span::styled("r", key),
            Span::styled(" reload", text),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn template_row(selected: bool, row: &RewardTemplateAdminRow) -> Line<'static> {
    let marker = if selected { ">" } else { " " };
    let status = if row.active { "on " } else { "off" };
    let style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(vec![
        Span::styled(format!("{marker} "), style),
        Span::styled(truncate(&row.title, 28), style),
        Span::styled(
            format!("  {}  {}c", status, row.reward_chips),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn field_line(field: AdminField, selected: bool, value: &str, editing: bool) -> Line<'static> {
    let label_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if editing {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default().fg(theme::TEXT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(vec![
        Span::raw(if selected { "> " } else { "  " }),
        Span::styled(format!("{:<7}", field.label()), label_style),
        Span::styled(truncate(value, 80), value_style),
    ])
}

fn field_value(field: AdminField, draft: &crate::app::hub::admin::state::AdminDraft) -> String {
    match field {
        AdminField::Title => draft.title.clone(),
        AdminField::Description => draft.description.clone(),
        AdminField::Target => draft.target.to_string(),
        AdminField::Reward => format!("{} chips", draft.reward_chips),
        AdminField::Weight => draft.weight.to_string(),
        AdminField::Active => {
            if draft.active {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
    }
}

fn scope_label(row: &RewardTemplateAdminRow) -> String {
    let quest = if row.is_quest { "quest" } else { "reward" };
    let cadence = row.cadence.as_deref().unwrap_or("-");
    let bucket = row.bucket.as_deref().unwrap_or("-");
    let difficulty = row.difficulty.as_deref().unwrap_or("-");
    format!(
        "{quest} / {cadence} / {} / {bucket} / {difficulty}",
        row.domain
    )
}

fn section_heading(label: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn status_style(status: &str) -> Style {
    match status {
        "dirty" => Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
        "saving" | "loading" | "editing" => Style::default().fg(theme::SUCCESS()),
        _ => Style::default().fg(theme::TEXT_FAINT()),
    }
}

fn visible_window_start(selected_index: usize, item_count: usize, height: usize) -> usize {
    if item_count <= height {
        return 0;
    }
    let half_height = height / 2;
    selected_index
        .saturating_sub(half_height)
        .min(item_count.saturating_sub(height))
}

fn truncate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut out = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    out.push_str("...");
    out
}
