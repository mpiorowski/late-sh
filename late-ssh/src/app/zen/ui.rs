//! Rendering for Home's tile tree: the chrome around every tile, the
//! guests, the pickers and the layouts list. Each tile's own drawing is
//! its domain's (`bonsai::render::draw_tile`, `hub::aquarium::ui::draw_reef_tile`,
//! `pet::ui::draw_tile`, ...): the `match` in `draw_rice` is the one wire
//! between a kind and the drawer, and the clock, the blank tile and the
//! board's day banner are the page's own.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use super::{
    attention::Item as AttentionItem,
    bigclock,
    layout::{self, Chrome},
    presets::{LayoutList, Layouts, MAX_LAYOUT_NAME_CHARS},
    state::{Guest, TileHint, TileKind, TileTitle, ZenState},
};

use crate::app::{
    audio::viz::EqState,
    bonsai::state::BonsaiState,
    chat::state::ActivityTickerEntry,
    chat::ui::{
        ChatRenderInput, DashboardChatView, EmbeddedRoomChatView, draw_chat_center,
        draw_dashboard_chat_card, draw_embedded_room_chat, draw_room_list_rail,
    },
    common::{
        primitives::{center_lines, draw_centered_note, hint_line},
        sidebar::{MusicStageProps, draw_music_tile, draw_panel_rule},
        theme,
    },
    files::terminal_image::TerminalImageFrame,
    hub::aquarium::ui::TankView,
    live::panel::LivePanelProps,
    lobby::daily::{panel::draw_daily_compact, state::DailyState},
    pet::ui::PetView,
    presence::ui::{PresenceView, draw_core_block},
};

/// A chat tile's frame: its room's label, the watcher count badge when the
/// room has a registered stream (`chat::ui::stream_count_badge`, the same
/// `[3]` the rail shows), and its view (`None` when the account has no room
/// at all). The active tile's view carries the composer and the selection;
/// the others only watch (`render.rs`).
pub(crate) struct ZenChatTile<'a> {
    pub label: String,
    pub stream_badge: Option<String>,
    pub view: Option<ChatTileView<'a>>,
}

/// What a chat tile draws: a room, or the News list for a tile holding
/// News (`ChatRoomBinding::News`). The active tile is drawn by the page's
/// chat instead, where News is the rail's row with its keys.
pub(crate) enum ChatTileView<'a> {
    Room(EmbeddedRoomChatView<'a>),
    News(crate::app::chat::news::ui::ArticleListView<'a>),
}

/// A chat tile's title: `Chat · #mat-live [3]`. The tile does not draw the
/// room's stream header, so the badge is the page's only watcher count, and
/// it must survive a narrow tile: the border clips a title from the right,
/// which would drop the count first. So the room label is what shortens,
/// never the badge. `width` is the tile's full width; the corners and the
/// title's padding spaces take four cells of it.
pub(crate) fn chat_tile_title(label: &str, stream_badge: Option<&str>, width: u16) -> String {
    let kind = TileKind::Chat.label();
    match stream_badge {
        None => format!("{kind} · {label}"),
        Some(badge) => {
            let fixed = UnicodeWidthStr::width(kind) + " · ".width() + 1 + badge.width();
            let budget = (width as usize).saturating_sub(4 + fixed);
            let label = crate::app::chat::ui::truncate_cells(label, budget);
            format!("{kind} · {label} {badge}")
        }
    }
}

/// What Home's own tiles read: the rail and the page's chat draw from
/// the same render input Home has always built, the chat as the #lounge
/// card while the lounge is selected.
pub(crate) struct HomeView<'a> {
    pub chat_view: ChatRenderInput<'a>,
    pub dashboard_view: DashboardChatView<'a>,
    pub home_selected: bool,
}

/// The now strip as a guest: what it shows and where its click lands.
pub(crate) struct StripGuest<'a> {
    pub strip: crate::app::live::state::LiveStripView<'a>,
    pub hit: &'a std::cell::Cell<Option<(Rect, crate::app::live::pick::LiveSource)>>,
}

