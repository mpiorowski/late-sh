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
//! Top to bottom: late.fetch (the fact grid, with the runner column
//! beside it for runners, each under its own heading), bio, showcases, the
//! bonsai (the whole canvas at its true size), the aquarium with the pet
//! beside it (each a full-width section when the other is not owned),
//! badges (all of them, always), and the chips ledger. The same order on
//! every screen; the only reflow is a pair of columns (grid and runner,
//! reef and pet) stacking into sections when the body is too narrow for
//! both.

use chrono::Utc;
use late_core::models::chat_message_gild::{GildCounts, GildTier};
use late_core::models::pet::PET_NAME_MAX_CHARS;
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
        fight::data as fight_data, fight::ui as fight_ui, runner::state::PORTRAIT_WIDTH,
        runner::ui as runner_ui,
    },
    hub::aquarium::{state::AquariumState, ui as aquarium_ui},
    pet::ui::portrait_lines as pet_portrait_lines,
    profile::svc::{ProfilePet, ProfileRunner},
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
/// The runner column: the portrait, the keys, and the widest row, `exp`
/// with its bar and six-digit figures on a marked climb.
const RUNNER_WIDTH: u16 = 42;
/// Columns between two columns that sit side by side.
const COLUMN_GAP: u16 = 3;
/// Two columns sit side by side when the body is at least this wide. Beside
/// the runner the grid keeps 45 columns, enough for every fact but a long
/// free-text one; beside the pet the reef keeps 63.
const BESIDE_MIN_WIDTH: u16 = 90;
/// The pet column beside the reef: the longest name a pet can carry.
const PET_WIDTH: u16 = PET_NAME_MAX_CHARS as u16;
/// Cells in the runner's signal and exp bars, the fight scene's count.
const RUNNER_BAR_CELLS: usize = 12;

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
    /// The reef with a text column `right_width` wide against the right
    /// edge: the pet beside its owner's tank.
    AquariumBeside {
        right: Vec<Line<'static>>,
        right_width: u16,
    },
}

impl Segment {
    fn height(&self) -> u16 {
        match self {
            Segment::Text(lines) => lines.len() as u16,
            Segment::Beside { left, right, .. } => left.len().max(right.len()) as u16,
            Segment::Aquarium | Segment::AquariumBeside { .. } => AQUARIUM_HEIGHT,
        }
    }
}

