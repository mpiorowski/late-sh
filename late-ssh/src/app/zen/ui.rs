//! Rendering for both Zen pages. Every widget here is a thin frame around a
//! renderer another domain already owns (the bonsai canvas, the aquarium
//! reef, the pet box, the embedded room chat, the equalizer); what this
//! file adds is the composition and the chrome.

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

use super::{
    bigclock,
    layout::{self, BONSAI_STATUS_ROWS, FLOOR_ROWS},
    state::{BorderKind, TileKind, ZenState},
};
use late_core::models::aquarium_care::CARE_DAYS;
use late_core::models::user::{AudioSource, IcecastStream, RadioStation};

use crate::app::{
    audio::stations::{icecast_stream_display_name, radio_station_display_name},
    audio::viz::{EqState, render_eq},
    bonsai::{
        render::{PREVIEW_WIDTH, apply_sway, canvas_lines, center_lines, render_preview_lines},
        state::{BonsaiState, CANVAS_HEIGHT, CANVAS_WIDTH},
    },
    chat::ui::{EmbeddedRoomChatView, draw_embedded_room_chat},
    common::{primitives::hint_line, theme},
    files::terminal_image::TerminalImageFrame,
    hub::aquarium::state::{AquariumCare, AquariumState, CareBar},
    lobby::daily::{panel::draw_daily_compact, state::DailyState},
    pet::ui::{Neighbours, PetPose, PetView, WatchTarget, draw_pet_box},
};

/// A chat tile's frame: its room's label and its view (`None` when the
/// account has no room at all). The active tile's view carries the
/// composer and the selection; the others only watch (`render.rs`).
pub(crate) struct ZenChatTile<'a> {
    pub label: String,
    pub view: Option<EmbeddedRoomChatView<'a>>,
}

/// Everything the Zen page reads, assembled once per frame in `render.rs`.
pub(crate) struct ZenView<'a> {
    pub zen: &'a ZenState,
    pub bonsai: &'a BonsaiState,
    /// The reef is drawn for everyone; `aquarium_owned` says whether the
    /// account has fish in it or gets the shop caption instead.
    pub aquarium: &'a AquariumState,
    pub aquarium_owned: bool,
    /// The owner's care: the title bar's fourteen dots read off it.
    pub aquarium_care: &'a AquariumCare,
    /// `None` when the account owns no pet.
    pub pet_strip: Option<PetView<'a>>,
    /// One entry per chat tile, in layout order.
    pub chats: Vec<ZenChatTile<'a>>,
    pub track: String,
    /// The source and, for the streams that have one, the station it is
    /// tuned to (`station_text`).
    pub station: String,
    pub eq_state: EqState,
    pub clock: &'a str,
    pub date: String,
    pub online_count: usize,
    pub friends: &'a [String],
    pub status: Option<crate::app::common::status::Status>,
    pub mentions_unread: i64,
    /// Daily correspondence games for the lobby tile, and whether the lobby
    /// label glows (your turn somewhere, or a result waiting).
    pub daily: &'a DailyState,
    pub lobby_glow: bool,
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
    // One frame per chat tile in layout order. Zoomed, the one tile drawn
    // is the focused one, so it takes the active chat's frame, not the
    // first.
    let mut chats: Vec<Option<ZenChatTile<'_>>> = std::mem::take(&mut view.chats)
        .into_iter()
        .map(Some)
        .collect();
    let mut next_chat = 0usize;
    let zen = view.zen;
    let zoomed = zen.zoomed.then_some(zen.focus);
    let gap = zen.rice.look.gap as u16;
    let rects = layout::tile_rects(&zen.rice.root, tiles_area, gap, zoomed);
    // A pet tile sharing an edge with a tank or a bonsai tile: the pet
    // goes and sits against that edge to watch, and alternates when it
    // touches both (zoomed, a lone tile has no neighbour at all).
    let neighbours = layout::pet_neighbours(&rects, gap);
    for (idx, (kind, rect)) in rects.iter().enumerate() {
        let focused = if zoomed.is_some() {
            true
        } else {
            idx == zen.focus
        };
        // Each chat tile takes the next frame in layout order and names
        // its room in the title, so `[` `]` walking the rooms shows where
        // you landed without reading the messages.
        let chat_tile = match kind {
            TileKind::Chat => {
                let index = match zoomed {
                    Some(_) => zen.active_chat_index().unwrap_or(next_chat),
                    None => next_chat,
                };
                next_chat += 1;
                chats.get_mut(index).and_then(Option::take)
            }
            TileKind::Bonsai
            | TileKind::Aquarium
            | TileKind::Pet
            | TileKind::Music
            | TileKind::Clock
            | TileKind::Visualizer
            | TileKind::Presence
            | TileKind::Lobby
            | TileKind::Blank => None,
        };
        let title = match (kind, &chat_tile) {
            (TileKind::Chat, Some(tile)) => format!("{} · {}", kind.label(), tile.label),
            (TileKind::Chat, None) => kind.label().to_string(),
            (
                TileKind::Bonsai
                | TileKind::Aquarium
                | TileKind::Pet
                | TileKind::Music
                | TileKind::Clock
                | TileKind::Visualizer
                | TileKind::Presence
                | TileKind::Lobby
                | TileKind::Blank,
                _,
            ) => kind.label().to_string(),
        };
        // The tank's title carries its care bar: fourteen dots, green
        // for the feeding streak or red for the days unfed.
        let title_tail = match kind {
            TileKind::Aquarium if view.aquarium_owned => {
                Some(care_bar_spans(view.aquarium_care.bar()))
            }
            TileKind::Aquarium
            | TileKind::Chat
            | TileKind::Bonsai
            | TileKind::Pet
            | TileKind::Music
            | TileKind::Clock
            | TileKind::Visualizer
            | TileKind::Presence
            | TileKind::Lobby
            | TileKind::Blank => None,
        };
        let keys = tile_keys(*kind, &view);
        let inner = draw_tile_chrome(
            frame,
            *rect,
            &title,
            title_tail,
            keys,
            focused,
            &zen.rice.look,
        );
        if inner.width == 0 || inner.height == 0 {
            continue;
        }
        match kind {
            TileKind::Bonsai => draw_bonsai_tile(frame, inner, view.bonsai, view.wall_tick),
            TileKind::Aquarium => {
                draw_aquarium_tile(frame, inner, view.aquarium, view.aquarium_owned)
            }
            TileKind::Pet => draw_pet_tile(frame, inner, view.pet_strip.as_ref(), neighbours),
            TileKind::Chat => draw_chat_tile(frame, inner, chat_tile, terminal_images),
            TileKind::Music => draw_music_tile(frame, inner, &view),
            TileKind::Clock => draw_clock_tile(frame, inner, &view),
            TileKind::Visualizer => {
                draw_visualizer_tile(frame, inner, view.wall_tick, view.eq_state)
            }
            TileKind::Presence => draw_presence_tile(frame, inner, &view),
            TileKind::Lobby => draw_lobby_tile(frame, inner, view.daily, view.lobby_glow),
            TileKind::Blank => draw_blank_tile(frame, inner, focused),
        }
    }
    draw_rice_hint(frame, hint_area, zen);
    draw_kind_picker(frame, area, zen);
}