/// Everything Home's tiles read but its rail and chat, assembled once per
/// frame in `render.rs`.
pub(crate) struct ZenView<'a> {
    pub zen: &'a ZenState,
    /// Home's named layouts, and the list `L` opens over them while it is
    /// up (arrange mode only).
    pub home_layouts: &'a Layouts,
    pub layout_list: Option<&'a LayoutList>,
    /// The music dock's inputs (`sidebar::draw_music_tile`): a tall music
    /// tile draws the dock, a short one the track over the station.
    pub music: MusicStageProps<'a>,
    /// The now strip for the tile hosting it as a guest, while something
    /// is up. The page's chat hosts it inside its card instead
    /// (`dashboard_view`).
    pub strip: Option<StripGuest<'a>>,
    /// What waits on you (`App::attention_items`): the attention guest
    /// takes its host while the list has rows.
    pub attention: &'a [AttentionItem],
    pub bonsai: &'a BonsaiState,
    /// The tank and its care, for the reef and the tank tiles.
    pub tank: TankView<'a>,
    /// `None` when the account owns no pet.
    pub pet: Option<PetView<'a>>,
    /// One entry per chat tile, in layout order.
    pub chats: Vec<ZenChatTile<'a>>,
    pub eq_state: EqState,
    /// Who is online now and the day behind it: the presence, pulse,
    /// friends and clock tiles read it.
    pub presence: PresenceView<'a>,
    /// The date under the clock tile's digits.
    pub date: &'a str,
    /// Daily correspondence games for the lobby tile, and whether the lobby
    /// label glows (your turn somewhere, or a result waiting).
    pub daily: &'a DailyState,
    pub lobby_glow: bool,
    /// The #lounge activity feed, newest first (`ChatState::activity_ticker`).
    pub activity: &'a [ActivityTickerEntry],
    /// Today's all-day board posts (`calendar::live::day_text`), the banner
    /// across the top of every tile; `None` on any other day.
    pub board_day: Option<&'a str>,
    /// The Live panel's rows, the sidebar's, for the Live tile: it draws
    /// them the same way and records the same click and key targets.
    pub live_panel: LivePanelProps<'a>,
    pub wall_tick: usize,
    /// The shared clock the ascii tiles are drawn at
    /// (`ascii::piece::clock_now`).
    pub ascii_clock: u64,
}