/// `viewer_is_runner` gates the runner column: until the public flip
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

    // ── late.fetch, and the runner ──
    // The runner is a second column beside the grid, each under its own
    // heading, for runners looking at a runner: the row is nobody else's
    // business until the public flip. A narrow body makes it a section.
    let grid = late_fetch_lines(state, profile);
    let runner = state.runner().filter(|_| viewer_is_runner);
    match runner {
        None => {
            let mut lines = vec![section_heading("late.fetch", width_usize)];
            lines.extend(grid);
            segments.push(Segment::Text(lines));
        }
        Some(runner) if width >= BESIDE_MIN_WIDTH => {
            let left_width = usize::from(width - RUNNER_WIDTH - COLUMN_GAP);
            let mut left = vec![section_heading("late.fetch", left_width)];
            left.extend(grid);
            let mut right = vec![runner_heading(runner, usize::from(RUNNER_WIDTH))];
            right.extend(runner_lines(runner));
            segments.push(Segment::Beside {
                left,
                right,
                right_width: RUNNER_WIDTH,
            });
        }
        Some(runner) => {
            let mut lines = vec![section_heading("late.fetch", width_usize)];
            lines.extend(grid);
            lines.push(Line::from(""));
            lines.push(runner_heading(runner, width_usize));
            lines.extend(runner_lines(runner));
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

    // ── bonsai ──
    // The whole canvas at its true size, never a preview: a column
    // narrower than the canvas cuts the tree evenly on both sides.
    let mut lines = section_lines("bonsai", width_usize);
    lines.extend(bonsai_lines(state, width_usize, wall_tick));
    segments.push(Segment::Text(lines));

    // ── aquarium, and the pet ──
    // The pet's mood is the one the owner's session last wrote: a readout
    // of how their night is going, honest because they never set it. With
    // a tank too, the pet sits in a column beside the reef, each under its
    // own heading; alone, or on a narrow body, each is a section.
    let has_fish = !state.aquarium_fish().is_empty();
    match state.pet() {
        Some(pet) if has_fish && width >= BESIDE_MIN_WIDTH => {
            let reef_width = usize::from(width - PET_WIDTH - COLUMN_GAP);
            let mut heading = section_heading("aquarium", reef_width).spans;
            heading.push(Span::raw(" ".repeat(usize::from(COLUMN_GAP))));
            heading.extend(section_heading("pet", usize::from(PET_WIDTH)).spans);
            segments.push(Segment::Text(vec![Line::from(""), Line::from(heading)]));

            // The pet stands halfway down the band, its name and its mood
            // on a row each so the longest name still fits the column.
            let mut right = pet_portrait_lines(pet.species, pet.mood, wall_tick);
            right.push(Line::from(pet_name_span(pet)));
            right.push(Line::from(Span::styled(pet.mood.as_str(), dim)));
            let above = usize::from(AQUARIUM_HEIGHT).saturating_sub(right.len()) / 2;
            right.splice(0..0, vec![Line::from(""); above]);
            segments.push(Segment::AquariumBeside {
                right,
                right_width: PET_WIDTH,
            });
        }
        pet => {
            if let Some(pet) = pet {
                let mut lines = section_lines("pet", width_usize);
                lines.extend(pet_portrait_lines(pet.species, pet.mood, wall_tick));
                lines.push(Line::from(vec![
                    pet_name_span(pet),
                    Span::styled(format!(" · {}", pet.mood.as_str()), dim),
                ]));
                segments.push(Segment::Text(lines));
            }
            if has_fish {
                segments.push(Segment::Text(section_lines("aquarium", width_usize)));
                segments.push(Segment::Aquarium);
            }
        }
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
                    width: right_area.x.saturating_sub(COLUMN_GAP),
                    ..area
                };
                Paragraph::new(left.clone()).render(left_area, &mut buf);
                Paragraph::new(right.clone()).render(right_area, &mut buf);
            }
            Segment::Aquarium => draw_aquarium(&mut buf, area, state),
            Segment::AquariumBeside { right, right_width } => {
                let right_area = Rect {
                    x: width.saturating_sub(*right_width),
                    width: (*right_width).min(width),
                    ..area
                };
                let reef_area = Rect {
                    width: right_area.x.saturating_sub(COLUMN_GAP),
                    ..area
                };
                draw_aquarium(&mut buf, reef_area, state);
                Paragraph::new(right.clone()).render(right_area, &mut buf);
            }
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

/// The pet's name, or its species while it has none.
fn pet_name_span(pet: &ProfilePet) -> Span<'static> {
    let name = match &pet.name {
        Some(name) => name.clone(),
        None => pet.species.as_str().to_string(),
    };
    Span::styled(
        name,
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    )
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

/// The runner's heading: `runner`, then the wire's own badge (`▚7`,
/// `▚7╬2`) in the level's band color, then the rule.
fn runner_heading(runner: &ProfileRunner, width: usize) -> Line<'static> {
    let sheet = &runner.sheet;
    let badge = match sheet.marks {
        0 => format!("{}{}", runner.look.mark, sheet.level),
        marks => format!("{}{}╬{marks}", runner.look.mark, sheet.level),
    };
    heading_line(
        vec![
            heading_label("runner"),
            Span::raw(" "),
            Span::styled(
                badge,
                Style::default()
                    .fg(runner_ui::level_color(sheet.level))
                    .add_modifier(Modifier::BOLD),
            ),
        ],
        width,
    )
}

