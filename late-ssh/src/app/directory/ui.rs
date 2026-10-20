use super::{
    input::FocusedItem,
    mouse::{Pane, Target},
};
use crate::app::common::mouse_ui::{self, Surface};
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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
        ui::{JobsShelfView, draw_jobs_shelf, for_you_lines, shelf_count},
    },
    usermap::state::{MapMode, UserMapState},
};

const PEOPLE_HINTS: &[(&str, &str)] = &[
    ("j/k", "people"),
    ("h/l", "focus"),
    ("Enter", "copy"),
    ("e", "edit"),
    ("d", "delete"),
    ("s", "search"),
    ("/", "mine"),
];
const PEOPLE_NARROW_HINTS: &[(&str, &str)] = &[
    ("j/k", "people"),
    ("l", "open"),
    ("s", "search"),
    ("/", "mine"),
];
/// Page-wide actions, on the right of every section's footer. Clickable,
/// so the mouse reaches the editor from any section.
const PAGE_ACTIONS: &[(&str, &str, u8)] =
    &[("w", "your profile", b'w'), ("i", "add project", b'i')];
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
    /// The map shelf. Mutable because its draw records where the map and
    /// the country list landed, which is what its mouse reads.
    pub(crate) usermap: &'a mut UserMapState,
    /// The viewer's languages from their profile, part of their match tags.
    pub(crate) viewer_langs: &'a [String],
    /// Whether the viewer set a country, so the map can say when they are
    /// not on it.
    pub(crate) viewer_has_country: bool,
    pub(crate) projects: &'a [ShowcaseFeedItem],
    pub(crate) people: &'a [WorkFeedItem],
    pub(crate) work_marker: Option<DateTime<Utc>>,
    pub(crate) showcase_marker: Option<DateTime<Utc>>,
    pub(crate) current_user_id: uuid::Uuid,
    pub(crate) can_moderate: bool,
    pub(crate) can_retract_jobs: bool,
    pub(crate) profile_base_url: &'a str,
}

pub(crate) fn draw_directory_page(frame: &mut Frame, area: Rect, view: DirectoryPageView<'_>) {
    let size = frame.area();
    view.directory.mouse.begin((size.width, size.height));
    view.jobs.mouse.begin((size.width, size.height));
    // The page draws on the buffer; the map paints on the frame, so it goes
    // last, into the body the page kept for it.
    let map_body = draw_page(
        &mut Surface {
            buffer: frame.buffer_mut(),
        },
        area,
        &view,
    );
    if let Some(body) = map_body {
        crate::app::usermap::ui::draw(frame, body, view.usermap, view.viewer_has_country);
    }
}

