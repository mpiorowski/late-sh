//! The profile modal: one layout for every terminal, a single column that
//! scrolls.
//!
//! The body is composed off-screen into a buffer as tall as the content,
//! then the visible rows are blitted into the frame. That is what lets the
//! aquarium (a live widget that paints cells) sit in the middle of a
//! scrolling text column without a second layout for small screens: every
//! section takes exactly the rows it needs, and nothing is ever cut.
//!
//! Top to bottom: late.fetch (the fact grid in the left half, the bonsai as
//! the neofetch logo in the right half, the tree scaled to the grid's
//! height), bio, showcases, badges (all of them, always), the aquarium, and
//! the chips ledger. The same order on every screen; the only reflow is the
//! hero stacking when the column is too narrow for two halves.

use chrono::Utc;
use late_core::models::chat_message_gild::{GildCounts, GildTier};
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
    bonsai::{state::stage_for, ui::render_tree_art_lines},
    bonsai_v2::render::render_preview_lines,
    chat::showcase::svc::ShowcaseFeedItem,
    common::{markdown::render_body_to_lines, theme, time::timezone_current_time},
    hub::aquarium::{state::AquariumState, ui as aquarium_ui},
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
/// The hero is two equal halves when the body is at least this wide: the
/// left half has to hold the chips row, the widest fact.
const HERO_SIDE_BY_SIDE_MIN_WIDTH: u16 = 90;
/// The hero is never shorter than this: a short fact grid must not squash
/// the tree, which is the one thing on the card that is a picture.
const HERO_MIN_HEIGHT: usize = 14;
/// The reef band: the tallest creature plus the surface and floor rows.
const AQUARIUM_HEIGHT: u16 = 11;

