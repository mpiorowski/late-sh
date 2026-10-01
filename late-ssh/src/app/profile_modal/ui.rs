//! The profile modal: one layout for every terminal, a single column that
//! scrolls.
//!
//! The body is composed off-screen into a buffer as tall as the content,
//! then the visible rows are blitted into the frame. That is what lets the
//! aquarium (a live widget that paints cells) sit in the middle of a
//! scrolling text column without a second layout for small screens: every
//! section takes exactly the rows it needs, and nothing is ever cut but the
//! bonsai's sides when the column is narrower than the canvas.
//!
//! Top to bottom: late.fetch (the fact grid, with the runner card beside
//! it for runners), bio, the bonsai (the whole canvas at its true size),
//! pet, the aquarium, showcases, badges (all of them, always), and the
//! chips ledger. The same order on every screen; the only reflow is the
//! runner card stacking under the grid when the column is too narrow for
//! both.

use chrono::Utc;
use late_core::models::chat_message_gild::{GildCounts, GildTier};
use late_core::models::showcase::Showcase;
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget,
    },
};

use crate::app::{
    bonsai::render::{apply_sway, canvas_lines_in},
    common::{markdown::render_body_to_lines, theme, time::timezone_current_time},
    deadchannel::{
        fight::data as fight_data,
        fight::ui as fight_ui,
        runner::state::{PORTRAIT_HEIGHT, PORTRAIT_WIDTH},
        runner::ui as runner_ui,
    },
    hub::aquarium::{state::AquariumState, ui as aquarium_ui},
    pet::ui::portrait_lines as pet_portrait_lines,
    profile::svc::ProfileRunner,
    settings_modal::data::country_label,
};

use super::{
    badges, ledger,
    state::{ProfileModalState, ScrollExtent},
};

/// The widest the modal gets; past this a text column reads badly.
const MAX_WIDTH: u16 = 110;
/// Below this the body cannot hold a row of the chips table.
const MIN_WIDTH: u16 = 48;
/// Border, blank, footer, border: the rows around the scrolling body.
const CHROME_ROWS: u16 = 4;
/// Left and right breathing room inside the border.
const SIDE_MARGIN: u16 = 2;
/// The reef band: the tallest creature plus the surface and floor rows.
const AQUARIUM_HEIGHT: u16 = 11;
/// The runner card's inside, between its borders: the boxed portrait and
/// the stat column, the widest row being `exp` with five-digit figures.
const CARD_INNER: usize = 40;
/// The whole card: the inside plus a border on each side.
const CARD_WIDTH: u16 = CARD_INNER as u16 + 2;
/// Columns between the fact grid and the card.
const CARD_GAP: u16 = 2;
/// The card sits beside the grid when the body is at least this wide: the
/// grid keeps 46 columns, enough for every fact but a long free-text one.
const CARD_BESIDE_MIN_WIDTH: u16 = 90;
/// Cells in the card's signal and exp bars.
const CARD_BAR_CELLS: usize = 8;

