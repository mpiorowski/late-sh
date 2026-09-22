use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    chat::{showcase::svc::ShowcaseFeedItem, work::svc::WorkFeedItem},
    common::{
        markdown::render_body_to_lines,
        primitives::{format_relative_time, hint_line, row_with_hint},
        theme,
    },
    directory::{
        editor::ui::{status_color, status_glyph},
        state::{DirectoryState, PersonEntry, PersonFocus, Shelf, person_entries, person_row},
    },
    jobs::{
        state::{FOR_YOU_LIMIT, JobsState, matches, viewer_tags, wants_matches},
        ui::{JobsShelfView, draw_jobs_shelf, footer_note, for_you_lines, shelf_count},
    },
};

const PEOPLE_HINTS: &[(&str, &str)] = &[
    ("j/k", "people"),
    ("h/l", "focus"),
    ("Enter", "copy"),
    ("w", "profile"),
    ("i", "project"),
    ("e", "edit"),
    ("d", "delete"),
    ("s", "search"),
    ("/", "mine"),
    ("Space", "jobs"),
];
const PEOPLE_NARROW_HINTS: &[(&str, &str)] = &[
    ("j/k", "people"),
    ("l", "open"),
    ("w", "your profile"),
    ("i", "add project"),
    ("s", "search"),
    ("/", "mine"),
    ("Space", "jobs"),
];
const DETAIL_NARROW_HINTS: &[(&str, &str)] = &[
    ("h", "back"),
    ("l", "focus"),
    ("Enter", "copy link"),
    ("e", "edit"),
    ("d", "delete"),
    ("o", "card"),
];

/// Below this width the list and the detail stack instead of sitting side
/// by side.
const STACK_BELOW_WIDTH: u16 = 100;
/// Below this height a person's row loses its tag line.
const SHORT_BELOW_HEIGHT: u16 = 24;

pub(crate) struct DirectoryPageView<'a> {
    pub(crate) directory: &'a DirectoryState,
    pub(crate) jobs: &'a JobsState,
    /// The viewer's languages from their profile, part of their match tags.
    pub(crate) viewer_langs: &'a [String],
    pub(crate) projects: &'a [ShowcaseFeedItem],
    pub(crate) people: &'a [WorkFeedItem],
    pub(crate) work_marker: Option<DateTime<Utc>>,
    pub(crate) showcase_marker: Option<DateTime<Utc>>,
    pub(crate) current_user_id: uuid::Uuid,
    pub(crate) profile_base_url: &'a str,
}

pub(crate) fn draw_directory_page(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let narrow = area.width < STACK_BELOW_WIDTH;
    view.directory.set_narrow(narrow);
    view.jobs.set_narrow(narrow);
    let short = area.height < SHORT_BELOW_HEIGHT;
    let own_tags = own_viewer_tags(&view);

    let search_height = if view.directory.search_mode() { 3 } else { 0 };
    let [strip, search, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(search_height),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let entries = person_entries(
        view.projects,
        view.people,
        view.directory.mine_only,
        view.current_user_id,
        view.directory.active_query(),
    );
    let selected = view
        .directory
        .selected()
        .min(entries.len().saturating_sub(1));

    draw_shelf_strip(frame, strip, &view, entries.len(), &own_tags);
    if view.directory.search_mode() {
        draw_search_box(frame, search, view.directory.search_query());
    }

    match view.directory.shelf() {
        Shelf::Jobs => {
            let shelf = JobsShelfView {
                jobs: view.jobs,
                viewer_tags: &own_tags,
                narrow,
                short,
            };
            draw_jobs_shelf(frame, body, &shelf);
            let line = row_with_hint(
                hint_line(crate::app::jobs::ui::hints(&shelf)).spans,
                footer_note(view.jobs),
                footer.width as usize,
            );
            frame.render_widget(Paragraph::new(line), footer);
        }
        Shelf::People => {
            let hints = if !narrow {
                let cols = Layout::horizontal([Constraint::Percentage(42), Constraint::Fill(1)])
                    .split(body);
                draw_people_list(frame, cols[0], &view, &entries, selected, short);
                draw_person_detail(frame, cols[1], &view, entries.get(selected), &own_tags);
                PEOPLE_HINTS
            } else if view.directory.detail_open() {
                draw_person_detail(frame, body, &view, entries.get(selected), &own_tags);
                DETAIL_NARROW_HINTS
            } else {
                draw_people_list(frame, body, &view, &entries, selected, short);
                PEOPLE_NARROW_HINTS
            };
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
            let line = row_with_hint(hint_line(hints).spans, right, footer.width as usize);
            frame.render_widget(Paragraph::new(line), footer);
        }
    }
}

/// The viewer's match tags: their own card's normalized skills plus their
/// profile languages.
fn own_viewer_tags(view: &DirectoryPageView<'_>) -> Vec<String> {
    let card = view
        .people
        .iter()
        .find(|item| item.profile.user_id == view.current_user_id)
        .map(|item| &item.profile);
    viewer_tags(card, view.viewer_langs)
}

/// `people 12 · jobs 31`, the active shelf bright, with the search named
/// on the right while it is open.
fn draw_shelf_strip(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    people: usize,
    own_tags: &[String],
) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (idx, shelf) in [Shelf::People, Shelf::Jobs].into_iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled(
                "  ·  ",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        let active = shelf == view.directory.shelf();
        let style = if active {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(shelf.title(), style));
        let count = match shelf {
            Shelf::People => Some(people),
            Shelf::Jobs => shelf_count(view.jobs, own_tags),
        };
        if let Some(count) = count {
            spans.push(Span::styled(
                format!(" {count}"),
                Style::default().fg(if active {
                    theme::AMBER_DIM()
                } else {
                    theme::TEXT_FAINT()
                }),
            ));
        }
    }
    let right = if view.directory.search_mode() {
        vec![Span::styled(
            "searching ",
            Style::default().fg(theme::TEXT_DIM()),
        )]
    } else {
        Vec::new()
    };
    frame.render_widget(
        Paragraph::new(row_with_hint(spans, right, area.width as usize)),
        area,
    );
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
        Paragraph::new(Line::from(vec![
            Span::styled(query.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled("▏", Style::default().fg(theme::AMBER())),
        ])),
        inner,
    );
}