/// Home's page: every tile of the layout, `home` drawing the rail and the
/// page's chat.
pub(crate) fn draw_rice(
    frame: &mut Frame,
    area: Rect,
    mut view: ZenView<'_>,
    home: HomeView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if !layout::rice_fits(area) {
        crate::app::common::primitives::draw_too_small(
            frame,
            area,
            "Rice",
            layout::RICE_MIN_COLS,
            layout::RICE_MIN_ROWS,
        );
        return;
    }
    let areas = layout::rice_areas(area, view.board_day.is_some(), view.zen.arrange);
    if let (Some(row), Some(text)) = (areas.banner, view.board_day) {
        draw_board_day(frame, row, text);
    }
    let tiles_area = areas.tiles;
    // One frame per chat tile in layout order.
    let mut chats = std::mem::take(&mut view.chats).into_iter();
    let zen = view.zen;
    let gaps = zen.gaps();
    let chrome = zen.chrome();
    let rects = layout::tile_rects(&zen.rice.root, tiles_area, gaps);
    // Under the rules chrome the gutter between two columns is one line,
    // owned by the split, so neither tile draws its own.
    if matches!(chrome, Chrome::Rules { .. }) {
        for gutter in layout::row_gutters(&zen.rice.root, tiles_area, gaps) {
            crate::app::common::sidebar::paint_vertical_separator(
                frame,
                gutter.x,
                gutter.y,
                gutter.height,
            );
        }
    }
    let guests = zen.rice.root.leaf_guests();
    // Taken by the page's chat, the one tile that draws from it whole.
    let mut home = Some(home);
    // The active chat tile is the page's chat: it draws Home's card (the
    // rail's selection, synthetic entries included) with the card's own
    // chrome, where every other chat tile watches its room under a rule.
    let page_chat = zen.active_chat_ordinal();
    // A pet tile sharing an edge with a tank or a bonsai tile: the pet
    // goes and sits against that edge to watch, and alternates when it
    // touches both.
    let neighbours = layout::pet_neighbours(&rects, gaps);
    for (ordinal, (kind, rect)) in rects.iter().enumerate() {
        let focused = ordinal == zen.focus;
        // Each chat tile takes the next frame in layout order and names
        // its room in the title, so `[` `]` walking the rooms shows where
        // you landed without reading the messages.
        let chat_tile = match *kind == TileKind::Chat {
            true => chats.next(),
            false => None,
        };
        let is_page_chat = page_chat == Some(ordinal);
        // An ascii tile names the piece it plays, so `[` `]` shows where
        // you landed.
        let title = match (&chat_tile, zen.piece_label(ordinal)) {
            (Some(tile), _) => {
                chat_tile_title(&tile.label, tile.stream_badge.as_deref(), rect.width)
            }
            (None, Some(label)) => format!("{} · {}", kind.label(), label),
            (None, None) => kind.label().to_string(),
        };
        // The tile's guests, while they have something to say, are the
        // tile: their labels in the rule and their panels in place of the
        // host's (stacked when both are up). On the page's chat the strip
        // inserts into the card instead.
        let hosted: &[Guest] = guests.get(ordinal).copied().unwrap_or(&[]);
        let up: Vec<Guest> = hosted
            .iter()
            .copied()
            .filter(|guest| match guest {
                Guest::Now => view.strip.is_some() && !is_page_chat,
                Guest::Attention => !view.attention.is_empty(),
            })
            .collect();
        let title = match (up.is_empty(), zen.arrange) {
            (false, false) => up
                .iter()
                .map(|guest| guest.label())
                .collect::<Vec<_>>()
                .join(" · "),
            (true, _) | (false, true) => title,
        };
        let title_override = zen.rice.root.title_at(ordinal);
        // Arranging, the tile names the guests it hosts (`+name`, as the
        // key that gives it one), so a quiet guest is never a surprise,
        // and a tile that goes bare outside the mode says so in the rule
        // it wears for the mode.
        let title = match zen.arrange {
            true => {
                let mut title = title;
                for guest in hosted {
                    title.push_str(" +");
                    title.push_str(guest.label());
                }
                match title_override {
                    TileTitle::Named => {}
                    TileTitle::Rule => title.push_str(" · rule"),
                    TileTitle::Hidden => title.push_str(" · hidden"),
                }
                title
            }
            false => title,
        };

        // The tile's keys, its guests' ahead of them while they are up
        // here; the ones the tile takes show only where they work.
        let mut hints: Vec<TileHint> = Vec::new();
        for guest in &up {
            hints.extend_from_slice(guest.hints());
        }
        hints.extend_from_slice(kind.hints());
        let keys = hints_shown(&hints, zen.arrange, focused);
        // The lobby's rule glows while a game waits on you, as the
        // sidebar's does.
        let glow = *kind == TileKind::Lobby && view.lobby_glow;
        let inner = draw_tile_chrome(
            frame,
            *rect,
            *kind,
            is_page_chat,
            title_override,
            &title,
            &keys,
            focused,
            glow,
            &chrome,
        );
        if inner.width == 0 || inner.height == 0 {
            continue;
        }
        if !up.is_empty() {
            draw_guests(frame, inner, &up, focused, &view);
            continue;
        }
        // The rules chrome already set every tile but the page's chat and
        // the rail one column in; the boxed one pads its lists here.
        let padded = match chrome {
            Chrome::Boxed { .. } => pad_sides(inner),
            Chrome::Rules { .. } => inner,
        };
        match kind {
            TileKind::Bonsai => {
                crate::app::bonsai::render::draw_tile(frame, inner, view.bonsai, view.wall_tick)
            }
            TileKind::Aquarium => {
                crate::app::hub::aquarium::ui::draw_reef_tile(frame, inner, &view.tank)
            }
            TileKind::Tank => crate::app::hub::aquarium::ui::draw_tank_tile(
                frame,
                inner,
                &view.tank,
                view.wall_tick,
            ),
            TileKind::Ascii => {
                if let Some(piece) = zen.piece_at(ordinal) {
                    crate::app::ascii::ui::draw_piece(frame, inner, piece, view.ascii_clock);
                }
            }
            TileKind::Pet => {
                crate::app::pet::ui::draw_tile(frame, inner, view.pet.as_ref(), neighbours)
            }
            TileKind::Chat => match (is_page_chat, home.take()) {
                (
                    true,
                    Some(HomeView {
                        chat_view,
                        dashboard_view,
                        home_selected,
                    }),
                ) => {
                    if home_selected {
                        draw_dashboard_chat_card(frame, inner, dashboard_view, terminal_images);
                    } else {
                        draw_chat_center(frame, inner, chat_view, terminal_images);
                    }
                }
                (true, None) | (false, _) => {
                    draw_chat_tile(frame, inner, chat_tile, terminal_images)
                }
            },
            TileKind::Music => draw_music_tile(frame, inner, &view.music),
            TileKind::Clock => draw_clock_tile(frame, inner, &view),
            TileKind::Visualizer => crate::app::audio::viz::draw_visualizer_tile(
                frame,
                inner,
                view.wall_tick,
                view.eq_state,
            ),
            // Top-aligned, so a short tile shows the games first.
            TileKind::Lobby => draw_daily_compact(frame, padded, view.daily, view.lobby_glow),
            TileKind::Presence => draw_core_block(
                frame,
                pad_right(inner),
                view.presence.clock,
                view.presence.online,
                view.presence.friends,
                view.wall_tick,
            ),
            TileKind::Rooms => match &home {
                Some(home) => draw_room_list_rail(frame, inner, &home.chat_view),
                None => draw_centered_note(frame, inner, &["the rail is Home's"]),
            },
            TileKind::Activity => crate::app::chat::ui::draw_activity_tile(
                frame,
                padded,
                view.activity,
                view.presence.friends,
            ),
            TileKind::Friends => {
                crate::app::chat::ui::draw_friends_tile(frame, padded, view.presence.friends)
            }
            TileKind::Pulse => crate::app::presence::ui::draw_pulse_tile(
                frame,
                padded,
                view.presence.online,
                view.presence.history,
            ),
            TileKind::Live => {
                crate::app::live::panel::draw_live_inline(frame, padded, &view.live_panel)
            }
            TileKind::Blank => draw_blank_tile(frame, inner, focused),
        }
    }
    // Arrange mode's key footer; a row too narrow for any key stays blank.
    if let Some(row) = areas.footer
        && let Some(keys) = crate::app::statusline::bar::build_home_arrange_row(row)
    {
        frame.render_widget(Paragraph::new(keys).right_aligned(), row);
    }
    draw_kind_picker(frame, area, zen);
    draw_guest_picker(frame, area, zen);
    if let Some(list) = view.layout_list {
        draw_layout_list(frame, area, view.home_layouts, list);
    }
}