/// One stretch of the body. Each knows its height, so the column can be
/// measured before it is painted.
enum Segment {
    Text(Vec<Line<'static>>),
    /// Two columns side by side: `right` is drawn `right_width` wide
    /// against the right edge, `left` takes what is left of the gap.
    Beside {
        left: Vec<Line<'static>>,
        right: Vec<Line<'static>>,
        right_width: u16,
    },
    Aquarium,
}

impl Segment {
    fn height(&self) -> u16 {
        match self {
            Segment::Text(lines) => lines.len() as u16,
            Segment::Beside { left, right, .. } => left.len().max(right.len()) as u16,
            Segment::Aquarium => AQUARIUM_HEIGHT,
        }
    }
}

/// `viewer_is_runner` gates the runner card: until the public flip
/// (deadchannel CONTEXT.md), what happens on the row is shown only to
/// people on it. One argument to drop at the flip.
pub(crate) fn draw(
    frame: &mut Frame,
    area: Rect,
    state: &ProfileModalState,
    wall_tick: usize,
    viewer_is_runner: bool,
) {
    let width = area.width.saturating_sub(4).clamp(MIN_WIDTH, MAX_WIDTH);
    let body_width = width.saturating_sub(2 + SIDE_MARGIN * 2);

    let (segments, chips_top) = build_segments(state, body_width, wall_tick, viewer_is_runner);
    let content_height: u16 = segments.iter().map(Segment::height).sum();

    // As tall as the terminal allows, but no taller than the content needs:
    // a short profile is a short card, not a tall box with a gap.
    let max_height = area.height.saturating_sub(2).max(8);
    let height = (content_height + CHROME_ROWS).min(max_height);
    let popup = centered_rect(width, height, area);
    state.set_popup_area(popup);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(" profile · {} ", header_name(state)))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    if inner.height < 3 || inner.width < MIN_WIDTH - 2 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1), // breathing room below the title border
        Constraint::Min(1),    // the scrolling body
        Constraint::Length(1), // footer hints
    ])
    .split(inner);
    let viewport = Rect {
        x: rows[1].x + SIDE_MARGIN,
        width: body_width,
        ..rows[1]
    };

    state.set_scroll_extent(ScrollExtent {
        content_height,
        viewport_height: viewport.height,
        chips_top,
    });
    let offset = state.scroll_offset();

    let body = compose(&segments, body_width, content_height, state);
    blit(frame.buffer_mut(), &body, viewport, offset);

    if content_height > viewport.height {
        let mut scrollbar_state = ScrollbarState::new(content_height as usize)
            .viewport_content_length(viewport.height as usize)
            .position(offset as usize);
        let track = Rect {
            x: inner.x + inner.width - 1,
            ..rows[1]
        };
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .track_style(Style::default().fg(theme::BORDER_DIM()))
                .thumb_style(Style::default().fg(theme::AMBER_DIM())),
            track,
            &mut scrollbar_state,
        );
    }

    draw_footer(frame, rows[2], content_height > viewport.height);
}

/// Every section in order, plus the body row the chips section starts on.
fn build_segments(
    state: &ProfileModalState,
    width: u16,
    wall_tick: usize,
    viewer_is_runner: bool,
) -> (Vec<Segment>, Option<u16>) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    let width_usize = width as usize;

    let Some(profile) = state.profile() else {
        return (
            vec![Segment::Text(vec![Line::from(Span::styled(
                "loading…",
                dim,
            ))])],
            None,
        );
    };

    let mut segments = Vec::new();

    // ── late.fetch ──
    // The runner card rides beside the grid, for runners looking at a
    // runner: the row is nobody else's business until the public flip.
    let mut lines = section_lines("late.fetch", width_usize);
    lines.remove(0); // the row under the border already breathes
    let grid = late_fetch_lines(state, profile);
    let card = state
        .runner()
        .filter(|_| viewer_is_runner)
        .map(runner_card_lines);
    match card {
        None => {
            lines.extend(grid);
            segments.push(Segment::Text(lines));
        }
        Some(card) if width >= CARD_BESIDE_MIN_WIDTH => {
            segments.push(Segment::Text(lines));
            segments.push(Segment::Beside {
                left: grid,
                right: card,
                right_width: CARD_WIDTH,
            });
        }
        Some(card) => {
            lines.extend(grid);
            lines.push(Line::from(""));
            lines.extend(card);
            segments.push(Segment::Text(lines));
        }
    }

    // ── bio ──
    let mut lines = section_lines("bio", width_usize);
    if profile.bio.trim().is_empty() {
        lines.push(Line::from(Span::styled("Not set", dim)));
    } else {
        lines.extend(render_body_to_lines(
            &profile.bio,
            width_usize,
            Span::raw(""),
            text,
        ));
    }
    segments.push(Segment::Text(lines));

    // ── bonsai ──
    // The whole canvas at its true size, never a preview: a column
    // narrower than the canvas cuts the tree evenly on both sides.
    let mut lines = section_lines("bonsai", width_usize);
    lines.extend(bonsai_lines(state, width_usize, wall_tick));
    segments.push(Segment::Text(lines));

    // ── pet ──
    // The mood is the one the owner's session last wrote: a readout of
    // how their night is going, honest because they never set it.
    if let Some(pet) = state.pet() {
        let mut lines = section_lines("pet", width_usize);
        lines.extend(pet_portrait_lines(pet.species, pet.mood, wall_tick));
        let name = pet
            .name
            .clone()
            .unwrap_or_else(|| pet.species.as_str().to_string());
        lines.push(Line::from(vec![
            Span::styled(
                name,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {}", pet.mood.as_str()), dim),
        ]));
        segments.push(Segment::Text(lines));
    }

    // ── aquarium ──
    if !state.aquarium_fish().is_empty() {
        segments.push(Segment::Text(section_lines("aquarium", width_usize)));
        segments.push(Segment::Aquarium);
    }

    // ── showcases ──
    let showcases = state.showcases();
    if !showcases.is_empty() {
        let mut lines = section_lines(&format!("showcases ({})", showcases.len()), width_usize);
        for (index, item) in showcases.iter().enumerate() {
            if index > 0 {
                lines.push(Line::from(""));
            }
            lines.extend(render_body_to_lines(
                &showcase_markdown(item),
                width_usize,
                Span::raw(""),
                text,
            ));
        }
        segments.push(Segment::Text(lines));
    }

    // ── badges: every one, wrapped, never folded ──
    let badge_lines = badges::badge_lines(state.profile_awards(), width_usize);
    if !badge_lines.is_empty() {
        let mut lines = section_lines("badges", width_usize);
        lines.extend(badge_lines);
        segments.push(Segment::Text(lines));
    }

    // ── chips ──
    let chips_top = segments.iter().map(Segment::height).sum::<u16>();
    let mut lines = section_lines("chips", width_usize);
    lines.push(ledger::summary_line(
        state.chip_balance(),
        state.chips_month(),
    ));
    lines.push(ledger::off_board_note());
    lines.push(Line::from(""));
    if state.chip_ledger().is_empty() {
        lines.push(Line::from(Span::styled("no chips moved yet", dim)));
    }
    for row in state.chip_ledger() {
        lines.push(ledger::row_line(row, width_usize));
    }
    segments.push(Segment::Text(lines));

    (segments, Some(chips_top))
}

