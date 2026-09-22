//! The Jobs shelf on the Profiles page: a list of postings beside a
//! detail pane, the same frame the People shelf uses, and the "for you"
//! lines a person's own card prints.

use late_core::models::job_posting::JobPosting;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::state::{JobsState, match_line, scope_label};
use crate::app::common::primitives::{
    format_relative_time, format_relative_time_short, row_with_hint,
};
use crate::app::common::theme;

pub(crate) const JOBS_HINTS: &[(&str, &str)] = &[
    ("j/k", "jobs"),
    ("Enter", "copy link"),
    ("/", "for me"),
    ("w", "your profile"),
    ("Space", "people"),
];
pub(crate) const JOBS_NARROW_HINTS: &[(&str, &str)] = &[
    ("j/k", "jobs"),
    ("l", "open"),
    ("/", "for me"),
    ("w", "your profile"),
    ("Space", "people"),
];
pub(crate) const JOBS_DETAIL_NARROW_HINTS: &[(&str, &str)] =
    &[("h", "back"), ("Enter", "copy link"), ("Space", "people")];

/// Tags on a list row.
const ROW_TAGS: usize = 4;

pub(crate) struct JobsShelfView<'a> {
    pub(crate) jobs: &'a JobsState,
    /// The viewer's match tags (card skills and langs), for `/`.
    pub(crate) viewer_tags: &'a [String],
    pub(crate) narrow: bool,
    pub(crate) short: bool,
}

/// The hints the page footer prints for the shelf's current layout.
pub(crate) fn hints(view: &JobsShelfView<'_>) -> &'static [(&'static str, &'static str)] {
    if !view.narrow {
        JOBS_HINTS
    } else if view.jobs.detail_open() {
        JOBS_DETAIL_NARROW_HINTS
    } else {
        JOBS_NARROW_HINTS
    }
}

pub(crate) fn draw_jobs_shelf(frame: &mut Frame, area: Rect, view: &JobsShelfView<'_>) {
    if !view.jobs.enabled() {
        draw_notice(
            frame,
            area,
            "The job press is stopped.",
            &["An admin turns it back on with /jobs on."],
        );
        return;
    }
    let visible = view.jobs.visible(view.viewer_tags);
    let selected = view.jobs.selected().min(visible.len().saturating_sub(1));
    if visible.is_empty() {
        draw_empty(frame, area, view);
        return;
    }
    if !view.narrow {
        let cols =
            Layout::horizontal([Constraint::Percentage(42), Constraint::Fill(1)]).split(area);
        draw_list(frame, cols[0], &visible, selected, view.short);
        draw_detail(frame, cols[1], visible[selected]);
    } else if view.jobs.detail_open() {
        draw_detail(frame, area, visible[selected]);
    } else {
        draw_list(frame, area, &visible, selected, view.short);
    }
}

fn draw_empty(frame: &mut Frame, area: Rect, view: &JobsShelfView<'_>) {
    if !view.jobs.loaded {
        draw_notice(frame, area, "Reading the shelf…", &[]);
    } else if view.jobs.for_me && view.viewer_tags.is_empty() {
        draw_notice(
            frame,
            area,
            "Nothing to match against yet.",
            &[
                "Press w and put skills on your card, or languages on your late.fetch;",
                "the shelf matches postings on those tags. / shows everything again.",
            ],
        );
    } else if view.jobs.for_me {
        draw_notice(
            frame,
            area,
            "No posting carries your tags right now.",
            &["New ones land every night. / shows everything."],
        );
    } else {
        draw_notice(
            frame,
            area,
            "No postings on the shelf yet.",
            &[
                "The press runs every night at 23:30 UTC: Ask HN Who is hiring,",
                "We Work Remotely, and Jobicy. Remote only, one card a posting, a link out.",
            ],
        );
    }
}

fn draw_notice(frame: &mut Frame, area: Rect, head: &str, rest: &[&str]) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {head}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
    ];
    if !rest.is_empty() {
        lines.push(Line::from(""));
    }
    for line in rest {
        lines.push(Line::from(Span::styled(format!("  {line}"), dim)));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn draw_list(frame: &mut Frame, area: Rect, visible: &[&JobPosting], selected: usize, short: bool) {
    // Three content lines plus the rule under each row; two when short.
    let item_height: u16 = if short { 3 } else { 4 };
    let visible_items = ((area.height / item_height).max(1)) as usize;
    let start_index = selected.saturating_sub(visible_items.saturating_sub(1));
    let end_index = (start_index + visible_items).min(visible.len());
    let visible_len = end_index.saturating_sub(start_index);

    let constraints =
        std::iter::repeat_n(Constraint::Length(item_height), visible_len).collect::<Vec<_>>();
    let rows = Layout::vertical(constraints).split(area);

    for (row, row_area) in rows.iter().copied().enumerate() {
        let index = start_index + row;
        let posting = visible[index];
        let is_selected = index == selected;
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(theme::BORDER_DIM()))
            .style(theme::row_style(is_selected));
        let content = block.inner(row_area);
        frame.render_widget(block, row_area);
        let lines = row_lines(posting, is_selected, short, content.width as usize);
        frame.render_widget(Paragraph::new(lines), content);
    }
}

