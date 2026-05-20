use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::{CatMood, CatNeedStatus, CatNeeds, CatPlayState, CatState, PLAY_RUN_NEEDED};
use crate::app::common::theme;

const MODAL_W: u16 = 72;
const MODAL_H: u16 = 26;

pub(crate) fn draw(frame: &mut Frame, state: &CatState) {
    let area = centered_rect(MODAL_W, MODAL_H, frame.area());
    frame.render_widget(Clear, area);

    let mood = state.mood();
    let needs = state.needs();
    let mood_color = mood_color(mood);

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

    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(2),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    if let Some(play) = state.play_session() {
        draw_play_scene(frame, rows[0], state, play, mood, mood_color);
        draw_play_status(frame, rows[1], play);
        draw_play_panel(frame, rows[2], play);
        draw_footer(frame, rows[4], true);
    } else {
        draw_scene(frame, rows[0], state, needs, mood, mood_color);
        draw_status(frame, rows[1], state, needs, mood, mood_color);
        draw_needs(frame, rows[2], needs);
        draw_footer(frame, rows[4], false);
    }
}

fn draw_scene(
    frame: &mut Frame,
    area: Rect,
    state: &CatState,
    needs: CatNeeds,
    mood: CatMood,
    mood_color: Color,
) {
    if area.height < 7 || area.width < 20 {
        return;
    }

    let tick = state.animation_ticks();
    let cat = cat_art(mood, tick);
    let cat_width = cat.iter().map(|line| line.len()).max().unwrap_or(0);
    let cat_x = cat_x(mood, tick, area.width as usize, cat_width);
    let cat_y = cat_y(mood, tick, area.height as usize, cat.len());

    let mut lines = Vec::new();
    for y in 0..area.height as usize {
        if y >= cat_y && y < cat_y + cat.len() {
            lines.push(indented(cat[y - cat_y].clone(), cat_x, mood_color));
        } else if y + 4 == area.height as usize {
            lines.push(prop_line_one(needs));
        } else if y + 3 == area.height as usize {
            lines.push(prop_line_two(needs));
        } else if y + 2 == area.height as usize {
            lines.push(prop_line_three(needs));
        } else if y + 1 == area.height as usize {
            lines.push(Line::from(Span::styled(
                "_".repeat(area.width as usize),
                Style::default().fg(theme::BORDER_DIM()),
            )));
        } else {
            lines.push(Line::from(""));
        }
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_play_scene(
    frame: &mut Frame,
    area: Rect,
    state: &CatState,
    play: &CatPlayState,
    mood: CatMood,
    mood_color: Color,
) {
    if area.height < 7 || area.width < 20 {
        return;
    }

    let width = area.width as usize;
    let height = area.height as usize;
    let mut grid = vec![vec![' '; width]; height];

    for x in 0..width {
        grid[height - 1][x] = '_';
    }

    let toy_col = field_col(play.toy_x, width);
    let toy_row = field_row(play.toy_y, height);
    put_char(&mut grid, toy_row, toy_col, '*');

    let cat = cat_art(mood, state.animation_ticks());
    let cat_width = cat.iter().map(|line| line.len()).max().unwrap_or(0);
    let cat_col = field_col(play.cat_x, width).saturating_sub(cat_width / 2);
    let cat_row = field_row(play.cat_y, height)
        .min(height.saturating_sub(cat.len() + 1))
        .max(1);
    for (row_offset, line) in cat.iter().enumerate() {
        put_text(&mut grid, cat_row + row_offset, cat_col, line);
    }

    let lines = grid
        .into_iter()
        .enumerate()
        .map(|(row, chars)| styled_play_line(row, &chars, mood_color))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_play_status(frame: &mut Frame, area: Rect, play: &CatPlayState) {
    let summary = Line::from(vec![
        Span::styled("run: ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(
            format!("{}/{}", play.run_energy, PLAY_RUN_NEEDED),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        dot(),
        Span::styled(
            format!("pounces {}", play.pounces),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ]);
    frame.render_widget(Paragraph::new(summary), area);

    let lower = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };
    frame.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                play.message.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ))
            .centered(),
        ),
        lower,
    );
}