/// Paint every segment into a buffer exactly as tall as the content.
fn compose(segments: &[Segment], width: u16, height: u16, state: &ProfileModalState) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width, height.max(1)));
    let mut y = 0u16;
    for segment in segments {
        let segment_height = segment.height();
        let area = Rect::new(0, y, width, segment_height);
        match segment {
            Segment::Text(lines) => {
                Paragraph::new(lines.clone()).render(area, &mut buf);
            }
            Segment::Beside {
                left,
                right,
                right_width,
            } => {
                let right_area = Rect {
                    x: width.saturating_sub(*right_width),
                    width: (*right_width).min(width),
                    ..area
                };
                let left_area = Rect {
                    width: right_area.x.saturating_sub(CARD_GAP),
                    ..area
                };
                Paragraph::new(left.clone()).render(left_area, &mut buf);
                Paragraph::new(right.clone()).render(right_area, &mut buf);
            }
            Segment::Aquarium => draw_aquarium(&mut buf, area, state),
        }
        y = y.saturating_add(segment_height);
    }
    buf
}

/// Copy the rows `[offset, offset + viewport.height)` of `body` into the
/// frame at `viewport`.
fn blit(frame_buf: &mut Buffer, body: &Buffer, viewport: Rect, offset: u16) {
    for row in 0..viewport.height {
        let src_y = offset.saturating_add(row);
        for x in 0..viewport.width {
            let Some(src) = body.cell((x, src_y)) else {
                continue;
            };
            if let Some(dst) = frame_buf.cell_mut((viewport.x + x, viewport.y + row)) {
                *dst = src.clone();
            }
        }
    }
}