/// The tile picker `space` opens: one row per kind, centered over the
/// page, the picker's row marked, the tile's current kind named, and a
/// row the page refuses (Chat past the cap) drawn faint.
fn draw_kind_picker(frame: &mut Frame, area: Rect, zen: &ZenState) {
    let Some(selected) = zen.kind_picker else {
        return;
    };
    // Every kind, a blank, the hint, and the two border rows; wide enough
    // for the full hint, which `hint_line_fitting` trims on a narrow page.
    let height = (TileKind::ALL.len() as u16 + 4).min(area.height);
    let width = 36u16.min(area.width);
    if height < 4 || width < 12 {
        return;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" tile ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let current = zen.focused_kind();
    let mut lines: Vec<Line<'static>> = TileKind::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let picked = index == selected;
            let allowed = zen.kind_allowed(*kind);
            let label_style = if !allowed {
                Style::default().fg(theme::TEXT_FAINT())
            } else if picked {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let mut spans = vec![
                Span::styled(
                    if picked { "▌ " } else { "  " },
                    Style::default().fg(theme::AMBER()),
                ),
                Span::styled(format!("{:<12}", kind.label()), label_style),
            ];
            if current == Some(*kind) {
                spans.push(Span::styled(
                    "current",
                    Style::default().fg(theme::TEXT_DIM()),
                ));
            } else if !allowed {
                spans.push(Span::styled(
                    "full",
                    Style::default().fg(theme::TEXT_FAINT()),
                ));
            }
            Line::from(spans)
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(hint_line_fitting(
        &[("jk", "move"), ("enter", "pick"), ("esc", "close")],
        inner.width as usize,
    ));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The keys a tile answers to, named on the right of its title so the
/// page explains itself in one place per tile; `t` hides the titles and
/// the keys with them. The layout keys are the footer's.
fn tile_keys(kind: TileKind, view: &ZenView<'_>) -> &'static [(&'static str, &'static str)] {
    match kind {
        TileKind::Bonsai => &[("w", "tend")],
        TileKind::Aquarium if view.aquarium_owned => &[("a", "feed")],
        TileKind::Aquarium => &[],
        TileKind::Pet if view.pet_strip.is_some() => &[("click", "pet")],
        TileKind::Pet => &[],
        TileKind::Chat => &[("[ ] ctrl+/", "room"), ("i", "write")],
        TileKind::Music => &[
            ("m", "mute"),
            ("-=", "vol"),
            ("v x", "source"),
            ("v1-5", "tune"),
        ],
        TileKind::Lobby => &[("ctrl+g", "open"), ("`", "toggle")],
        TileKind::Clock | TileKind::Visualizer | TileKind::Presence => &[],
        TileKind::Blank => &[],
    }
}

/// Border, title, keys, and focus ring per the look; returns the tile's
/// inner area. The keys sit on the right of the title row when there is
/// room for them after the title and its tail.
fn draw_tile_chrome(
    frame: &mut Frame,
    rect: Rect,
    title: &str,
    title_tail: Option<Vec<Span<'static>>>,
    keys: &[(&str, &str)],
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
                let mut spans = vec![Span::styled(format!(" {title} "), title_style)];
                if let Some(tail) = title_tail {
                    spans.extend(tail);
                    spans.push(Span::raw(" "));
                }
                let left = Line::from(spans);
                // The corners take two cells; the keys need a cell of
                // border on each side of them to read as a second title.
                // Styled like the footer: the key amber, the word dim.
                let room = (rect.width as usize).saturating_sub(2 + left.width());
                let mut right = hint_line(keys);
                right.spans.push(Span::raw(" "));
                if !keys.is_empty() && right.width() <= room {
                    block = block.title(left).title(right.right_aligned());
                } else {
                    block = block.title(left);
                }
            }
            frame.render_widget(block, rect);
            layout::tile_inner(rect, look)
        }
        None => {
            if !look.titles {
                return rect;
            }
            let marker = if focused { "▌" } else { " " };
            let mut spans = vec![
                Span::styled(marker, ring),
                Span::styled(title.to_string(), title_style),
            ];
            if let Some(tail) = title_tail {
                spans.push(Span::raw(" "));
                spans.extend(tail);
            }
            let left = Line::from(spans);
            let room = (rect.width as usize).saturating_sub(left.width());
            let mut line_spans = left.spans;
            let right = hint_line(keys);
            if !keys.is_empty() && right.width() < room {
                line_spans.push(Span::raw(" ".repeat(room - right.width())));
                line_spans.extend(right.spans);
            }
            frame.render_widget(
                Paragraph::new(Line::from(line_spans)),
                Rect::new(rect.x, rect.y, rect.width, 1),
            );
            layout::tile_inner(rect, look)
        }
    }
}

/// The care bar: one dot per day of the fourteen both clocks run on.
/// Streak dots fill green, unfed dots red, and a minded tank (the
/// shield's auto feeder) shows all fourteen empty.
pub(crate) fn care_bar_spans(bar: CareBar) -> Vec<Span<'static>> {
    let (filled, color) = match bar {
        CareBar::Streak(days) => (days, theme::SUCCESS()),
        CareBar::Dry(days) => (days, theme::ERROR()),
        CareBar::Minded => (0, theme::TEXT_FAINT()),
    };
    let filled = filled.min(CARE_DAYS) as usize;
    let empty = CARE_DAYS as usize - filled;
    vec![
        Span::styled("●".repeat(filled), Style::default().fg(color)),
        Span::styled("○".repeat(empty), Style::default().fg(theme::TEXT_FAINT())),
    ]
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
    // The layout keys, the way out and the guide first, then by how often
    // they are used; the tail is dropped hint by hint on a narrow terminal
    // so nothing is cut in half. The tiles name their own keys.
    let head_width: usize = spans.iter().map(Span::width).sum();
    let hints = hint_line_fitting(
        &[
            ("Ctrl+F", "back"),
            ("?", "keys"),
            ("Tab ←→", "focus"),
            ("space", "kind"),
            ("S", "split"),
            ("X", "close"),
            ("z", "zoom"),
            ("<>{}", "resize"),
            ("r", "flip"),
            ("R", "reset"),
        ],
        (area.width as usize).saturating_sub(head_width),
    );
    spans.extend(hints.spans);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// `hint_line` with as many leading hints as fit in `width` cells.
fn hint_line_fitting(hints: &[(&str, &str)], width: usize) -> Line<'static> {
    let mut keep = hints.len();
    while keep > 0 && hint_line(&hints[..keep]).width() > width {
        keep -= 1;
    }
    hint_line(&hints[..keep])
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
        Paragraph::new(bonsai_status_line(state)).centered(),
        status_area,
    );
}

