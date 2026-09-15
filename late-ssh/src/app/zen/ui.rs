//! Rendering for both Zen pages. Every widget here is a thin frame around a
//! renderer another domain already owns (the bonsai canvas, the aquarium
//! reef, the pet box, the embedded room chat, the equalizer); what this
//! file adds is the composition and the chrome.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use uuid::Uuid;

use super::{
    bigclock,
    layout::{self, BONSAI_STATUS_ROWS, FLOOR_ROWS},
    rows::{Headline, InboxRow},
    state::{BorderKind, TileKind, ZenState},
};
use late_core::models::aquarium_care::CARE_DAYS;
use late_core::models::user::{AudioSource, IcecastStream, RadioStation};

use crate::app::{
    audio::stations::{icecast_stream_display_name, radio_station_display_name},
    audio::viz::{Dance, EqState, dance_lines, render_eq},
    bonsai::{
        render::{PREVIEW_WIDTH, apply_sway, canvas_lines, center_lines, render_preview_lines},
        state::{BonsaiState, CANVAS_HEIGHT, CANVAS_WIDTH},
    },
    chat::state::{ActiveFriend, ActivityTickerEntry},
    chat::ui::{EmbeddedRoomChatView, draw_embedded_room_chat},
    common::{
        primitives::{format_relative_time_short, hint_line},
        theme,
    },
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
    pub mentions_unread: i64,
    /// Daily correspondence games for the lobby tile, and whether the lobby
    /// label glows (your turn somewhere, or a result waiting).
    pub daily: &'a DailyState,
    pub lobby_glow: bool,
    /// The #lounge activity feed, newest first (`ChatState::activity_ticker`).
    pub activity: &'a [ActivityTickerEntry],
    pub active_friends: &'a [ActiveFriend],
    /// Per-peer `/status` badges, for the Friends tile.
    pub peer_statuses: &'a HashMap<Uuid, String>,
    /// The viewer's chips and today's care, for the Pulse tile.
    pub chip_balance: i64,
    pub care: Care,
    /// Built only while an Inbox or Headlines tile is on the page.
    pub inbox: Vec<InboxRow>,
    pub headlines: Vec<Headline>,
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
            | TileKind::Visualizer            | TileKind::Lobby
            | TileKind::Activity
            | TileKind::Friends
            | TileKind::Pulse
            | TileKind::Inbox
            | TileKind::Headlines
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
                | TileKind::Visualizer                | TileKind::Lobby
                | TileKind::Activity
                | TileKind::Friends
                | TileKind::Pulse
                | TileKind::Inbox
                | TileKind::Headlines
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
            | TileKind::Visualizer            | TileKind::Lobby
            | TileKind::Activity
            | TileKind::Friends
            | TileKind::Pulse
            | TileKind::Inbox
            | TileKind::Headlines
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
            TileKind::Music => draw_music_tile(
                frame,
                inner,
                &view.track,
                &view.station,
                view.wall_tick,
                view.eq_state,
            ),
            TileKind::Clock => draw_clock_tile(frame, inner, &view),
            TileKind::Visualizer => {
                draw_visualizer_tile(frame, inner, view.wall_tick, view.eq_state)
            }
            TileKind::Lobby => draw_lobby_tile(frame, inner, view.daily, view.lobby_glow),
            TileKind::Activity => {
                draw_activity_tile(frame, inner, view.activity, view.active_friends)
            }
            TileKind::Friends => {
                draw_friends_tile(frame, inner, view.active_friends, view.peer_statuses)
            }
            TileKind::Pulse => draw_pulse_tile(
                frame,
                inner,
                &PulseView {
                    online: view.online_count,
                    chips: view.chip_balance,
                    mentions: view.mentions_unread,
                    friends: view.active_friends.len(),
                    care: view.care,
                },
            ),
            TileKind::Inbox => {
                draw_inbox_tile(frame, inner, &view.inbox, zen.inbox_selected, focused)
            }
            TileKind::Headlines => draw_headlines_tile(frame, inner, &view.headlines),
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
    // A short page shows a window of the kinds that follows the selection;
    // the blank and the hint keep their two rows.
    let visible = (inner.height as usize).saturating_sub(2).max(1);
    let first = (selected + 1).saturating_sub(visible);
    let mut lines: Vec<Line<'static>> = TileKind::ALL
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
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
        TileKind::Inbox => &[("jk", "pick"), ("enter", "open")],
        TileKind::Clock
        | TileKind::Visualizer        | TileKind::Activity
        | TileKind::Friends
        | TileKind::Pulse
        | TileKind::Headlines => &[],
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

/// Track and station always sit on the tile's last two rows, the station
/// first to go when the tile is a single row. The visualizer takes every
/// row above them. The keys are on the title.
fn draw_music_tile(
    frame: &mut Frame,
    area: Rect,
    track: &str,
    station: &str,
    wall_tick: usize,
    eq_state: EqState,
) {
    if area.height == 0 {
        return;
    }
    let text_rows: u16 = area.height.min(2);
    let eq_rows = area.height - text_rows;
    if eq_rows > 0 {
        draw_visualizer_tile(
            frame,
            Rect::new(area.x, area.y, area.width, eq_rows),
            wall_tick,
            eq_state,
        );
    }
    let track = Line::from(vec![
        Span::styled("♪ ", Style::default().fg(theme::AMBER())),
        Span::styled(track.to_string(), Style::default().fg(theme::TEXT_BRIGHT())),
    ])
    .centered();
    let station =
        Line::from(Span::styled(station.to_string(), Style::default().fg(theme::TEXT_DIM())))
            .centered();
    frame.render_widget(
        Paragraph::new(vec![track, station]),
        Rect::new(area.x, area.y + eq_rows, area.width, text_rows),
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
                view.date.clone(),
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

/// The visualizer tile is the equalizer at the tile's full height: the
/// same bars and peak caps as the music stage, only taller.
fn draw_visualizer_tile(frame: &mut Frame, area: Rect, wall_tick: usize, eq_state: EqState) {
    if area.width < 2 || area.height == 0 {
        return;
    }
    let dance = match eq_state {
        EqState::Live(live) => Dance::Live(live),
        EqState::Ambient => Dance::Ambient,
        EqState::Muted | EqState::Unpaired => {
            render_eq(frame, area, wall_tick, eq_state);
            return;
        }
    };
    let lines = dance_lines(dance, wall_tick, area.width as usize, area.height as usize);
    frame.render_widget(Paragraph::new(lines), area);
}

/// The #lounge activity feed as a list: newest on top, one event a row with
/// its age flush right, a friend's line in the friend color.
fn draw_activity_tile(
    frame: &mut Frame,
    area: Rect,
    entries: &[ActivityTickerEntry],
    friends: &[ActiveFriend],
) {
    if entries.is_empty() {
        draw_centered_note(frame, area, &["quiet for now", "wins, joins, and crowns land here"]);
        return;
    }
    let width = area.width as usize;
    let lines: Vec<Line<'static>> = entries
        .iter()
        .take(area.height as usize)
        .map(|entry| {
            let about_a_friend = friends
                .iter()
                .any(|friend| names_actor(&entry.text, &friend.username));
            let style = if about_a_friend {
                Style::default().fg(theme::SUCCESS())
            } else {
                Style::default().fg(theme::TEXT_DIM())
            };
            stamped_row(
                vec![Span::styled(entry.text.clone(), style)],
                format_relative_time_short(entry.at),
                width,
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// Whether a feed line is about `username`: feed lines open with the name.
fn names_actor(text: &str, username: &str) -> bool {
    text.strip_prefix(username)
        .is_some_and(|rest| rest.starts_with(' '))
}

/// Connected friends, the most recent login first: the name, their
/// `/status` when set, their audio source, and how long they have been on.
fn draw_friends_tile(
    frame: &mut Frame,
    area: Rect,
    friends: &[ActiveFriend],
    peer_statuses: &HashMap<Uuid, String>,
) {
    if friends.is_empty() {
        draw_centered_note(frame, area, &["no friends online"]);
        return;
    }
    let width = area.width as usize;
    let now = Instant::now();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let lines: Vec<Line<'static>> = friends
        .iter()
        .take(area.height as usize)
        .map(|friend| {
            let mut spans = vec![
                Span::styled("● ", Style::default().fg(theme::SUCCESS())),
                Span::styled(
                    friend.username.clone(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
            ];
            if let Some(badge) = peer_statuses.get(&friend.user_id) {
                spans.push(Span::styled(
                    format!("  {badge}"),
                    Style::default().fg(theme::AMBER()),
                ));
            }
            spans.push(Span::styled(
                format!("  ♪ {}", audio_source_word(friend.audio_source)),
                dim,
            ));
            stamped_row(
                spans,
                compact_elapsed(now.saturating_duration_since(friend.online_since)),
                width,
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn audio_source_word(source: AudioSource) -> &'static str {
    match source {
        AudioSource::Youtube => "youtube",
        AudioSource::Radio => "radio",
        AudioSource::Icecast => "icecast",
    }
}

fn compact_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        "now".to_string()
    } else if secs < 3_600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3_600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

/// What the Pulse tile draws.
pub(crate) struct PulseView {
    pub online: usize,
    pub chips: i64,
    pub mentions: i64,
    pub friends: usize,
    pub care: Care,
}

/// Today's care for one companion: tended, still due, or not owned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chore {
    Done,
    Due,
    NotOwned,
}

impl Chore {
    pub(crate) fn of(owned: bool, done_today: bool) -> Self {
        match (owned, done_today) {
            (false, _) => Chore::NotOwned,
            (true, true) => Chore::Done,
            (true, false) => Chore::Due,
        }
    }
}

/// The three once-a-day chores Pulse's care row names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Care {
    pub bonsai: Chore,
    pub tank: Chore,
    pub pet: Chore,
}

/// The numbers worth a glance, one `label  value` row each and every row
/// always drawn, zero included: people online, your chips, unread mentions
/// (the mention color once there are some), friends online, and today's
/// care, each companion green once tended, amber while due, faint when not
/// owned. A short tile keeps the top rows; the block sits centered.
fn draw_pulse_tile(frame: &mut Frame, area: Rect, pulse: &PulseView) {
    use crate::app::common::primitives::thousands;

    let bright = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let mentions_style = if pulse.mentions > 0 {
        Style::default()
            .fg(theme::MENTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let chore = |name: &'static str, chore: Chore| {
        let color = match chore {
            Chore::Done => theme::SUCCESS(),
            Chore::Due => theme::AMBER(),
            Chore::NotOwned => theme::TEXT_FAINT(),
        };
        Span::styled(name, Style::default().fg(color))
    };
    let rows: [(&str, Vec<Span<'static>>); 5] = [
        ("online", vec![Span::styled(pulse.online.to_string(), bright)]),
        ("chips", vec![Span::styled(thousands(pulse.chips), bright)]),
        (
            "mentions",
            vec![Span::styled(pulse.mentions.to_string(), mentions_style)],
        ),
        ("friends", vec![Span::styled(pulse.friends.to_string(), bright)]),
        (
            "care",
            vec![
                chore("bonsai", pulse.care.bonsai),
                Span::raw(" "),
                chore("tank", pulse.care.tank),
                Span::raw(" "),
                chore("pet", pulse.care.pet),
            ],
        ),
    ];

    let width = area.width as usize;
    let height = area.height as usize;
    let top_pad = height.saturating_sub(rows.len()) / 2;
    let mut lines: Vec<Line<'static>> = vec![Line::from(""); top_pad];
    lines.extend(
        rows.into_iter()
            .take(height)
            .map(|(label, value)| stat_row(label, value, width)),
    );
    frame.render_widget(Paragraph::new(lines), area);
}

/// `label` dim on the left, `value` flush right; the label gives way first
/// when the tile is too narrow for both.
fn stat_row(label: &str, value: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let value_width: usize = value.iter().map(|span| span.content.width()).sum();
    let label = fit(label, width.saturating_sub(value_width + 1));
    let pad = width.saturating_sub(label.width() + value_width);
    let mut spans = vec![
        Span::styled(label, Style::default().fg(theme::TEXT_DIM())),
        Span::raw(" ".repeat(pad)),
    ];
    spans.extend(value);
    Line::from(spans)
}

/// Unread DMs, then mentions. The focused tile marks its selected row;
/// Enter opens that row in the page's first chat tile (`input.rs`).
fn draw_inbox_tile(
    frame: &mut Frame,
    area: Rect,
    rows: &[InboxRow],
    selected: usize,
    focused: bool,
) {
    if rows.is_empty() {
        draw_centered_note(frame, area, &["all caught up", "mentions and unread DMs land here"]);
        return;
    }
    let width = area.width as usize;
    let visible = area.height as usize;
    let selected = selected.min(rows.len() - 1);
    let first = (selected + 1).saturating_sub(visible);
    let amber = Style::default().fg(theme::AMBER());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let lines: Vec<Line<'static>> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, row)| {
            let marked = focused && index == selected;
            let marker = Span::styled(if marked { "▌ " } else { "  " }, amber);
            let emphasis = |style: Style| {
                if marked {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                }
            };
            match row {
                InboxRow::Dm { peer, unread, .. } => stamped_row(
                    vec![
                        marker,
                        Span::styled("✉ ", amber),
                        Span::styled(
                            format!("@{peer}"),
                            emphasis(Style::default().fg(theme::TEXT_BRIGHT())),
                        ),
                    ],
                    format!("{unread} new"),
                    width,
                ),
                InboxRow::Mention {
                    actor,
                    room,
                    preview,
                    at,
                    unread,
                    ..
                } => {
                    let (name_color, preview_color) = if *unread {
                        (theme::MENTION(), theme::TEXT())
                    } else {
                        (theme::TEXT_DIM(), theme::TEXT_DIM())
                    };
                    let place = match room {
                        Some(slug) => format!(" in #{slug}: "),
                        None => ": ".to_string(),
                    };
                    stamped_row(
                        vec![
                            marker,
                            Span::styled(
                                format!("@{actor}"),
                                emphasis(Style::default().fg(name_color)),
                            ),
                            Span::styled(place, faint),
                            Span::styled(preview.clone(), Style::default().fg(preview_color)),
                        ],
                        format_relative_time_short(*at),
                        width,
                    )
                }
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// News articles and the viewer's RSS entries, newest first, two rows each:
/// the title with its source and age, then the link.
fn draw_headlines_tile(frame: &mut Frame, area: Rect, rows: &[Headline]) {
    if rows.is_empty() {
        draw_centered_note(frame, area, &["no headlines yet", "News and your RSS feeds land here"]);
        return;
    }
    let width = area.width as usize;
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let lines: Vec<Line<'static>> = rows
        .iter()
        .take((area.height as usize).div_ceil(2))
        .flat_map(|row| {
            [
                stamped_row(
                    vec![
                        Span::styled(row.title.clone(), Style::default().fg(theme::TEXT())),
                        Span::styled(format!(" · {}", row.source), faint),
                    ],
                    format_relative_time_short(row.at),
                    width,
                ),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(fit(&row.url, width.saturating_sub(2)), faint),
                ]),
            ]
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// One list row: `spans` from the left, cut with an ellipsis where the
/// room runs out, and `stamp` flush right in faint.
fn stamped_row(spans: Vec<Span<'static>>, stamp: String, width: usize) -> Line<'static> {
    let stamp_width = stamp.width();
    let budget = width.saturating_sub(stamp_width + 1);
    let mut used = 0usize;
    let mut out: Vec<Span<'static>> = Vec::with_capacity(spans.len() + 2);
    for span in spans {
        let room = budget.saturating_sub(used);
        if room == 0 {
            break;
        }
        let text = fit(&span.content, room);
        used += text.width();
        out.push(Span::styled(text, span.style));
    }
    out.push(Span::raw(" ".repeat(width.saturating_sub(used + stamp_width))));
    out.push(Span::styled(stamp, Style::default().fg(theme::TEXT_FAINT())));
    Line::from(out)
}

/// `text` cut to `width` columns, ending in an ellipsis when it was cut.
fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if used + ch_width + 1 > width {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    if width > 0 {
        out.push('…');
    }
    out
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