/// The reef paints into its own fixed-size buffer, keyed on the band's
/// size alone, so scrolling never rebuilds it; the band is then copied into
/// the body at whatever row it landed on.
fn draw_aquarium(body: &mut Buffer, area: Rect, state: &ProfileModalState) {
    let band = Rect::new(0, 0, area.width, area.height);
    let cell = state.aquarium_cell();
    let mut slot = cell.borrow_mut();
    if slot.is_none() || state.aquarium_area().get() != band {
        state.aquarium_area().set(band);
        *slot = AquariumState::default_for_area(band)
            .ok()
            .map(|mut aquarium| {
                aquarium.set_active_creatures(state.aquarium_fish(), None, false);
                aquarium
            });
    }

    let mut reef = Buffer::empty(band);
    match slot.as_ref() {
        Some(aquarium) => aquarium_ui::draw_into(&mut reef, band, aquarium),
        None => Paragraph::new(Line::from(Span::styled(
            "aquarium unavailable",
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .render(band, &mut reef),
    }
    blit(body, &reef, area, 0);
}

fn header_name(state: &ProfileModalState) -> String {
    if let Some(profile) = state.profile() {
        let username = profile.username.trim();
        if !username.is_empty() {
            return username.to_string();
        }
    }
    if state.fallback_name().is_empty() {
        "loading".to_string()
    } else {
        state.fallback_name().to_string()
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, scrollable: bool) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let dim = Style::default().fg(theme::TEXT_DIM());

    let mut spans = vec![Span::raw("  ")];
    if scrollable {
        spans.push(Span::styled("↑↓ j/k", key));
        spans.push(Span::styled(" scroll  ", dim));
        spans.push(Span::styled("g/G", key));
        spans.push(Span::styled(" top/bottom  ", dim));
    }
    spans.push(Span::styled("Esc/q", key));
    spans.push(Span::styled(" close", dim));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The runner card: a framed panel titled with the runner's badge. The
/// portrait sits in its own box, losing cells to static in proportion to
/// the missing signal (`fight::ui::corrupt`, the fight scene's wound), with
/// the level, the signal and exp bars, and the bits beside it; the kit and
/// the glyphs put down (and the Old Signal marks with their title once
/// there are any) run under it. The sheet arrives settled for today (the
/// service applies the day roll to the view), so the signal is what the
/// runner would find on the row. Rations are not here: the street's strip
/// and the frame HUD carry them for the runner themself.
fn runner_card_lines(runner: &ProfileRunner) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    let key = Style::default().fg(theme::AMBER_DIM());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let frame = Style::default().fg(theme::BORDER_DIM());
    let sheet = &runner.sheet;
    let level = Style::default()
        .fg(runner_ui::level_color(sheet.level))
        .add_modifier(Modifier::BOLD);

    // The badge in the title, the wire's own token (`▚7`, `▚7╬2`).
    let badge = match sheet.marks {
        0 => format!("{}{}", runner.look.mark, sheet.level),
        marks => format!("{}{}╬{marks}", runner.look.mark, sheet.level),
    };
    let title = vec![
        Span::styled("╭─ ", frame),
        Span::styled(
            "runner",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ", frame),
        Span::styled(badge, level),
        Span::styled(" ", frame),
    ];
    let title_width: usize = title.iter().map(Span::width).sum();
    let mut top = title;
    top.push(Span::styled(
        format!(
            "{}╮",
            "─".repeat((CARD_INNER + 1).saturating_sub(title_width))
        ),
        frame,
    ));

    // The face, wounded by the missing signal; the seed is the runner, so
    // the same cells are gone on every open.
    let missing =
        1.0 - sheet.signal.clamp(0, sheet.max_signal()) as f32 / sheet.max_signal() as f32;
    let worn = runner.look.rows();
    let face = fight_ui::corrupt(
        worn.each_ref().map(|worn| worn.piece.row),
        missing,
        sheet.user_id.as_u128() as u64,
    );
    let face_rows: Vec<Vec<Span<'static>>> = face
        .into_iter()
        .zip(worn)
        .map(|(cells, worn)| {
            let tint = Style::default().fg(runner_ui::tint_color(worn.tint));
            cells
                .into_iter()
                .map(|(ch, lost)| match lost {
                    true => Span::styled(ch.to_string(), dim),
                    false => Span::styled(ch.to_string(), tint),
                })
                .collect()
        })
        .collect();

    let signal = match sheet.is_down() {
        true => Span::styled("down", Style::default().fg(theme::ERROR())),
        false => Span::styled(format!("{}/{}", sheet.signal, sheet.max_signal()), text),
    };
    // Past the top of the ladder the exp climbs toward the Old Signal.
    let exp_goal = match fight_data::exp_to_advance(sheet.level, sheet.marks) {
        Some(need) => need,
        None => fight_data::exp_to_seek(sheet.marks),
    };
    let stats: [Vec<Span<'static>>; PORTRAIT_HEIGHT] = [
        {
            let mut row = vec![Span::styled("signal ", key)];
            row.extend(card_bar(
                sheet.signal.into(),
                sheet.max_signal().into(),
                Style::default().fg(theme::BONSAI_LEAF()),
            ));
            row.push(Span::raw(" "));
            row.push(signal);
            row
        },
        {
            let mut row = vec![Span::styled("exp    ", key)];
            row.extend(card_bar(
                sheet.exp,
                exp_goal,
                Style::default().fg(theme::AMBER()),
            ));
            row.push(Span::styled(format!(" {}/{exp_goal}", sheet.exp), text));
            row
        },
        vec![
            Span::styled("bits   ", key),
            Span::styled(ledger::thousands(sheet.bits), bright),
        ],
    ];

    let rule = "─".repeat(PORTRAIT_WIDTH);
    let mut inner: Vec<Vec<Span<'static>>> = vec![
        vec![],
        vec![
            Span::styled(format!(" ┌{rule}┐   "), frame),
            Span::styled(format!("lv {}", sheet.level), level),
        ],
    ];
    for (face, stat) in face_rows.into_iter().zip(stats) {
        let mut row = vec![Span::styled(" │", frame)];
        row.extend(face);
        row.push(Span::styled("│   ", frame));
        row.extend(stat);
        inner.push(row);
    }
    inner.push(vec![Span::styled(format!(" └{rule}┘"), frame)]);
    inner.push(vec![]);
    inner.push(vec![
        Span::styled(" weapon ", key),
        Span::styled(fight_ui::weapon_name(sheet).to_string(), text),
    ]);
    inner.push(vec![
        Span::styled(" armor  ", key),
        Span::styled(fight_ui::armor_name(sheet).to_string(), text),
    ]);
    let glyphs = match sheet.kills {
        1 => "1 down".to_string(),
        n => format!("{n} down"),
    };
    inner.push(vec![
        Span::styled(" glyphs ", key),
        Span::styled(glyphs, text),
    ]);
    if let Some(title) = fight_data::title(sheet.marks) {
        inner.push(vec![
            Span::styled(" marks  ", key),
            Span::styled(format!("╬{} {title}", sheet.marks), text),
        ]);
    }

    let mut lines = vec![Line::from(top)];
    for mut row in inner {
        let used: usize = row.iter().map(Span::width).sum();
        row.insert(0, Span::styled("│", frame));
        row.push(Span::raw(" ".repeat(CARD_INNER.saturating_sub(used))));
        row.push(Span::styled("│", frame));
        lines.push(Line::from(row));
    }
    lines.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(CARD_INNER)),
        frame,
    )));
    lines
}

