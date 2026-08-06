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
        work::{self, svc::WorkFeedItem},
    },
    common::{
        composer,
        markdown::render_body_to_lines,
        primitives::{format_relative_time, hint_line},
        theme,
    },
    directory::state::{DirectoryEntry, DirectoryFilter, DirectoryState, merged_entries},
};

const IDLE_HINTS: &[(&str, &str)] = &[
    ("Enter", "copy link"),
    ("o", "profile"),
    ("i", "new project"),
    ("w", "work card"),
    ("e", "edit"),
    ("d", "delete"),
    ("/", "mine"),
    ("s", "search"),
];

/// Uniform merged-feed row height: four content lines plus the bottom border.
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
        Constraint::Length(1),
        Constraint::Length(search_height),
        Constraint::Fill(1),
        Constraint::Length(footer_height),
    ])
    .split(area);

    draw_filter_strip(frame, layout[0], &view);
    if view.directory.search_mode() {
        draw_search_box(frame, layout[1], view.directory.search_query());
    }

    let entries = merged_entries(
        view.showcase_state.all_items(),
        view.work_state.all_items(),
        view.directory.filter,
        view.directory.mine_only,
        view.current_user_id,
        view.directory.active_query(),
    );
    let selected = view
        .directory
        .selected()
        .min(entries.len().saturating_sub(1));

    let body = layout[2];
    if body.width >= DETAIL_MIN_WIDTH {
        let cols =
            Layout::horizontal([Constraint::Percentage(44), Constraint::Fill(1)]).split(body);
        draw_entry_list(frame, cols[0], &view, &entries, selected);
        draw_detail(frame, cols[1], &view, entries.get(selected).copied());
    } else {
        draw_entry_list(frame, body, &view, &entries, selected);
    }

    if work_composing {
        work::ui::draw_work_composer(
            frame,
            layout[3],
            &work::ui::WorkComposerView {
                state: view.work_state,
            },
        );
    } else if showcase_composing {
        showcase::ui::draw_showcase_composer(
            frame,
            layout[3],
            &showcase::ui::ShowcaseComposerView {
                state: view.showcase_state,
            },
        );
    } else {
        frame.render_widget(Paragraph::new(hint_line(IDLE_HINTS)), layout[3]);
    }
}

/// Header row: the three filter chips (with per-kind unread counts), the
/// mine-only indicator, and the switch hint pinned right.
fn draw_filter_strip(frame: &mut Frame, area: Rect, view: &DirectoryPageView<'_>) {
    let chips = [
        (DirectoryFilter::All, 0),
        (DirectoryFilter::Projects, view.showcase_state.unread_count()),
        (DirectoryFilter::People, view.work_state.unread_count()),
    ];

    let mut chip_spans = Vec::new();
    chip_spans.push(Span::raw(" "));
    for (idx, (filter, unread)) in chips.iter().enumerate() {
        if idx > 0 {
            chip_spans.push(Span::raw("  "));
        }
        let active = *filter == view.directory.filter;
        let style = if active {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        let suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        chip_spans.push(Span::styled(format!(" {}{suffix} ", filter.label()), style));
    }

    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let faint = Style::default().fg(theme::TEXT_FAINT());

    let mut right_spans = Vec::new();
    if view.directory.mine_only {
        right_spans.push(Span::styled(
            "mine only",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::raw("   "));
    }
    right_spans.push(Span::styled("[", key_style));
    right_spans.push(Span::styled(" ", faint));
    right_spans.push(Span::styled("]", key_style));
    right_spans.push(Span::styled(" ", faint));
    right_spans.push(Span::styled("h/l", key_style));
    right_spans.push(Span::styled(" filter ", faint));

    let right_w: u16 = right_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(chip_spans)), left);
    frame.render_widget(Paragraph::new(Line::from(right_spans)), right);
}