/// The layouts list `L` opens in arrange mode: one row per layout with
/// its digit and name, the current one marked, the selected row lit;
/// while a name is being typed (`n`) the field sits under the rows with
/// its cursor.
fn draw_layout_list(frame: &mut Frame, area: Rect, layouts: &Layouts, list: &LayoutList) {
    let rows = layouts.count() as u16;
    // The rows, a blank, the hint, the two border rows; two more rows
    // while a name is typed.
    let edit_rows = match list.editing {
        Some(_) => 2,
        None => 0,
    };
    let Some(inner) = draw_popup(frame, area, " layouts ", 50, rows + 4 + edit_rows) else {
        return;
    };
    let mut lines: Vec<Line<'static>> = layouts
        .items()
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let picked = index == list.selected;
            let current = index == layouts.current();
            let name_style = if picked {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let note = if current { "current" } else { "" };
            Line::from(vec![
                Span::styled(
                    if picked { "▌ " } else { "  " },
                    Style::default().fg(theme::AMBER()),
                ),
                Span::styled(
                    format!("{} ", index + 1),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled(
                    format!("{:<width$} ", item.name, width = MAX_LAYOUT_NAME_CHARS),
                    name_style,
                ),
                Span::styled(note.to_string(), Style::default().fg(theme::TEXT_FAINT())),
            ])
        })
        .collect();
    if let Some(edit) = &list.editing {
        let label = "save as: ";
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ".to_string(), Style::default()),
            Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                edit.text.clone(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("▏".to_string(), Style::default().fg(theme::AMBER())),
        ]));
    }
    lines.push(Line::from(""));
    let hints: &[(&str, &str)] = match list.editing {
        Some(_) => &[("enter", "keep"), ("esc", "drop")],
        None => &[
            ("jk", "move"),
            ("enter", "load"),
            ("n", "new"),
            ("d", "delete"),
            ("R", "reset"),
            ("esc", "close"),
        ],
    };
    lines.push(hint_line_fitting(hints, inner.width as usize));
    frame.render_widget(Paragraph::new(lines), inner);
}