/// One stretch of the body. Each knows its height, so the column can be
/// measured before it is painted.
enum Segment {
    Text(Vec<Line<'static>>),
    Hero {
        art: Vec<Line<'static>>,
        grid: Vec<Line<'static>>,
        side_by_side: bool,
    },
    Aquarium,
}

impl Segment {
    fn height(&self) -> u16 {
        match self {
            Segment::Text(lines) => lines.len() as u16,
            Segment::Hero {
                art,
                grid,
                side_by_side,
            } => {
                if *side_by_side {
                    art.len().max(grid.len()) as u16
                } else {
                    (art.len() + 1 + grid.len()) as u16
                }
            }
            Segment::Aquarium => AQUARIUM_HEIGHT,
        }
    }
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &ProfileModalState) {
    let width = area.width.saturating_sub(4).clamp(MIN_WIDTH, MAX_WIDTH);
    let body_width = width.saturating_sub(2 + SIDE_MARGIN * 2);

    let (segments, chips_top) = build_segments(state, body_width);
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
fn build_segments(state: &ProfileModalState, width: u16) -> (Vec<Segment>, Option<u16>) {
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

    // ── late.fetch: the grid as the info column, the bonsai as the logo ──
    // The tree is fitted to the grid's height, so the hero is exactly as
    // tall as the facts and never a column of air beside them.
    let side_by_side = width >= HERO_SIDE_BY_SIDE_MIN_WIDTH;
    let grid = late_fetch_lines(state, profile);
    let art_width = if side_by_side { width / 2 } else { width };
    let art = bonsai_block(state, art_width as usize, grid.len().max(HERO_MIN_HEIGHT));
    let mut heading = section_lines("late.fetch", width_usize);
    heading.remove(0); // the row under the border already breathes
    segments.push(Segment::Text(heading));
    segments.push(Segment::Hero {
        art,
        grid,
        side_by_side,
    });

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
    let showcases = state.showcases_for_viewed();
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

    // ── aquarium ──
    if !state.aquarium_fish().is_empty() {
        segments.push(Segment::Text(section_lines("aquarium", width_usize)));
        segments.push(Segment::Aquarium);
    }

    // ── chips ──
    let chips_top = segments.iter().map(Segment::height).sum::<u16>();
    let mut lines = section_lines("chips", width_usize);
    lines.push(ledger::summary_line(
        state.chip_balance(),
        state.chips_earned_month(),
    ));
    lines.push(ledger::off_board_note());
    lines.push(Line::from(""));
    if state.chip_ledger().is_empty() {
        lines.push(Line::from(Span::styled("no chips moved yet", dim)));
    }
    for entry in state.chip_ledger() {
        lines.push(ledger::row_line(
            entry,
            width_usize,
            |id| state.ledger_username(id).map(str::to_string),
            |message_id| state.ledger_gild(message_id).cloned(),
        ));
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
            Segment::Hero {
                art,
                grid,
                side_by_side,
            } => {
                if *side_by_side {
                    let half = width / 2;
                    let grid_area = Rect {
                        width: half,
                        ..area
                    };
                    let art_area = Rect {
                        x: half,
                        width: width - half,
                        ..area
                    };
                    Paragraph::new(art.clone()).render(art_area, &mut buf);
                    Paragraph::new(grid.clone()).render(grid_area, &mut buf);
                } else {
                    let art_area = Rect {
                        height: art.len() as u16,
                        ..area
                    };
                    let grid_area = Rect {
                        y: area.y + art.len() as u16 + 1,
                        height: grid.len() as u16,
                        ..area
                    };
                    Paragraph::new(art.clone()).render(art_area, &mut buf);
                    Paragraph::new(grid.clone()).render(grid_area, &mut buf);
                }
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
                aquarium.set_active_creatures(state.aquarium_fish());
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

/// The bonsai as exactly `height` rows, the pot on the last one. A Dynamic
/// Bonsai is scaled down to fit (never up); the classic sprite is cropped
/// from the crown so the pot and trunk stay.
fn bonsai_block(state: &ProfileModalState, width: usize, height: usize) -> Vec<Line<'static>> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let placeholder = |text: &str| vec![Line::from(Span::styled(text.to_string(), dim)).centered()];

    let mut tree = if state.dynamic_bonsai_selected() {
        match state.bonsai_v2() {
            Some(bonsai) => render_preview_lines(bonsai, width, height),
            None => placeholder("Dynamic Bonsai not planted yet"),
        }
    } else if let Some(tree) = state.bonsai() {
        let stage = stage_for(tree.is_alive, tree.growth_points);
        let age_days = (Utc::now().date_naive() - tree.created.date_naive())
            .num_days()
            .max(0);
        let wilting = tree.is_alive
            && tree
                .last_watered
                .map(|last| (Utc::now().date_naive() - last).num_days() >= 2)
                .unwrap_or(age_days >= 2);
        // Wall tick 0: the profile preview stays still (sin(0) sway).
        render_tree_art_lines(stage, tree.seed, wilting, width, 0, None)
    } else {
        placeholder("no bonsai yet")
    };

    if tree.len() > height {
        tree.drain(0..tree.len() - height);
    }
    bottom_pad(tree, height)
}

/// Pad `lines` with blank rows on top until they are `height` tall.
fn bottom_pad(mut lines: Vec<Line<'static>>, height: usize) -> Vec<Line<'static>> {
    let top_pad = height.saturating_sub(lines.len());
    let mut out = vec![Line::from(""); top_pad];
    out.append(&mut lines);
    out
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
    let mut chips = Vec::new();
    match state.chip_balance() {
        Some(balance) => chips.push(Span::styled(ledger::thousands(balance), bright)),
        None => chips.push(Span::styled("…".to_string(), dim)),
    }
    let earned = state.chips_earned_month();
    let earned_style = match earned.signum() {
        1 => Style::default().fg(theme::SUCCESS()),
        -1 => Style::default().fg(theme::ERROR()),
        _ => dim,
    };
    chips.push(Span::styled(
        "  ·  ",
        Style::default().fg(theme::BORDER_DIM()),
    ));
    chips.push(Span::styled(
        format!(
            "{}{} this month",
            if earned > 0 { "+" } else { "" },
            ledger::thousands(earned)
        ),
        earned_style,
    ));
    lines.push(row("chips", chips));

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

fn showcase_markdown(item: &ShowcaseFeedItem) -> String {
    let s = &item.showcase;
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
