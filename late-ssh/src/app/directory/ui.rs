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
        primitives::{format_relative_time, hint_line},
        theme,
    },
    directory::state::DirectoryTab,
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
    draw_tab_strip(
        frame,
        layout[0],
        view.tab,
        view.work_state.unread_count(),
        view.showcase_state.unread_count(),
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

    // Switch hint pinned to the right edge so the tab row keeps its air even on
    // narrow terminals. `[` and `]` are the bindings, coloured like keys.
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let switch_spans = vec![
        Span::styled("[", key_style),
        Span::styled(" ", faint),
        Span::styled("]", key_style),
        Span::styled(" switch ", faint),
    ];
    let switch_w: u16 = switch_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(switch_w)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), left);
    frame.render_widget(Paragraph::new(Line::from(switch_spans)), right);
}

fn draw_tab_footer(frame: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    frame.render_widget(Paragraph::new(hint_line(hints)), area);
}

fn draw_profiles_tab(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let composing = view.work_state.composing();
    // Idle tabs end in a single-line hint footer (matching Projects and the
    // Pinstar browser); the bordered composer only appears while editing.
    let footer_height = if composing { 11 } else { 1 };
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_height)]).split(area);

    let body = layout[0];
    if body.width >= 86 {
        let cols =
            Layout::horizontal([Constraint::Percentage(44), Constraint::Fill(1)]).split(body);
        work::ui::draw_work_list(frame, cols[0], &view.profiles);
        draw_profile_detail(frame, cols[1], &view);
    } else {
        work::ui::draw_work_list(frame, body, &view.profiles);
    }

    if composing {
        work::ui::draw_work_composer(
            frame,
            layout[1],
            &work::ui::WorkComposerView {
                state: view.work_state,
            },
        );
    } else {
        draw_tab_footer(frame, layout[1], PROFILE_HINTS);
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

    if !profile.contact.trim().is_empty() {
        lines.push(section_header("contact"));
        lines.push(Line::from(Span::styled(
            profile.contact.trim().to_string(),
            Style::default().fg(theme::TEXT()),
        )));
        lines.push(Line::from(""));
    }

    if !author_projects.is_empty() {
        lines.push(section_header("projects"));
        for project in author_projects.into_iter().take(5) {
            lines.push(Line::from(vec![
                Span::styled("-> ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    project.showcase.title.clone(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
                Span::styled(
                    format!("  {}", project.showcase.url),
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ]));
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

fn draw_projects_tab(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let composing = view.showcase_state.composing();
    let footer_height = if composing { 10 } else { 1 };
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(footer_height)]).split(area);

    showcase::ui::draw_showcase_list(frame, layout[0], &view.projects);
    if composing {
        showcase::ui::draw_showcase_composer(
            frame,
            layout[1],
            &showcase::ui::ShowcaseComposerView {
                state: view.showcase_state,
            },
        );
    } else {
        draw_tab_footer(frame, layout[1], PROJECT_HINTS);
    }
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
