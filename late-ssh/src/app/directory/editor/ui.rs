use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::state::{EditorState, Field, FieldKind, Page, ProjectRow, ProjectsView, Scope};
use crate::app::common::{
    composer::placeholder_with_cursor,
    primitives::{format_relative_time_short, hint_line, row_with_hint},
    theme,
};
use late_core::models::work_profile::WorkStatus;

/// Label column, then a space, then the value.
const LABEL_W: u16 = 11;
/// The gutter the focus bar lives in, left of every row.
const GUTTER: u16 = 2;
/// Wide enough for the key line on every page with room to spare; the
/// screen caps it.
const MODAL_W: u16 = 96;

pub(crate) struct EditorView<'a> {
    pub(crate) state: &'a EditorState,
    /// The viewer's projects, newest first, for the projects page.
    pub(crate) projects: &'a [ProjectRow],
    pub(crate) viewer_name: &'a str,
}

/// Draw the editor centred over `area`: a framed modal with the page strip
/// on top, the page's rows, then an error line and the keys.
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &EditorView<'_>) {
    let state = view.state;
    state.clear_row_rects();
    let rows_height = page_height(state, view.projects.len());
    // frame(2) + padding(2) + strip(1) + blank(1) + rows + blank(1) + error(1) + keys(1)
    let wanted = rows_height + 10;
    let popup = centered_rect(area, MODAL_W, wanted);
    frame.render_widget(Clear, popup);

    let subject = super::input::subject_label(state.scope(), view.viewer_name);
    let heading = match state.scope() {
        Scope::Own => " Your profile ".to_string(),
        Scope::CardOf { .. } => format!(" Work card of {subject} "),
        Scope::ProjectOf { .. } => format!(" Project of {subject} "),
    };
    let block = Block::default()
        .title(Span::styled(
            heading,
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [strip, _, body, _, error_row, keys_row] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    draw_page_strip(frame, strip, state, &subject);

    match (state.page(), state.projects_view()) {
        (Page::Projects, ProjectsView::List { selected }) => {
            draw_project_list(frame, body, state, view.projects, *selected);
        }
        _ => draw_fields(frame, body, state),
    }

    if let Some((_, message)) = state.error() {
        frame.render_widget(
            on_canvas(Line::from(Span::styled(
                format!("✗ {message}"),
                Style::default().fg(theme::ERROR()),
            ))),
            indent(error_row),
        );
    }

    draw_keys(frame, keys_row, state);
}

/// `card · about · projects`, the current page bright, the subject on the
/// right. A single-page scope shows its one page and nothing to switch to.
fn draw_page_strip(frame: &mut Frame, area: Rect, state: &EditorState, subject: &str) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for (idx, page) in state.scope().pages().iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(
                "  ·  ",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        let style = if *page == state.page() {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(page.title(), style));
    }
    let right = vec![Span::styled(
        subject.to_string(),
        Style::default().fg(theme::TEXT_DIM()),
    )];
    frame.render_widget(
        on_canvas(row_with_hint(
            spans,
            right,
            area.width.saturating_sub(GUTTER) as usize,
        )),
        indent(area),
    );
}

fn page_height(state: &EditorState, project_count: usize) -> u16 {
    match (state.page(), state.projects_view()) {
        (Page::Projects, ProjectsView::List { .. }) => (project_count.max(1) as u16).clamp(1, 8),
        _ => state.fields().iter().map(|field| field.height()).sum(),
    }
}

/// Every row of the page: `label  value`, the active row marked by a bar in
/// the gutter and a bright label, the row being typed showing its cursor.
fn draw_fields(frame: &mut Frame, area: Rect, state: &EditorState) {
    let fields = state.fields();
    let constraints: Vec<Constraint> = fields
        .iter()
        .map(|field| Constraint::Length(field.height()))
        .collect();
    let rows = Layout::vertical(constraints).split(area);
    for (idx, (field, row_area)) in fields.iter().zip(rows.iter().copied()).enumerate() {
        state.record_row_rect(idx, row_area);
        draw_field_row(frame, row_area, state, *field, idx == state.row());
    }
}

