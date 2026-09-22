//! The Jobs shelf on the Profiles page: a list of postings beside a
//! detail pane, the same frame the People shelf uses, the "for you"
//! lines a person's own card prints, and the post form over the page.

use late_core::models::job_posting::{JobPosting, JobSource};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::post::{POST_FIELDS, PostField, PostForm, PostKind, scope_choice_label};
use super::state::{JobsState, match_line, scope_label};
use crate::app::common::composer::placeholder_with_cursor;
use crate::app::common::primitives::{
    format_relative_time, format_relative_time_short, hint_line, row_with_hint,
};
use crate::app::common::theme;

pub(crate) const JOBS_HINTS: &[(&str, &str)] = &[
    ("j/k", "jobs"),
    ("Enter", "copy link"),
    ("/", "for me"),
    ("n", "post a job"),
    ("d", "take down"),
    ("w", "your profile"),
    ("Space", "people"),
];
pub(crate) const JOBS_NARROW_HINTS: &[(&str, &str)] = &[
    ("j/k", "jobs"),
    ("l", "open"),
    ("/", "for me"),
    ("n", "post"),
    ("w", "your profile"),
    ("Space", "people"),
];
pub(crate) const JOBS_DETAIL_NARROW_HINTS: &[(&str, &str)] = &[
    ("h", "back"),
    ("Enter", "copy link"),
    ("d", "take down"),
    ("Space", "people"),
];

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
        match posting.source {
            JobSource::Late => {
                "Posted here on late.sh. Enter copies the link; d takes it down, for its writer or a moderator."
            }
            JobSource::Hn | JobSource::Wwr | JobSource::Jobicy => {
                "Enter copies the link. Nothing beyond this card is stored here; the posting lives on its site."
            }
        },
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

// The post form

/// Label column, then a space, then the value.
const POST_LABEL_W: u16 = 9;
const POST_GUTTER: u16 = 2;
const POST_MODAL_W: u16 = 84;

/// The post form centred over the page: the rows, a line on what a
/// posting is here for, the error, the keys.
pub(crate) fn draw_post_form(frame: &mut Frame, area: Rect, form: &PostForm) {
    let rows_height: u16 = POST_FIELDS.iter().map(|field| field.height()).sum();
    // frame(2) + padding(2) + rows + blank(1) + note(1) + error(1) + keys(1)
    let wanted = rows_height + 8;
    let width = POST_MODAL_W.min(area.width);
    let height = wanted.min(area.height);
    let popup = Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(Span::styled(
            " Post a job ",
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

    let [body, _, note_row, error_row, keys_row] = Layout::vertical([
        Constraint::Length(rows_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(inner);

    let constraints: Vec<Constraint> = POST_FIELDS
        .iter()
        .map(|field| Constraint::Length(field.height()))
        .collect();
    let rows = Layout::vertical(constraints).split(body);
    for (idx, (field, row_area)) in POST_FIELDS.iter().zip(rows.iter().copied()).enumerate() {
        draw_post_row(frame, row_area, form, *field, idx == form.row());
    }

    let faint = Style::default().fg(theme::TEXT_FAINT());
    frame.render_widget(
        on_canvas(Line::from(Span::styled(
            "Live at once, for 30 days. Three a person; d on yours takes it down.",
            faint,
        ))),
        indent(note_row),
    );
    if let Some(error) = form.error() {
        frame.render_widget(
            on_canvas(Line::from(Span::styled(
                format!("✗ {}", error.message),
                Style::default().fg(theme::ERROR()),
            ))),
            indent(error_row),
        );
    }
    let keys = if form.pending() {
        Line::from(Span::styled("Saving…", faint))
    } else if form.editing() {
        let mut hints: Vec<(&str, &str)> = vec![("Enter/Tab", "next row"), ("Esc", "done")];
        if form.active_field().kind() == PostKind::Multi {
            hints.push(("Alt+Enter", "new line"));
        }
        hints.push(("Ctrl+S", "post"));
        hint_line(&hints)
    } else {
        hint_line(&[
            ("Enter", "edit row"),
            ("j/k", "rows"),
            ("←/→", "cycle"),
            ("Ctrl+S", "post"),
            ("Esc", "close"),
        ])
    };
    frame.render_widget(on_canvas(keys), keys_row);
}

fn draw_post_row(frame: &mut Frame, area: Rect, form: &PostForm, field: PostField, active: bool) {
    let typing = active && form.editing();
    let [gutter, label_col, _, value_col] = Layout::horizontal([
        Constraint::Length(POST_GUTTER),
        Constraint::Length(POST_LABEL_W),
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

    let errored = form.error().is_some_and(|error| error.field == Some(field));
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

    let faint = Style::default().fg(theme::TEXT_FAINT());
    match field.kind() {
        PostKind::Choice => {
            let spans = vec![Span::styled(
                scope_choice_label(form.scope()).to_string(),
                Style::default()
                    .fg(if active {
                        theme::TEXT_BRIGHT()
                    } else {
                        theme::TEXT()
                    })
                    .add_modifier(Modifier::BOLD),
            )];
            let right = if active {
                vec![Span::styled("←/→ cycle", faint)]
            } else {
                Vec::new()
            };
            frame.render_widget(
                on_canvas(row_with_hint(spans, right, value_col.width as usize)),
                value_col,
            );
        }
        PostKind::Tags => {
            let mut spans: Vec<Span<'static>> = Vec::new();
            if form.tags().is_empty() {
                spans.push(Span::styled(field.placeholder().to_string(), faint));
            }
            for (idx, tag) in form.tags().iter().enumerate() {
                if idx > 0 {
                    spans.push(Span::styled(" · ", faint));
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
                spans.push(Span::styled("   Enter pick", faint));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans))
                    .wrap(Wrap { trim: false })
                    .style(Style::default().bg(theme::BG_CANVAS())),
                value_col,
            );
        }
        PostKind::Text | PostKind::Multi => {
            if typing && form.field_text(field).is_empty() {
                frame.render_widget(
                    Paragraph::new(placeholder_with_cursor(field.placeholder()))
                        .style(Style::default().bg(theme::BG_CANVAS())),
                    value_col,
                );
            } else if typing {
                frame.render_widget(form.field(field), value_col);
            } else {
                let text = form.field_text(field);
                let lines: Vec<Line<'static>> = if text.is_empty() {
                    vec![Line::from(Span::styled(
                        field.placeholder().to_string(),
                        faint,
                    ))]
                } else {
                    let style = Style::default().fg(if active {
                        theme::TEXT_BRIGHT()
                    } else {
                        theme::TEXT()
                    });
                    text.lines()
                        .take(value_col.height as usize)
                        .map(|line| Line::from(Span::styled(line.to_string(), style)))
                        .collect()
                };
                frame.render_widget(
                    Paragraph::new(lines)
                        .wrap(Wrap { trim: false })
                        .style(Style::default().bg(theme::BG_CANVAS())),
                    value_col,
                );
            }
        }
    }
}

fn indent(area: Rect) -> Rect {
    Rect {
        x: area.x + POST_GUTTER,
        width: area.width.saturating_sub(POST_GUTTER),
        ..area
    }
}

fn on_canvas(line: Line<'static>) -> Paragraph<'static> {
    Paragraph::new(line).style(Style::default().bg(theme::BG_CANVAS()))
}