fn bonsai_status_line(state: &BonsaiState) -> Line<'static> {
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
    }
    Line::from(spans)
}

/// The tank. Without the shop unlock the tile reads like the pet's, a
/// centered note pointing at the shop; owned, it is the live reef, nothing
/// else: the sprout on its floor is tended on its Shop row.
fn draw_aquarium_tile(frame: &mut Frame, area: Rect, state: &AquariumState, owned: bool) {
    if !owned {
        draw_centered_note(frame, area, &["no tank yet", "the Aquarium is in /shop"]);
        return;
    }
    crate::app::hub::aquarium::ui::draw(frame, area, state);
}

/// The lobby in a tile, compact: the games running, then one footer row
/// with the open count and the keys. Top-aligned so a short tile shows the
/// games first.
fn draw_lobby_tile(frame: &mut Frame, area: Rect, daily: &DailyState, glow: bool) {
    draw_daily_compact(frame, area, daily, glow);
}

/// The pet's box at tile size: a name and mood row on top when there is
/// room, and the whole rest of the tile to roam. `neighbours` names the
/// side a tank and a bonsai are on; a calm pet goes and watches them.
fn draw_pet_tile(frame: &mut Frame, area: Rect, pet: Option<&PetView<'_>>, neighbours: Neighbours) {
    let Some(view) = pet else {
        draw_centered_note(
            frame,
            area,
            &["no pet yet", "the Pet Companion is in /shop"],
        );
        return;
    };
    if area.height < FLOOR_ROWS {
        return;
    }
    let box_area = if area.height >= FLOOR_ROWS + 2 {
        let state = view.state;
        let dim = Style::default().fg(theme::TEXT_DIM());
        let name = state
            .name
            .clone()
            .unwrap_or_else(|| state.species.as_str().to_string());
        let pose = PetPose::for_frame(
            state.mood(),
            neighbours,
            state.perch(),
            state.animation_ticks(),
        );
        let line = Line::from(vec![
            Span::styled(
                name,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" · {}", state.mood().as_str()), dim),
            Span::styled(
                match pose {
                    PetPose::Watch(WatchTarget::Tank, _) => " · watching the fish",
                    PetPose::Watch(WatchTarget::Bonsai, _) => " · watching the tree",
                    PetPose::At(_) => " · at your cursor",
                    PetPose::Stroll | PetPose::Sulk | PetPose::Sleep => "",
                },
                dim,
            ),
        ])
        .centered();
        frame.render_widget(
            Paragraph::new(line),
            Rect::new(area.x, area.y, area.width, 1),
        );
        Rect::new(area.x, area.y + 1, area.width, area.height - 1)
    } else {
        area
    };
    draw_pet_box(frame, box_area, view, neighbours);
}

