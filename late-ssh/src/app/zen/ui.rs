//! Rendering for both Zen pages. Every widget here is a thin frame around a
//! renderer another domain already owns (the bonsai canvas, the aquarium
//! reef, the pet strip, the embedded room chat, the equalizer); what this
//! file adds is the composition and the chrome.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

use super::{
    bigclock,
    layout::{self, BONSAI_STATUS_ROWS, FLOOR_ROWS},
    state::{BorderKind, TileKind, ZenState},
};
use crate::app::{
    audio::viz::{EqState, render_eq},
    bonsai::{
        render::{PREVIEW_WIDTH, apply_sway, canvas_lines, center_lines, render_preview_lines},
        state::{BonsaiState, CANVAS_HEIGHT, CANVAS_WIDTH},
    },
    chat::ui::{ComposerBlockView, EmbeddedRoomChatView, draw_embedded_room_chat},
    common::{primitives::hint_line, theme},
    files::terminal_image::TerminalImageFrame,
    hub::aquarium::state::AquariumState,
    pet::{
        state::PetState,
        ui::{PetStripView, draw_pet_strip},
    },
};
use late_core::models::chat_message::ChatMessage;

/// Everything a Zen page reads, assembled once per frame in `render.rs`.
pub(crate) struct ZenView<'a> {
    pub zen: &'a ZenState,
    pub bonsai: &'a BonsaiState,
    /// `None` when the account owns no aquarium.
    pub aquarium: Option<&'a AquariumState>,
    /// `None` when the account owns no pet.
    pub pet_strip: Option<PetStripView<'a>>,
    /// The pet itself, for the Room's floor; `None` when not owned.
    pub pet: Option<&'a PetState>,
    /// The current room's messages (the Room's monitor shows the tail).
    pub messages: &'a [ChatMessage],
    pub usernames: &'a crate::usernames::UsernameLookup<'a>,
    /// The Room's composer footer; `None` when there is no room to talk in.
    pub composer: Option<ComposerBlockView<'a>>,
    /// The current room, drawn at most once per frame (taken by the first
    /// chat surface that claims it).
    pub chat: Option<EmbeddedRoomChatView<'a>>,
    pub room_label: String,
    pub track: String,
    pub source_label: &'static str,
    pub eq_state: EqState,
    pub clock: &'a str,
    pub date: String,
    pub online_count: usize,
    pub friends: &'a [String],
    pub afk: Option<&'a str>,
    pub mentions_unread: i64,
    pub wall_tick: usize,
}

pub(crate) fn draw_rice(
    frame: &mut Frame,
    area: Rect,
    mut view: ZenView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.width < 40 || area.height < 12 {
        crate::app::common::primitives::draw_too_small(frame, area, "Rice", 40, 12);
        return;
    }
    let (tiles_area, hint_area) = layout::rice_areas(area);
    let had_chat = view.chat.is_some();
    let zen = view.zen;
    let zoomed = zen.zoomed.then_some(zen.focus);
    let rects = layout::tile_rects(&zen.rice.root, tiles_area, zen.rice.look.gap as u16, zoomed);
    for (idx, (kind, rect)) in rects.iter().enumerate() {
        let focused = if zoomed.is_some() {
            true
        } else {
            idx == zen.focus
        };
        let inner = draw_tile_chrome(frame, *rect, *kind, focused, &zen.rice.look);
        if inner.width == 0 || inner.height == 0 {
            continue;
        }
        match kind {
            TileKind::Bonsai => draw_bonsai_tile(frame, inner, view.bonsai, view.wall_tick),
            TileKind::Aquarium => draw_aquarium_tile(frame, inner, view.aquarium),
            TileKind::Pet => draw_pet_tile(frame, inner, view.pet_strip.as_ref()),
            TileKind::Chat => {
                let label = view.room_label.clone();
                draw_chat_tile(
                    frame,
                    inner,
                    view.chat.take(),
                    had_chat,
                    &label,
                    terminal_images,
                );
            }
            TileKind::Music => draw_music_tile(frame, inner, &view),
            TileKind::Clock => draw_clock_tile(frame, inner, &view),
            TileKind::Visualizer => {
                draw_visualizer_tile(frame, inner, view.wall_tick, view.eq_state)
            }
            TileKind::Presence => draw_presence_tile(frame, inner, &view),
            TileKind::Blank => draw_blank_tile(frame, inner, focused),
        }
    }
    draw_rice_hint(frame, hint_area, zen);
}