fn draw_play_panel(frame: &mut Frame, area: Rect, play: &CatPlayState) {
    let lines = vec![
        Line::from(vec![
            Span::styled("toy x", Style::default().fg(theme::TEXT_FAINT())),
            Span::raw("  "),
            meter(play.toy_x),
        ]),
        Line::from(vec![
            Span::styled("toy y", Style::default().fg(theme::TEXT_FAINT())),
            Span::raw("  "),
            meter(play.toy_y),
        ]),
        Line::from(vec![
            Span::styled("run", Style::default().fg(theme::TEXT_FAINT())),
            Span::raw("  "),
            progress_meter(play.run_energy, PLAY_RUN_NEEDED),
        ]),
        Line::from(vec![
            Span::styled("goal", Style::default().fg(theme::TEXT_FAINT())),
            Span::raw("  "),
            Span::styled(
                "keep away until full",
                Style::default()
                    .fg(theme::TEXT_DIM())
                    .add_modifier(Modifier::ITALIC),
            ),
        ]),
        Line::from(""),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_status(
    frame: &mut Frame,
    area: Rect,
    state: &CatState,
    needs: CatNeeds,
    mood: CatMood,
    mood_color: Color,
) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("mood: ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(
                mood.label(),
                Style::default().fg(mood_color).add_modifier(Modifier::BOLD),
            ),
            dot(),
            Span::styled(
                mood_message(mood).to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        area,
    );

    let feedback = state
        .action_feedback
        .map(str::to_string)
        .unwrap_or_else(|| next_action_message(needs).to_string());
    let line = Line::from(Span::styled(
        feedback,
        Style::default()
            .fg(if state.action_feedback.is_some() {
                theme::AMBER()
            } else {
                theme::TEXT_FAINT()
            })
            .add_modifier(if state.action_feedback.is_some() {
                Modifier::BOLD
            } else {
                Modifier::ITALIC
            }),
    ))
    .centered();
    let lower = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };
    frame.render_widget(Paragraph::new(line), lower);
}