/// A chat tile: its room's messages, with the composer on the active tile
/// only. `None` is a tile past the frames built this frame, which cannot
/// happen (one frame per chat tile) but is drawn as an empty note rather
/// than crashed on.
fn draw_chat_tile(
    frame: &mut Frame,
    area: Rect,
    tile: Option<ZenChatTile<'_>>,
    terminal_images: &mut TerminalImageFrame,
) {
    match tile {
        Some(ZenChatTile {
            view: Some(view), ..
        }) => {
            if area.height < 4 || area.width < 20 {
                return;
            }
            draw_embedded_room_chat(frame, area, view, terminal_images);
        }
        Some(ZenChatTile { view: None, .. }) | None => {
            draw_centered_note(frame, area, &["no room open", "1 opens Home to pick one"])
        }
    }
}

fn draw_music_tile(frame: &mut Frame, area: Rect, view: &ZenView<'_>) {
    if area.height < 2 {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    // Track and station, one row each, the station first to go when the
    // tile is short. The equalizer takes what is left, up to three. The
    // keys are on the title.
    let text_rows: u16 = area.height.min(2);
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
    let station = Line::from(Span::styled(view.station.clone(), dim)).centered();
    frame.render_widget(
        Paragraph::new(vec![track, station]),
        Rect::new(
            area.x,
            top + eq_rows,
            area.width,
            text_rows.min(area.height),
        ),
    );
}

/// The player's second row: the source, then the station or stream it is
/// tuned to. YouTube has no station, so it stays one word.
pub(crate) fn station_text(
    source: AudioSource,
    station: RadioStation,
    stream: IcecastStream,
) -> String {
    match source {
        AudioSource::Radio => format!("radio · {}", radio_station_display_name(station)),
        AudioSource::Icecast => format!("icecast · {}", icecast_stream_display_name(stream)),
        AudioSource::Youtube => "youtube".to_string(),
    }
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
                format!("{} · {} online", view.date, view.online_count),
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
    if let Some(status) = view.status {
        lines.push(Line::from(""));
        lines.push(
            Line::from(Span::styled(
                format!("{} {}", status.glyph(), status.word()),
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

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