/// Draw the page but the map; returns the body when the map shelf is up, for
/// the caller to paint the map into.
fn draw_page(frame: &mut Surface<'_>, area: Rect, view: &DirectoryPageView<'_>) -> Option<Rect> {
    let narrow = area.width < STACK_BELOW_WIDTH;
    view.directory.set_narrow(narrow);
    view.jobs.set_narrow(narrow);
    let short = area.height < SHORT_BELOW_HEIGHT;
    let own_tags = own_viewer_tags(view);

    let jobs_search = view.directory.shelf() == Shelf::Jobs && view.jobs.search_mode();
    let search_height = if view.directory.search_mode() || jobs_search {
        3
    } else {
        0
    };
    let [section_row, search, body, footer] = Layout::vertical([
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
        view.directory.online(),
    );
    let selected = view
        .directory
        .selected()
        .min(entries.len().saturating_sub(1));

    draw_section_row(frame, section_row, view, entries.len(), &own_tags);
    if view.directory.search_mode() {
        draw_search_box(
            frame,
            search,
            " Search people and projects ",
            view.directory.search_query(),
        );
    } else if jobs_search {
        draw_search_box(frame, search, " Search jobs ", view.jobs.search_query());
    }

    let mut map_body = None;
    let hints = match view.directory.shelf() {
        Shelf::Map => {
            map_body = Some(body);
            crate::app::usermap::ui::hints(view.usermap)
        }
        Shelf::Jobs => {
            let shelf = JobsShelfView {
                jobs: view.jobs,
                viewer_tags: &own_tags,
                narrow,
                short,
                can_moderate: view.can_retract_jobs,
                current_user_id: view.current_user_id,
            };
            draw_jobs_shelf(frame, body, &shelf);
            crate::app::jobs::ui::hints(&shelf)
        }
        Shelf::Profiles => {
            if !narrow {
                let cols = Layout::horizontal([Constraint::Percentage(42), Constraint::Fill(1)])
                    .split(body);
                draw_people_list(frame, cols[0], view, &entries, selected, short);
                draw_person_detail(frame, cols[1], view, entries.get(selected), &own_tags);
                PEOPLE_HINTS
            } else if view.directory.detail_open() {
                draw_person_detail(frame, body, view, entries.get(selected), &own_tags);
                DETAIL_NARROW_HINTS
            } else {
                draw_people_list(frame, body, view, &entries, selected, short);
                PEOPLE_NARROW_HINTS
            }
        }
    };
    draw_footer(frame, footer, view, hints);
    view.directory.mouse.finish();
    view.jobs.mouse.finish();
    map_body
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

/// One clickable label on the section row's right: a filter of the
/// current section. The active one is lit and has nothing to do on a click.
struct Filter {
    label: &'static str,
    active: bool,
    target: Target,
}

/// The section's filters, in the order the row prints them.
fn section_filters(view: &DirectoryPageView<'_>) -> Vec<Filter> {
    match view.directory.shelf() {
        Shelf::Map => MapMode::ALL
            .into_iter()
            .map(|mode| Filter {
                label: mode.tab_label(),
                active: mode == view.usermap.mode(),
                target: Target::MapMode(mode),
            })
            .collect(),
        Shelf::Profiles => vec![
            Filter {
                label: "everyone",
                active: !view.directory.mine_only,
                target: Target::Key(b'/'),
            },
            Filter {
                label: "mine",
                active: view.directory.mine_only,
                target: Target::Key(b'/'),
            },
            Filter {
                label: "search",
                active: view.directory.search_mode(),
                target: Target::Key(b's'),
            },
        ],
        Shelf::Jobs => vec![
            Filter {
                label: "all",
                active: !view.jobs.for_me,
                target: Target::Key(b'/'),
            },
            Filter {
                label: "for me",
                active: view.jobs.for_me,
                target: Target::Key(b'/'),
            },
            Filter {
                label: "search",
                active: view.jobs.search_mode(),
                target: Target::Key(b's'),
            },
        ],
    }
}

/// ` Map 1   Profiles 12   Jobs 31 ` on the left, the active section lit
/// like the page tabs in the title bar; the section's filters on the right.
/// Every label is a click target. The filters drop when the row is too
/// narrow for both; the keys still reach them.
fn draw_section_row(
    frame: &mut Surface<'_>,
    area: Rect,
    view: &DirectoryPageView<'_>,
    people: usize,
    own_tags: &[String],
) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut x = area.x;
    for shelf in Shelf::ALL {
        let active = shelf == view.directory.shelf();
        let count = match shelf {
            Shelf::Map => Some(view.directory.online().len()),
            Shelf::Profiles => Some(people),
            Shelf::Jobs => shelf_count(view.jobs, own_tags),
        };
        let count = match count {
            Some(count) => format!(" {count}"),
            None => String::new(),
        };
        let (name_style, count_style) = if active {
            let lit = Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD);
            (lit, lit)
        } else {
            (
                Style::default().fg(theme::TEXT_DIM()),
                Style::default().fg(theme::TEXT_FAINT()),
            )
        };
        let width = (shelf.title().chars().count() + count.chars().count() + 2) as u16;
        spans.push(Span::styled(format!(" {}", shelf.title()), name_style));
        spans.push(Span::styled(format!("{count} "), count_style));
        spans.push(Span::raw(" "));
        if x < area.right() {
            view.directory.mouse.hit(
                Rect::new(x, area.y, width.min(area.right() - x), area.height.min(1)),
                Target::Shelf(shelf),
            );
        }
        x = x.saturating_add(width + 1);
    }

    let filters = section_filters(view);
    let mut right: Vec<Span<'static>> = Vec::new();
    for (idx, filter) in filters.iter().enumerate() {
        if idx > 0 {
            right.push(Span::styled(
                " · ",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        let style = if filter.active {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        right.push(Span::styled(filter.label, style));
    }
    right.push(Span::raw(" "));
    let left_len = spans.len();
    let line = row_with_hint(spans, right, area.width as usize);
    let fitted = line.spans.len() > left_len;
    frame.render_widget(Paragraph::new(line), area);

    // Only when the filters made it onto the row: they end at its right edge.
    if fitted {
        let widths: Vec<u16> = filters
            .iter()
            .map(|filter| filter.label.chars().count() as u16)
            .collect();
        let total = widths.iter().sum::<u16>() + 3 * (widths.len() as u16 - 1) + 1;
        let mut x = area.right() - total;
        for (filter, width) in filters.into_iter().zip(widths) {
            if !filter.active {
                view.directory
                    .mouse
                    .hit(Rect::new(x, area.y, width, 1), filter.target);
            }
            x += width + 3;
        }
    }
}

/// The section's keys on the left, the page-wide actions on the right (each
/// a click target). The actions drop when the row is too narrow for both.
fn draw_footer(
    frame: &mut Surface<'_>,
    area: Rect,
    view: &DirectoryPageView<'_>,
    hints: &[(&str, &str)],
) {
    let key_style = Style::default()
        .fg(theme::AMBER_DIM())
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme::TEXT_DIM());
    let mut right: Vec<Span<'static>> = Vec::new();
    for (idx, (key, desc, _)) in PAGE_ACTIONS.iter().enumerate() {
        if idx > 0 {
            right.push(Span::styled(
                " · ",
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        right.push(Span::styled(*key, key_style));
        right.push(Span::styled(format!(" {desc}"), desc_style));
    }
    right.push(Span::raw(" "));
    let left = hint_line(hints).spans;
    let left_len = left.len();
    let line = row_with_hint(left, right, area.width as usize);
    let fitted = line.spans.len() > left_len;
    frame.render_widget(Paragraph::new(line), area);

    if fitted {
        let widths: Vec<u16> = PAGE_ACTIONS
            .iter()
            .map(|(key, desc, _)| (key.chars().count() + 1 + desc.chars().count()) as u16)
            .collect();
        let total = widths.iter().sum::<u16>() + 3 * (widths.len() as u16 - 1) + 1;
        let mut x = area.right() - total;
        for ((_, _, byte), width) in PAGE_ACTIONS.iter().zip(widths) {
            view.directory
                .mouse
                .hit(Rect::new(x, area.y, width, 1), Target::Key(*byte));
            x += width + 3;
        }
    }
}

fn draw_search_box(frame: &mut Surface<'_>, area: Rect, title: &'static str, query: &str) {
    if area.is_empty() {
        return;
    }
    let block = Block::default()
        .title(title)
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
    frame: &mut Surface<'_>,
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

    let item_height: u16 = if short { 3 } else { 4 };
    // The connected lead the list under a rule of their own and the rest
    // follow under theirs; with nobody connected there is nothing to rule
    // off and the rows sit where they always did.
    let online = entries.iter().take_while(|entry| entry.online).count();
    let rules = match (online > 0, online < entries.len()) {
        (false, _) => 0,
        (true, false) => 1,
        (true, true) => 2,
    };
    let rules_above = |index: usize| match (online > 0, index < online) {
        (false, _) => 0,
        (true, true) => 1,
        (true, false) => 2,
    };
    let row_top = |index: usize| index * usize::from(item_height) + rules_above(index);
    // A row right under a rule brings the rule into view with it.
    let led_by_rule = online > 0 && (selected == 0 || selected == online);
    let focus = row_top(selected) - usize::from(led_by_rule);
    mouse_ui::scroll(
        frame,
        area,
        &view.directory.mouse,
        Pane::PeopleList,
        entries.len() * usize::from(item_height) + rules,
        focus..row_top(selected) + usize::from(item_height),
        |frame, content, window| {
            let offline_rule = (online * usize::from(item_height) + 1) as u16;
            let groups = [
                (0, "online", online, theme::SUCCESS()),
                (
                    offline_rule,
                    "offline",
                    entries.len() - online,
                    theme::TEXT(),
                ),
            ];
            for (y, label, count, color) in groups.into_iter().take(rules) {
                if y >= window.y && y < window.bottom() {
                    frame.render_widget(
                        Paragraph::new(group_rule(label, count, color, usize::from(content.width))),
                        Rect::new(0, y, content.width, 1),
                    );
                }
            }
            for (index, entry) in entries.iter().enumerate() {
                let y = row_top(index) as u16;
                if y >= window.bottom() {
                    break;
                }
                let row_area = Rect::new(0, y, content.width, item_height.min(content.height - y));
                if row_area.bottom() <= window.y {
                    continue;
                }
                let block = Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme::BORDER_DIM()))
                    .style(theme::row_style(index == selected));
                let content = block.inner(row_area);
                frame.render_widget(block, row_area);
                let person = person_row(
                    entry,
                    view.current_user_id,
                    view.work_marker,
                    view.showcase_marker,
                );
                frame.render_widget(
                    Paragraph::new(person_row_lines(
                        &person,
                        index == selected,
                        short,
                        content.width as usize,
                    )),
                    content,
                );
                view.directory
                    .mouse
                    .hit(content, Target::Person(entry.user_id));
            }
        },
    );
}

/// `── online ────────`: the rule over a group of the people list.
fn group_rule(label: &str, count: usize, color: Color, width: usize) -> Line<'static> {
    let rule = Style::default().fg(theme::BORDER());
    let lead = "\u{2500}\u{2500} ";
    let count = format!(" {count}");
    let tail = width
        .saturating_sub(lead.chars().count() + label.chars().count() + count.chars().count() + 1);
    Line::from(vec![
        Span::styled(lead, rule),
        Span::styled(
            label.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(count, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(format!(" {}", "\u{2500}".repeat(tail)), rule),
    ])
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
                width.saturating_sub(1 + Span::raw(second_prefix).width()),
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
        let right_width: usize = right.iter().map(|span| span.width()).sum();
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
/// late.fetch last. The `h`/`l` focus cursor paints a `▸`
/// on the focused section; Enter/e/d act on it.
fn draw_person_detail(
    frame: &mut Surface<'_>,
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

    let width = inner.width.saturating_sub(1) as usize;
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
    let mut targets = Vec::new();
    let mut focus_line = 0;

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
    targets.push((
        lines.len(),
        Target::Profile(entry.user_id, entry.username.to_string()),
    ));
    lines.push(row_with_hint(header, age, width));
    lines.push(Line::from(""));

    if let Some(item) = entry.work {
        let p = &item.profile;
        let item = FocusedItem::Card(p.id);
        targets.push((lines.len(), Target::Item(item, 0)));
        if card_focused {
            focus_line = lines.len();
        }
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
            targets.push((lines.len(), Target::Copy(link.clone())));
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
        targets.push((
            lines.len(),
            Target::Copy(super::super::chat::work::state::profile_url(
                view.profile_base_url,
                &p.slug,
            )),
        ));
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
            if focused {
                focus_line = lines.len();
            }
            targets.push((
                lines.len(),
                Target::Item(FocusedItem::Project(item.showcase.id), 0),
            ));
            let project = project_lines(item, focused);
            targets.push((lines.len() + 1, Target::Copy(item.showcase.url.clone())));
            lines.extend(project);
        }
        lines.push(Line::from(""));
    }

    // FOR YOU: the viewer's own open or casual card gets its matches off
    // the shelf; anyone else's card, and a not-looking one, does not.
    if entry.user_id == view.current_user_id
        && let Some(item) = entry.work
        && wants_matches(item.profile.status)
        && view.jobs.loaded
    {
        let found = matches(&view.jobs.items, own_tags, FOR_YOU_LIMIT);
        lines.push(section_heading("for you", false));
        for (index, job) in found.iter().enumerate() {
            targets.push((lines.len() + index, Target::Job(job.id)));
        }
        lines.extend(for_you_lines(&found, width));
        lines.push(Line::from(""));
    }

    if let Some(author_profile) = entry.author_profile() {
        lines.extend(late_fetch_lines(author_profile, width));
    }

    let selection = entry.focus_target(focus).map(|item| match item {
        PersonFocus::Card(item) => FocusedItem::Card(item.profile.id),
        PersonFocus::Project(item) => FocusedItem::Project(item.showcase.id),
    });
    let mut actions = vec![(
        "[Profile]",
        Target::Profile(entry.user_id, entry.username.to_string()),
    )];
    if view.directory.narrow() {
        actions.insert(0, ("[Back]", Target::Back));
    }
    if let Some(item) = selection {
        actions.push(("[Copy]", Target::Item(item, b'c')));
        if entry.user_id == view.current_user_id || view.can_moderate {
            actions.extend([
                ("[Edit]", Target::Item(item, b'e')),
                ("[Delete]", Target::Item(item, b'd')),
            ]);
        }
    }
    let [controls, viewport] = Layout::vertical([
        Constraint::Length(mouse_ui::buttons_height(inner.width, &actions)),
        Constraint::Fill(1),
    ])
    .areas(inner);
    mouse_ui::buttons(frame, controls, &view.directory.mouse, &actions);
    let lines: Vec<_> = lines
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            (
                line,
                targets
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, target)| target.clone()),
            )
        })
        .collect();
    let row_of = |index: usize| {
        lines
            .iter()
            .take(index)
            .map(|(line, _)| mouse_ui::line_height(line, viewport.width.saturating_sub(1)))
            .sum::<usize>()
    };
    let rows = row_of(lines.len());
    let focus = row_of(focus_line);
    mouse_ui::scroll(
        frame,
        viewport,
        &view.directory.mouse,
        Pane::PeopleDetail,
        rows,
        focus..focus + 1,
        |frame, content, window| {
            mouse_ui::lines(frame, content, window, &view.directory.mouse, &lines)
        },
    );
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
            format!("  {}", format_relative_time(s.created)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])];
    lines.push(Line::from(Span::styled(
        format!("  {}", display_link(&s.url)),
        Style::default().fg(theme::AMBER_DIM()),
    )));
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

