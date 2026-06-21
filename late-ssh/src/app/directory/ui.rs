use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{
    chat::{showcase, work},
    common::{
        markdown::render_body_to_lines,
        primitives::{format_relative_time, hint_line},
        theme,
    },
    directory::state::{
        DirectoryState, DirectoryTab, filtered_profile_indices, filtered_project_indices,
    },
};

const PROFILE_HINTS: &[(&str, &str)] = &[
    ("Enter", "copy link"),
    ("i", "edit mine"),
    ("e", "edit selected"),
    ("d", "delete"),
    ("/", "show mine"),
];

const PROJECT_HINTS: &[(&str, &str)] = &[
    ("Enter", "copy link"),
    ("i", "new"),
    ("e", "edit"),
    ("d", "delete"),
    ("/", "show mine"),
];

pub(crate) struct DirectoryPageView<'a> {
    pub(crate) directory: &'a DirectoryState,
    pub(crate) tab: DirectoryTab,
    pub(crate) profiles: work::ui::WorkListView<'a>,
    pub(crate) work_state: &'a work::state::State,
    pub(crate) projects: showcase::ui::ShowcaseListView<'a>,
    pub(crate) showcase_state: &'a showcase::state::State,
    pub(crate) pinstar_state: Option<&'a mut crate::app::pinstar::state::PinstarState>,
    pub(crate) pinstar_browser: Option<&'a crate::app::pinstar::browser::DiagramBrowser>,
}

pub(crate) fn draw_directory_page(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let layout = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(area);

    // The active "mine only" filter rides in the tab strip rather than eating a
    // row off the list. Only the current tab's filter is surfaced.
    let mine_only_label = match view.tab {
        DirectoryTab::Profiles if view.profiles.mine_only => Some("work"),
        DirectoryTab::Projects if view.projects.mine_only => Some("showcases"),
        _ => None,
    };

    draw_tab_strip(
        frame,
        layout[0],
        view.tab,
        view.work_state.unread_count(),
        view.showcase_state.unread_count(),
        mine_only_label,
    );

    match view.tab {
        DirectoryTab::Profiles => draw_profiles_tab(frame, layout[1], view),
        DirectoryTab::Projects => draw_projects_tab(frame, layout[1], view),
        DirectoryTab::Pinstar => draw_pinstar_tab(frame, layout[1], view),
    }
}