fn draw_field_row(frame: &mut Frame, area: Rect, state: &EditorState, field: Field, active: bool) {
    let typing = active && state.editing();
    let [gutter, label_col, _, value_col] = Layout::horizontal([
        Constraint::Length(GUTTER),
        Constraint::Length(LABEL_W),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(area);

    let bar_style = if active {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER_DIM())
    };
    let bars: Vec<Line> = (0..gutter.height)
        .map(|_| Line::from(Span::styled("\u{258f}", bar_style)))
        .collect();
    frame.render_widget(
        Paragraph::new(bars).style(Style::default().bg(theme::BG_CANVAS())),
        gutter,
    );

    let errored = state.error().is_some_and(|(errored, _)| errored == field);
    let label_style = if errored {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else if active {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    frame.render_widget(
        on_canvas(Line::from(Span::styled(field.label(), label_style))),
        label_col,
    );

    match field.kind() {
        FieldKind::Choice => {
            let (glyph, colour) = match field {
                Field::Status => (status_glyph(state.status()), status_color(state.status())),
                _ => ("", theme::TEXT()),
            };
            let mut spans = Vec::new();
            if !glyph.is_empty() {
                spans.push(Span::styled(
                    format!("{glyph} "),
                    Style::default().fg(colour),
                ));
            }
            spans.push(Span::styled(
                state.field_text(field),
                Style::default()
                    .fg(if active {
                        theme::TEXT_BRIGHT()
                    } else {
                        theme::TEXT()
                    })
                    .add_modifier(Modifier::BOLD),
            ));
            let right = if active {
                vec![Span::styled(
                    "←/→ cycle",
                    Style::default().fg(theme::TEXT_FAINT()),
                )]
            } else {
                Vec::new()
            };
            frame.render_widget(
                on_canvas(row_with_hint(spans, right, value_col.width as usize)),
                value_col,
            );
        }
        FieldKind::Tags => draw_tags_value(frame, value_col, state, field, active),
        FieldKind::Text | FieldKind::Multi => {
            if typing && state.field_text(field).is_empty() {
                // The block cursor sits on the hint's first letter; a bare
                // `TextArea` would draw it in a cell of its own before it.
                frame.render_widget(
                    Paragraph::new(placeholder_with_cursor(field.placeholder()))
                        .style(Style::default().bg(theme::BG_CANVAS())),
                    value_col,
                );
            } else if typing {
                frame.render_widget(state.field(field), value_col);
            } else {
                draw_static_value(frame, value_col, state, field, active);
            }
        }
    }
}

/// A tag row: the picked tags in amber, wrapped over the row's lines, with
/// the picker hint on the right when the row is active; the placeholder
/// dim when nothing is picked.
fn draw_tags_value(frame: &mut Frame, area: Rect, state: &EditorState, field: Field, active: bool) {
    let tags = state.tags(field);
    let mut spans: Vec<Span<'static>> = Vec::new();
    if tags.is_empty() {
        spans.push(Span::styled(
            field.placeholder().to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    for (idx, tag) in tags.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(
                " · ",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        spans.push(Span::styled(
            tag.clone(),
            Style::default().fg(if active {
                theme::AMBER()
            } else {
                theme::AMBER_DIM()
            }),
        ));
    }
    if active {
        spans.push(Span::styled(
            "   Enter pick",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .wrap(Wrap { trim: false })
            .style(Style::default().bg(theme::BG_CANVAS())),
        area,
    );
}

/// A row not being typed: its text, or the placeholder dim when empty.
fn draw_static_value(
    frame: &mut Frame,
    area: Rect,
    state: &EditorState,
    field: Field,
    active: bool,
) {
    let text = state.field_text(field);
    let lines: Vec<Line<'static>> = if text.is_empty() {
        vec![Line::from(Span::styled(
            field.placeholder().to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        ))]
    } else {
        let style = Style::default().fg(if active {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT()
        });
        text.lines()
            .take(area.height as usize)
            .map(|line| Line::from(Span::styled(truncate(line, area.width as usize), style)))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::BG_CANVAS())),
        area,
    );
}

/// The projects page's list: one line a project, `title  tags  age`, the
/// selected one on the selection background.
fn draw_project_list(
    frame: &mut Frame,
    area: Rect,
    state: &EditorState,
    projects: &[ProjectRow],
    selected: usize,
) {
    if projects.is_empty() {
        frame.render_widget(
            on_canvas(Line::from(Span::styled(
                "No projects yet. Press a to add one.",
                Style::default().fg(theme::TEXT_DIM()),
            ))),
            indent(area),
        );
        return;
    }
    let visible = area.height as usize;
    let start = selected.saturating_sub(visible.saturating_sub(1));
    for (offset, project) in projects.iter().skip(start).take(visible).enumerate() {
        let idx = start + offset;
        let row_area = Rect {
            y: area.y + offset as u16,
            height: 1,
            ..area
        };
        state.record_row_rect(idx, row_area);
        let is_selected = idx == selected;
        let marker_style = if is_selected {
            Style::default().fg(theme::BORDER_ACTIVE())
        } else {
            Style::default().fg(theme::BORDER_DIM())
        };
        let title_style = if is_selected {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT())
        };
        let tags = project
            .tags
            .iter()
            .take(4)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        let left = vec![
            Span::styled("\u{258f} ", marker_style),
            Span::styled(truncate(&project.title, 32), title_style),
            Span::styled(format!("  {tags}"), Style::default().fg(theme::AMBER_DIM())),
        ];
        let right = vec![Span::styled(
            format_relative_time_short(project.created),
            Style::default().fg(theme::TEXT_FAINT()),
        )];
        frame.render_widget(
            Paragraph::new(row_with_hint(left, right, area.width as usize))
                .style(theme::row_style(is_selected)),
            row_area,
        );
    }
}

fn draw_keys(frame: &mut Frame, area: Rect, state: &EditorState) {
    let line = if state.confirm_discard() {
        Line::from(vec![
            Span::styled(
                "Discard unsaved changes? ",
                Style::default()
                    .fg(theme::ERROR())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("y", Style::default().fg(theme::AMBER())),
            Span::styled(" discard  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("n", Style::default().fg(theme::AMBER())),
            Span::styled(" keep editing", Style::default().fg(theme::TEXT_DIM())),
        ])
    } else if state.editing() {
        let newline: &[(&str, &str)] = match state.active_field().map(Field::kind) {
            Some(FieldKind::Multi) => &[("Alt+Enter", "new line")],
            _ => &[],
        };
        let mut hints: Vec<(&str, &str)> = vec![("Enter/Tab", "next row"), ("Esc", "done")];
        hints.extend_from_slice(newline);
        hints.push(("Ctrl+S", "save"));
        hint_line(&hints)
    } else {
        match (state.page(), state.projects_view()) {
            (Page::Projects, ProjectsView::List { .. }) => hint_line(&[
                ("a", "add"),
                ("Enter", "edit"),
                ("d", "delete"),
                ("j/k", "select"),
                ("Tab", "page"),
                ("Esc", "close"),
            ]),
            (Page::Projects, ProjectsView::Form(_)) => {
                let back = if state.scope() == &Scope::Own {
                    "back"
                } else {
                    "close"
                };
                hint_line(&[
                    ("Enter", "edit row"),
                    ("j/k", "rows"),
                    ("Ctrl+S", "save project"),
                    ("Esc", back),
                ])
            }
            (Page::Card | Page::About, _) => {
                let mut hints: Vec<(&str, &str)> =
                    vec![("Enter", "edit row"), ("j/k", "rows"), ("←/→", "cycle")];
                if state.scope().pages().len() > 1 {
                    hints.push(("Tab", "page"));
                }
                hints.push(("Ctrl+S", "save"));
                hints.push(("Esc", "close"));
                hint_line(&hints)
            }
        }
    };
    frame.render_widget(on_canvas(line), area);
}

pub(crate) fn status_glyph(status: WorkStatus) -> &'static str {
    match status {
        WorkStatus::Open => "●",
        WorkStatus::Casual => "◐",
        WorkStatus::NotLooking => "○",
    }
}

pub(crate) fn status_color(status: WorkStatus) -> ratatui::style::Color {
    match status {
        WorkStatus::Open => theme::SUCCESS(),
        WorkStatus::Casual => theme::AMBER(),
        WorkStatus::NotLooking => theme::TEXT_DIM(),
    }
}

fn indent(area: Rect) -> Rect {
    Rect {
        x: area.x + GUTTER,
        width: area.width.saturating_sub(GUTTER),
        ..area
    }
}

fn on_canvas(line: Line<'static>) -> Paragraph<'static> {
    Paragraph::new(line).style(Style::default().bg(theme::BG_CANVAS()))
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > width - 1 {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}
