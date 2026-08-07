use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    chat::{
        showcase::{self, svc::ShowcaseFeedItem},
        work,
    },
    common::{
        markdown::render_body_to_lines,
        primitives::{format_relative_time, hint_line, row_with_hint},
        theme,
    },
    directory::state::{DirectoryState, PersonEntry, PersonFocus, person_entries},
};

const IDLE_HINTS: &[(&str, &str)] = &[
    ("j/k", "people"),
    ("h/l", "focus"),
    ("Enter", "copy link"),
    ("o", "profile"),
    ("i", "new project"),
    ("w", "work card"),
    ("e", "edit"),
    ("d", "delete"),
    ("/", "mine"),
    ("s", "search"),
];

/// Uniform people-row height: four content lines plus the bottom border.
const ITEM_HEIGHT: u16 = 5;
/// Minimum page width for the side-by-side list + detail layout.
const DETAIL_MIN_WIDTH: u16 = 86;

pub(crate) struct DirectoryPageView<'a> {
    pub(crate) directory: &'a DirectoryState,
    pub(crate) work_state: &'a work::state::State,
    pub(crate) showcase_state: &'a showcase::state::State,
    pub(crate) current_user_id: uuid::Uuid,
    pub(crate) profile_base_url: &'a str,
}

pub(crate) fn draw_directory_page(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let work_composing = view.work_state.composing();
    let showcase_composing = view.showcase_state.composing();
    let footer_height = if work_composing {
        11
    } else if showcase_composing {
        10
    } else {
        1
    };
    let search_height = if view.directory.search_mode() { 3 } else { 0 };
    let layout = Layout::vertical([
        Constraint::Length(search_height),
        Constraint::Fill(1),
        Constraint::Length(footer_height),
    ])
    .split(area);

    if view.directory.search_mode() {
        draw_search_box(frame, layout[0], view.directory.search_query());
    }

    let entries = person_entries(
        view.showcase_state.all_items(),
        view.work_state.all_items(),
        view.directory.mine_only,
        view.current_user_id,
        view.directory.active_query(),
    );
    let selected = view
        .directory
        .selected()
        .min(entries.len().saturating_sub(1));

    let body = layout[1];
    if body.width >= DETAIL_MIN_WIDTH {
        let cols =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Fill(1)]).split(body);
        draw_people_list(frame, cols[0], &view, &entries, selected);
        draw_person_detail(frame, cols[1], &view, entries.get(selected));
    } else {
        draw_people_list(frame, body, &view, &entries, selected);
    }

    if work_composing {
        work::ui::draw_work_composer(
            frame,
            layout[2],
            &work::ui::WorkComposerView {
                state: view.work_state,
            },
        );
    } else if showcase_composing {
        showcase::ui::draw_showcase_composer(
            frame,
            layout[2],
            &showcase::ui::ShowcaseComposerView {
                state: view.showcase_state,
            },
        );
    } else {
        let right = if view.directory.mine_only {
            vec![Span::styled(
                "mine only ",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )]
        } else {
            Vec::new()
        };
        let line = row_with_hint(hint_line(IDLE_HINTS).spans, right, layout[2].width as usize);
        frame.render_widget(Paragraph::new(line), layout[2]);
    }
}

fn draw_search_box(frame: &mut Frame, area: Rect, query: &str) {
    if area.height == 0 {
        return;
    }
    let block = Block::default()
        .title(" Search people and projects ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            query.to_string(),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ))),
        inner,
    );
}