/// A card bar: `CARD_BAR_CELLS` cells, the filled run in `filled` and the
/// rest as dim shade.
fn card_bar(current: i64, max: i64, filled: Style) -> [Span<'static>; 2] {
    let max = max.max(1);
    let cells =
        ((current.clamp(0, max) as f64 / max as f64) * CARD_BAR_CELLS as f64).round() as usize;
    [
        Span::styled("█".repeat(cells), filled),
        Span::styled(
            "░".repeat(CARD_BAR_CELLS - cells),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ]
}

/// A section heading: a dim label trailed by a rule, with a blank row above
/// it so sections breathe.
fn section_lines(label: &str, width: usize) -> Vec<Line<'static>> {
    let used = label.chars().count() + 1;
    let rule = width.saturating_sub(used);
    vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                label.to_string(),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("─".repeat(rule), Style::default().fg(theme::BORDER_DIM())),
        ]),
    ]
}

/// The bonsai section's body: the whole canvas at its true size, centered
/// (or cut evenly on both sides when `width` is narrower than the canvas),
/// swaying on the wall tick.
fn bonsai_lines(state: &ProfileModalState, width: usize, wall_tick: usize) -> Vec<Line<'static>> {
    match state.bonsai() {
        Some(bonsai) => {
            let mut lines = canvas_lines_in(bonsai, width);
            apply_sway(&mut lines, wall_tick);
            lines
        }
        None => vec![Line::from(Span::styled(
            "no bonsai yet",
            Style::default().fg(theme::TEXT_DIM()),
        ))],
    }
}