/// Border, title, and focus ring per the look; returns the tile's inner area.
fn draw_tile_chrome(
    frame: &mut Frame,
    rect: Rect,
    kind: TileKind,
    focused: bool,
    look: &super::state::Look,
) -> Rect {
    if rect.width == 0 || rect.height == 0 {
        return rect;
    }
    let ring = if focused {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::BORDER_DIM())
    };
    let title_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let border_type = match look.border {
        BorderKind::None => None,
        BorderKind::Plain => Some(BorderType::Plain),
        BorderKind::Rounded => Some(BorderType::Rounded),
        BorderKind::Double => Some(BorderType::Double),
        BorderKind::Thick => Some(BorderType::Thick),
    };
    match border_type {
        Some(border_type) => {
            let mut block = Block::default()
                .borders(Borders::ALL)
                .border_type(border_type)
                .border_style(ring);
            if look.titles {
                block = block.title(Span::styled(format!(" {} ", kind.label()), title_style));
            }
            frame.render_widget(block, rect);
            layout::tile_inner(rect, look)
        }
        None => {
            if !look.titles {
                return rect;
            }
            let marker = if focused { "▌" } else { " " };
            let line = Line::from(vec![
                Span::styled(marker, ring),
                Span::styled(kind.label(), title_style),
            ]);
            frame.render_widget(
                Paragraph::new(line),
                Rect::new(rect.x, rect.y, rect.width, 1),
            );
            layout::tile_inner(rect, look)
        }
    }
}