/// Line 1: company, role, age at the right. Line 2: scope, pay. Line 3:
/// tags, the source at the right.
fn row_lines(
    posting: &JobPosting,
    selected: bool,
    short: bool,
    width: usize,
) -> Vec<Line<'static>> {
    let gutter = if selected { "▎" } else { " " };
    let gutter_style = Style::default().fg(theme::BORDER_ACTIVE());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());

    let age = vec![Span::styled(
        format!(
            "{} ",
            format_relative_time_short(released_or_posted(posting))
        ),
        faint,
    )];
    let age_width: usize = age.iter().map(|span| span.content.width()).sum();
    let head_budget = width.saturating_sub(1 + age_width + 2);
    let company = truncate_to_width(&posting.company, head_budget);
    let role_budget = head_budget.saturating_sub(company.width() + 3);
    let first = vec![
        Span::styled(gutter, gutter_style),
        Span::styled(
            company,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", faint),
        Span::styled(
            truncate_to_width(&posting.title, role_budget),
            Style::default().fg(if selected {
                theme::TEXT_BRIGHT()
            } else {
                theme::TEXT()
            }),
        ),
    ];
    let mut lines = vec![row_with_hint(first, age, width)];

    let mut second = scope_label(posting);
    if !posting.pay.trim().is_empty() {
        second.push_str(" · ");
        second.push_str(posting.pay.trim());
    }
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(truncate_to_width(&second, width.saturating_sub(1)), dim),
    ]));

    if !short {
        let right = vec![Span::styled(
            format!("via {} ", posting.source.site()),
            faint,
        )];
        let right_width: usize = right.iter().map(|span| span.content.width()).sum();
        let tags_budget = width.saturating_sub(1 + right_width + 2);
        let tags = posting
            .tags
            .iter()
            .take(ROW_TAGS)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        lines.push(row_with_hint(
            vec![
                Span::raw(" "),
                Span::styled(
                    truncate_to_width(&tags, tags_budget),
                    Style::default().fg(theme::AMBER_DIM()),
                ),
            ],
            right,
            width,
        ));
    }
    lines
}

/// The whole card: company and role, the scope, tags, pay, the excerpt,
/// the link, and where it came from.
fn draw_detail(frame: &mut Frame, area: Rect, posting: &JobPosting) {
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
    let width = inner.width as usize;
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let body = Style::default().fg(theme::TEXT());

    let header = vec![Span::styled(
        posting.company.clone(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )];
    let age = vec![Span::styled(
        format!("posted {}", format_relative_time(posting.posted_at)),
        faint,
    )];
    let mut lines = vec![
        row_with_hint(header, age, width),
        Line::from(Span::styled(
            posting.title.clone(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("where    ", faint),
            Span::styled(scope_label(posting), body),
        ]),
    ];
    if !posting.tags.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("stack    ", faint),
            Span::styled(
                posting.tags.join(" · "),
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]));
    }
    if !posting.pay.trim().is_empty() {
        lines.push(Line::from(vec![
            Span::styled("pay      ", faint),
            Span::styled(posting.pay.trim().to_string(), body),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("link     ", faint),
        Span::styled(
            display_link(&posting.url),
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::styled("via      ", faint),
        Span::styled(posting.source.site().to_string(), dim),
    ]));
    if !posting.excerpt.trim().is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            posting.excerpt.trim().to_string(),
            body,
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter copies the link. Nothing beyond this card is stored here; the posting lives on its site.",
        faint,
    )));
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

/// The FOR YOU lines under a person's own card: one line per match,
/// then a pointer at the shelf.
pub(crate) fn for_you_lines(matches: &[&JobPosting], width: usize) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut lines = Vec::new();
    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            "No posting on the shelf carries your tags yet; new ones land every night.",
            dim,
        )));
        return lines;
    }
    for posting in matches {
        lines.push(Line::from(vec![
            Span::styled("  ", faint),
            Span::styled(
                truncate_to_width(&match_line(posting, ROW_TAGS), width.saturating_sub(2)),
                Style::default().fg(theme::TEXT()),
            ),
        ]));
    }
    lines.push(Line::from(Span::styled(
        "  Space opens the Jobs shelf; / there keeps only your matches.",
        faint,
    )));
    lines
}

/// The row's age counts from the release (the day it reached the shelf),
/// so a fortnight-old HN post released today reads as today's.
fn released_or_posted(posting: &JobPosting) -> chrono::DateTime<chrono::Utc> {
    match posting.released_on {
        Some(day) => day
            .and_hms_opt(0, 0, 0)
            .expect("midnight exists on every date")
            .and_utc(),
        None => posting.posted_at,
    }
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

fn display_link(url: &str) -> String {
    let stripped = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    stripped.trim_end_matches('/').to_string()
}

/// The strip count: how many rows the shelf has for this viewer.
pub(crate) fn shelf_count(jobs: &JobsState, viewer_tags: &[String]) -> Option<usize> {
    if !jobs.loaded || !jobs.enabled() {
        return None;
    }
    Some(jobs.visible(viewer_tags).len())
}

/// The page footer's right side while the for-me filter is on.
pub(crate) fn footer_note(jobs: &JobsState) -> Vec<Span<'static>> {
    if jobs.for_me {
        vec![Span::styled(
            "for me ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )]
    } else {
        Vec::new()
    }
}