/// The neofetch column: one `key   value` row per fact (the name is already
/// the modal's title). Unset values are dim rather than absent, so every
/// profile has the same shape.
fn late_fetch_lines(
    state: &ProfileModalState,
    profile: &late_core::models::profile::Profile,
) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default().fg(theme::AMBER_DIM());
    let value = Style::default().fg(theme::TEXT());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());

    let mut lines = Vec::new();

    let row = |label: &str, spans: Vec<Span<'static>>| {
        let mut out = vec![Span::styled(format!("{label:<10}"), key)];
        out.extend(spans);
        Line::from(out)
    };
    let set_or = |text: Option<String>| match text {
        Some(text) if !text.trim().is_empty() => Span::styled(text, value),
        _ => Span::styled("not set".to_string(), dim),
    };

    lines.push(row(
        "country",
        vec![Span::styled(
            country_label(profile.country.as_deref()),
            value,
        )],
    ));
    if let Some(time) = timezone_current_time(Utc::now(), profile.timezone.as_deref()) {
        lines.push(row("local", vec![Span::styled(time, value)]));
    }
    // Balance only; the month's earned and net figures head the chips
    // section below.
    let chips = match state.chip_balance() {
        Some(balance) => Span::styled(ledger::thousands(balance), bright),
        None => Span::styled("…".to_string(), dim),
    };
    lines.push(row("chips", vec![chips]));

    let gilds = gild_spans(state.gild_counts());
    if !gilds.is_empty() {
        lines.push(row("gilds", gilds));
    }
    let gallery = state.gallery_counts();
    if gallery.pieces > 0 {
        lines.push(row(
            "gallery",
            vec![
                Span::styled(
                    format!(
                        "{} {}",
                        gallery.pieces,
                        if gallery.pieces == 1 {
                            "piece"
                        } else {
                            "pieces"
                        }
                    ),
                    value,
                ),
                Span::styled(format!(" · {} applause", gallery.applause), dim),
            ],
        ));
    }

    if !state.profile_awards().is_empty() {
        lines.push(row(
            "badges",
            vec![Span::styled(
                state.profile_awards().len().to_string(),
                value,
            )],
        ));
    }
    lines.push(row(
        "created",
        vec![set_or(
            profile
                .created_at
                .as_ref()
                .map(|at| at.format("%Y-%m-%d").to_string()),
        )],
    ));
    if let Some(created) = profile.created_at.as_ref() {
        let days = (Utc::now() - *created).num_days().max(0);
        let member = match days {
            0 => "since today".to_string(),
            1 => "1 day".to_string(),
            2..=59 => format!("{days} days"),
            _ => format!("{} months", days / 30),
        };
        lines.push(row("member", vec![Span::styled(member, value)]));
    }
    lines.push(row("ide", vec![set_or(profile.ide.clone())]));
    lines.push(row("os", vec![set_or(profile.os.clone())]));
    lines.push(row("terminal", vec![set_or(profile.terminal.clone())]));
    let theme_id = profile.theme_id.as_deref().unwrap_or(theme::DEFAULT_ID);
    lines.push(row(
        "theme",
        vec![Span::styled(
            theme::label_for_id(theme_id).to_string(),
            value,
        )],
    ));
    lines.push(row(
        "langs",
        vec![set_or(
            (!profile.langs.is_empty()).then(|| profile.langs.join(", ")),
        )],
    ));
    lines
}

/// Gilds received as `● x2  ○ x5`, each tier in its own colour. Empty when
/// there are none: an empty scoreboard reads as one nobody asked to be on.
fn gild_spans(counts: GildCounts) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for tier in GildTier::ALL.iter().filter(|tier| counts.get(**tier) > 0) {
        let color = match tier {
            GildTier::Bronze => theme::BADGE_BRONZE(),
            GildTier::Silver => theme::BADGE_SILVER(),
            GildTier::Gold => theme::BADGE_GOLD(),
        };
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            tier.marker().to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {} x{}", tier.label(), counts.get(*tier)),
            Style::default().fg(theme::TEXT()),
        ));
    }
    spans
}

fn showcase_markdown(s: &Showcase) -> String {
    let mut out = String::new();
    out.push_str("### ");
    out.push_str(s.title.trim());
    out.push_str("\n\n> ");
    out.push_str(s.url.trim());
    let description = s.description.trim();
    if !description.is_empty() {
        out.push_str("\n\n");
        out.push_str(description);
    }
    if !s.tags.is_empty() {
        out.push_str("\n\n");
        let mut first = true;
        for tag in &s.tags {
            if !first {
                out.push(' ');
            }
            first = false;
            out.push('`');
            out.push('#');
            out.push_str(tag);
            out.push('`');
        }
    }
    out
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