/// The runner's rows, no frame anywhere: the three-row portrait on the
/// left, losing cells to static in proportion to the missing signal
/// (`fight::ui::corrupt`, the fight scene's wound), and one `key  value`
/// column beside and below it, the grid's own shape: the signal and exp
/// bars and the bits beside the face, then the kit by name, the glyphs put
/// down, and the Old Signal marks with their title once there are any.
/// The level is the heading's badge, so it has no row. The sheet arrives
/// settled for today (the service applies the day roll to the view), so
/// the signal is what the runner would find on the row. Rations are not
/// here: the street's strip and the frame HUD carry them for the runner
/// themself.
fn runner_lines(runner: &ProfileRunner) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    let key = Style::default().fg(theme::AMBER_DIM());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let sheet = &runner.sheet;

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
    let portrait = face.into_iter().zip(worn).map(|(cells, worn)| {
        let tint = Style::default().fg(runner_ui::tint_color(worn.tint));
        cells
            .into_iter()
            .map(|(ch, lost)| match lost {
                true => Span::styled(ch.to_string(), dim),
                false => Span::styled(ch.to_string(), tint),
            })
            .collect::<Vec<Span<'static>>>()
    });

    let fact = |label: &str, mut value: Vec<Span<'static>>| {
        let mut row = vec![Span::styled(format!("{label:<8}"), key)];
        row.append(&mut value);
        row
    };
    let signal = match sheet.is_down() {
        true => Span::styled(" down", Style::default().fg(theme::ERROR())),
        false => Span::styled(format!(" {}/{}", sheet.signal, sheet.max_signal()), text),
    };
    // Past the top of the ladder the exp climbs toward the Old Signal.
    let exp_goal = match fight_data::exp_to_advance(sheet.level, sheet.marks) {
        Some(need) => need,
        None => fight_data::exp_to_seek(sheet.marks),
    };
    let glyphs = match sheet.kills {
        1 => "1 down".to_string(),
        n => format!("{n} down"),
    };
    let mut facts = vec![
        fact("signal", {
            let mut value = runner_bar(
                sheet.signal.into(),
                sheet.max_signal().into(),
                Style::default().fg(theme::BONSAI_LEAF()),
            )
            .to_vec();
            value.push(signal);
            value
        }),
        fact("exp", {
            let mut value =
                runner_bar(sheet.exp, exp_goal, Style::default().fg(theme::AMBER())).to_vec();
            value.push(Span::styled(format!(" {}/{exp_goal}", sheet.exp), text));
            value
        }),
        fact(
            "bits",
            vec![Span::styled(ledger::thousands(sheet.bits), bright)],
        ),
        fact(
            "weapon",
            vec![Span::styled(fight_ui::weapon_name(sheet).to_string(), text)],
        ),
        fact(
            "armor",
            vec![Span::styled(fight_ui::armor_name(sheet).to_string(), text)],
        ),
        fact("glyphs", vec![Span::styled(glyphs, text)]),
    ];
    if let Some(title) = fight_data::title(sheet.marks) {
        facts.push(fact(
            "marks",
            vec![Span::styled(format!("╬{} {title}", sheet.marks), text)],
        ));
    }

    // The facts run past the portrait; the rows under it keep its column
    // empty so the keys stay in one line.
    let blank = " ".repeat(PORTRAIT_WIDTH);
    let mut portrait = portrait.into_iter();
    facts
        .into_iter()
        .map(|fact| {
            let mut row = match portrait.next() {
                Some(row) => row,
                None => vec![Span::raw(blank.clone())],
            };
            row.push(Span::raw("  "));
            row.extend(fact);
            Line::from(row)
        })
        .collect()
}

/// A runner bar: `RUNNER_BAR_CELLS` cells, the filled run in `filled` and
/// the rest as dim shade.
fn runner_bar(current: i64, max: i64, filled: Style) -> [Span<'static>; 2] {
    let max = max.max(1);
    let cells =
        ((current.clamp(0, max) as f64 / max as f64) * RUNNER_BAR_CELLS as f64).round() as usize;
    [
        Span::styled("█".repeat(cells), filled),
        Span::styled(
            "░".repeat(RUNNER_BAR_CELLS - cells),
            Style::default().fg(theme::BORDER_DIM()),
        ),
    ]
}

/// A section heading: a dim label trailed by a rule, with a blank row above
/// it so sections breathe.
fn section_lines(label: &str, width: usize) -> Vec<Line<'static>> {
    vec![Line::from(""), section_heading(label, width)]
}

/// A section's heading row alone: the label, then the rule out to `width`.
fn section_heading(label: &str, width: usize) -> Line<'static> {
    heading_line(vec![heading_label(label)], width)
}

fn heading_label(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

/// `spans`, a space, then a dim rule filling the rest of `width`.
fn heading_line(mut spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let used: usize = spans.iter().map(Span::width).sum::<usize>() + 1;
    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        "─".repeat(width.saturating_sub(used)),
        Style::default().fg(theme::BORDER_DIM()),
    ));
    Line::from(spans)
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

pub(crate) fn draw_calendar_link(frame: &mut Frame, state: &ProfileModalState, available: bool) {
    state.calendar_link.set(Rect::default());
    let popup = state.popup_area();
    if available && popup.width >= 24 && popup.height >= 4 {
        let area = Rect::new(popup.right() - 17, popup.bottom() - 2, 15, 1);
        frame.render_widget(
            Paragraph::new("c Open calendar")
                .style(Style::default().fg(theme::AMBER()).bg(theme::BG_CANVAS())),
            area,
        );
        state.calendar_link.set(area);
    }
}