fn draw_people_list(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    entries: &[PersonEntry<'_>],
    selected: usize,
    short: bool,
) {
    if entries.is_empty() {
        let dim = Style::default().fg(theme::TEXT_DIM());
        let lines = if view.directory.search_mode() || view.directory.mine_only {
            vec![
                Line::from(""),
                Line::from(Span::styled("  Nobody matches.", dim)),
            ]
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled("  Nobody here yet.", dim)),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Press ", dim),
                    Span::styled("w", Style::default().fg(theme::AMBER())),
                    Span::styled(" to post your work card or ", dim),
                    Span::styled("i", Style::default().fg(theme::AMBER())),
                    Span::styled(" to share a project;", dim),
                ]),
                Line::from(Span::styled("  either one puts you on this page.", dim)),
            ]
        };
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    // Three content lines plus the rule under each row; two when short.
    let item_height: u16 = if short { 3 } else { 4 };
    let visible_items = ((area.height / item_height).max(1)) as usize;
    let start_index = selected.saturating_sub(visible_items.saturating_sub(1));
    let end_index = (start_index + visible_items).min(entries.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(item_height), visible_len).collect::<Vec<_>>();
    let rows = Layout::vertical(constraints).split(area);

    for (row, row_area) in rows.iter().copied().enumerate() {
        let entry_idx = start_index + row;
        let entry = &entries[entry_idx];
        let is_selected = entry_idx == selected;
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER_DIM()))
            .style(theme::row_style(is_selected));
        let content = block.inner(row_area);
        frame.render_widget(block, row_area);

        let person = person_row(
            entry,
            view.current_user_id,
            view.work_marker,
            view.showcase_marker,
        );
        let lines = person_row_lines(&person, is_selected, short, content.width as usize);
        frame.render_widget(Paragraph::new(lines), content);
    }
}

