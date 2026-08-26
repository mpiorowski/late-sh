//! Clubhouse renderer: the tavern viewport (camera-follow over the floor
//! plan, the live shared crowd, animated fire/jukebox/dog/candles, proximity
//! popovers) with the #lounge composer pinned to the bottom of the screen.
//! There is no chat panel: fresh #lounge messages render as speech bubbles
//! over their author's head, emotes play on avatars, and arrivals slip in at
//! the door. Dwarf Fortress vibes, single-width glyphs only: walking people
//! are 3-row stick figures (`o` head, `/|\` arms, `Λ` legs; you get an `@`),
//! a seated user is an `o` perched on their stool, and the dog is a pocket
//! `(ᴥ)` with a wagging tail that trots wherever the shared lobby says.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::collections::HashMap;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use uuid::Uuid;

use crate::app::common::primitives::Screen;
use crate::app::common::theme;
use crate::app::common::username_effect::{CROWN_GLYPH, NameStyle, ResolvedName, char_color};
use late_core::api_types::NowPlaying;
use late_core::models::chat_message::ChatMessage;
use late_core::models::drinks::{DRUNK_LABEL_MIN_LEVEL, DRUNK_MAX_LEVEL};

use super::lobby::{Emote, Placement};
use super::map;
use super::state::{BannerLine, ClubhouseHit, State, Tutorial};

const LABEL_MAX: usize = 10;
const FIRE_CHARS: [char; 6] = ['(', ')', '~', '^', '*', '\''];
const EQ_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// Phosphor pixels for the arcade cabinet's attract mode.
const SCREEN_CHARS: [char; 4] = ['▀', '▄', '·', ' '];
/// How long a #lounge message floats over its author's head.
const BUBBLE_MS: i64 = 10_000;
/// Bubble width tiers: short quips stay cozy, longer messages (a bartender
/// answer, a pasted sentence) widen before they truncate.
const BUBBLE_WIDTHS: [usize; 3] = [28, 36, 44];
const BUBBLE_MAX_LINES: usize = 3;

pub(crate) struct ClubhouseView<'a> {
    pub state: &'a State,
    pub own_username: &'a str,
    /// Resolved 24h username-effect styles; painted over name labels.
    pub name_flair: &'a std::collections::HashMap<Uuid, ResolvedName>,
    pub now_playing: Option<&'a NowPlaying>,
    /// The #lounge tail, for speech bubbles.
    pub lounge_messages: &'a [ChatMessage],
    /// Staff bot ids so their #lounge lines can bubble over their sprites.
    pub graybeard_user_id: Option<Uuid>,
    pub bot_user_id: Option<Uuid>,
    /// The shared composer block, pinned under the tavern. `None` only
    /// before the #lounge room id is known.
    pub composer: Option<crate::app::chat::ui::ComposerBlockView<'a>>,
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: ClubhouseView<'_>) {
    let Some(composer) = &view.composer else {
        draw_tavern(frame, area, &view);
        return;
    };
    // The composer footer keeps the compact height the dashboard card uses:
    // one placeholder line while idle, growing with the draft while typing.
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let composer_lines = crate::app::chat::ui::chat_composer_lines_for_height(
        composer.composer,
        composer_text_width,
    )
    .max(crate::app::chat::ui::composer_placeholder_lines(
        composer,
        composer_text_width,
    ));
    let composer_height = (composer_lines.min(4) as u16 + 2).min(area.height.saturating_sub(4));
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(area);

    draw_tavern(frame, layout[0], &view);
    crate::app::chat::ui::draw_composer_block(frame, layout[1], composer);
}