fn draw_tab_strip(
    frame: &mut Frame,
    area: Rect,
    current: DirectoryTab,
    profile_unread: i64,
    project_unread: i64,
    mine_only_label: Option<&str>,
) {
    let tabs = [
        (DirectoryTab::Profiles, "Profiles", profile_unread),
        (DirectoryTab::Projects, "Projects", project_unread),
        (DirectoryTab::Pinstar, "Pinstar", 0),
    ];

    let mut tab_spans = Vec::new();
    tab_spans.push(Span::raw(" "));
    for (idx, (tab, label, unread)) in tabs.iter().enumerate() {
        if idx > 0 {
            tab_spans.push(Span::raw("  "));
        }
        let active = *tab == current;
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
        tab_spans.push(Span::styled(format!(" {label}{suffix} "), style));
    }

    // Right cluster, pinned to the right edge so the tab row keeps its air on
    // narrow terminals: the active "mine only" filter (when set) sits just left
    // of the switch hint.
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let faint = Style::default().fg(theme::TEXT_FAINT());

    let mut right_spans = Vec::new();
    if let Some(label) = mine_only_label {
        right_spans.push(Span::styled(
            "mine only",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled(
            format!(" · showing your {label}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        right_spans.push(Span::raw("   "));
    }
    right_spans.push(Span::styled("[", key_style));
    right_spans.push(Span::styled(" ", faint));
    right_spans.push(Span::styled("]", key_style));
    right_spans.push(Span::styled(" ", faint));
    right_spans.push(Span::styled("h/l", key_style));
    right_spans.push(Span::styled(" switch ", faint));

    let right_w: u16 = right_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(right_w)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), left);
    frame.render_widget(Paragraph::new(Line::from(right_spans)), right);
}

fn draw_tab_footer(frame: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    frame.render_widget(Paragraph::new(hint_line(hints)), area);
}

fn draw_profiles_tab(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let composing = view.work_state.composing();
    // Idle tabs end in a single-line hint footer (matching Projects and the
    // Pinstar browser); the bordered composer only appears while editing.
    let footer_height = if composing { 11 } else { 1 };
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

    // The "mine only" banner is drawn in the tab strip, so the list keeps its
    // full height here.
    let list_view = work::ui::WorkListView {
        mine_only: false,
        ..view.profiles
    };
    let body = layout[1];
    let filtered_items = view.directory.search_mode().then(|| {
        filtered_profile_indices(list_view.items, view.directory.search_query())
            .into_iter()
            .map(|(_, item)| item.clone())
            .collect::<Vec<_>>()
    });
    if body.width >= 86 {
        let cols =
            Layout::horizontal([Constraint::Percentage(44), Constraint::Fill(1)]).split(body);
        if let Some(items) = &filtered_items {
            let search_view = work::ui::WorkListView {
                items,
                selected_index: view.directory.search_selected(),
                ..list_view
            };
            work::ui::draw_work_list(frame, cols[0], &search_view);
        } else {
            work::ui::draw_work_list(frame, cols[0], &list_view);
        }
        draw_profile_detail(frame, cols[1], &view);
    } else if let Some(items) = &filtered_items {
        let search_view = work::ui::WorkListView {
            items,
            selected_index: view.directory.search_selected(),
            ..list_view
        };
        work::ui::draw_work_list(frame, body, &search_view);
    } else {
        work::ui::draw_work_list(frame, body, &list_view);
    }

    if composing {
        work::ui::draw_work_composer(
            frame,
            layout[2],
            &work::ui::WorkComposerView {
                state: view.work_state,
            },
        );
    } else {
        draw_tab_footer(frame, layout[2], PROFILE_HINTS);
    }
}

fn draw_profile_detail(frame: &mut Frame, area: Rect, view: &DirectoryPageView<'_>) {
    let block = Block::default()
        .title(" Profile ")
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(item) = view.work_state.selected_item() else {
        frame.render_widget(
            Paragraph::new("No profile selected.").style(Style::default().fg(theme::TEXT_DIM())),
            inner,
        );
        return;
    };

    let profile = &item.profile;
    let author_projects = view
        .showcase_state
        .all_items()
        .iter()
        .filter(|project| project.showcase.user_id == profile.user_id)
        .collect::<Vec<_>>();
    let author_profile = item.author_profile.as_ref();
    let detail_width = inner.width as usize;

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
            work::state::status_label(&profile.status),
            Style::default()
                .fg(theme::SUCCESS())
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
        lines.push(section_header("showcases"));
        for project in author_projects.into_iter().take(5) {
            let showcase = &project.showcase;
            lines.push(Line::from(vec![
                Span::styled("-> ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    showcase.title.clone(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    format!("  {}", showcase.url),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ]));
            if !showcase.description.trim().is_empty() {
                for paragraph in showcase
                    .description
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .take(2)
                {
                    lines.push(Line::from(Span::styled(
                        format!("   {}", paragraph.trim()),
                        Style::default().fg(theme::TEXT_DIM()),
                    )));
                }
            }
            if !showcase.tags.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "   {}",
                        showcase
                            .tags
                            .iter()
                            .map(|tag| format!("#{tag}"))
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                    Style::default().fg(theme::AMBER_DIM()),
                )));
            }
        }
        lines.push(Line::from(""));
    }

    let base_url = view.profiles.profile_base_url;
    lines.push(Line::from(Span::styled(
        work::state::profile_url(base_url, &profile.slug),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )));

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
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

fn draw_projects_tab(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let composing = view.showcase_state.composing();
    let footer_height = if composing { 10 } else { 1 };
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

    let list_view = showcase::ui::ShowcaseListView {
        mine_only: false,
        ..view.projects
    };
    if view.directory.search_mode() {
        let filtered_items =
            filtered_project_indices(list_view.items, view.directory.search_query())
                .into_iter()
                .map(|(_, item)| item.clone())
                .collect::<Vec<_>>();
        let search_view = showcase::ui::ShowcaseListView {
            items: &filtered_items,
            selected_index: view.directory.search_selected(),
            ..list_view
        };
        showcase::ui::draw_showcase_list(frame, layout[1], &search_view);
    } else {
        showcase::ui::draw_showcase_list(frame, layout[1], &list_view);
    }
    if composing {
        showcase::ui::draw_showcase_composer(
            frame,
            layout[2],
            &showcase::ui::ShowcaseComposerView {
                state: view.showcase_state,
            },
        );
    } else {
        draw_tab_footer(frame, layout[2], PROJECT_HINTS);
    }
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
        Paragraph::new(Line::from(vec![
            Span::styled("s ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(query.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
        ])),
        inner,
    );
}

fn draw_pinstar_tab(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    if let Some(state) = view.pinstar_state {
        let theme = crate::app::pinstar::helpers::PinstarTheme::default();
        crate::app::pinstar::ui::draw_pinstar_view(frame, area, state, &theme);
    } else if let Some(browser) = view.pinstar_browser {
        crate::app::pinstar::ui::draw_diagram_browser(frame, area, browser);
    } else {
        let placeholder = Paragraph::new(Line::from(Span::styled(
            "Pinstar diagrams unavailable.",
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .centered();
        frame.render_widget(placeholder, area);
    }
}