/// Line 1: name, status (or project count without a card), age at the
/// right. Line 2: headline or newest project. Line 3: tags, and the project
/// count at the right when the person has both.
fn person_row_lines(
    person: &super::state::PersonRow,
    selected: bool,
    short: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let gutter = if selected { "▎" } else { " " };
    let gutter_style = Style::default().fg(theme::BORDER_ACTIVE());
    let name_style = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);

    let mut first: Vec<Span<'static>> = vec![Span::styled(gutter, gutter_style)];
    if person.unread {
        first.push(Span::styled(
            "● ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    }
    first.push(Span::styled(format!("@{}", person.name), name_style));
    if person.own {
        first.push(Span::styled(
            " you",
            Style::default().fg(theme::AMBER_DIM()),
        ));
    }
    match person.status {
        Some(status) => {
            first.push(Span::styled(
                format!("  {} {}", status_glyph(status), status.label()),
                Style::default().fg(status_color(status)),
            ));
        }
        None => {
            first.push(Span::styled(
                format!("  {}", project_count_label(person.project_count)),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
    }
    let age = vec![Span::styled(
        format!("{} ", person.age),
        Style::default().fg(theme::TEXT_FAINT()),
    )];
    let mut lines = vec![row_with_hint(first, age, width)];

    let second_style = Style::default().fg(if selected {
        theme::TEXT_BRIGHT()
    } else {
        theme::TEXT()
    });
    let second_prefix = if person.status.is_none() && !person.second.is_empty() {
        "↳ "
    } else {
        ""
    };
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(second_prefix, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            truncate_to_width(
                &person.second,
                width.saturating_sub(1 + second_prefix.width()),
            ),
            second_style,
        ),
    ]));

    if !short {
        let right = if person.status.is_some() && person.project_count > 0 {
            vec![Span::styled(
                format!("{} ", project_count_label(person.project_count)),
                Style::default().fg(theme::TEXT_FAINT()),
            )]
        } else {
            Vec::new()
        };
        // The count keeps its place: the tags give way to it.
        let right_width: usize = right.iter().map(|span| span.content.width()).sum();
        // `row_with_hint` wants two cells of air before the right side.
        let tags_budget = if right_width > 0 {
            width.saturating_sub(1 + right_width + 2)
        } else {
            width.saturating_sub(1)
        };
        let tags = vec![
            Span::raw(" "),
            Span::styled(
                truncate_to_width(&person.tags.join(" · "), tags_budget),
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ];
        lines.push(row_with_hint(tags, right, width));
    }
    lines
}

fn project_count_label(count: usize) -> String {
    match count {
        1 => "1 project".to_string(),
        n => format!("{n} projects"),
    }
}

/// The whole person: a header row, the card, the bio, the projects, and
/// late.fetch last as a dim block. The `h`/`l` focus cursor paints a `▸`
/// on the focused section; Enter/e/d act on it.
fn draw_person_detail(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    entry: Option<&PersonEntry<'_>>,
    own_tags: &[String],
) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(2),
        ..inner
    };

    let Some(entry) = entry else {
        frame.render_widget(
            Paragraph::new("Nobody selected.").style(Style::default().fg(theme::TEXT_DIM())),
            inner,
        );
        return;
    };

    let width = inner.width as usize;
    let focus = view
        .directory
        .focus()
        .min(entry.focus_len().saturating_sub(1));
    let card_focused = entry.work.is_some() && focus == 0;
    let focused_project = match entry.focus_target(focus) {
        Some(PersonFocus::Project(item)) => Some(item.showcase.id),
        Some(PersonFocus::Card(_)) | None => None,
    };
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let body = Style::default().fg(theme::TEXT());

    let mut lines: Vec<Line<'static>> = Vec::new();

    // Header: who, how open, what kind, where; how fresh at the right.
    let mut header: Vec<Span<'static>> = vec![Span::styled(
        format!("@{}", entry.username),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )];
    if let Some(item) = entry.work {
        let p = &item.profile;
        header.push(Span::styled(" · ", faint));
        header.push(Span::styled(
            format!("{} {}", status_glyph(p.status), p.status.label()),
            Style::default().fg(status_color(p.status)),
        ));
        header.push(Span::styled(format!(" · {}", p.work_type.label()), dim));
        if !p.location.trim().is_empty() {
            header.push(Span::styled(format!(" · {}", p.location.trim()), dim));
        }
    } else {
        header.push(Span::styled(
            format!("  {}", project_count_label(entry.projects.len())),
            dim,
        ));
    }
    let age = vec![Span::styled(
        format!("active {}", format_relative_time(entry.latest_activity())),
        faint,
    )];
    lines.push(row_with_hint(header, age, width));
    lines.push(Line::from(""));

    if let Some(item) = entry.work {
        let p = &item.profile;
        lines.push(section_heading("card", card_focused));
        lines.push(Line::from(Span::styled(
            p.headline.clone(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )));
        for paragraph in p
            .summary
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            lines.push(Line::from(Span::styled(paragraph.to_string(), body)));
        }
        if !p.skills.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("skills   ", faint),
                Span::styled(
                    p.skills.join(" · "),
                    Style::default().fg(theme::AMBER_DIM()),
                ),
            ]));
        }
        for (idx, link) in p.links.iter().enumerate() {
            let label = if idx == 0 { "links    " } else { "         " };
            lines.push(Line::from(vec![
                Span::styled(label, faint),
                Span::styled(display_link(link), Style::default().fg(theme::AMBER_DIM())),
            ]));
        }
        if !p.contact.trim().is_empty() {
            lines.push(Line::from(vec![
                Span::styled("contact  ", faint),
                Span::styled(p.contact.trim().to_string(), body),
            ]));
        }
        lines.push(Line::from(vec![
            Span::styled("page     ", faint),
            Span::styled(
                display_link(&super::super::chat::work::state::profile_url(
                    view.profile_base_url,
                    &p.slug,
                )),
                faint,
            ),
        ]));
        lines.push(Line::from(""));
    }

    if let Some(author_profile) = entry.author_profile()
        && !author_profile.bio.trim().is_empty()
    {
        lines.push(section_heading("about", false));
        lines.extend(render_body_to_lines(
            &author_profile.bio,
            width,
            Span::raw(""),
            body,
        ));
        lines.push(Line::from(""));
    }

    if !entry.projects.is_empty() {
        lines.push(section_heading("projects", false));
        for item in &entry.projects {
            let focused = focused_project == Some(item.showcase.id);
            lines.extend(project_lines(item, focused));
        }
        lines.push(Line::from(""));
    }

    // FOR YOU: the viewer's own open or casual card gets its matches off
    // the shelf; anyone else's card, and a not-looking one, does not.
    if entry.user_id == view.current_user_id
        && let Some(item) = entry.work
        && wants_matches(item.profile.status)
        && view.jobs.enabled()
        && view.jobs.loaded
    {
        let found = matches(&view.jobs.items, own_tags, FOR_YOU_LIMIT);
        lines.push(section_heading("for you", false));
        lines.extend(for_you_lines(&found, width));
        lines.push(Line::from(""));
    }

    if let Some(author_profile) = entry.author_profile() {
        lines.extend(late_fetch_lines(author_profile, width));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// One project: `▸ title  age`, tags, and the description (whole when
/// focused, first line otherwise).
fn project_lines(item: &ShowcaseFeedItem, focused: bool) -> Vec<Line<'static>> {
    let s = &item.showcase;
    let marker = if focused { "▸ " } else { "  " };
    let marker_style = if focused {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(marker.to_string(), marker_style),
        Span::styled(
            s.title.clone(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", display_link(&s.url)),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled(
            format!("  {}", format_relative_time(s.created)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])];
    let paragraphs: Vec<&str> = s
        .description
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let shown = if focused { paragraphs.len() } else { 1 };
    for paragraph in paragraphs.iter().take(shown) {
        lines.push(Line::from(Span::styled(
            format!("  {paragraph}"),
            Style::default().fg(if focused {
                theme::TEXT()
            } else {
                theme::TEXT_DIM()
            }),
        )));
    }
    if !s.tags.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", s.tags.join(" · ")),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    }
    lines
}

fn section_heading(label: &'static str, focused: bool) -> Line<'static> {
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
            label.to_uppercase(),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// late.fetch as two dim lines at the foot of the pane: context, not the
/// point of the page.
fn late_fetch_lines(
    profile: &late_core::models::profile::Profile,
    width: usize,
) -> Vec<Line<'static>> {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut parts: Vec<String> = Vec::new();
    if let Some(country) = profile.country.as_deref().filter(|v| !v.trim().is_empty()) {
        parts.push(country.trim().to_string());
    }
    if !profile.langs.is_empty() {
        parts.push(profile.langs.join(" · "));
    }
    for (label, value) in [
        ("ide", profile.ide.as_deref()),
        ("os", profile.os.as_deref()),
        ("terminal", profile.terminal.as_deref()),
    ] {
        if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
            parts.push(format!("{label} {}", value.trim()));
        }
    }
    if let Some(created) = profile.created_at {
        parts.push(format!("since {}", created.format("%Y-%m")));
    }
    if parts.is_empty() {
        return Vec::new();
    }
    let theme_label = theme::label_for_id(profile.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID));
    parts.push(format!("theme {theme_label}"));
    vec![
        Line::from(Span::styled("late.fetch", dim)),
        Line::from(Span::styled(
            truncate_to_width(&parts.join("  ·  "), width),
            faint,
        )),
    ]
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

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
