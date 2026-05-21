use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::{CatMood, CatNeedStatus, CatNeeds, CatPlayState, CatState, PLAY_RUN_NEEDED};
use crate::app::common::theme;

const MODAL_W: u16 = 64;
const MODAL_H: u16 = 16;

pub(crate) fn draw(frame: &mut Frame, state: &CatState) {
    let area = centered_rect(MODAL_W, MODAL_H, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .title(Span::styled(
            " Cat Companion ",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    match state.play_session() {
        Some(play) => draw_play(frame, inner, state, play),
        None => draw_home(frame, inner, state),
    }
}

// --- home -----------------------------------------------------------------

fn draw_home(frame: &mut Frame, inner: Rect, state: &CatState) {
    let mood = state.mood();
    let needs = state.needs();
    let rows = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Length(7), // cat scene + floor (more room to roam)
        Constraint::Length(3), // care stations (bowl + label)
        Constraint::Length(1), // spacer
        Constraint::Length(1), // mood line
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_scene(frame, rows[1], state, needs, mood);
    draw_stations(frame, rows[2], needs);
    draw_mood_line(frame, rows[4], state, mood);
    draw_footer(frame, rows[5], false);
}

/// The cat ambles on a floor, drifting toward whichever bowl still needs care.
fn draw_scene(frame: &mut Frame, area: Rect, state: &CatState, needs: CatNeeds, mood: CatMood) {
    let w = area.width as usize;
    let h = area.height as usize;
    if w < 12 || h < 4 {
        return;
    }

    let tick = state.animation_ticks();
    let cat = cat_art(mood, tick);
    let cat_w = cat
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);

    let floor_row = h - 1;
    let jump = cat_jump(cat_activity(mood), tick);
    let cat_bottom = floor_row.saturating_sub(1 + jump);
    let cat_top = cat_bottom.saturating_sub(cat.len().saturating_sub(1));
    let cat_left = cat_left(needs, mood, tick, w, cat_w);
    let mood_col = mood_color(mood);

    let mut lines = Vec::with_capacity(h);
    for y in 0..h {
        if y == floor_row {
            lines.push(Line::from(Span::styled(
                "_".repeat(w),
                Style::default().fg(theme::BORDER_DIM()),
            )));
        } else if y >= cat_top && y <= cat_bottom {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(cat_left)),
                Span::styled(
                    cat[y - cat_top].clone(),
                    Style::default().fg(mood_col).add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(""));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_stations(frame: &mut Frame, area: Rect, needs: CatNeeds) {
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(area);

    draw_station(frame, cols[0], "food", needs.food, StationArt::Kibble);
    draw_station(frame, cols[1], "water", needs.water, StationArt::Water);
    draw_station(frame, cols[2], "play", needs.play, StationArt::Yarn);
}

#[derive(Clone, Copy)]
enum StationArt {
    Kibble,
    Water,
    Yarn,
}

fn draw_station(
    frame: &mut Frame,
    area: Rect,
    label: &'static str,
    status: CatNeedStatus,
    kind: StationArt,
) {
    if area.width < 9 || area.height < 3 {
        return;
    }
    let color = status_color(status);
    let [top, base] = station_art(kind, status);

    // The bowl carries the status on its own: a full green bowl is done, an
    // empty amber/red one still needs care. No status word required.
    let lines = vec![
        Line::from(Span::styled(top, Style::default().fg(color))).centered(),
        Line::from(Span::styled(base, Style::default().fg(color))).centered(),
        Line::from(Span::styled(label, Style::default().fg(theme::TEXT_DIM()))).centered(),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

/// Two-line glyph per care station. A full bowl / wound yarn means done.
fn station_art(kind: StationArt, status: CatNeedStatus) -> [String; 2] {
    match kind {
        StationArt::Kibble => bowl('*', status),
        StationArt::Water => bowl('~', status),
        StationArt::Yarn => ["  .---.  ".to_string(), " ( (@) ) ".to_string()],
    }
}

fn bowl(fill: char, status: CatNeedStatus) -> [String; 2] {
    let inside = if status == CatNeedStatus::Done {
        fill.to_string().repeat(7)
    } else {
        " ".repeat(7)
    };
    [format!("({inside})"), " \\_____/ ".to_string()]
}

fn draw_mood_line(frame: &mut Frame, area: Rect, state: &CatState, mood: CatMood) {
    let line = if let Some(feedback) = state.action_feedback {
        Line::from(Span::styled(
            feedback,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                mood.label(),
                Style::default()
                    .fg(mood_color(mood))
                    .add_modifier(Modifier::BOLD),
            ),
            dot(),
            Span::styled(mood_message(mood), Style::default().fg(theme::TEXT_DIM())),
        ])
    };
    frame.render_widget(Paragraph::new(line.centered()), area);
}

// --- play -----------------------------------------------------------------

fn draw_play(frame: &mut Frame, inner: Rect, state: &CatState, play: &CatPlayState) {
    let rows = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Fill(1),   // play field
        Constraint::Length(1), // run meter
        Constraint::Length(1), // message
        Constraint::Length(1), // footer
    ])
    .split(inner);

    draw_play_field(frame, rows[1], state, play);
    draw_play_meter(frame, rows[2], play);
    draw_play_message(frame, rows[3], play);
    draw_footer(frame, rows[4], true);
}

fn draw_play_field(frame: &mut Frame, area: Rect, state: &CatState, play: &CatPlayState) {
    let w = area.width as usize;
    let h = area.height as usize;
    if w < 12 || h < 5 {
        return;
    }

    let mut grid = vec![vec![' '; w]; h];
    for cell in grid[h - 1].iter_mut() {
        *cell = '_';
    }

    let toy_col = field_col(play.toy_x, w);
    let toy_row = field_row(play.toy_y, h);
    put_char(
        &mut grid,
        toy_row,
        toy_col,
        toy_glyph(state.animation_ticks()),
    );

    let mood = state.mood();
    let cat = cat_art(mood, state.animation_ticks());
    let cat_w = cat
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let cat_col = field_col(play.cat_x, w).saturating_sub(cat_w / 2);
    let cat_row = field_row(play.cat_y, h).min(h.saturating_sub(cat.len() + 1));
    for (offset, line) in cat.iter().enumerate() {
        put_text(&mut grid, cat_row + offset, cat_col, line);
    }

    let mood_col = mood_color(mood);
    let lines = grid
        .into_iter()
        .map(|chars| styled_play_line(&chars, mood_col))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_play_meter(frame: &mut Frame, area: Rect, play: &CatPlayState) {
    let width = 26usize;
    let filled =
        (play.run_energy.min(PLAY_RUN_NEEDED) as usize * width) / PLAY_RUN_NEEDED.max(1) as usize;

    let line = Line::from(vec![
        Span::styled("run  ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled("[", Style::default().fg(theme::BORDER_DIM())),
        Span::styled("#".repeat(filled), Style::default().fg(theme::AMBER_GLOW())),
        Span::styled(
            "-".repeat(width - filled),
            Style::default().fg(theme::BORDER_DIM()),
        ),
        Span::styled("]", Style::default().fg(theme::BORDER_DIM())),
        Span::styled(
            format!("   pounces {}", play.pounces),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]);
    frame.render_widget(Paragraph::new(line.centered()), area);
}

fn draw_play_message(frame: &mut Frame, area: Rect, play: &CatPlayState) {
    frame.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                play.message,
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ))
            .centered(),
        ),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, playing: bool) {
    let line = if playing {
        Line::from(vec![
            key("hjkl"),
            text(" move"),
            gap(),
            key("space"),
            text(" dash"),
            gap(),
            key("c"),
            text(" stop"),
            gap(),
            key("q"),
            text(" close"),
        ])
    } else {
        Line::from(vec![
            key("f"),
            text(" feed"),
            gap(),
            key("w"),
            text(" water"),
            gap(),
            key("p"),
            text(" play"),
            gap(),
            key("q"),
            text(" close"),
        ])
    };
    frame.render_widget(Paragraph::new(line.centered()), area);
}

// --- cat art --------------------------------------------------------------

/// How lively the cat looks, 0 (still) to 3 (bouncy). Mood is the only input;
/// it drives the hop, the tail-flick cadence, the blink, and the parked sway.
fn cat_activity(mood: CatMood) -> u8 {
    match mood {
        CatMood::Happy => 3,
        CatMood::Content => 2,
        CatMood::Bored | CatMood::Hungry | CatMood::Thirsty => 1,
        CatMood::Sad => 0,
    }
}

/// The classic three-line cat. Eyes and mouth shift with mood; the tail droops
/// when the cat is low and flicks faster the livelier it feels.
fn cat_art(mood: CatMood, tick: usize) -> Vec<String> {
    let activity = cat_activity(mood);
    let blink = activity > 0 && tick % 64 < 3;
    let eyes = if blink { "-.-" } else { mood.eyes() };
    let mouth = mood_mouth(mood);
    let [top_tail, body_tail] = tail_frames(activity, tick);

    vec![
        format!(" /\\_/\\ {top_tail}"),
        format!("( {eyes} ){body_tail}"),
        format!(" > {mouth} <  "),
    ]
}

/// Tail glyphs `[top, body]`. A still cat lets the tail droop; otherwise it
/// rests straight out and flicks up on a cadence that quickens with activity.
fn tail_frames(activity: u8, tick: usize) -> [char; 2] {
    if activity == 0 {
        return [' ', '\\']; // drooped, limp
    }
    let period = match activity {
        3 => 14,
        2 => 26,
        _ => 50,
    };
    if tick % period >= period - 4 {
        [')', '/'] // flicked up
    } else {
        [' ', '~'] // resting
    }
}

fn mood_mouth(mood: CatMood) -> char {
    match mood {
        CatMood::Happy => 'w',
        CatMood::Content => '^',
        CatMood::Bored => '.',
        CatMood::Hungry => 'o',
        CatMood::Thirsty => 'u',
        CatMood::Sad => '_',
    }
}

/// Left column for the cat: parked by the bowl that needs care, or strolling
/// when every need is met. A livelier cat shifts its weight; a sad one stands
/// dead still.
fn cat_left(needs: CatNeeds, mood: CatMood, tick: usize, width: usize, cat_w: usize) -> usize {
    let travel = width.saturating_sub(cat_w);
    if travel == 0 {
        return 0;
    }

    let station = |idx: usize| (width * (2 * idx + 1) / 6).saturating_sub(cat_w / 2);
    let parked = if needs.food.is_missing() {
        station(0)
    } else if needs.water.is_missing() {
        station(1)
    } else if needs.play.is_missing() {
        station(2)
    } else {
        return (1 + ping_pong(tick / 4, travel.saturating_sub(2))).min(travel);
    };

    let sway = if cat_activity(mood) == 0 {
        0
    } else {
        usize::from((tick / 14).is_multiple_of(5))
    };
    (parked + sway).min(travel)
}

/// Vertical hop height in rows. Only a lively cat bounces.
fn cat_jump(activity: u8, tick: usize) -> usize {
    match activity {
        3 => match tick % 22 {
            0..=2 => 2,
            3..=5 => 1,
            _ => 0,
        },
        2 => usize::from(tick % 54 < 3),
        _ => 0,
    }
}

fn toy_glyph(tick: usize) -> char {
    if tick % 8 < 4 { '*' } else { '+' }
}

// --- layout helpers -------------------------------------------------------

fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(h.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(w.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn ping_pong(tick: usize, width: usize) -> usize {
    let period = width.saturating_mul(2).max(1);
    let pos = tick % period;
    if pos <= width { pos } else { period - pos }
}

fn field_col(value: i16, width: usize) -> usize {
    if width <= 1 {
        return 0;
    }
    ((value.clamp(0, 1000) as usize) * (width - 1)) / 1000
}

fn field_row(value: i16, height: usize) -> usize {
    if height <= 2 {
        return 0;
    }
    let playable_height = height.saturating_sub(2).max(1);
    ((value.clamp(0, 1000) as usize) * playable_height) / 1000
}

fn put_char(grid: &mut [Vec<char>], row: usize, col: usize, ch: char) {
    if let Some(line) = grid.get_mut(row)
        && let Some(cell) = line.get_mut(col)
    {
        *cell = ch;
    }
}

fn put_text(grid: &mut [Vec<char>], row: usize, col: usize, text: &str) {
    let Some(line) = grid.get_mut(row) else {
        return;
    };
    for (offset, ch) in text.chars().enumerate() {
        if let Some(cell) = line.get_mut(col + offset) {
            *cell = ch;
        }
    }
}

fn styled_play_line(chars: &[char], mood_col: Color) -> Line<'static> {
    let spans = chars
        .iter()
        .copied()
        .map(|ch| {
            let style = match ch {
                '*' | '+' => Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
                '_' => Style::default().fg(theme::BORDER_DIM()),
                ' ' => Style::default(),
                _ => Style::default().fg(mood_col).add_modifier(Modifier::BOLD),
            };
            Span::styled(ch.to_string(), style)
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

// --- palette --------------------------------------------------------------

fn mood_message(mood: CatMood) -> &'static str {
    match mood {
        CatMood::Happy => "all needs met today",
        CatMood::Content => "mostly cared for",
        CatMood::Bored => "wants to play",
        CatMood::Hungry => "the food bowl is empty",
        CatMood::Thirsty => "the water bowl is low",
        CatMood::Sad => "needs some care",
    }
}

fn mood_color(mood: CatMood) -> Color {
    match mood {
        CatMood::Happy => theme::AMBER_GLOW(),
        CatMood::Content => theme::TEXT_BRIGHT(),
        CatMood::Bored => theme::AMBER_DIM(),
        CatMood::Hungry | CatMood::Thirsty => theme::AMBER(),
        CatMood::Sad => theme::TEXT_DIM(),
    }
}

fn status_color(status: CatNeedStatus) -> Color {
    match status {
        CatNeedStatus::Done => theme::SUCCESS(),
        CatNeedStatus::Due => theme::AMBER(),
        CatNeedStatus::Overdue => theme::ERROR(),
    }
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
}

fn dot() -> Span<'static> {
    Span::styled("  .  ", Style::default().fg(theme::BORDER_DIM()))
}

fn gap() -> Span<'static> {
    Span::raw("   ")
}