/// A popup of up to `width` by `height` centred over `area`, cleared and
/// framed under `title`: the area inside its frame, or `None` on a page too
/// small to hold one.
fn draw_popup(
    frame: &mut Frame,
    area: Rect,
    title: &'static str,
    width: u16,
    height: u16,
) -> Option<Rect> {
    let height = height.min(area.height);
    let width = width.min(area.width);
    if height < 4 || width < 12 {
        return None;
    }
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(title)
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
    Some(inner)
}

/// Paint the guests that have taken their host's tile. One guest has the
/// whole tile, the strip centred down it and attention's rows from its
/// top; with both up the strip takes its picture rows at the top and
/// attention the rest. Neither draws a title
/// of its own: the tile's chrome names the guests up in place of the
/// host's title, where the host wears one. The host draws nothing
/// underneath, so the moment the last guest goes the host shows whole.
fn draw_guests(frame: &mut Frame, area: Rect, up: &[Guest], focused: bool, view: &ZenView<'_>) {
    let now = up.contains(&Guest::Now);
    let attention = up.contains(&Guest::Attention);
    match (now, attention) {
        (true, true) => {
            let (_, strip_height) = now_strip_size(area);
            let top = Rect::new(area.x, area.y, area.width, strip_height);
            draw_guest(frame, top, Guest::Now, focused, view);
            let below = Rect::new(
                area.x,
                area.y + strip_height,
                area.width,
                area.height - strip_height,
            );
            draw_guest(frame, below, Guest::Attention, focused, view);
        }
        (true, false) => draw_guest(frame, area, Guest::Now, focused, view),
        (false, true) => draw_guest(frame, area, Guest::Attention, focused, view),
        (false, false) => {}
    }
}

/// The strip's form in a tile of `area`: full where the tile is wide and
/// tall enough for the picture, its one-row form otherwise, and the rows
/// the form paints (the tile form has no rule, `StripHost::Tile`).
fn now_strip_size(area: Rect) -> (crate::app::live::ui::StripSize, u16) {
    use crate::app::live::ui::{
        LIVE_STRIP_COMPACT_HEIGHT, MIN_FULL_WIDTH, PICTURE_ROWS, StripSize,
    };
    match area.width >= MIN_FULL_WIDTH && area.height >= PICTURE_ROWS {
        true => (StripSize::Full, PICTURE_ROWS),
        false => (
            StripSize::Compact,
            LIVE_STRIP_COMPACT_HEIGHT.min(area.height),
        ),
    }
}

/// Paint one guest into `area`: the strip's picture and words centred
/// down it (the click lands on those rows), or attention's rows from its
/// top, the selected one lit while the tile is focused.
fn draw_guest(frame: &mut Frame, area: Rect, guest: Guest, focused: bool, view: &ZenView<'_>) {
    match guest {
        Guest::Now => {
            let Some(strip) = &view.strip else {
                return;
            };
            let (size, height) = now_strip_size(area);
            let strip_area = Rect::new(
                area.x,
                area.y + (area.height - height) / 2,
                area.width,
                height,
            );
            crate::app::live::ui::draw_live_strip(
                frame,
                strip_area,
                size,
                crate::app::live::ui::StripHost::Tile,
                &strip.strip,
                strip.hit,
            );
        }
        Guest::Attention => draw_attention_tile(
            frame,
            area,
            view.attention,
            view.zen.attention_selected,
            focused,
        ),
    }
}