fn draw_tavern(frame: &mut Frame, area: Rect, view: &ClubhouseView<'_>) {
    let state = view.state;
    // No widget border: the room's own walls are the frame. The headcount
    // and keybinds live in the app frame title (`app_frame_title` in
    // `render.rs`), so the tavern gets every cell.
    let inner = area;
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let mut cells = styled_base_grid();
    animate(&mut cells, view);
    let (anchors, map_hits) = place_people(&mut cells, view);
    draw_door_events(&mut cells, view);
    draw_bubbles(&mut cells, view, &anchors);

    // Camera: follow the player, clamped to the map; center when the
    // viewport is larger than the room.
    let vw = usize::from(inner.width);
    let vh = usize::from(inner.height);
    let map_w = usize::from(map::MAP_W);
    let map_h = usize::from(map::MAP_H);
    let cam_x = camera_origin(usize::from(state.player_x), vw, map_w);
    let cam_y = camera_origin(usize::from(state.player_y), vh, map_h);
    let pad_x = vw.saturating_sub(map_w) / 2;
    let pad_y = vh.saturating_sub(map_h) / 2;

    // Re-express the map-space clickable boxes in absolute terminal cells,
    // under the same camera transform the grid is drawn with, so a click can
    // be resolved back to a user (`State::hit_test`).
    let hits = map_hits
        .into_iter()
        .filter_map(|h| project_hit(h, inner, cam_x, cam_y, pad_x, pad_y))
        .collect();
    state.set_hit_layout(hits);

    let mut lines: Vec<Line> = Vec::with_capacity(vh);
    for _ in 0..pad_y {
        lines.push(Line::default());
    }
    for row in cells.iter().skip(cam_y).take(vh.saturating_sub(pad_y)) {
        let mut spans: Vec<Span> = Vec::new();
        if pad_x > 0 {
            spans.push(Span::raw(" ".repeat(pad_x)));
        }
        // Batch runs of same-styled cells into one span per run instead of
        // one heap string per cell; room floors are long same-style runs.
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        for (i, &(ch, style)) in row
            .iter()
            .skip(cam_x)
            .take(vw.saturating_sub(pad_x))
            .enumerate()
        {
            // A wide glyph's tail is not a character. When the camera cuts
            // between a glyph and its tail, the tail is the first cell of
            // the row and has to hold the column open as a blank instead.
            let ch = match (ch, i) {
                (WIDE_TAIL, 0) => ' ',
                (WIDE_TAIL, _) => continue,
                (ch, _) => ch,
            };
            match run_style {
                Some(current) if current == style => run.push(ch),
                Some(current) => {
                    spans.push(Span::styled(std::mem::take(&mut run), current));
                    run.push(ch);
                    run_style = Some(style);
                }
                None => {
                    run.push(ch);
                    run_style = Some(style);
                }
            }
        }
        if let Some(style) = run_style {
            spans.push(Span::styled(run, style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    draw_overlays(frame, inner, view);
}

fn camera_origin(player: usize, viewport: usize, map_len: usize) -> usize {
    if viewport >= map_len {
        return 0;
    }
    player.saturating_sub(viewport / 2).min(map_len - viewport)
}

/// Project a map-space clickable box into absolute terminal cells using the
/// same camera pan/pad as the grid render. Boxes fully off the viewport drop
/// out; partly-visible ones are clamped to the visible edge.
fn project_hit(
    hit: ClubhouseHit,
    inner: Rect,
    cam_x: usize,
    cam_y: usize,
    pad_x: usize,
    pad_y: usize,
) -> Option<ClubhouseHit> {
    let vw = i64::from(inner.width);
    let vh = i64::from(inner.height);
    let col = |mx: u16| pad_x as i64 + i64::from(mx) - cam_x as i64;
    let row = |my: u16| pad_y as i64 + i64::from(my) - cam_y as i64;
    let (c0, c1, r0, r1) = (col(hit.x0), col(hit.x1), row(hit.y0), row(hit.y1));
    if c1 < 0 || c0 > vw - 1 || r1 < 0 || r0 > vh - 1 {
        return None;
    }
    let clamp_col = |c: i64| c.clamp(0, vw - 1) as u16;
    let clamp_row = |r: i64| r.clamp(0, vh - 1) as u16;
    Some(ClubhouseHit {
        user_id: hit.user_id,
        username: hit.username,
        x0: inner.x.saturating_add(clamp_col(c0)),
        y0: inner.y.saturating_add(clamp_row(r0)),
        x1: inner.x.saturating_add(clamp_col(c1)),
        y1: inner.y.saturating_add(clamp_row(r1)),
    })
}

type Cells = Vec<Vec<(char, Style)>>;

/// The second cell of a double-width glyph. The floor is one char per cell,
/// so a wide char (the crown emoji) takes its own cell plus this one; the
/// flush skips it, which keeps the rest of the row aligned to the walls.
const WIDE_TAIL: char = '\0';

fn styled_base_grid() -> Cells {
    map::grid()
        .iter()
        .enumerate()
        .map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(|(x, &ch)| (ch, base_style(ch, x as u16, y as u16)))
                .collect()
        })
        .collect()
}

fn base_style(ch: char, x: u16, y: u16) -> Style {
    let dim = Style::default().fg(theme::TEXT_DIM());
    // The sign over the door.
    if y == 0 && !matches!(ch, '═' | '╔' | '╗') {
        return match ch {
            '☾' | '☽' => Style::default().fg(theme::AMBER_GLOW()),
            '╡' | '╞' => dim,
            _ => Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        };
    }
    // The back-bar shelf: every bottle body gets its own liquor glint.
    if map::BACK_BAR.contains(x, y) {
        return match ch {
            '█' => Style::default().fg(hashed_color(x, y, BOTTLE_PALETTE)),
            _ => Style::default().fg(theme::TEXT_MUTED()),
        };
    }
    // The neon house sign burns over the north wall.
    if map::NEON_SIGN.contains(x, y) {
        return match ch {
            '╭' | '╮' | '╰' | '╯' | '─' | '│' => Style::default().fg(theme::ERROR()),
            _ => Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        };
    }
    // Moonlight in the windows.
    if map::WINDOWS.iter().any(|w| w.contains(x, y)) {
        return match ch {
            '☾' => Style::default().fg(theme::AMBER_GLOW()),
            '·' | '*' => Style::default().fg(theme::TEXT_MUTED()),
            _ => dim,
        };
    }
    // Interactive props wear red frames so they read as "walk up to me";
    // their names sit amber-bold in the art with the page digit glowing.
    let signpost_text = |ch: char| {
        if ch.is_ascii_digit() {
            Some(
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )
        } else if ch.is_ascii_alphabetic() || ch == '·' {
            Some(
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            None
        }
    };
    if map::JUKEBOX.contains(x, y) {
        return match ch {
            '♪' => Style::default().fg(theme::AMBER_GLOW()),
            '[' | ']' | '·' | '▞' | '▚' | '○' => Style::default().fg(theme::TEXT_MUTED()),
            _ => signpost_text(ch).unwrap_or_else(|| Style::default().fg(theme::ERROR())),
        };
    }
    if map::ARCADE_SCREEN.contains(x, y) {
        return Style::default().fg(theme::SUCCESS());
    }
    if map::ARCADE.contains(x, y) {
        return match ch {
            '●' => Style::default().fg(theme::ERROR()),
            '┃' => Style::default().fg(theme::TEXT_BRIGHT()),
            '╭' | '╮' | '╰' | '╯' | '─' | '│' => dim,
            _ => signpost_text(ch).unwrap_or_else(|| Style::default().fg(theme::ERROR())),
        };
    }
    if map::DOORS.contains(x, y) {
        if x == map::DOORS.x0 || x == map::DOORS.x1 || matches!(ch, '╭' | '╮' | '╰' | '╯' | '─')
        {
            return Style::default().fg(theme::ERROR());
        }
        return match ch {
            '○' => Style::default().fg(theme::AMBER_GLOW()),
            '║' => Style::default().fg(theme::AMBER()),
            '│' | '▒' => Style::default().fg(theme::AMBER_DIM()),
            _ => signpost_text(ch).unwrap_or(dim),
        };
    }
    if map::POKER_TABLE.contains(x, y) {
        return match ch {
            '▒' => Style::default().fg(theme::SUCCESS()),
            '♥' | '♦' => Style::default().fg(theme::ERROR()),
            '♠' | '♣' => Style::default().fg(theme::TEXT_BRIGHT()),
            _ => signpost_text(ch).unwrap_or_else(|| Style::default().fg(theme::ERROR())),
        };
    }
    if map::EASEL.contains(x, y) {
        // The title row is the ARTBOARD·5 signpost; the rest of the canvas
        // is paint splatter.
        if y == map::EASEL.y0 + 1
            && let Some(style) = signpost_text(ch)
        {
            return style;
        }
        return match ch {
            '·' | '~' | '°' | '*' => Style::default().fg(hashed_color(x, y, PAINT_PALETTE)),
            '╱' | '╲' => Style::default().fg(theme::TEXT_MUTED()),
            _ => Style::default().fg(theme::ERROR()),
        };
    }
    if map::BOOKSHELF.contains(x, y) {
        if x == map::BOOKSHELF.x0
            || x == map::BOOKSHELF.x1
            || matches!(ch, '╔' | '╗' | '╚' | '╝' | '╠' | '╣' | '═')
        {
            return Style::default().fg(theme::AMBER_DIM());
        }
        return Style::default().fg(hashed_color(x, y, BOOK_PALETTE));
    }
    if map::FIREPLACE.contains(x, y) {
        return match ch {
            '¡' => Style::default().fg(theme::AMBER_GLOW()),
            '▒' => Style::default().fg(theme::AMBER_DIM()),
            '█' | '▓' | '▄' | '▀' => Style::default().fg(theme::TEXT_MUTED()),
            '╔' | '╗' | '╚' | '╝' | '═' | '║' => {
                Style::default().fg(theme::TEXT_MUTED())
            }
            _ => Style::default().fg(theme::AMBER()),
        };
    }
    match ch {
        '║' | '═' | '╔' | '╗' | '╚' | '╝' | '╡' | '╞' => dim,
        '▔' | '▄' | '▀' => Style::default().fg(theme::AMBER_DIM()),
        '█' => Style::default().fg(theme::TEXT_MUTED()),
        '╥' => Style::default().fg(theme::AMBER()),
        '≡' | '·' => Style::default().fg(theme::AMBER_DIM()),
        '¡' | '!' => Style::default().fg(theme::AMBER_GLOW()),
        '╭' | '╮' | '╰' | '╯' | '─' | '│' | '┬' | '┴' => {
            Style::default().fg(theme::AMBER_DIM())
        }
        '▒' => Style::default().fg(theme::TEXT_FAINT()),
        '(' | ')' | '_' => dim,
        '▐' => Style::default().fg(theme::TEXT_MUTED()),
        '░' => Style::default().fg(theme::TEXT_FAINT()),
        '♣' => Style::default().fg(theme::SUCCESS()),
        '$' => Style::default().fg(theme::SUCCESS()),
        '[' | ']' => dim,
        _ if ch.is_ascii_alphabetic() => Style::default().fg(theme::AMBER_DIM()),
        _ => Style::default().fg(theme::TEXT_MUTED()),
    }
}

const BOTTLE_PALETTE: [fn() -> ratatui::style::Color; 5] = [
    theme::AMBER,
    theme::SUCCESS,
    theme::ERROR,
    theme::CHAT_AUTHOR,
    theme::TEXT_MUTED,
];
const PAINT_PALETTE: [fn() -> ratatui::style::Color; 5] = [
    theme::CHAT_AUTHOR,
    theme::SUCCESS,
    theme::AMBER,
    theme::MENTION,
    theme::ERROR,
];
const BOOK_PALETTE: [fn() -> ratatui::style::Color; 5] = [
    theme::CHAT_AUTHOR,
    theme::SUCCESS,
    theme::AMBER,
    theme::MENTION,
    theme::TEXT_MUTED,
];

/// A stable per-cell pick from a small palette, so the bottle shelf and the
/// easel's paint read as a colorful jumble without flickering per frame.
fn hashed_color(
    x: u16,
    y: u16,
    palette: [fn() -> ratatui::style::Color; 5],
) -> ratatui::style::Color {
    let h = mix(u64::from(x) * 31 + u64::from(y) * 131);
    palette[(h % palette.len() as u64) as usize]()
}

fn animate(cells: &mut Cells, view: &ClubhouseView<'_>) {
    let t = view.state.anim_tick;

    // Fire: flicker glyph and color per cell.
    for y in map::FIRE_CELLS.y0..=map::FIRE_CELLS.y1 {
        for x in map::FIRE_CELLS.x0..=map::FIRE_CELLS.x1 {
            let h = mix(u64::from(x) * 31 + u64::from(y) * 131 + t / 3);
            let ch = FIRE_CHARS[(h % FIRE_CHARS.len() as u64) as usize];
            let color = match h / 7 % 3 {
                0 => theme::ERROR(),
                1 => theme::AMBER_GLOW(),
                _ => theme::AMBER(),
            };
            set(cells, x, y, ch, Style::default().fg(color));
        }
    }

    // Candle flames breathe on the tables and the mantle.
    for &(x, y) in map::CANDLES.iter() {
        let h = mix(u64::from(x) * 31 + u64::from(y) * 131 + t / 6);
        let ch = if h.is_multiple_of(7) { '!' } else { '¡' };
        let color = if h.is_multiple_of(3) {
            theme::AMBER()
        } else {
            theme::AMBER_GLOW()
        };
        set(cells, x, y, ch, Style::default().fg(color));
    }

    // Jukebox equalizer: dances while something is playing, sleeps flat when
    // the stream is quiet.
    for x in map::JUKEBOX_EQ.x0..=map::JUKEBOX_EQ.x1 {
        let y = map::JUKEBOX_EQ.y0;
        if view.now_playing.is_some() {
            let h = mix(u64::from(x) * 97 + t / 2);
            let ch = EQ_CHARS[(h % EQ_CHARS.len() as u64) as usize];
            set(cells, x, y, ch, Style::default().fg(theme::AMBER_GLOW()));
        } else {
            set(cells, x, y, '▁', Style::default().fg(theme::TEXT_FAINT()));
        }
    }

    // Notes drift out of the jukebox, across the floor below it.
    if view.now_playing.is_some() {
        let (jx, jy) = (map::JUKEBOX.x0, map::JUKEBOX.y1);
        let phase = ((t / 5) % 6) as u16;
        put_if_floor(
            cells,
            jx + 1 + phase,
            jy + 1 + (phase % 2),
            '♪',
            theme::AMBER_GLOW(),
        );
        let phase2 = ((t / 5 + 3) % 6) as u16;
        put_if_floor(
            cells,
            jx + 8 + phase2,
            jy + 2 - (phase2 % 2),
            '♫',
            theme::AMBER(),
        );
    }

    // The arcade cabinet plays its attract mode to an empty room.
    for y in map::ARCADE_SCREEN.y0..=map::ARCADE_SCREEN.y1 {
        for x in map::ARCADE_SCREEN.x0..=map::ARCADE_SCREEN.x1 {
            let h = mix(u64::from(x) * 97 + u64::from(y) * 53 + t / 4);
            let ch = SCREEN_CHARS[(h % SCREEN_CHARS.len() as u64) as usize];
            let color = if h.is_multiple_of(5) {
                theme::TEXT_BRIGHT()
            } else {
                theme::SUCCESS()
            };
            set(cells, x, y, ch, Style::default().fg(color));
        }
    }

    // Stars twinkle in the window panes (the moon holds still).
    for window in map::WINDOWS.iter() {
        for y in window.y0..=window.y1 {
            for x in window.x0..=window.x1 {
                if !matches!(map::char_at(x, y), '·' | '*') {
                    continue;
                }
                let h = mix(u64::from(x) * 53 + u64::from(y) * 97 + t / 10);
                let (ch, color) = match h % 5 {
                    0 => ('*', theme::TEXT_BRIGHT()),
                    1 => (' ', theme::TEXT_FAINT()),
                    _ => ('·', theme::TEXT_MUTED()),
                };
                set(cells, x, y, ch, Style::default().fg(color));
            }
        }
    }

    // The neon sign shorts out for a frame now and then.
    if mix(t / 4).is_multiple_of(19) {
        for y in map::NEON_SIGN.y0..=map::NEON_SIGN.y1 {
            for x in map::NEON_SIGN.x0..=map::NEON_SIGN.x1 {
                let ch = map::char_at(x, y);
                if ch != ' ' {
                    set(cells, x, y, ch, Style::default().fg(theme::TEXT_FAINT()));
                }
            }
        }
    }

    // The door sign glows while someone is slipping in.
    if view.state.door_glow() {
        for x in map::DOOR_SIGN.x0..=map::DOOR_SIGN.x1 {
            let ch = map::char_at(x, map::DOOR_SIGN.y0);
            set(
                cells,
                x,
                map::DOOR_SIGN.y0,
                ch,
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    // Once the tour has come home, the bar sign pulses until the newcomer
    // claims the hidden welcome pour: the only pointer at the treasure.
    if view.state.bar_glow() {
        let pulse = if (t / 8).is_multiple_of(2) {
            theme::AMBER_GLOW()
        } else {
            theme::AMBER()
        };
        let y = map::BAR_COUNTER.y1;
        for x in map::BAR_COUNTER.x0..=map::BAR_COUNTER.x1 {
            let ch = map::char_at(x, y);
            if ch != '█' && ch != ' ' {
                set(
                    cells,
                    x,
                    y,
                    ch,
                    Style::default().fg(pulse).add_modifier(Modifier::BOLD),
                );
            }
        }
    }

    // The dog: a pocket wanderer, `(ᴥ)` plus a wagging tail, drawn from the
    // shared lobby so every session sees the same trot. Napping slows the
    // tail and drifts a `z`; a fresh pet speeds it up, floats hearts, and
    // credits the petter.
    let dog = view.state.snapshot.dog;
    let (dx, dy) = (dog.x, dog.y);
    let amber = Style::default().fg(theme::AMBER());
    let petted = view.state.snapshot.dog_pet.as_ref();
    set(cells, dx.saturating_sub(1), dy, '(', amber);
    set(cells, dx, dy, 'ᴥ', amber);
    set(cells, dx + 1, dy, ')', amber);
    let wag = if petted.is_some() {
        2
    } else if dog.resting {
        16
    } else {
        5
    };
    let tail = if (t / wag).is_multiple_of(2) {
        '/'
    } else {
        '\\'
    };
    let tail_x = if dog.facing_left {
        dx + 2
    } else {
        dx.saturating_sub(2)
    };
    set(cells, tail_x, dy, tail, amber);
    if let Some((name, _)) = petted {
        let beat = ((t / 4) % 3) as u16;
        put_if_floor(
            cells,
            dx.saturating_sub(1) + beat,
            dy.saturating_sub(1),
            '♥',
            theme::ERROR(),
        );
        put_if_floor(
            cells,
            dx + 2,
            dy.saturating_sub(1) - (beat % 2),
            '♥',
            theme::ERROR(),
        );
        put_label(
            cells,
            dx,
            dy + 1,
            &format!("{} pets the dog", truncate_name(name)),
            Style::default().fg(theme::TEXT_FAINT()),
        );
    } else if dog.resting && (t / 40).is_multiple_of(3) {
        put_if_floor(
            cells,
            dx + 2,
            dy.saturating_sub(1),
            'z',
            theme::TEXT_FAINT(),
        );
    }
}

/// A 3-row stick figure standing on `(x, y)` (the feet cell). Degrades near
/// the top wall: torso needs one row of headroom, the head needs two.
fn draw_figure(cells: &mut Cells, x: u16, y: u16, head: char, style: Style) {
    set(cells, x, y, 'Λ', style);
    if y >= 2 {
        set(cells, x.saturating_sub(1), y - 1, '/', style);
        set(cells, x, y - 1, '|', style);
        set(cells, x + 1, y - 1, '\\', style);
    }
    if y >= 3 {
        set(cells, x, y - 2, head, style);
    }
}

/// A tipsy stick figure: the same 3-row build as [`draw_figure`], nudged to
/// read as unsteady the drunker the patron is. Buzzed (level 2) throws both
/// arms up in a loose "woo"; sloshed (level 3) slumps off-balance, one knee
/// buckling (`v` legs) and the head lolling onto a shoulder. Level 4 is
/// handled by [`draw_passed_out`], not here.
fn draw_drunk_figure(cells: &mut Cells, x: u16, y: u16, head: char, style: Style, level: u8) {
    let legs = if level >= 3 { 'v' } else { 'Λ' };
    set(cells, x, y, legs, style);
    if y >= 2 {
        // Buzzed throws both arms up; sloshed sags to a neutral, wobbly stance.
        let (left, right) = if level >= 3 { ('/', '\\') } else { ('\\', '/') };
        set(cells, x.saturating_sub(1), y - 1, left, style);
        set(cells, x, y - 1, '|', style);
        set(cells, x + 1, y - 1, right, style);
    }
    if y >= 3 {
        // Sloshed lolls the head onto a shoulder; buzzed keeps it upright.
        let head_x = if level >= 3 { x.saturating_sub(1) } else { x };
        set(cells, head_x, y - 2, head, style);
    }
}

/// A patron knocked out cold on the floor: an X-eyed head sprawled between
/// limp arms, with a little sleepy `z` drifting up. Drawn in place of the
/// upright stick figure once someone hits the top drunk level.
fn draw_passed_out(cells: &mut Cells, x: u16, y: u16, style: Style) {
    set(cells, x.saturating_sub(1), y, '_', style);
    set(cells, x, y, 'x', style);
    set(cells, x + 1, y, '_', style);
    if y >= 1 {
        set(
            cells,
            x,
            y - 1,
            'z',
            Style::default().fg(theme::TEXT_MUTED()),
        );
    }
}

/// True once a patron is at the top drunk bucket ("wasted") and should be
/// shown slumped/passed out rather than upright.
fn is_passed_out(drunk_level: u8) -> bool {
    drunk_level >= DRUNK_MAX_LEVEL
}

/// Where an occupant's head goes for a seat: perched above a stool, sunk
/// into an armchair.
fn seat_head_y(seat: &map::Seat) -> u16 {
    match seat.kind {
        map::SeatKind::Stool => seat.y - 1,
        map::SeatKind::Armchair => seat.y,
    }
}

/// Where speech bubbles anchor for each drawn person: the row just above
/// their name label, keyed by user id.
type BubbleAnchors = HashMap<Uuid, (u16, u16)>;

fn place_people(cells: &mut Cells, view: &ClubhouseView<'_>) -> (BubbleAnchors, Vec<ClubhouseHit>) {
    let state = view.state;
    let mut anchors: BubbleAnchors = HashMap::new();
    // Map-space clickable boxes, one per drawn person; the caller reprojects
    // them into terminal cells once the camera is known.
    let mut hits: Vec<ClubhouseHit> = Vec::new();

    // Staff first, so patrons' labels can never erase the bartender.
    let bartender_style = if state.bartender_online {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let (bx, by) = map::BARTENDER;
    set(cells, bx, by, 'O', bartender_style);
    set(cells, bx - 1, by + 1, '/', bartender_style);
    set(cells, bx, by + 1, '|', bartender_style);
    set(cells, bx + 1, by + 1, '\\', bartender_style);
    put_label(cells, bx, by - 1, "bartender", bartender_style);
    // No bubble anchor for the bartender: his lines render as the pinned
    // banner in the top-left corner (`draw_bartender_banner`), out of the
    // way of patron bubbles at the busy bar.

    if state.graybeard_online {
        let seat = map::GRAYBEARD_SEAT;
        let style = Style::default().fg(theme::TEXT_MUTED());
        let head_y = seat_head_y(&seat);
        set(cells, seat.x, head_y, 'o', style);
        let label_y = seat.y + 2;
        put_label(cells, seat.x, label_y, "graybeard", style);
        if let Some(id) = view.graybeard_user_id {
            anchors.insert(id, (seat.x, head_y.saturating_sub(1)));
            let half = "graybeard".len() as u16 / 2;
            hits.push(ClubhouseHit {
                user_id: id,
                username: "graybeard".to_string(),
                x0: seat.x.saturating_sub(half),
                y0: head_y,
                x1: seat.x + half,
                y1: label_y,
            });
        }
    }

    if state.bot_online {
        let (x, y) = map::BOT_SPOT;
        let style = Style::default().fg(theme::TEXT_MUTED());
        draw_figure(cells, x, y, 'o', style);
        let label_y = y.saturating_sub(3).max(1);
        put_label(cells, x, label_y, "bot", style);
        if let Some(id) = view.bot_user_id {
            anchors.insert(id, (x, label_y.saturating_sub(1)));
            let half = "bot".len() as u16 / 2;
            hits.push(ClubhouseHit {
                user_id: id,
                username: "bot".to_string(),
                x0: x.saturating_sub(half),
                y0: label_y,
                x1: x + half,
                y1: y,
            });
        }
    }

    let own_id = state.own_user_id();
    for who in state.snapshot.people.iter().filter(|p| p.user_id != own_id) {
        let style = Style::default().fg(occupant_color(who.user_id));
        let label_style = Style::default().fg(theme::TEXT_DIM());
        let (anchor, (x0, y0, x1, y1)) = draw_presence(
            cells,
            who.placement,
            'o',
            style,
            &who.username,
            label_style,
            view.name_flair.get(&who.user_id),
            who.drunk_level,
        );
        anchors.insert(who.user_id, anchor);
        hits.push(ClubhouseHit {
            user_id: who.user_id,
            username: who.username.clone(),
            x0,
            y0,
            x1,
            y1,
        });
        // A passed-out patron can't wave or dance.
        if !is_passed_out(who.drunk_level)
            && let Some(emote) = who.emote
        {
            draw_emote(cells, who.placement, emote, state.anim_tick, style);
        }
    }

    if view.state.snapshot.door_overflow > 0 {
        put_label(
            cells,
            map::DOOR_LABEL.0,
            map::DOOR_LABEL.1,
            &format!("+{} at the door", view.state.snapshot.door_overflow),
            Style::default().fg(theme::AMBER_DIM()),
        );
    }

    // You, last: always on top.
    let own_style = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let own_label_style = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let own_placement = state
        .snapshot
        .find(own_id)
        .map(|p| p.placement)
        .unwrap_or(Placement::Walking(state.player_x, state.player_y));
    let own_drunk_level = state
        .snapshot
        .find(own_id)
        .map(|p| p.drunk_level)
        .unwrap_or(0);
    let (anchor, (x0, y0, x1, y1)) = draw_presence(
        cells,
        own_placement,
        '@',
        own_style,
        view.own_username,
        own_label_style,
        view.name_flair.get(&own_id),
        own_drunk_level,
    );
    anchors.insert(own_id, anchor);
    hits.push(ClubhouseHit {
        user_id: own_id,
        username: view.own_username.to_string(),
        x0,
        y0,
        x1,
        y1,
    });
    if !is_passed_out(own_drunk_level)
        && let Some(emote) = state.snapshot.find(own_id).and_then(|p| p.emote)
    {
        draw_emote(cells, own_placement, emote, state.anim_tick, own_style);
    }

    (anchors, hits)
}

/// Draw one person at their placement and return their bubble anchor (the
/// row above their name label) plus a map-space clickable box `(x0, y0, x1,
/// y1)` spanning their figure and name label, for profile-on-click.
#[allow(clippy::too_many_arguments)]
fn draw_presence(
    cells: &mut Cells,
    placement: Placement,
    head: char,
    style: Style,
    username: &str,
    label_style: Style,
    flair: Option<&ResolvedName>,
    drunk_level: u8,
) -> ((u16, u16), (u16, u16, u16, u16)) {
    let passed_out = is_passed_out(drunk_level);
    let name_style = flair.and_then(|flair| flair.style);
    // A rented title trails the name here the way it does in chat. The floor
    // is a crowded character grid, so the title truncates to the same
    // `LABEL_MAX` the name does: a label is at most a name plus a title, never
    // wider.
    let label = clubhouse_label(username, flair);
    // The name label spans this many cells, centered on the avatar (matches
    // `put_label`), so the clickable box tracks the drawn label width.
    let label_w = UnicodeWidthStr::width(label.as_str()) as u16;
    let name_len = truncate_name(username).chars().count();
    // The crown sits one space after the name (`clubhouse_label`).
    let crown_at = flair
        .is_some_and(|flair| flair.crown)
        .then_some(name_len + 1);
    let label_span = |center: u16| {
        let x0 = center.saturating_sub(label_w / 2);
        (x0, x0 + label_w.saturating_sub(1))
    };
    match placement {
        Placement::Seated(i) => {
            let seat = &map::SEATS[i.min(map::SEATS.len() - 1)];
            let head_y = seat_head_y(seat);
            // Slumped in the seat: X-eyed head in place of the usual glyph.
            // (Buzzed/sloshed patrons keep their head but wear the drunk name
            // badge; a seat has no body to throw a pose with.)
            let seat_head = if passed_out { 'x' } else { head };
            set(cells, seat.x, head_y, seat_head, style);
            let label_y = if seat.label_below {
                seat.y + 2
            } else {
                head_y.saturating_sub(1).max(1)
            };
            put_label_styled(
                cells,
                seat.x,
                label_y,
                &label,
                name_len,
                crown_at,
                label_style,
                name_style,
            );
            let (lx0, lx1) = label_span(seat.x);
            let hit = (
                lx0.min(seat.x),
                head_y.min(label_y),
                lx1.max(seat.x),
                head_y.max(label_y),
            );
            let anchor = if seat.label_below {
                (seat.x, head_y.saturating_sub(1))
            } else {
                (seat.x, label_y.saturating_sub(1))
            };
            (anchor, hit)
        }
        Placement::Standing(_) | Placement::Door(_) | Placement::Walking(..) => {
            let (x, y) = placement.position();
            if passed_out {
                draw_passed_out(cells, x, y, style);
            } else if drunk_level >= DRUNK_LABEL_MIN_LEVEL {
                draw_drunk_figure(cells, x, y, head, style, drunk_level);
            } else {
                draw_figure(cells, x, y, head, style);
            }
            let label_y = y.saturating_sub(3).max(1);
            put_label_styled(
                cells,
                x,
                label_y,
                &label,
                name_len,
                crown_at,
                label_style,
                name_style,
            );
            let (lx0, lx1) = label_span(x);
            // Figure body is `x-1..=x+1`; the box unions it with the label.
            let hit = (lx0.min(x.saturating_sub(1)), label_y, lx1.max(x + 1), y);
            ((x, label_y.saturating_sub(1)), hit)
        }
    }
}

/// Two-frame emote animation on an avatar; walkers get full-body frames,
/// seated patrons get a marker beside the head.
fn draw_emote(cells: &mut Cells, placement: Placement, emote: Emote, tick: u64, style: Style) {
    let frame = (tick / 4).is_multiple_of(2);
    let note = Style::default().fg(theme::AMBER_GLOW());
    match placement {
        Placement::Seated(i) => {
            let seat = &map::SEATS[i.min(map::SEATS.len() - 1)];
            let head_y = seat_head_y(seat);
            match emote {
                Emote::Wave => {
                    let arm = if frame { '/' } else { '\'' };
                    set(cells, seat.x + 1, head_y, arm, style);
                }
                Emote::Dance => {
                    let (lx, rx) = (seat.x.saturating_sub(1), seat.x + 1);
                    let x = if frame { lx } else { rx };
                    set(cells, x, head_y, '♪', note.add_modifier(Modifier::BOLD));
                }
            }
        }
        Placement::Standing(_) | Placement::Door(_) | Placement::Walking(..) => {
            let (x, y) = placement.position();
            if y < 2 {
                return;
            }
            match emote {
                Emote::Wave => {
                    // The right arm swings up and down.
                    let (left, right) = if frame { ('─', '/') } else { ('/', '\\') };
                    set(cells, x.saturating_sub(1), y - 1, left, style);
                    set(cells, x + 1, y - 1, right, style);
                }
                Emote::Dance => {
                    // Arms flail, a note bounces side to side.
                    let (left, right) = if frame { ('\\', '/') } else { ('/', '\\') };
                    set(cells, x.saturating_sub(1), y - 1, left, style);
                    set(cells, x + 1, y - 1, right, style);
                    if y >= 3 {
                        let nx = if frame { x.saturating_sub(2) } else { x + 2 };
                        set(cells, nx, y - 2, '♪', note);
                    }
                }
            }
        }
    }
}

/// `* name slipped in` lines stacked over the welcome mat.
fn draw_door_events(cells: &mut Cells, view: &ClubhouseView<'_>) {
    let base_y = 41u16;
    for (i, event) in view.state.door_events.iter().enumerate().take(4) {
        let verb = if event.arrived {
            "slipped in"
        } else {
            "headed out"
        };
        put_label(
            cells,
            map::SPAWN.0,
            base_y + i as u16,
            &format!("* {} {}", truncate_name(&event.username), verb),
            Style::default().fg(theme::TEXT_FAINT()),
        );
    }
}

/// Fresh #lounge messages float over their author's head.
fn draw_bubbles(cells: &mut Cells, view: &ClubhouseView<'_>, anchors: &BubbleAnchors) {
    for message in fresh_bubble_messages(view.lounge_messages, chrono::Utc::now()) {
        let Some(&(x, bottom_y)) = anchors.get(&message.user_id) else {
            continue;
        };
        let lines = wrap_bubble_fitting(bubble_text(&message.body));
        if lines.is_empty() {
            continue;
        }
        draw_bubble_box(cells, x, bottom_y, &lines);
    }
}

/// The bubble-worthy slice of a room tail: the newest fresh message per
/// author. Room message lists are newest-first (see
/// `ChatState::push_message`), so iterate in natural order and stop at the
/// first stale message.
fn fresh_bubble_messages(
    messages: &[ChatMessage],
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<&ChatMessage> {
    let mut seen_authors: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    let mut fresh = Vec::new();
    for message in messages {
        let age_ms = now
            .signed_duration_since(message.created)
            .num_milliseconds();
        if age_ms > BUBBLE_MS {
            break;
        }
        if seen_authors.insert(message.user_id) {
            fresh.push(message);
        }
    }
    fresh
}

/// The bubble body: replies drop their quote line; whitespace collapses.
fn bubble_text(body: &str) -> String {
    let body = match body.split_once('\n') {
        Some((first, rest)) if first.trim_start().starts_with("> ") && !rest.trim().is_empty() => {
            rest
        }
        _ => body,
    };
    to_single_width(&body.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// Fold user-controlled text down to one terminal cell per char so it lands
/// cleanly in the tavern grid, which assumes single-width glyphs everywhere.
/// Wide glyphs (emoji, CJK) and zero-width/combining marks would otherwise
/// desync the row they draw into; each is replaced with a `·` placeholder.
fn to_single_width(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.width() == Some(1) { ch } else { '·' })
        .collect()
}

/// Wrap at the narrowest width tier that fits the whole message; fall back
/// to the widest tier with an ellipsis.
fn wrap_bubble_fitting(text: String) -> Vec<String> {
    for width in BUBBLE_WIDTHS {
        let (lines, truncated) = wrap_bubble(text.clone(), width, BUBBLE_MAX_LINES);
        if !truncated {
            return lines;
        }
    }
    wrap_bubble(
        text,
        BUBBLE_WIDTHS[BUBBLE_WIDTHS.len() - 1],
        BUBBLE_MAX_LINES,
    )
    .0
}

/// Greedy word wrap into at most `max_lines` lines of `width` chars; the
/// last line gets an ellipsis when the text keeps going, and the flag
/// reports whether that happened.
fn wrap_bubble(text: String, width: usize, max_lines: usize) -> (Vec<String>, bool) {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut truncated = false;
    for word in text.split_whitespace() {
        let word: String = word.chars().take(width).collect();
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
            if lines.len() == max_lines {
                truncated = true;
                break;
            }
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(&word);
    }
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(current);
    } else if !current.is_empty() {
        truncated = true;
    }
    if truncated && let Some(last) = lines.last_mut() {
        while last.chars().count() >= width {
            last.pop();
        }
        last.push('…');
    }
    (lines, truncated)
}

/// A bordered speech bubble whose bottom row sits at `bottom_y`, centered
/// on `x`. Flips below the anchor when the top wall is too close.
fn draw_bubble_box(cells: &mut Cells, x: u16, bottom_y: u16, lines: &[String]) {
    let text_w = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as u16;
    let box_w = text_w + 4;
    let box_h = lines.len() as u16 + 2;
    let mut top = bottom_y.saturating_sub(box_h - 1);
    if top == 0 {
        top = bottom_y.saturating_add(2).min(map::MAP_H - 1 - box_h);
    }
    let max_left = map::MAP_W.saturating_sub(box_w + 1);
    let left = x.saturating_sub(box_w / 2).clamp(1, max_left.max(1));

    let border = Style::default().fg(theme::TEXT_MUTED());
    let text = Style::default().fg(theme::TEXT_BRIGHT());
    for row in 0..box_h {
        for col in 0..box_w {
            let (cx, cy) = (left + col, top + row);
            let ch = match (row, col) {
                (0, 0) => '╭',
                (0, c) if c == box_w - 1 => '╮',
                (r, 0) if r == box_h - 1 => '╰',
                (r, c) if r == box_h - 1 && c == box_w - 1 => '╯',
                (0, _) => '─',
                (r, _) if r == box_h - 1 => '─',
                (_, 0) => '│',
                (_, c) if c == box_w - 1 => '│',
                _ => ' ',
            };
            let style = if ch == ' ' { text } else { border };
            set(cells, cx, cy, ch, style);
        }
        if row > 0 && row < box_h - 1 {
            let line = &lines[(row - 1) as usize];
            for (i, ch) in line.chars().enumerate() {
                set(cells, left + 2 + i as u16, top + row, ch, text);
            }
        }
    }
}

fn draw_overlays(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) {
    draw_bartender_banner(frame, inner, view);
    if draw_tutorial(frame, inner, view) {
        return;
    }
    draw_popover(frame, inner, view);
}

/// The bartender speaks to the whole room: his #lounge lines pin to the
/// top-left corner of the viewport (camera-independent, so you never miss
/// him from across the tavern) instead of bubbling over his sprite, where
/// patron bubbles at the bar would collide with it. Which line shows, and
/// for how long, is the banner queue's call (`State::update_bartender_banner`):
/// a burst of answers plays one at a time instead of overwriting itself.
fn draw_bartender_banner(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) {
    let body = match view.state.bartender_banner_line() {
        None => return,
        Some(BannerLine::Local(line)) => line.as_str(),
        Some(BannerLine::Lounge(message_id)) => {
            let Some(message) = view.lounge_messages.iter().find(|m| m.id == *message_id) else {
                return;
            };
            message.body.as_str()
        }
    };
    // Roomy on purpose: his replies are up to three sanitized lines of real
    // directions, and the banner is the only place they render.
    let width_budget = usize::from(inner.width.saturating_sub(6)).min(56);
    let (lines, _) = wrap_bubble(bubble_text(body), width_budget.max(16), 8);
    if lines.is_empty() {
        return;
    }

    let border = Style::default().fg(theme::ERROR());
    let text = Style::default().fg(theme::TEXT_BRIGHT());
    let title = " O the bartender ";
    let width = (lines
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(inner.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(inner.height);
    let rect = Rect {
        x: inner.x + 1,
        y: inner.y,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .map(|l| Line::from(Span::styled(l, text)))
                .collect::<Vec<_>>(),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(title, border.add_modifier(Modifier::BOLD))),
        ),
        rect,
    );
}

/// The first-visit tour's tavern boxes: the welcome mat, the wander nudge,
/// and the homecoming send-off. The page stops between them live in
/// [`draw_tour_overlay`], which also renders a reminder here when a mid-tour
/// player wanders home early. Returns true when a tutorial overlay owned the
/// frame (prop popovers wait their turn).
fn draw_tutorial(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) -> bool {
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let border = Style::default().fg(theme::AMBER());

    let (title, lines): (&str, Vec<Line>) = match view.state.tutorial {
        Tutorial::Welcome => (
            " ☾ welcome to the late lounge ☽ ",
            vec![
                Line::from(vec![
                    Span::styled(
                        "late.sh",
                        Style::default()
                            .fg(theme::TEXT_BRIGHT())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(": a late-night clubhouse that lives in a terminal.", text),
                ]),
                Line::from(Span::styled(
                    "one tavern and six pages: chat, games, a shared canvas,",
                    text,
                )),
                Line::from(Span::styled(
                    "the people, the scores. everyone you'll see here is real.",
                    text,
                )),
                Line::default(),
                Line::from(Span::styled(
                    "you're on the welcome mat. let's take the tour.",
                    text,
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("[1] ", key),
                    Span::styled("first stop: the chat", text),
                ]),
            ],
        ),
        Tutorial::Homecoming => (
            " ☾ make yourself at home ☽ ",
            vec![
                Line::from(Span::styled(
                    "that was the house. this room is its map:",
                    text,
                )),
                Line::from(Span::styled(
                    "walk up to anything and press Enter to step through.",
                    text,
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("[arrows/hjkl] ", key),
                    Span::styled("walk around", text),
                ]),
                Line::from(vec![
                    Span::styled("[i] ", key),
                    Span::styled("say something, it floats over your head", text),
                ]),
                Line::from(vec![
                    Span::styled("[w] ", key),
                    Span::styled("wave · ", text),
                    Span::styled("[x] ", key),
                    Span::styled("dance · ", text),
                    Span::styled("[Ctrl+G] ", key),
                    Span::styled("the lobby", text),
                ]),
                Line::from(vec![
                    Span::styled("[Ctrl+O] ", key),
                    Span::styled("introduce yourself · ", text),
                    Span::styled("[?] ", key),
                    Span::styled("the full guide", text),
                ]),
                Line::default(),
                Line::from(Span::styled(
                    "psst: see the bar glowing, northwest? walk over.",
                    dim,
                )),
                Line::from(Span::styled(
                    "the bartender pours every new face their first drink.",
                    dim,
                )),
                Line::default(),
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("settle in", dim),
                ]),
            ],
        ),
        // Mid-loop stages never render in the tavern: the forced gate only
        // lets the route's digits through, and `0` lands straight on
        // Homecoming.
        Tutorial::VisitChat
        | Tutorial::VisitMusic
        | Tutorial::VisitArcade
        | Tutorial::VisitLobby
        | Tutorial::VisitGames
        | Tutorial::VisitArtboard
        | Tutorial::VisitDirectory
        | Tutorial::VisitLeaderboard
        | Tutorial::Off
        | Tutorial::Pending
        | Tutorial::Done => return false,
    };

    let width = (lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(inner.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(inner.height.saturating_sub(1));
    let rect = Rect {
        x: inner.x + (inner.width.saturating_sub(width)) / 2,
        y: inner.y + (inner.height.saturating_sub(height)) / 3,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(title, border.add_modifier(Modifier::BOLD))),
        ),
        rect,
    );
    true
}

/// The page stops of the first-visit tour, drawn centered over the page
/// they pitch (`render.rs` calls this on every non-clubhouse screen). The
/// forced gate guarantees the current screen is the stop's own page, so a
/// mismatch, like off-tour stages, draws nothing.
pub fn draw_tour_overlay(frame: &mut Frame, area: Rect, stage: Tutorial, screen: Screen) {
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    // Proper nouns (game names, late.sh, Late Chips) pop out of the prose so
    // a skimming eye still catches the roster.
    let name = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let border = Style::default().fg(theme::AMBER());

    let (home, title, pitch, next_key, next_label): (Screen, &str, Vec<Line>, &str, &str) =
        match stage {
            Tutorial::VisitChat => (
                Screen::Dashboard,
                " ✦ the tour · home ",
                vec![
                    Line::from(vec![
                        Span::styled("every room, thread and DM on ", text),
                        Span::styled("late.sh", name),
                        Span::styled(" lives here.", text),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("[i] ", key),
                        Span::styled("write · ", text),
                        Span::styled("[Ctrl+/] ", key),
                        Span::styled("jump anywhere, type ?query to search", text),
                    ]),
                    Line::from(vec![
                        Span::styled("[Ctrl+]] ", key),
                        Span::styled("pick an icon to sign your messages", text),
                    ]),
                    Line::from(vec![
                        Span::styled("[/dm @user] ", key),
                        Span::styled("direct message · ", text),
                        Span::styled("[/public #room] ", key),
                        Span::styled("open a room", text),
                    ]),
                    Line::from(vec![
                        Span::styled("[/private #room] ", key),
                        Span::styled("invite-only · your ", text),
                        Span::styled("Mentions", name),
                        Span::styled(" wait in the rail", text),
                    ]),
                ],
                "Enter",
                "the music",
            ),
            Tutorial::VisitMusic => (
                Screen::Dashboard,
                " ✦ the tour · the music ",
                vec![
                    Line::from(Span::styled(
                        "the house has a soundtrack, always on: our own radio",
                        text,
                    )),
                    Line::from(vec![
                        Span::styled("streams, ", text),
                        Span::styled("Nightride", name),
                        Span::styled(" guest stations, and a community ", text),
                        Span::styled("YouTube", name),
                    ]),
                    Line::from(Span::styled(
                        "jukebox: queue tracks, vote on them, browse the history.",
                        text,
                    )),
                    Line::default(),
                    Line::from(Span::styled(
                        "one catch: SSH carries no sound. two ways to listen:",
                        text,
                    )),
                    Line::from(vec![
                        Span::styled("late.sh/listen", name),
                        Span::styled(" plays it in any browser, nothing to install;", text),
                    ]),
                    Line::from(vec![
                        Span::styled("the ", text),
                        Span::styled("late", name),
                        Span::styled(" CLI plays it right here in your terminal.", text),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("[v v] ", key),
                        Span::styled("Music Booth · ", text),
                        Span::styled("[v x] ", key),
                        Span::styled("source · ", text),
                        Span::styled("[?] ", key),
                        Span::styled("the install guide", text),
                    ]),
                ],
                "2",
                "the arcade",
            ),
            Tutorial::VisitArcade => (
                Screen::Arcade,
                " ✦ the tour · the arcade ",
                vec![
                    Line::from(vec![
                        Span::styled("solo games: ", text),
                        Span::styled("Lateris, Snake, 2048, Sudoku, Solitaire", name),
                        Span::styled("...", text),
                    ]),
                    Line::from(vec![
                        Span::styled("daily puzzles pay ", text),
                        Span::styled("Late Chips", name),
                        Span::styled("; quests and streaks stack up top.", text),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("chips buy things. ", text),
                        Span::styled("[/shop] ", key),
                        Span::styled("rented badges, flags, titles, name effects,", text),
                    ]),
                    Line::from(Span::styled(
                        "a pet companion to feed, an aquarium with real fish.",
                        text,
                    )),
                ],
                "Enter",
                "the lobby",
            ),
            Tutorial::VisitLobby => (
                Screen::Arcade,
                " ✦ the tour · the lobby ",
                vec![
                    Line::from(vec![
                        Span::styled("[Ctrl+G] ", key),
                        Span::styled("opens the lobby from anywhere: all the multiplayer.", text),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("seven daily duels: ", text),
                        Span::styled("chess, backgammon, battleship,", name),
                    ]),
                    Line::from(vec![
                        Span::styled("connect four, reversi, checkers, briscola", name),
                        Span::styled(". challenge anyone,", text),
                    ]),
                    Line::from(Span::styled(
                        "walk away, play a move whenever; 24h on the clock per move.",
                        text,
                    )),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("five live tables, always open: ", text),
                        Span::styled("Poker, Blackjack,", name),
                    ]),
                    Line::from(vec![
                        Span::styled("Asterion, Tron, Super Snake", name),
                        Span::styled(". every table seats its own", text),
                    ]),
                    Line::from(Span::styled("chat and voice; pull up a chair.", text)),
                ],
                "3",
                "the heavy door",
            ),
            Tutorial::VisitGames => (
                Screen::Games,
                " ✦ the tour · the games ",
                vec![
                    Line::from(Span::styled("the big ones live behind this door:", text)),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("Lateania", name),
                        Span::styled(": our own MMO. one shared world, bosses, mounts.", text),
                    ]),
                    Line::from(vec![
                        Span::styled("DCSS", name),
                        Span::styled(": the most played roguelike alive, for good reason.", text),
                    ]),
                    Line::from(Span::styled(
                        "pick a species, pledge a god, dive for the Orb of Zot;",
                        text,
                    )),
                    Line::from(Span::styled(
                        "easy to start, years to master, no two runs alike.",
                        text,
                    )),
                    Line::from(vec![
                        Span::styled("NetHack", name),
                        Span::styled(
                            ": the legend itself; the DevTeam thought of everything.",
                            text,
                        ),
                    ]),
                    Line::from(vec![
                        Span::styled("Brogue", name),
                        Span::styled(": the most beautiful dungeon ASCII ever drew.", text),
                    ]),
                    Line::from(vec![
                        Span::styled("Green Dragon", name),
                        Span::styled(": the legendary BBS door, reborn.", text),
                    ]),
                    Line::default(),
                    Line::from(Span::styled(
                        "and much more; your wins stick to your name.",
                        text,
                    )),
                ],
                "4",
                "the artboard",
            ),
            Tutorial::VisitArtboard => (
                Screen::Artboard,
                " ✦ the tour · the artboard ",
                vec![
                    Line::from(Span::styled(
                        "one shared canvas, the whole house draws at once.",
                        text,
                    )),
                    Line::from(Span::styled(
                        "everything stays, and every glyph remembers who drew it.",
                        text,
                    )),
                    Line::from(vec![
                        Span::styled("the whole board hangs public at ", text),
                        Span::styled("late.sh/gallery", name),
                        Span::styled(".", text),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("[i] ", key),
                        Span::styled("or a click starts drawing · ", text),
                        Span::styled("[Ctrl+]] ", key),
                        Span::styled("the glyph picker", text),
                    ]),
                    Line::from(vec![
                        Span::styled("[g] ", key),
                        Span::styled("time-travel the snapshots · ", text),
                        Span::styled("[?] ", key),
                        Span::styled("the local guide, here and everywhere", text),
                    ]),
                ],
                "5",
                "the profiles",
            ),
            Tutorial::VisitDirectory => (
                Screen::Profiles,
                " ✦ the tour · the profiles ",
                vec![
                    Line::from(Span::styled(
                        "the people: everyone who ships a project or posts a work card.",
                        text,
                    )),
                    Line::from(vec![
                        Span::styled("your profile gets a public page at ", text),
                        Span::styled("late.sh/profiles", name),
                        Span::styled(".", text),
                    ]),
                    Line::default(),
                    Line::from(vec![
                        Span::styled("[Ctrl+O] ", key),
                        Span::styled("fill yours in: bio, links, what you're building.", text),
                    ]),
                ],
                "6",
                "the leaderboards",
            ),
            Tutorial::VisitLeaderboard => (
                Screen::Leaderboard,
                " ✦ the tour · the leaderboards ",
                vec![
                    Line::from(Span::styled(
                        "every game keeps score: chips, wins, streaks, high scores,",
                        text,
                    )),
                    Line::from(Span::styled(
                        "monthly and all-time. each month's top three wear badges",
                        text,
                    )),
                    Line::from(Span::styled(
                        "beside their name in chat, for everyone to see.",
                        text,
                    )),
                    Line::default(),
                    Line::from(Span::styled(
                        "your name lands here sooner than you think.",
                        text,
                    )),
                ],
                "0",
                "home to the lounge",
            ),
            Tutorial::Off
            | Tutorial::Pending
            | Tutorial::Welcome
            | Tutorial::Homecoming
            | Tutorial::Done => return,
        };

    if screen != home {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut lines = pitch;
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "take it in; everything above unlocks when the tour ends.",
        dim,
    )));
    lines.push(Line::from(vec![
        Span::styled(format!("[{next_key}] "), key),
        Span::styled(format!("next: {next_label}"), text),
    ]));

    let width = (lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(area.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(title, border.add_modifier(Modifier::BOLD))),
        ),
        rect,
    );
}

