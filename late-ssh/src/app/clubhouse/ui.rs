//! Clubhouse renderer: the tavern viewport (camera-follow over the floor
//! plan, live occupants, animated fire/jukebox/cat, proximity popovers) with
//! the embedded #lounge chat pinned to the bottom of the screen.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;
use crate::app::files::terminal_image::TerminalImageFrame;
use late_core::api_types::NowPlaying;

use super::map;
use super::state::State;

const LABEL_MAX: usize = 10;
const FIRE_CHARS: [char; 6] = ['(', ')', '~', '^', '*', '\''];
const EQ_CHARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub struct ClubhouseView<'a> {
    pub state: &'a State,
    pub own_username: &'a str,
    pub now_playing: Option<&'a NowPlaying>,
    pub chat: Option<crate::app::chat::ui::EmbeddedRoomChatView<'a>>,
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: ClubhouseView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    // Bottom ~30% is the live #lounge; the tavern gets the rest.
    let chat_height = ((u32::from(area.height) * 3 / 10) as u16)
        .clamp(8, 12)
        .min(area.height.saturating_sub(8));
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(chat_height)]).split(area);

    draw_tavern(frame, layout[0], &view);

    if let Some(chat) = view.chat {
        crate::app::chat::ui::draw_embedded_room_chat(frame, layout[1], chat, terminal_images);
    }
}