fn draw_search_box(frame: &mut Frame, area: Rect, query: &str) {
    if area.height == 0 {
        return;
    }
    let block = Block::default()
        .title(" Search ")
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

fn draw_entry_list(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    entries: &[DirectoryEntry<'_>],
    selected: usize,
) {
    if entries.is_empty() {
        let message = match view.directory.filter {
            DirectoryFilter::All => "Nothing here yet.",
            DirectoryFilter::Projects => "No projects yet.",
            DirectoryFilter::People => "No work cards yet.",
        };
        let lines = vec![
            Line::from(Span::styled(
                message,
                Style::default().fg(theme::TEXT_DIM()),
            )),
            Line::from(Span::styled(
                "Press 'i' to share a project, 'w' to post your work card.",
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
        let entry = entries[entry_idx];
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

        let lines = match entry {
            DirectoryEntry::Project(item) => project_row_lines(
                item,
                view.current_user_id,
                view.showcase_state.marker_read_at(),
                content.width as usize,
            ),
            DirectoryEntry::Person(item) => person_row_lines(
                item,
                view.current_user_id,
                view.work_state.marker_read_at(),
                content.width as usize,
            ),
        };
        frame.render_widget(Paragraph::new(lines), content);
    }
}

/// Four-line project row: title, author + time, description snippet, tags.
fn project_row_lines(
    item: &ShowcaseFeedItem,
    viewer: uuid::Uuid,
    marker_read_at: Option<chrono::DateTime<chrono::Utc>>,
    width: usize,
) -> Vec<Line<'static>> {
    let s = &item.showcase;
    let is_unread = marker_read_at
        .map(|last_read_at| s.created > last_read_at)
        .unwrap_or(true);
    let owner = s.user_id == viewer;

    let mut lines = Vec::with_capacity(4);
    lines.push(title_line(&s.title, owner, is_unread, width));
    lines.push(Line::from(vec![
        Span::styled(
            format!("@{}", item.author_username),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_to_width(
                &format!(" · {}", format_relative_time(s.created)),
                width.saturating_sub(item.author_username.chars().count() + 1),
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(snippet_line(&s.description, width));
    if s.tags.is_empty() {
        lines.push(Line::from(Span::styled(
            truncate_to_width(&display_link(&s.url), width),
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
    } else {
        let tags_text = s
            .tags
            .iter()
            .map(|t| format!("#{t}"))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(Line::from(Span::styled(
            truncate_to_width(&tags_text, width),
            Style::default().fg(theme::AMBER_DIM()),
        )));
    }
    lines
}

/// Four-line person row: headline, author + colored status meta, summary
/// snippet, skills.
fn person_row_lines(
    item: &WorkFeedItem,
    viewer: uuid::Uuid,
    marker_read_at: Option<chrono::DateTime<chrono::Utc>>,
    width: usize,
) -> Vec<Line<'static>> {
    let p = &item.profile;
    let is_unread = marker_read_at
        .map(|last_read_at| p.updated > last_read_at)
        .unwrap_or(true);
    let owner = p.user_id == viewer;

    let mut lines = Vec::with_capacity(4);
    lines.push(title_line(&p.headline, owner, is_unread, width));

    let prefix = format!("@{}", item.author_username);
    let status = work::state::status_label(&p.status);
    let trailing = format!(
        "  {} · {} · {}",
        p.work_type,
        p.location,
        format_relative_time(p.updated)
    );
    let status_budget = width.saturating_sub(prefix.chars().count() + 2);
    lines.push(Line::from(vec![
        Span::styled(
            prefix,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            truncate_to_width(status, status_budget),
            Style::default()
                .fg(status_color(&p.status))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            truncate_to_width(
                &trailing,
                status_budget.saturating_sub(status.chars().count()),
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(snippet_line(&p.summary, width));
    let skills_text = p
        .skills
        .iter()
        .map(|s| format!("#{s}"))
        .collect::<Vec<_>>()
        .join(" ");
    lines.push(Line::from(Span::styled(
        truncate_to_width(&skills_text, width),
        Style::default().fg(theme::AMBER_DIM()),
    )));
    lines
}

fn status_color(status: &str) -> Color {
    match status {
        "open" => theme::SUCCESS(),
        "casual" => theme::AMBER(),
        _ => theme::TEXT_DIM(),
    }
}

fn title_line(title: &str, owner: bool, is_unread: bool, width: usize) -> Line<'static> {
    let unread_prefix = if is_unread { "● " } else { "" };
    let unread_w = UnicodeWidthStr::width(unread_prefix);
    let badge = if owner { "(yours)" } else { "" };
    let badge_w = UnicodeWidthStr::width(badge);
    let title_budget = if owner {
        width
            .saturating_sub(unread_w)
            .saturating_sub(badge_w + 1)
            .max(4)
    } else {
        width.saturating_sub(unread_w).max(4)
    };
    let truncated = truncate_to_width(title, title_budget);
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

/// First non-empty line of a body, wrapped once and inline-ellipsized.
fn snippet_line(body: &str, width: usize) -> Line<'static> {
    let first = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    let rows = composer::build_composer_rows(first, width.max(1));
    let mut text = rows
        .into_iter()
        .next()
        .map(|row| row.text)
        .unwrap_or_default();
    if UnicodeWidthStr::width(text.as_str()) < UnicodeWidthStr::width(first) {
        text = truncate_to_width(&text, width.max(1));
    }
    Line::from(Span::styled(text, Style::default().fg(theme::TEXT())))
}

fn draw_detail(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    entry: Option<DirectoryEntry<'_>>,
) {
    let block = Block::default()
        .title(" Detail ")
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match entry {
        Some(DirectoryEntry::Project(item)) => draw_project_detail(frame, inner, view, item),
        Some(DirectoryEntry::Person(item)) => draw_person_detail(frame, inner, view, item),
        None => {
            frame.render_widget(
                Paragraph::new("Nothing selected.").style(Style::default().fg(theme::TEXT_DIM())),
                inner,
            );
        }
    }
}

/// Project detail: the full showcase, then an author card assembled from the
/// author's work profile (if any), settings bio, late.fetch, and their other
/// projects.
fn draw_project_detail(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    item: &ShowcaseFeedItem,
) {
    let s = &item.showcase;
    let detail_width = area.width as usize;
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled(
        s.title.clone(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled(
            format!("@{}", item.author_username),
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(
            format!("  shared {}", format_relative_time(s.created)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("↗ ", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(
            display_link(&s.url),
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    if !s.description.trim().is_empty() {
        for paragraph in s.description.lines().filter(|line| !line.trim().is_empty()) {
            lines.push(Line::from(Span::styled(
                paragraph.trim().to_string(),
                Style::default().fg(theme::TEXT()),
            )));
        }
        lines.push(Line::from(""));
    }

    if !s.tags.is_empty() {
        lines.push(Line::from(Span::styled(
            s.tags
                .iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join(" "),
            Style::default().fg(theme::AMBER_DIM()),
        )));
        lines.push(Line::from(""));
    }

    // Author card.
    lines.push(section_header("author"));
    let work_profile = view
        .work_state
        .all_items()
        .iter()
        .find(|candidate| candidate.profile.user_id == s.user_id);
    if let Some(work_item) = work_profile {
        let p = &work_item.profile;
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
        ]));
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
        lines.push(Line::from(Span::styled(
            work::state::profile_url(view.profile_base_url, &p.slug),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    if let Some(author_profile) = item.author_profile.as_ref() {
        if !author_profile.bio.trim().is_empty() {
            lines.extend(render_body_to_lines(
                &author_profile.bio,
                detail_width,
                Span::raw(""),
                Style::default().fg(theme::TEXT()),
            ));
        }
        lines.push(Line::from(""));
        lines.extend(late_fetch_lines(author_profile, detail_width));
    }
    lines.push(Line::from(""));

    let other_projects = view
        .showcase_state
        .all_items()
        .iter()
        .filter(|candidate| {
            candidate.showcase.user_id == s.user_id && candidate.showcase.id != s.id
        })
        .collect::<Vec<_>>();
    if !other_projects.is_empty() {
        lines.push(section_header("more by this author"));
        for project in other_projects.into_iter().take(4) {
            lines.push(Line::from(vec![
                Span::styled("-> ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    project.showcase.title.clone(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    format!("  {}", display_link(&project.showcase.url)),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ]));
        }
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

/// Person detail: the full work profile plus the author's settings bio,
/// late.fetch, and showcases (the same sections the public web profile has).
fn draw_person_detail(
    frame: &mut Frame,
    area: Rect,
    view: &DirectoryPageView<'_>,
    item: &WorkFeedItem,
) {
    let profile = &item.profile;
    let author_projects = view
        .showcase_state
        .all_items()
        .iter()
        .filter(|project| project.showcase.user_id == profile.user_id)
        .collect::<Vec<_>>();
    let author_profile = item.author_profile.as_ref();
    let detail_width = area.width as usize;

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        profile.headline.clone(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled(
            format!("@{}", item.author_username),
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            work::state::status_label(&profile.status).to_string(),
            Style::default()
                .fg(status_color(&profile.status))
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}  {}", profile.work_type, profile.location),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!("updated {}", format_relative_time(profile.updated)),
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::from(""));

    if !profile.summary.trim().is_empty() {
        lines.push(section_header("summary"));
        for paragraph in profile
            .summary
            .lines()
            .filter(|line| !line.trim().is_empty())
        {
            lines.push(Line::from(Span::styled(
                paragraph.trim().to_string(),
                Style::default().fg(theme::TEXT()),
            )));
        }
        lines.push(Line::from(""));
    }

    if !profile.skills.is_empty() {
        lines.push(section_header("skills"));
        lines.push(Line::from(Span::styled(
            profile
                .skills
                .iter()
                .map(|skill| format!("#{skill}"))
                .collect::<Vec<_>>()
                .join(" "),
            Style::default().fg(theme::AMBER_DIM()),
        )));
        lines.push(Line::from(""));
    }

    if !profile.contact.trim().is_empty() {
        lines.push(section_header("contact"));
        lines.push(Line::from(Span::styled(
            profile.contact.trim().to_string(),
            Style::default().fg(theme::TEXT()),
        )));
        lines.push(Line::from(""));
    }

    if !profile.links.is_empty() {
        lines.push(section_header("links"));
        for link in &profile.links {
            lines.push(Line::from(Span::styled(
                format!("-> {link}"),
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
        lines.push(Line::from(""));
    }

    if let Some(author_profile) = author_profile {
        if !author_profile.bio.trim().is_empty() {
            lines.push(section_header("bio"));
            lines.extend(render_body_to_lines(
                &author_profile.bio,
                detail_width,
                Span::raw(""),
                Style::default().fg(theme::TEXT()),
            ));
            lines.push(Line::from(""));
        }

        lines.push(section_header("late.fetch"));
        lines.extend(late_fetch_lines(author_profile, detail_width));
        lines.push(Line::from(""));
    }

    if !author_projects.is_empty() {
        lines.push(section_header("projects"));
        for project in author_projects.into_iter().take(5) {
            let showcase = &project.showcase;
            lines.push(Line::from(vec![
                Span::styled("-> ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    showcase.title.clone(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    format!("  {}", display_link(&showcase.url)),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        work::state::profile_url(view.profile_base_url, &profile.slug),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn section_header(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        format!("# {label}"),
        Style::default()
            .fg(theme::TEXT_DIM())
            .add_modifier(Modifier::BOLD),
    ))
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