fn draw_popover(frame: &mut Frame, inner: Rect, view: &ClubhouseView<'_>) {
    let Some(prop) = view.state.nearby() else {
        return;
    };

    let interactive = Style::default().fg(theme::ERROR());
    let flavor = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);

    let (title, border, lines): (&str, Style, Vec<Line>) = match prop {
        map::Interactive::Bartender => (
            " O the bartender ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[t] ", key),
                    Span::styled("talk to the bartender", text),
                ]),
                Line::from(Span::styled(
                    "ask about the house: rooms, music, games",
                    dim,
                )),
            ],
        ),
        map::Interactive::Jukebox => {
            let now = view
                .now_playing
                .map(|np| format!("♪ {}", np.track))
                .unwrap_or_else(|| "the jukebox hums to itself".to_string());
            (
                " ♫ jukebox ",
                interactive,
                vec![
                    Line::from(Span::styled(now, Style::default().fg(theme::AMBER_GLOW()))),
                    Line::from(Span::styled("v v music booth · v x cycle source", text)),
                    Line::from(Span::styled("v s skip vote · v 1-4 pick a station", text)),
                    Line::from(Span::styled("m mute · +/- volume · Enter opens booth", dim)),
                    Line::from(Span::styled("[?] full guide, opens on the Pair tab", dim)),
                ],
            )
        }
        map::Interactive::Arcade => (
            " ● arcade cabinet ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("play, the Arcade is page 2", text),
                ]),
                Line::from(Span::styled("daily puzzles, high scores, chips", dim)),
            ],
        ),
        map::Interactive::Doors => (
            " ○ the heavy door ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("the door games, page 3", text),
                ]),
                Line::from(Span::styled(
                    "Lateania · NetHack · DCSS · Brogue · Usurper · Green Dragon · dopewars · Rebels",
                    dim,
                )),
            ],
        ),
        map::Interactive::Poker => (
            " ♠ the big table ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("the Lobby: house tables + daily games", text),
                ]),
                Line::from(Span::styled(
                    "poker · blackjack · asterion · tron, chips on the line",
                    dim,
                )),
            ],
        ),
        map::Interactive::Easel => (
            " ° the easel ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("the Artboard, page 5", text),
                ]),
                Line::from(Span::styled("one shared canvas, everyone paints", dim)),
            ],
        ),
        map::Interactive::Dog => (
            " ∪ the dog ",
            flavor,
            vec![
                Line::from(vec![
                    Span::styled("[Enter] ", key),
                    Span::styled("pet the dog", text),
                ]),
                Line::from(Span::styled(
                    "thumps tail. has never once deployed on a friday.",
                    dim,
                )),
            ],
        ),
        map::Interactive::Fireplace => (
            " )( fireplace ",
            flavor,
            vec![Line::from(Span::styled(
                "the fire crackles. someone kept your seat warm.",
                text,
            ))],
        ),
    };

    let width = (lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(inner.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(inner.height.saturating_sub(1));
    let rect = Rect {
        x: inner.x + inner.width.saturating_sub(width + 1),
        y: inner.y + inner.height.saturating_sub(height),
        width,
        height,
    };

    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(Span::styled(title, border.add_modifier(Modifier::BOLD))),
        ),
        rect,
    );
}