/// The guest picker `+` opens in arrange mode: one row per guest, a check
/// beside each the focused tile hosts, `elsewhere` beside one another
/// tile hosts (picking it moves it here). Space or Enter picks, or takes
/// the tile's own off, and the picker stays up.
fn draw_guest_picker(frame: &mut Frame, area: Rect, zen: &ZenState) {
    let Some(selected) = zen.guest_picker else {
        return;
    };
    let Some(inner) = draw_popup(frame, area, " guest ", 44, Guest::ALL.len() as u16 + 4) else {
        return;
    };
    let here = zen.focused_guests();
    let mut lines: Vec<Line<'static>> = Guest::ALL
        .iter()
        .enumerate()
        .map(|(index, guest)| {
            let picked = index == selected;
            let on_tile = here.contains(guest);
            let elsewhere = !on_tile && zen.hosts(*guest);
            let label_style = if picked {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let mark = if on_tile { "[x] " } else { "[ ] " };
            let note = if elsewhere { "elsewhere" } else { guest.when() };
            Line::from(vec![
                Span::styled(
                    if picked { "▌ " } else { "  " },
                    Style::default().fg(theme::AMBER()),
                ),
                Span::styled(mark, Style::default().fg(theme::TEXT_DIM())),
                Span::styled(format!("{:<10}", guest.label()), label_style),
                Span::styled(note.to_string(), Style::default().fg(theme::TEXT_FAINT())),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(hint_line_fitting(
        &[("jk", "move"), ("space", "pick"), ("esc", "close")],
        inner.width as usize,
    ));
    frame.render_widget(Paragraph::new(lines), inner);
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
    let Some(inner) = draw_popup(frame, area, " tile ", 36, TileKind::ALL.len() as u16 + 4) else {
        return;
    };

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
                    if kind.one_per_page() {
                        "one per page"
                    } else {
                        "full"
                    },
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

/// The hints that show on a tile now (`TileKind::hints`, `Guest::hints`):
/// none outside arrange mode; in it, every house key the tile names, and
/// the keys it takes itself where the focus is.
fn hints_shown(
    hints: &[TileHint],
    arrange: bool,
    focused: bool,
) -> Vec<(&'static str, &'static str)> {
    hints
        .iter()
        .filter(|hint| match hint {
            TileHint::Takes(_) => arrange && focused,
            TileHint::Names(_) => arrange,
        })
        .map(|hint| hint.text())
        .collect()
}

/// Border, title, keys, and focus ring per the look; returns the tile's
/// inner area. A title row is the title on the left and the tile's keys
/// on the right when there is room for them, nothing else.
#[allow(clippy::too_many_arguments)]
fn draw_tile_chrome(
    frame: &mut Frame,
    rect: Rect,
    kind: TileKind,
    page_chat: bool,
    title_override: TileTitle,
    title: &str,
    keys: &[(&str, &str)],
    focused: bool,
    glow: bool,
    chrome: &Chrome,
) -> Rect {
    if rect.width == 0 || rect.height == 0 {
        return rect;
    }
    let border = match chrome {
        Chrome::Boxed { border, .. } => *border,
        Chrome::Rules { arrange } => {
            let inner = layout::tile_inner(rect, chrome, kind, page_chat, title_override);
            let row = Rect::new(inner.x, rect.y, inner.width, 1);
            match chrome.wears(title_override) {
                TileTitle::Hidden => return inner,
                TileTitle::Rule => {
                    crate::app::common::sidebar::draw_bare_rule(frame, row, glow);
                    return inner;
                }
                TileTitle::Named => {}
            }
            // The sidebar's rule, the title italic and dim, lit amber while
            // the focus is on it in arrange mode or the lobby glows, with
            // the tile's keys on its right end.
            draw_panel_rule(frame, row, title, glow || (*arrange && focused));
            if !keys.is_empty() {
                let mut right = hint_line(keys);
                right.spans.push(Span::raw(" "));
                let used = 3 + title.chars().count() + 1;
                if right.width() + used < row.width as usize {
                    frame.render_widget(Paragraph::new(right).right_aligned(), row);
                }
            }
            return inner;
        }
    };
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
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(border)
        .border_style(ring);
    if chrome.wears(title_override) == TileTitle::Named {
        let left = Line::from(vec![Span::styled(format!(" {title} "), title_style)]);
        // The corners take two cells; the keys need a cell of border on
        // each side of them to read as a second title. Styled like the
        // footer: the key amber, the word dim.
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
    layout::tile_inner(rect, chrome, kind, page_chat, title_override)
}

/// `hint_line` with as many leading hints as fit in `width` cells.
fn hint_line_fitting(hints: &[(&str, &str)], width: usize) -> Line<'static> {
    let mut keep = hints.len();
    while keep > 0 && hint_line(&hints[..keep]).width() > width {
        keep -= 1;
    }
    hint_line(&hints[..keep])
}

/// A chat tile: its room's messages, with the composer on the active tile
/// only, or the News list. `None` is a tile past the frames built this
/// frame, which cannot happen (one frame per chat tile) but is drawn as an
/// empty note rather than crashed on.
fn draw_chat_tile(
    frame: &mut Frame,
    area: Rect,
    tile: Option<ZenChatTile<'_>>,
    terminal_images: &mut TerminalImageFrame,
) {
    match tile {
        Some(ZenChatTile {
            view: Some(ChatTileView::Room(view)),
            ..
        }) => {
            if area.height < 4 || area.width < 20 {
                return;
            }
            draw_embedded_room_chat(frame, area, view, terminal_images);
        }
        Some(ZenChatTile {
            view: Some(ChatTileView::News(view)),
            ..
        }) => crate::app::chat::news::ui::draw_article_list(frame, area, &view),
        Some(ZenChatTile { view: None, .. }) | None => {
            draw_centered_note(frame, area, &["no room open", "1 opens Home to pick one"])
        }
    }
}

fn draw_clock_tile(frame: &mut Frame, area: Rect, view: &ZenView<'_>) {
    let width = area.width as usize;
    let height = area.height as usize;
    let date_rows = if height >= 7 { 2 } else { 0 };
    // Only the digits go big; the zone prefix rides the date row.
    let digits: String = view
        .presence
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
                    view.presence.clock.to_string(),
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
                view.date.to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ))
            .centered(),
        );
    }
    let top_pad = (height.saturating_sub(lines.len()) / 2) as u16;
    let below = Rect::new(area.x, area.y + top_pad, area.width, area.height - top_pad);
    frame.render_widget(Paragraph::new(lines), below);
}

/// The board's day across the top of Home, over every tile: today's
/// all-day posts in amber, the board's colour, the board's key flush right
/// in the strip's key style, one column in from each edge.
fn draw_board_day(frame: &mut Frame, row: Rect, text: &str) {
    let row = Rect::new(
        row.x.saturating_add(1),
        row.y,
        row.width.saturating_sub(2),
        row.height,
    );
    let width = usize::from(row.width);
    let hint = crate::app::live::ui::key_hint_spans(
        width,
        &[
            crate::app::live::ui::HintPart::Key("7"),
            crate::app::live::ui::HintPart::Text(" for the board"),
        ],
    );
    let hint_width: usize = hint.iter().map(|span| span.width()).sum();
    let text_room = width.saturating_sub(hint_width + 1);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            crate::app::chat::ui::truncate_cells(text, text_room),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))),
        row,
    );
    frame.render_widget(Paragraph::new(Line::from(hint)).right_aligned(), row);
}