fn draw_people_list(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    entries: &[PersonEntry<'_>],
    selected: usize,
) {
    if entries.is_empty() {
        let lines = vec![
            Line::from(Span::styled(
                "Nobody here yet.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(Span::styled(
                "Press 'i' to share a project, 'w' to post your work card;",
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(Span::styled(
                "either one puts you on this page.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let visible_items = ((area.height / ITEM_HEIGHT).max(1)) as usize;
    let start_index = selected.saturating_sub(visible_items.saturating_sub(1));
    let end_index = (start_index + visible_items).min(entries.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(ITEM_HEIGHT), visible_len).collect::<Vec<_>>();
    let rows = Layout::vertical(constraints).split(area);

    for (row, row_area) in rows.iter().copied().enumerate() {
        let entry_idx = start_index + row;
        let entry = &entries[entry_idx];
        let is_selected = entry_idx == selected;
        let bg = if is_selected {
            theme::BG_SELECTION()
        } else {
            Color::Reset
        };
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER()))
            .style(Style::default().bg(bg));
        let content = block.inner(row_area);
        frame.render_widget(block, row_area);

        let lines = person_row_lines(
            entry,
            view.current_user_id,
            view.work_state.marker_read_at(),
            view.showcase_state.marker_read_at(),
            content.width as usize,
        );
        frame.render_widget(Paragraph::new(lines), content);
    }
}

/// Four-line person row: @username, what they are (status or project count),
/// their freshest line (headline or latest project), tags.
fn person_row_lines(
    entry: &PersonEntry<'_>,
    viewer: uuid::Uuid,
    work_marker: Option<chrono::DateTime<chrono::Utc>>,
    showcase_marker: Option<chrono::DateTime<chrono::Utc>>,
    width: usize,
) -> Vec<Line<'static>> {
    let is_unread = entry.is_unread(work_marker, showcase_marker);
    let own = entry.user_id == viewer;

    let mut lines = Vec::with_capacity(4);
    lines.push(name_line(entry.username, own, is_unread, width));

    // Meta: status word (colored) when they have a card, project count, and
    // how fresh their latest activity is.
    let mut meta_spans: Vec<Span<'static>> = Vec::new();
    if let Some(item) = entry.work {
        meta_spans.push(Span::styled(
            work::state::status_label(&item.profile.status).to_string(),
            Style::default()
                .fg(status_color(&item.profile.status))
                .add_modifier(Modifier::BOLD),
        ));
    }
    let mut trailing = String::new();
    if !entry.projects.is_empty() {
        if entry.work.is_some() {
            trailing.push_str(" · ");
        }
        let count = entry.projects.len();
        let noun = if count == 1 { "project" } else { "projects" };
        trailing.push_str(&format!("{count} {noun}"));
    }
    trailing.push_str(&format!(
        " · {}",
        format_relative_time(entry.latest_activity())
    ));
    meta_spans.push(Span::styled(
        truncate_to_width(&trailing, width),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    lines.push(Line::from(meta_spans));

    // Freshest line: the card headline when present, else the latest project.
    match (entry.work, entry.projects.first()) {
        (Some(item), _) => lines.push(Line::from(Span::styled(
            truncate_to_width(&item.profile.headline, width),
            Style::default().fg(theme::TEXT()),
        ))),
        (None, Some(item)) => lines.push(Line::from(vec![
            Span::styled("↳ ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                truncate_to_width(&item.showcase.title, width.saturating_sub(2)),
                Style::default().fg(theme::TEXT()),
            ),
        ])),
        (None, None) => lines.push(Line::from("")),
    }

    // Tags: skills when they have a card, else the latest project's tags.
    let tags = match (entry.work, entry.projects.first()) {
        (Some(item), _) => item
            .profile
            .skills
            .iter()
            .map(|s| format!("#{s}"))
            .collect::<Vec<_>>()
            .join(" "),
        (None, Some(item)) => item
            .showcase
            .tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(" "),
        (None, None) => String::new(),
    };
    lines.push(Line::from(Span::styled(
        truncate_to_width(&tags, width),
        Style::default().fg(theme::AMBER_DIM()),
    )));
    lines
}

fn name_line(username: &str, own: bool, is_unread: bool, width: usize) -> Line<'static> {
    let unread_prefix = if is_unread { "● " } else { "" };
    let unread_w = UnicodeWidthStr::width(unread_prefix);
    let badge = if own { "(you)" } else { "" };
    let badge_w = UnicodeWidthStr::width(badge);
    let name = format!("@{username}");
    let name_budget = if own {
        width
            .saturating_sub(unread_w)
            .saturating_sub(badge_w + 1)
            .max(4)
    } else {
        width.saturating_sub(unread_w).max(4)
    };
    let truncated = truncate_to_width(&name, name_budget);
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
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ));
    if own {
        let used = unread_w + truncated_w;
        let pad = width.saturating_sub(used + badge_w).max(1);
        spans.push(Span::raw(" ".repeat(pad)));
        spans.push(Span::styled(badge, Style::default().fg(theme::AMBER_DIM())));
    }
    Line::from(spans)
}

fn status_color(status: &str) -> Color {
    match status {
        "open" => theme::SUCCESS(),
        "casual" => theme::AMBER(),
        _ => theme::TEXT_DIM(),
    }
}

/// The whole person in one scroll: header, bio, late.fetch, their work card,
/// then every project. The `h`/`l` focus cursor paints a `▸` marker on the
/// focused section; Enter/e/d act on it.
fn draw_person_detail(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    entry: Option<&PersonEntry<'_>>,
) {
    let block = Block::default()
        .title(" Person ")
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(entry) = entry else {
        frame.render_widget(
            Paragraph::new("Nobody selected.").style(Style::default().fg(theme::TEXT_DIM())),
            inner,
        );
        return;
    };

    let detail_width = inner.width as usize;
    let focus = view
        .directory
        .focus()
        .min(entry.focus_len().saturating_sub(1));
    let card_focused = entry.work.is_some() && focus == 0;
    let focused_project = match entry.focus_target(focus) {
        Some(PersonFocus::Project(item)) => Some(item.showcase.id),
        Some(PersonFocus::Card(_)) => None,
        None => None,
    };

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Header: who this is and how fresh.
    lines.push(Line::from(vec![
        Span::styled(
            format!("@{}", entry.username),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  active {}", format_relative_time(entry.latest_activity())),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]));
    lines.push(Line::from(""));

    if let Some(author_profile) = entry.author_profile() {
        if !author_profile.bio.trim().is_empty() {
            lines.extend(render_body_to_lines(
                &author_profile.bio,
                detail_width,
                Span::raw(""),
                Style::default().fg(theme::TEXT()),
            ));
            lines.push(Line::from(""));
        }
        lines.extend(late_fetch_lines(author_profile, detail_width));
        lines.push(Line::from(""));
    }

    if let Some(item) = entry.work {
        let p = &item.profile;
        lines.push(focus_header("work card", card_focused));
        lines.push(Line::from(vec![
            Span::styled(
                p.headline.clone(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                work::state::status_label(&p.status).to_string(),
                Style::default()
                    .fg(status_color(&p.status))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}  {}", p.work_type, p.location),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        for paragraph in p.summary.lines().filter(|line| !line.trim().is_empty()) {
            lines.push(Line::from(Span::styled(
                paragraph.trim().to_string(),
                Style::default().fg(theme::TEXT()),
            )));
        }
        if !p.skills.is_empty() {
            lines.push(Line::from(Span::styled(
                p.skills
                    .iter()
                    .map(|skill| format!("#{skill}"))
                    .collect::<Vec<_>>()
                    .join(" "),
                Style::default().fg(theme::AMBER_DIM()),
            )));
        }
        if !p.contact.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                format!("contact: {}", p.contact.trim()),
                Style::default().fg(theme::TEXT()),
            )));
        }
        for link in &p.links {
            lines.push(Line::from(Span::styled(
                format!("-> {link}"),
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
        lines.push(Line::from(Span::styled(
            work::state::profile_url(view.profile_base_url, &p.slug),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }

    for item in &entry.projects {
        let focused = focused_project == Some(item.showcase.id);
        lines.extend(project_lines(item, focused));
        lines.push(Line::from(""));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// One project section: title (with focus marker), URL, description (full
/// when focused, first line otherwise), tags.
fn project_lines(item: &ShowcaseFeedItem, focused: bool) -> Vec<Line<'static>> {
    let s = &item.showcase;
    let mut lines = Vec::new();

    let marker = if focused { "▸ " } else { "  " };
    let marker_style = if focused {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    lines.push(Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(
            s.title.clone(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", format_relative_time(s.created)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("  ↗ ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            display_link(&s.url),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    if focused {
        for paragraph in s.description.lines().filter(|line| !line.trim().is_empty()) {
            lines.push(Line::from(Span::styled(
                format!("  {}", paragraph.trim()),
                Style::default().fg(theme::TEXT()),
            )));
        }
    } else if let Some(first) = s
        .description
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        lines.push(Line::from(Span::styled(
            format!("  {first}"),
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    if !s.tags.is_empty() {
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                s.tags
                    .iter()
                    .map(|tag| format!("#{tag}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    }
    lines
}

fn focus_header(label: &'static str, focused: bool) -> Line<'static> {
    let marker = if focused { "▸ " } else { "  " };
    let marker_style = if focused {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(
            format!("# {label}"),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn late_fetch_lines(
    profile: &late_core::models::profile::Profile,
    width: usize,
) -> Vec<Line<'static>> {
    let label = Style::default().fg(theme::AMBER_DIM());
    let value = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());

    let created = profile
        .created_at
        .as_ref()
        .map(format_created_at)
        .unwrap_or_else(|| "-".to_string());
    let theme_id = profile.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID);
    let theme_label = theme::label_for_id(theme_id).to_string();
    let ide = profile.ide.as_deref().unwrap_or("-");
    let terminal = profile.terminal.as_deref().unwrap_or("-");
    let os = profile.os.as_deref().unwrap_or("-");
    let langs = if profile.langs.is_empty() {
        "-".to_string()
    } else {
        profile.langs.join(" · ")
    };

    let col_w = (width / 2).max(12);
    vec![
        Line::from(format_late_fetch_row(
            ("created", &created),
            ("theme", &theme_label),
            col_w,
            label,
            value,
            dim,
        )),
        Line::from(format_late_fetch_row(
            ("ide", ide),
            ("terminal", terminal),
            col_w,
            label,
            value,
            dim,
        )),
        Line::from(format_late_fetch_row(
            ("os", os),
            ("langs", &langs),
            col_w,
            label,
            value,
            dim,
        )),
    ]
}

fn format_late_fetch_row(
    a: (&str, &str),
    b: (&str, &str),
    col_w: usize,
    label_style: Style,
    value_style: Style,
    sep_style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (idx, (label, value)) in [a, b].into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(" | ", sep_style));
        }
        let label_padded = format!("{label:<9} ");
        let used = label_padded.chars().count() + value.chars().count();
        let pad = col_w.saturating_sub(used + if idx == 0 { 2 } else { 0 });
        spans.push(Span::styled(label_padded, label_style));
        spans.push(Span::styled(value.to_string(), value_style));
        if idx == 0 {
            spans.push(Span::raw(" ".repeat(pad)));
        }
    }
    spans
}

fn format_created_at(created_at: &chrono::DateTime<chrono::Utc>) -> String {
    created_at.format("%Y-%m-%d").to_string()
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
    let budget = width - 1;
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
