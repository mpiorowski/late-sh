use crate::app::common::theme;
use crate::app::common::{composer, primitives::format_relative_time};
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::state::{ComposerField, State, status_label};
use super::svc::WorkFeedItem;

const META_SEP: &str = " · ";

pub struct WorkListView<'a> {
    pub items: &'a [WorkFeedItem],
    pub selected_index: usize,
    pub current_user_id: uuid::Uuid,
    pub is_admin: bool,
    pub marker_read_at: Option<DateTime<Utc>>,
    pub profile_base_url: &'a str,
    pub mine_only: bool,
}

const ITEM_HEIGHT: u16 = 9;
const SUMMARY_LINES: usize = 2;

pub fn draw_work_list(frame: &mut Frame, area: Rect, view: &WorkListView<'_>) {
    let inner = area;

    if view.items.is_empty() {
        let text = Text::from(vec![
            Line::from(Span::styled(
                "No work profiles yet.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(Span::styled(
                "Press 'i' to create yours.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ]);
        frame.render_widget(Paragraph::new(text), inner);
        return;
    }

    let visible_items = ((inner.height / ITEM_HEIGHT).max(1)) as usize;
    let selected_index = view.selected_index.min(view.items.len().saturating_sub(1));
    let start_index = selected_index.saturating_sub(visible_items.saturating_sub(1));
    let end_index = (start_index + visible_items).min(view.items.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    for (row, item_area) in layout.iter().copied().enumerate() {
        let item_idx = start_index + row;
        let item = &view.items[item_idx];
        let p = &item.profile;
        let is_selected = item_idx == selected_index;
        let is_unread = view
            .marker_read_at
            .map(|last_read_at| p.updated > last_read_at)
            .unwrap_or(true);
        let bg = if is_selected {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };

        let item_block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(Style::default().bg(bg));
        let content_area = item_block.inner(item_area);
        frame.render_widget(item_block, item_area);

        let owner = p.user_id == view.current_user_id;
        let inner_w = content_area.width as usize;
        let mut lines: Vec<Line<'static>> = Vec::with_capacity(8);

        // Row 1: title — unread dot + headline left, `(yours)` right-aligned when owner.
        lines.push(build_title_line(&p.headline, owner, is_unread, inner_w));

        // Row 2: meta — `@user · status · type · location · just now`
        lines.push(build_meta_line(
            &item.author_username,
            status_label(&p.status),
            &p.work_type,
            &p.location,
            &format_relative_time(p.updated),
            inner_w,
        ));

        // Rows 3-4: summary (up to SUMMARY_LINES)
        let (mut summary_rows, truncated) = summary_lines(&p.summary, inner_w, SUMMARY_LINES);
        if truncated && let Some(last) = summary_rows.last_mut() {
            apply_inline_ellipsis(last, inner_w);
        }
        for row in summary_rows {
            lines.push(Line::from(Span::styled(
                row,
                Style::default().fg(theme::TEXT()),
            )));
        }

        // Row 5: skills, joined and truncated. Skipped entirely when empty.
        if !p.skills.is_empty() {
            let skills_text = p
                .skills
                .iter()
                .map(|s| format!("#{s}"))
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(Line::from(Span::styled(
                truncate_to_width(&skills_text, inner_w),
                Style::default().fg(theme::AMBER_DIM()),
            )));
        }

        // Row 6: ALL links — protocol stripped, joined with ` · `.
        if !p.links.is_empty() {
            let links_text = p
                .links
                .iter()
                .map(|link| display_link(link))
                .collect::<Vec<_>>()
                .join(META_SEP);
            lines.push(Line::from(vec![
                Span::styled("↗ ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    truncate_to_width(&links_text, inner_w.saturating_sub(2)),
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }

        // Row 7: contact.
        if !p.contact.trim().is_empty() {
            let contact_text = format!("contact: {}", p.contact.trim());
            lines.push(Line::from(Span::styled(
                truncate_to_width(&contact_text, inner_w),
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }

        // Row 8: share footer — `late.sh/profiles/w_abc...` (protocol stripped).
        let share_url = super::state::profile_url(view.profile_base_url, &p.slug);
        let share_display = display_link(&share_url);
        lines.push(Line::from(Span::styled(
            truncate_to_width(&share_display, inner_w),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        )));

        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: true }),
            content_area,
        );
    }
}

fn build_title_line(headline: &str, owner: bool, is_unread: bool, width: usize) -> Line<'static> {
    let unread_prefix = if is_unread { "● " } else { "" };
    let unread_w = UnicodeWidthStr::width(unread_prefix);
    let badge = if owner { "(yours)" } else { "" };
    let badge_w = UnicodeWidthStr::width(badge);
    // Keep at least one space between headline and badge; if width is so tight
    // we can't fit the badge, drop it rather than crowd the headline.
    let headline_budget = if owner {
        width
            .saturating_sub(unread_w)
            .saturating_sub(badge_w + 1)
            .max(4)
    } else {
        width.saturating_sub(unread_w).max(4)
    };
    let truncated = truncate_to_width(headline, headline_budget);
    let truncated_w = UnicodeWidthStr::width(truncated.as_str());

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(4);
    if is_unread {
        spans.push(Span::styled(
            "● ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        truncated,
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ));

    if owner {
        let used = unread_w + truncated_w;
        let pad = width.saturating_sub(used + badge_w).max(1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(badge, Style::default().fg(theme::AMBER_DIM())));
    }
    Line::from(spans)
}

fn build_meta_line(
    username: &str,
    status: &str,
    work_type: &str,
    location: &str,
    relative_time: &str,
    width: usize,
) -> Line<'static> {
    // Build meta as a single styled string then truncate. The username keeps
    // its own color span; everything after collapses into one dim trailing
    // span so truncation stays simple.
    let trailing = format!(
        "{sep}{status}{sep}{work_type}{sep}{location}{sep}{time}",
        sep = META_SEP,
        status = status,
        work_type = work_type,
        location = location,
        time = relative_time,
    );
    let prefix = format!("@{username}");
    let prefix_w = UnicodeWidthStr::width(prefix.as_str());
    let trailing_budget = width.saturating_sub(prefix_w);
    let trailing_truncated = truncate_to_width(&trailing, trailing_budget);

    Line::from(vec![
        Span::styled(
            prefix,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(trailing_truncated, Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn display_link(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    stripped.trim_end_matches('/').to_string()
}

fn truncate_to_width(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let budget = width - 1; // reserve one cell for the ellipsis
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > budget {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

fn apply_inline_ellipsis(line: &mut String, width: usize) {
    let width = width.max(1);
    if line.chars().count() < width {
        line.push_str("...");
        return;
    }
    line.pop();
    line.push('.');
}

fn summary_lines(summary: &str, width: usize, max_lines: usize) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut truncated = false;
    for paragraph in summary.lines().filter(|line| !line.trim().is_empty()) {
        let wrapped = composer::build_composer_rows(paragraph.trim(), width.max(1));
        let rows: Vec<String> = if wrapped.is_empty() {
            vec![String::new()]
        } else {
            wrapped.into_iter().map(|row| row.text).collect()
        };
        for row in rows {
            if out.len() == max_lines {
                truncated = true;
                break;
            }
            out.push(row);
        }
        if truncated {
            break;
        }
    }
    (out, truncated)
}

pub struct WorkComposerView<'a> {
    pub state: &'a State,
}

pub fn draw_work_composer(frame: &mut Frame, area: Rect, view: &WorkComposerView<'_>) {
    let editing = view.state.editing();
    let composing = view.state.composing();
    let active = view.state.active_field();

    let title = if !composing {
        " Work "
    } else if editing {
        " Editing work profile - Tab/S+Tab switch - Enter submit - Alt+Enter/Ctrl+J newline - Esc cancel "
    } else {
        " New work profile - Tab/S+Tab switch - Enter submit - Alt+Enter/Ctrl+J newline - Esc cancel "
    };
    let border_style = if composing {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else {
        Style::default().fg(theme::BORDER())
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !composing {
        let hint = Paragraph::new(Line::from(Span::styled(
            " j/k navigate - Enter/c copy profile - i create/edit yours - e edit selected - d delete own - / filter mine",
            Style::default().fg(theme::TEXT_DIM()),
        )));
        frame.render_widget(hint, inner);
        return;
    }

    let constraints = [
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(2),
    ];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);

    draw_field(frame, rows[0], view.state, ComposerField::Headline, active);
    draw_field(frame, rows[1], view.state, ComposerField::Status, active);
    draw_field(frame, rows[2], view.state, ComposerField::Type, active);
    draw_field(frame, rows[3], view.state, ComposerField::Location, active);
    draw_field(frame, rows[4], view.state, ComposerField::Contact, active);
    draw_field(frame, rows[5], view.state, ComposerField::Links, active);
    draw_field(frame, rows[6], view.state, ComposerField::Skills, active);
    draw_field(frame, rows[7], view.state, ComposerField::Summary, active);
}

fn draw_field(
    frame: &mut Frame,
    area: Rect,
    state: &State,
    field: ComposerField,
    active: ComposerField,
) {
    let is_active = field == active;
    let label_style = if is_active {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let label_w: u16 = 14;
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(label_w),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);
    let prefix = if is_active { "> " } else { "  " };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{prefix}{}:", field.label()),
            label_style,
        ))),
        split[0],
    );
    frame.render_widget(Paragraph::new(" "), split[1]);
    if state.field_is_empty(field) {
        draw_empty_placeholder(frame, split[2], field.placeholder(), is_active);
    } else {
        frame.render_widget(state.field_textarea(field), split[2]);
    }
}

fn draw_empty_placeholder(frame: &mut Frame, area: Rect, placeholder: &str, active: bool) {
    let mut chars = placeholder.chars();
    let Some(first) = chars.next() else {
        return;
    };
    let rest = chars.collect::<String>();
    let first = if active {
        Span::styled(
            first.to_string(),
            Style::default()
                .fg(theme::BG_CANVAS())
                .bg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(first.to_string(), Style::default().fg(theme::TEXT_DIM()))
    };
    let line = Line::from(vec![
        first,
        Span::styled(rest, Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line).wrap(Wrap { trim: false }), area);
}

#[cfg(test)]
mod tests {
    use super::{display_link, summary_lines, truncate_to_width};

    #[test]
    fn summary_lines_wrap_to_budget() {
        let (lines, truncated) = summary_lines("hello wide world", 8, 2);
        assert_eq!(lines, vec!["hello", "wide"]);
        assert!(truncated);
    }

    #[test]
    fn display_link_strips_protocol_and_trailing_slash() {
        assert_eq!(display_link("https://github.com/me/"), "github.com/me");
        assert_eq!(display_link("http://cv.example/"), "cv.example");
        assert_eq!(display_link("ftp://no-strip"), "ftp://no-strip");
    }

    #[test]
    fn truncate_to_width_appends_ellipsis_when_overflowing() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello world", 8), "hello w…");
        assert_eq!(truncate_to_width("hello", 0), "");
        assert_eq!(truncate_to_width("hello", 1), "…");
    }
}