/// Below this pane width late.fetch stacks into one column.
const LATE_FETCH_TWO_COLUMN_MIN: usize = 56;
/// `terminal ` is the longest label; values start after it.
const LATE_FETCH_LABEL_WIDTH: usize = 9;

/// late.fetch as its own section at the foot of the pane: who they are on
/// the left (country, langs, since, theme), their setup on the right (ide,
/// terminal, os). One column when the pane is too narrow for two.
fn late_fetch_lines(
    profile: &late_core::models::profile::Profile,
    width: usize,
) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let langs = Style::default().fg(theme::AMBER_DIM());
    let filled = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };

    let mut identity: Vec<(&'static str, String, Style)> = Vec::new();
    if let Some(country) = filled(profile.country.as_deref()) {
        identity.push(("country", country, dim));
    }
    if !profile.langs.is_empty() {
        identity.push(("langs", profile.langs.join(" · "), langs));
    }
    if let Some(created) = profile.created_at {
        identity.push(("since", created.format("%Y-%m").to_string(), dim));
    }
    let mut setup: Vec<(&'static str, String, Style)> = Vec::new();
    for (label, value) in [
        ("ide", profile.ide.as_deref()),
        ("terminal", profile.terminal.as_deref()),
        ("os", profile.os.as_deref()),
    ] {
        if let Some(value) = filled(value) {
            setup.push((label, value, dim));
        }
    }
    if identity.is_empty() && setup.is_empty() {
        return Vec::new();
    }
    let theme_label = theme::label_for_id(profile.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID));
    identity.push(("theme", theme_label.to_string(), dim));

    let mut lines = vec![section_heading("late.fetch", false)];
    if width < LATE_FETCH_TWO_COLUMN_MIN {
        for field in identity.iter().chain(setup.iter()) {
            lines.push(Line::from(late_fetch_cell(field, width)));
        }
        return lines;
    }
    let column = width / 2;
    let rows = identity.len().max(setup.len());
    for row in 0..rows {
        let mut spans = match identity.get(row) {
            Some(field) => late_fetch_cell(field, column),
            None => Vec::new(),
        };
        if let Some(field) = setup.get(row) {
            let used: usize = spans.iter().map(|span| span.width()).sum();
            spans.push(Span::raw(" ".repeat(column - used)));
            spans.extend(late_fetch_cell(field, width - column));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// One `label    value` cell, the value truncated to leave a gap before
/// whatever sits to its right.
fn late_fetch_cell(
    (label, value, style): &(&'static str, String, Style),
    width: usize,
) -> Vec<Span<'static>> {
    let budget = width.saturating_sub(LATE_FETCH_LABEL_WIDTH + 2);
    vec![
        Span::styled(
            format!("{label:<w$}", w = LATE_FETCH_LABEL_WIDTH),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(truncate_to_width(value, budget), *style),
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
    if Span::raw(text).width() <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let budget = width - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let cw = Span::raw(ch.to_string()).width();
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