fn draw_rice_hint(frame: &mut Frame, area: Rect, zen: &ZenState) {
    if area.height == 0 {
        return;
    }
    let focus = zen.focused_kind().map(TileKind::label).unwrap_or("nothing");
    let mut spans = vec![
        Span::styled(" ▌ ", Style::default().fg(theme::AMBER())),
        Span::styled(
            focus.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if zen.zoomed { " zoomed" } else { "" },
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ];
    let hints = hint_line(&[
        ("←→", "focus"),
        ("space", "kind"),
        ("S", "split"),
        ("X", "close"),
        ("<>", "width"),
        ("{}", "height"),
        ("r", "flip"),
        ("z", "zoom"),
        ("b", "border"),
        ("g", "gap"),
        ("t", "titles"),
        ("R", "reset"),
        ("[]", "room"),
        ("i", "chat"),
        ("o", "the room"),
    ]);
    spans.extend(hints.spans);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The tree at its true size when the tile has the room, the preview
/// otherwise; pot on the floor, status row under it.
fn draw_bonsai_tile(frame: &mut Frame, area: Rect, state: &BonsaiState, wall_tick: usize) {
    if area.height < 4 || area.width < PREVIEW_WIDTH as u16 {
        return;
    }
    let tree_area = Rect::new(
        area.x,
        area.y,
        area.width,
        area.height.saturating_sub(BONSAI_STATUS_ROWS),
    );
    let full = tree_area.width >= CANVAS_WIDTH as u16 && tree_area.height >= CANVAS_HEIGHT as u16;
    let (mut lines, block_width) = if full {
        (canvas_lines(state, true), CANVAS_WIDTH)
    } else {
        (render_preview_lines(state), PREVIEW_WIDTH)
    };
    apply_sway(&mut lines, wall_tick);
    center_lines(&mut lines, tree_area.width as usize, block_width);
    let visible = lines.len().min(tree_area.height as usize);
    let dropped = lines.len() - visible;
    let mut lines: Vec<Line<'static>> = lines.into_iter().skip(dropped).collect();
    let top_pad = (tree_area.height as usize).saturating_sub(lines.len());
    let mut padded = Vec::with_capacity(top_pad + lines.len());
    for _ in 0..top_pad {
        padded.push(Line::from(""));
    }
    padded.append(&mut lines);
    frame.render_widget(Paragraph::new(padded), tree_area);

    let status_area = Rect::new(area.x, tree_area.bottom(), area.width, BONSAI_STATUS_ROWS);
    frame.render_widget(
        Paragraph::new(bonsai_status_line(state, area.width >= 100)).centered(),
        status_area,
    );
}

fn bonsai_status_line(state: &BonsaiState, wide: bool) -> Line<'static> {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let dot = || Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT()));
    let (label, color) = if !state.is_alive {
        ("rip", theme::ERROR())
    } else if state.water_stress >= 60 {
        ("dry", theme::ERROR())
    } else if state.water_stress >= 25 {
        ("watch", theme::AMBER())
    } else {
        ("alive", theme::SUCCESS())
    };
    let mut spans = vec![
        Span::styled(format!("day {}", state.age_days), dim),
        dot(),
        Span::styled(
            format!("vigor {}", state.vigor),
            Style::default().fg(theme::SUCCESS()),
        ),
        dot(),
        Span::styled(
            format!("stress {}", state.water_stress),
            Style::default().fg(color),
        ),
        dot(),
        Span::styled(label, Style::default().fg(color)),
    ];
    if let Some(message) = state.message.as_deref() {
        spans.push(dot());
        spans.push(Span::styled(
            message.to_string(),
            Style::default().fg(theme::AMBER()),
        ));
    } else if wide {
        spans.push(dot());
        spans.push(Span::styled(
            "w water · n branch · hjkl steer · x cut · p pinch · s split",
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    Line::from(spans)
}

fn draw_aquarium_tile(frame: &mut Frame, area: Rect, state: Option<&AquariumState>) {
    match state {
        Some(state) => crate::app::hub::aquarium::ui::draw(frame, area, state),
        None => draw_centered_note(frame, area, &["no aquarium in this room", "/shop has one"]),
    }
}

fn draw_pet_tile(frame: &mut Frame, area: Rect, pet: Option<&PetStripView<'_>>) {
    let Some(view) = pet else {
        draw_centered_note(frame, area, &["no pet in this room", "/shop has one"]);
        return;
    };
    if area.height < FLOOR_ROWS {
        return;
    }
    let strip = Rect::new(
        area.x,
        area.bottom().saturating_sub(FLOOR_ROWS),
        area.width,
        FLOOR_ROWS,
    );
    if area.height >= FLOOR_ROWS + 2 {
        let state = view.state;
        let dim = Style::default().fg(theme::TEXT_DIM());
        let name = state.name.clone().unwrap_or_else(|| state.species.clone());
        let line = Line::from(vec![
            Span::styled(
                name,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · {} · {}", state.mood().label(), state.age_label()),
                dim,
            ),
        ])
        .centered();
        let above = Rect::new(area.x, area.y, area.width, area.height - FLOOR_ROWS);
        let pad = above.height.saturating_sub(1) / 2;
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(above.x, above.y + pad, above.width, 1),
        );
    }
    draw_pet_strip(frame, strip, view);
}

fn draw_chat_tile(
    frame: &mut Frame,
    area: Rect,
    chat: Option<EmbeddedRoomChatView<'_>>,
    had_chat: bool,
    room_label: &str,
    terminal_images: &mut TerminalImageFrame,
) {
    match chat {
        Some(view) => {
            if area.height < 4 || area.width < 20 {
                return;
            }
            draw_embedded_room_chat(frame, area, view, terminal_images);
        }
        None if had_chat => {
            draw_centered_note(frame, area, &[room_label, "chat already has a tile"])
        }
        None => draw_centered_note(frame, area, &["no room open", "1 opens Home to pick one"]),
    }
}

fn draw_music_tile(frame: &mut Frame, area: Rect, view: &ZenView<'_>) {
    if area.height < 2 {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let text_rows: u16 = 2;
    let eq_rows = area.height.saturating_sub(text_rows).min(3);
    let total = eq_rows + text_rows;
    let top = area.y + area.height.saturating_sub(total) / 2;
    if eq_rows > 0 {
        render_eq(
            frame,
            Rect::new(area.x, top, area.width, eq_rows),
            view.wall_tick,
            view.eq_state,
        );
    }
    let track = Line::from(vec![
        Span::styled("♪ ", Style::default().fg(theme::AMBER())),
        Span::styled(
            view.track.clone(),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
    ])
    .centered();
    let controls = Line::from(vec![
        Span::styled(view.source_label, dim),
        Span::styled(" · m mute · -= vol · v+x source", faint),
    ])
    .centered();
    frame.render_widget(
        Paragraph::new(vec![track, controls]),
        Rect::new(
            area.x,
            top + eq_rows,
            area.width,
            text_rows.min(area.height),
        ),
    );
}

fn draw_clock_tile(frame: &mut Frame, area: Rect, view: &ZenView<'_>) {
    let width = area.width as usize;
    let height = area.height as usize;
    let date_rows = if height >= 7 { 2 } else { 0 };
    // Only the digits go big; the zone prefix rides the date row.
    let digits: String = view
        .clock
        .chars()
        .filter(|ch| ch.is_ascii_digit() || *ch == ':')
        .collect();
    let mut lines: Vec<Line<'static>> =
        match bigclock::fitting_scale(&digits, width, height.saturating_sub(date_rows)) {
            Some(scale) => {
                let block_width = bigclock::width_for(&digits, scale);
                let mut lines = bigclock::render(&digits, scale);
                for line in &mut lines {
                    line.spans.iter_mut().for_each(|span| {
                        span.style = Style::default().fg(theme::AMBER_GLOW());
                    });
                }
                center_lines(&mut lines, width, block_width);
                lines
            }
            None => vec![
                Line::from(Span::styled(
                    view.clock.to_string(),
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD),
                ))
                .centered(),
            ],
        };
    if date_rows > 0 {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                format!("{} · {}", view.date, view.clock),
                Style::default().fg(theme::TEXT_DIM()),
            ))
            .centered(),
        );
    }
    let top_pad = height.saturating_sub(lines.len()) / 2;
    let mut padded = Vec::with_capacity(top_pad + lines.len());
    for _ in 0..top_pad {
        padded.push(Line::from(""));
    }
    padded.append(&mut lines);
    frame.render_widget(Paragraph::new(padded), area);
}

const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

fn bar_phase(seed: usize) -> f32 {
    let hashed = (seed as u32).wrapping_mul(2_654_435_761);
    (hashed >> 8) as f32 / (1u32 << 24) as f32 * std::f32::consts::TAU
}

/// Bar height in 0..=1 for one animation frame: the sidebar band's recipe
/// at whatever height the tile allows.
fn bar_unit(bar: usize, bars: usize, anim_frame: usize) -> f32 {
    let t = anim_frame as f32;
    let fast = (t * 0.51 + bar_phase(bar)).sin();
    let slow = (t * 0.173 + bar_phase(bar + 101)).sin();
    let swell = (t * 0.071 - bar as f32 * 0.9).sin();
    let position = bar as f32 / bars.max(1) as f32;
    let envelope = 1.0 - 0.35 * position;
    ((0.42 + 0.30 * fast + 0.18 * slow + 0.10 * swell).max(0.04) * envelope).clamp(0.02, 1.0)
}

fn draw_visualizer_tile(frame: &mut Frame, area: Rect, wall_tick: usize, eq_state: EqState) {
    if area.width < 2 || area.height == 0 {
        return;
    }
    if eq_state != EqState::Playing {
        render_eq(frame, area, wall_tick, eq_state);
        return;
    }
    let anim_frame = wall_tick / 2;
    let width = area.width as usize;
    let height = area.height as usize;
    let bars = width.div_ceil(2);
    let subcells = height * 8;
    let levels: Vec<usize> = (0..bars)
        .map(|b| ((bar_unit(b, bars, anim_frame) * subcells as f32) as usize).clamp(1, subcells))
        .collect();
    let mut lines = Vec::with_capacity(height);
    for row in 0..height {
        // Row 0 is the top; a cell is full when the bar reaches past it.
        let cell_bottom = (height - 1 - row) * 8;
        let mut spans = Vec::with_capacity(width);
        let mut text = String::with_capacity(width);
        for col in 0..width {
            if col % 2 == 1 {
                text.push(' ');
                continue;
            }
            let level = levels[col / 2];
            let filled = level.saturating_sub(cell_bottom).min(8);
            text.push(BLOCKS[filled]);
        }
        let position = row as f32 / height.max(1) as f32;
        let color = if position < 0.25 {
            theme::AMBER_GLOW()
        } else if position < 0.6 {
            theme::AMBER()
        } else {
            theme::AMBER_DIM()
        };
        spans.push(Span::styled(text, Style::default().fg(color)));
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_presence_tile(frame: &mut Frame, area: Rect, view: &ZenView<'_>) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                view.online_count.to_string(),
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" online", dim),
        ])
        .centered(),
    ];
    if !view.friends.is_empty() {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                view.friends.join(" · "),
                Style::default().fg(theme::SUCCESS()),
            ))
            .centered(),
        );
    }
    if let Some(afk) = view.afk {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                format!("brb: {afk}"),
                Style::default().fg(theme::AMBER()),
            ))
            .centered(),
        );
    }
    if view.mentions_unread > 0 {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                format!("@{} unread", view.mentions_unread),
                Style::default().fg(theme::MENTION()),
            ))
            .centered(),
        );
    }
    let top_pad = (area.height as usize).saturating_sub(lines.len()) / 2;
    let mut padded = Vec::with_capacity(top_pad + lines.len());
    for _ in 0..top_pad {
        padded.push(Line::from(""));
    }
    padded.append(&mut lines);
    frame.render_widget(Paragraph::new(padded), area);
}

fn draw_blank_tile(frame: &mut Frame, area: Rect, focused: bool) {
    if focused {
        draw_centered_note(frame, area, &["space picks what lives here"]);
    }
}

fn draw_centered_note(frame: &mut Frame, area: Rect, rows: &[&str]) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut lines: Vec<Line<'static>> = rows
        .iter()
        .map(|row| Line::from(Span::styled(row.to_string(), faint)))
        .collect();
    let top_pad = (area.height as usize).saturating_sub(lines.len()) / 2;
    let mut padded = Vec::with_capacity(top_pad + lines.len());
    for _ in 0..top_pad {
        padded.push(Line::from(""));
    }
    padded.append(&mut lines);
    frame.render_widget(Paragraph::new(padded).alignment(Alignment::Center), area);
}