fn set(cells: &mut Cells, x: u16, y: u16, ch: char, style: Style) {
    if x < map::MAP_W && y < map::MAP_H {
        cells[usize::from(y)][usize::from(x)] = (ch, style);
    }
}

/// Draw only over bare floor so scenery never gets stomped by an effect.
fn put_if_floor(cells: &mut Cells, x: u16, y: u16, ch: char, color: ratatui::style::Color) {
    if x < map::MAP_W && y < map::MAP_H && matches!(map::char_at(x, y), ' ' | '░') {
        cells[usize::from(y)][usize::from(x)] = (ch, Style::default().fg(color));
    }
}

/// Write a name centered on `x_center`, clamped inside the walls.
fn put_label(cells: &mut Cells, x_center: u16, y: u16, label: &str, style: Style) {
    put_label_styled(cells, x_center, y, label, 0, None, style, None);
}

/// Paints a floor label. `name_len` is how many leading characters of `label`
/// are the username: only those take the bought color effect, so a trailing
/// `, title` stays in the dim label color, exactly as in chat. `crown_at` is
/// the char index of the crown glyph, painted in the same amber chat uses
/// (`push_author_prefix_spans`) rather than the name's effect or the dim
/// label color. Cells are one column each; a double-width char
/// (the crown) takes its cell plus a `WIDE_TAIL`, so the label is placed by
/// display width, not char count.
#[allow(clippy::too_many_arguments)]
fn put_label_styled(
    cells: &mut Cells,
    x_center: u16,
    y: u16,
    label: &str,
    name_len: usize,
    crown_at: Option<usize>,
    style: Style,
    name_style: Option<NameStyle>,
) {
    if y == 0 || y >= map::MAP_H - 1 {
        return;
    }
    let width = UnicodeWidthStr::width(label) as u16;
    let max_start = map::MAP_W.saturating_sub(width + 1);
    let start = x_center
        .saturating_sub(width / 2)
        .clamp(1, max_start.max(1));
    let mut col = start;
    for (i, ch) in label.chars().enumerate() {
        let cell_style = match name_style {
            _ if crown_at == Some(i) => style.fg(theme::AMBER_GLOW()),
            Some(name_style) if i < name_len => style.fg(char_color(name_style, i, name_len)),
            Some(_) | None => style,
        };
        set(cells, col, y, ch, cell_style);
        col += 1;
        if ch.width() == Some(2) {
            set(cells, col, y, WIDE_TAIL, cell_style);
            col += 1;
        }
    }
}