fn draw_needs(frame: &mut Frame, area: Rect, needs: CatNeeds) {
    let lines = vec![
        need_line("food", needs.food, "f feed", "once today"),
        need_line("water", needs.water, "w water", "once today"),
        need_line("play", needs.play, "p play", "chase game"),
    ];

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, play_active: bool) {
    let line = if play_active {
        Line::from(vec![
            key("hjkl"),
            text(" move"),
            gap(),
            key("wasd"),
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
    }
    .centered();
    frame.render_widget(Paragraph::new(line), area);
}

fn cat_art(mood: CatMood, tick: usize) -> Vec<String> {
    let tail = if matches!(mood, CatMood::Happy | CatMood::Content) && tick % 24 < 12 {
        "~"
    } else {
        "-"
    };
    let paws = if mood == CatMood::Happy && tick % 16 < 8 {
        "  / \\  "
    } else if mood == CatMood::Sad {
        "  /_\\  "
    } else {
        "  / \\  "
    };
    let body = match mood {
        CatMood::Happy => " /| |\\ ",
        CatMood::Content => " /|_|\\ ",
        CatMood::Bored => " /| |\\ ",
        CatMood::Hungry => " /|_|\\ ",
        CatMood::Thirsty => " /|_|\\ ",
        CatMood::Sad => " /|_|\\ ",
    };

    vec![
        " /\\_/\\ ".to_string(),
        format!("( {} ){tail}", mood.eyes()),
        body.to_string(),
        paws.to_string(),
    ]
}

fn centered_rect(w: u16, h: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(h.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(w.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn cat_x(mood: CatMood, tick: usize, area_width: usize, cat_width: usize) -> usize {
    let travel = area_width.saturating_sub(cat_width + 2);
    if travel == 0 {
        return 0;
    }
    match mood {
        CatMood::Happy => 1 + ping_pong(tick / 2, travel),
        CatMood::Content => area_width.saturating_sub(cat_width) / 2 + small_wiggle(tick),
        CatMood::Bored => area_width.saturating_sub(cat_width) / 3,
        CatMood::Hungry => area_width.saturating_sub(cat_width) / 4,
        CatMood::Thirsty => area_width.saturating_sub(cat_width) / 2,
        CatMood::Sad => area_width.saturating_sub(cat_width) / 2,
    }
    .min(travel)
}

fn cat_y(mood: CatMood, tick: usize, area_height: usize, cat_height: usize) -> usize {
    let prop_rows = 4usize;
    let base = area_height
        .saturating_sub(prop_rows)
        .saturating_sub(cat_height);
    let jump = match mood {
        CatMood::Happy => {
            if tick % 18 < 5 {
                2
            } else if tick % 18 < 8 {
                1
            } else {
                0
            }
        }
        CatMood::Content => {
            if tick % 36 < 4 {
                1
            } else {
                0
            }
        }
        _ => 0,
    };
    base.saturating_sub(jump)
}

fn ping_pong(tick: usize, width: usize) -> usize {
    let period = width.saturating_mul(2).max(1);
    let pos = tick % period;
    if pos <= width { pos } else { period - pos }
}

fn small_wiggle(tick: usize) -> usize {
    usize::from(tick % 30 < 15)
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

fn styled_play_line(row: usize, chars: &[char], mood_color: Color) -> Line<'static> {
    let spans = chars
        .iter()
        .copied()
        .map(|ch| {
            let style = match ch {
                '*' => Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
                '_' => Style::default().fg(theme::BORDER_DIM()),
                ' ' => Style::default(),
                _ => Style::default().fg(mood_color),
            };
            Span::styled(ch.to_string(), style)
        })
        .collect::<Vec<_>>();
    if row == 0 {
        Line::from(spans).centered()
    } else {
        Line::from(spans)
    }
}

fn meter(value: i16) -> Span<'static> {
    let width = 18usize;
    let filled = ((value.clamp(0, 1000) as usize) * width) / 1000;
    let mut text = String::with_capacity(width + 2);
    text.push('[');
    for idx in 0..width {
        text.push(if idx == filled.min(width.saturating_sub(1)) {
            '|'
        } else {
            '-'
        });
    }
    text.push(']');
    Span::styled(text, Style::default().fg(theme::TEXT_DIM()))
}

fn progress_meter(value: u16, max: u16) -> Span<'static> {
    let width = 18usize;
    let filled = ((value.min(max) as usize) * width) / max.max(1) as usize;
    let mut text = String::with_capacity(width + 2);
    text.push('[');
    for idx in 0..width {
        text.push(if idx < filled { '#' } else { '-' });
    }
    text.push(']');
    Span::styled(text, Style::default().fg(theme::AMBER_DIM()))
}

fn indented(text: String, spaces: usize, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::raw(" ".repeat(spaces)),
        Span::styled(
            text,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn prop_line_one(needs: CatNeeds) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        text("food "),
        prop("["),
        prop_fill(needs.food, "####", "....", "    "),
        prop("]"),
        Span::raw("     "),
        text("water "),
        prop("["),
        prop_fill(needs.water, "~~~~", "....", "    "),
        prop("]"),
        Span::raw("     "),
        text("yarn "),
        prop_fill(needs.play, "@@@", " o ", "   "),
    ])
}

fn prop_line_two(needs: CatNeeds) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        prop(".----.       .----.       .---."),
        Span::raw("       "),
        Span::styled(
            if needs.all_required_done() {
                "daily care done"
            } else {
                "daily care open"
            },
            Style::default().fg(if needs.all_required_done() {
                theme::SUCCESS()
            } else {
                theme::TEXT_DIM()
            }),
        ),
    ])
}

fn prop_line_three(_needs: CatNeeds) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        prop("'----'       '----'       '---'"),
    ])
}

fn need_line(
    label: &'static str,
    status: CatNeedStatus,
    action: &'static str,
    cadence: &'static str,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label:<5}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{:<4}", status.label()),
            Style::default()
                .fg(status_color(status))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        key(action),
        Span::raw("  "),
        Span::styled(cadence.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn mood_message(mood: CatMood) -> &'static str {
    match mood {
        CatMood::Happy => "all needs met",
        CatMood::Content => "mostly cared for",
        CatMood::Bored => "needs play",
        CatMood::Hungry => "needs food",
        CatMood::Thirsty => "needs water",
        CatMood::Sad => "needs care",
    }
}

fn next_action_message(needs: CatNeeds) -> &'static str {
    if needs.food.is_missing() {
        "food bowl is waiting"
    } else if needs.water.is_missing() {
        "water bowl is low"
    } else if needs.play.is_missing() {
        "the yarn is untouched"
    } else {
        "care done for today"
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

fn prop_fill(
    status: CatNeedStatus,
    done: &'static str,
    due: &'static str,
    late: &'static str,
) -> Span<'static> {
    Span::styled(
        match status {
            CatNeedStatus::Done => done,
            CatNeedStatus::Due => due,
            CatNeedStatus::Overdue => late,
        },
        Style::default().fg(status_color(status)),
    )
}

fn prop(label: &'static str) -> Span<'static> {
    Span::styled(label, Style::default().fg(theme::BORDER_DIM()))
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