/// `area` less one column on each side, when it has columns to spare.
fn pad_sides(area: Rect) -> Rect {
    if area.width <= 2 {
        return area;
    }
    Rect::new(area.x + 1, area.y, area.width - 2, area.height)
}

/// `area` less its last column: the presence block keeps its right edge
/// clear of the border.
fn pad_right(area: Rect) -> Rect {
    if area.width <= 1 {
        return area;
    }
    Rect::new(area.x, area.y, area.width - 1, area.height)
}

fn draw_blank_tile(frame: &mut Frame, area: Rect, focused: bool) {
    if focused {
        draw_centered_note(frame, area, &["space picks what lives here"]);
    }
}

/// The attention guest's rows into `area`, scrolled so the selected one is in view.
/// `selected` is clamped to the rows, since they come and go.
fn draw_attention_tile(
    frame: &mut Frame,
    area: Rect,
    items: &[AttentionItem],
    selected: usize,
    focused: bool,
) {
    if items.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let visible = area.height as usize;
    let selected = selected.min(items.len() - 1);
    let first = (selected + 1).saturating_sub(visible);
    let amber = Style::default().fg(theme::AMBER());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let lines: Vec<Line<'static>> = items
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, item)| {
            // The rows start at the tile's edge, no marker column: the
            // selected row is lit instead, its glyph glowing and its
            // words bold.
            let marked = focused && index == selected;
            let (head_style, words_style) = match marked {
                true => (
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD),
                    bright.add_modifier(Modifier::BOLD),
                ),
                false => (amber, bright),
            };
            let act = item.act();
            let head = format!("{} ", item.glyph());
            let budget = width.saturating_sub(head.width() + act.width() + 1);
            let words = crate::app::chat::ui::truncate_cells(&item.label(), budget);
            let pad = width.saturating_sub(head.width() + words.width() + act.width());
            Line::from(vec![
                Span::styled(head, head_style),
                Span::styled(words, words_style),
                Span::raw(" ".repeat(pad)),
                Span::styled(act.to_string(), faint),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