/// The floor label for one patron: the truncated name, the crown glyph when
/// they wear it, then `, <title>` when a title is rented, itself truncated to
/// `LABEL_MAX`. The glyph follows the name after one space exactly as it does
/// in chat, so the two surfaces read the same; it is the one wide char a label can hold
/// (names and titles are folded to single width), and `put_label_styled`
/// spends two cells on it.
pub(crate) fn clubhouse_label(username: &str, flair: Option<&ResolvedName>) -> String {
    let mut label = truncate_name(username);
    if flair.is_some_and(|flair| flair.crown) {
        label.push(' ');
        label.push_str(CROWN_GLYPH);
    }
    if let Some(title) = flair.and_then(|flair| flair.title.as_deref()) {
        label.push_str(&format!(", {}", truncate_name(title)));
    }
    label
}

pub(crate) fn truncate_name(name: &str) -> String {
    let name = to_single_width(name);
    if name.chars().count() <= LABEL_MAX {
        return name;
    }
    let mut out: String = name.chars().take(LABEL_MAX - 1).collect();
    out.push('…');
    out
}

fn occupant_color(user_id: uuid::Uuid) -> ratatui::style::Color {
    let palette = [
        theme::CHAT_AUTHOR(),
        theme::SUCCESS(),
        theme::AMBER(),
        theme::MENTION(),
        theme::TEXT_BRIGHT(),
    ];
    let h = mix(user_id.as_u128() as u64);
    palette[(h % palette.len() as u64) as usize]
}

fn mix(mut v: u64) -> u64 {
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    v
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