fn draw_tavern(frame: &mut Frame, area: Rect, view: &ClubhouseView<'_>) {
    let state = view.state;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::AMBER_DIM()))
        .title(Line::from(vec![
            Span::styled(
                " ☾ the clubhouse ",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("· {} inside ", state.headcount()),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]))
        .title_bottom(Line::from(Span::styled(
            " arrows/hjkl walk · i chat · Enter interact · J/K messages ",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 4 || inner.height < 4 {
        return;
    }

    let mut cells = styled_base_grid();
    animate(&mut cells, view);
    place_people(&mut cells, view);

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

    let mut lines: Vec<Line> = Vec::with_capacity(vh);
    for _ in 0..pad_y {
        lines.push(Line::default());
    }
    for row in cells.iter().skip(cam_y).take(vh.saturating_sub(pad_y)) {
        let mut spans: Vec<Span> = Vec::with_capacity(vw);
        if pad_x > 0 {
            spans.push(Span::raw(" ".repeat(pad_x)));
        }
        for &(ch, style) in row.iter().skip(cam_x).take(vw.saturating_sub(pad_x)) {
            spans.push(Span::styled(ch.to_string(), style));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    draw_popover(frame, inner, view);
}

fn camera_origin(player: usize, viewport: usize, map_len: usize) -> usize {
    if viewport >= map_len {
        return 0;
    }
    player.saturating_sub(viewport / 2).min(map_len - viewport)
}

type Cells = Vec<Vec<(char, Style)>>;

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
    // Interactive props glow red so they read as "walk up to me".
    if map::JUKEBOX.contains(x, y) {
        return match ch {
            '♪' => Style::default().fg(theme::AMBER_GLOW()),
            '[' | ']' | '·' => Style::default().fg(theme::TEXT_MUTED()),
            _ => Style::default().fg(theme::ERROR()),
        };
    }
    if map::DARTBOARD.contains(x, y) {
        return match ch {
            '◎' => Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
            '×' => Style::default().fg(theme::ERROR()),
            '─' | '│' | '┌' | '┐' | '└' | '┘' => dim,
            _ => Style::default().fg(theme::TEXT_MUTED()),
        };
    }
    match ch {
        '║' | '═' | '╔' | '╗' | '╚' | '╝' | '╡' | '╞' => dim,
        '▔' | '▄' | '▀' => Style::default().fg(theme::AMBER_DIM()),
        '█' => Style::default().fg(theme::TEXT_MUTED()),
        '╥' => Style::default().fg(theme::AMBER()),
        '╿' | '╽' | '▯' => Style::default().fg(theme::TEXT_MUTED()),
        '≡' => Style::default().fg(theme::AMBER_DIM()),
        '╭' | '╮' | '╰' | '╯' | '─' | '│' => Style::default().fg(theme::AMBER_DIM()),
        '▒' => Style::default().fg(theme::TEXT_FAINT()),
        'h' => dim,
        '░' => Style::default().fg(theme::TEXT_FAINT()),
        '♣' => Style::default().fg(theme::SUCCESS()),
        '◎' => Style::default().fg(theme::AMBER_GLOW()),
        _ if ch.is_ascii_alphabetic() => Style::default().fg(theme::AMBER_DIM()),
        _ => Style::default().fg(theme::TEXT_MUTED()),
    }
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

    // Notes drift up and away from the jukebox.
    if view.now_playing.is_some() {
        let phase = (t / 5) % 6;
        let note_x = 74u16.saturating_sub((phase / 2) as u16);
        let note_y = 6u16.saturating_sub(phase as u16).max(1);
        put_if_floor(cells, note_x, note_y, '♪', theme::AMBER_GLOW());
        let phase2 = (t / 5 + 3) % 6;
        let note2_x = 73u16.saturating_sub((phase2 / 2) as u16);
        let note2_y = 7u16.saturating_sub(phase2 as u16).max(1);
        put_if_floor(cells, note2_x, note2_y, '♫', theme::AMBER());
    }

    // The cat: tail flick, occasional z.
    let tail = if (t / 12) % 4 == 0 { '-' } else { '~' };
    set(
        cells,
        map::CAT.0,
        map::CAT.1,
        tail,
        Style::default().fg(theme::AMBER_DIM()),
    );
    set(
        cells,
        map::CAT.0 + 1,
        map::CAT.1,
        'o',
        Style::default().fg(theme::AMBER()),
    );
    if (t / 40) % 3 == 0 {
        put_if_floor(
            cells,
            map::CAT.0 + 2,
            map::CAT.1 - 1,
            'z',
            theme::TEXT_FAINT(),
        );
    }
}

fn place_people(cells: &mut Cells, view: &ClubhouseView<'_>) {
    let state = view.state;

    // Staff first, so patrons' labels can never erase the bartender.
    let bartender_style = if state.bartender_online {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    set(
        cells,
        map::BARTENDER.0,
        map::BARTENDER.1,
        '☻',
        bartender_style,
    );
    put_label(
        cells,
        map::BARTENDER.0,
        map::BARTENDER.1 - 1,
        "bartender",
        bartender_style,
    );

    if state.graybeard_online {
        let seat = map::GRAYBEARD_SEAT;
        let style = Style::default().fg(theme::TEXT_MUTED());
        set(cells, seat.x, seat.y, '☺', style);
        put_label(cells, seat.x, seat.y - 1, "graybeard", style);
    }

    for (seat, who) in state.seated() {
        let style = Style::default().fg(occupant_color(who.user_id));
        set(cells, seat.x, seat.y, '☺', style);
        let label_y = if seat.label_below {
            seat.y + 1
        } else {
            seat.y - 1
        };
        put_label(
            cells,
            seat.x,
            label_y,
            &truncate_name(&who.username),
            Style::default().fg(theme::TEXT_DIM()),
        );
    }

    for ((x, y), who) in state.standing() {
        let style = Style::default().fg(occupant_color(who.user_id));
        set(cells, x, y, '☺', style);
        put_label(
            cells,
            x,
            y - 1,
            &truncate_name(&who.username),
            Style::default().fg(theme::TEXT_DIM()),
        );
    }

    let door_count = state.door_count();
    if door_count > 0 {
        put_label(
            cells,
            58,
            24,
            &format!("+{door_count} at the door"),
            Style::default().fg(theme::AMBER_DIM()),
        );
    }

    // You, last: always on top.
    set(
        cells,
        state.player_x,
        state.player_y,
        '@',
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD),
    );
    put_label(
        cells,
        state.player_x,
        state.player_y.saturating_sub(1).max(1),
        &truncate_name(view.own_username),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
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

    let (title, border, lines): (&str, Style, Vec<Line>) = match prop {
        map::Interactive::Bartender => (
            " ☻ the bartender ",
            interactive,
            vec![
                Line::from(vec![
                    Span::styled(
                        "[t] ",
                        Style::default()
                            .fg(theme::AMBER_GLOW())
                            .add_modifier(Modifier::BOLD),
                    ),
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
                ],
            )
        }
        map::Interactive::Dartboard => (
            " ◎ darts ",
            flavor,
            vec![Line::from(Span::styled(
                "the real board lives on page 5, the Artboard",
                text,
            ))],
        ),
        map::Interactive::Cat => (
            " ~o the cat ",
            flavor,
            vec![Line::from(Span::styled(
                "purring at 15 fps. do not deploy on fridays.",
                text,
            ))],
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
    if y == 0 || y >= map::MAP_H - 1 {
        return;
    }
    let len = label.chars().count() as u16;
    let max_start = map::MAP_W.saturating_sub(len + 1);
    let start = x_center.saturating_sub(len / 2).clamp(1, max_start.max(1));
    for (i, ch) in label.chars().enumerate() {
        set(cells, start + i as u16, y, ch, style);
    }
}

pub(crate) fn truncate_name(name: &str) -> String {
    if name.chars().count() <= LABEL_MAX {
        return name.to_string();
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
mod tests {
    use super::*;

    #[test]
    fn truncate_name_keeps_short_names_and_cuts_long_ones() {
        assert_eq!(truncate_name("alice"), "alice");
        assert_eq!(truncate_name("exactly-10"), "exactly-10");
        assert_eq!(truncate_name("much-too-long-name"), "much-too-…");
    }

    #[test]
    fn camera_centers_small_maps_and_clamps_large_ones() {
        // Viewport wider than the map: origin pinned to 0 (padding centers).
        assert_eq!(camera_origin(10, 200, 94), 0);
        // Player near the left edge: no negative origin.
        assert_eq!(camera_origin(2, 40, 94), 0);
        // Player mid-map: centered on the player.
        assert_eq!(camera_origin(50, 40, 94), 30);
        // Player near the right edge: clamped to the map end.
        assert_eq!(camera_origin(93, 40, 94), 54);
    }

    #[test]
    fn labels_clamp_inside_the_walls() {
        let mut cells: Cells =
            vec![vec![(' ', Style::default()); usize::from(map::MAP_W)]; usize::from(map::MAP_H)];
        put_label(&mut cells, 1, 5, "longishname", Style::default());
        assert_eq!(cells[5][1].0, 'l');
        put_label(
            &mut cells,
            map::MAP_W - 2,
            6,
            "longishname",
            Style::default(),
        );
        let end: String = cells[6].iter().map(|(ch, _)| *ch).collect();
        assert!(end.trim_end().ends_with("longishname"));
    }
}
