use asterion_core::{AlarmLevel, Hero, MAX_MAZE_ID, POWER_UPS_PER_ROOM};
use late_core::models::asterion::ASTERION_DAILY_ESCAPE_PAYOUT;
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use crate::app::{
    common::{i18n, theme},
    lobby::house::{
        asterion::state::State,
        game_ui::{draw_game_frame_with_info_sidebar, info_label_value, key_hint},
    },
};
use crate::usernames::UsernameLookup;

const RADAR_PREFIXES: [&str; 9] = [
    "",
    "▁",
    "▁▂",
    "▁▂▃",
    "▁▂▃▄",
    "▁▂▃▄▅",
    "▁▂▃▄▅▆",
    "▁▂▃▄▅▆▇",
    "▁▂▃▄▅▆▇█",
];
const SIDEBAR_WIDTH: u16 = 28;
const MAZE_MIN_WIDTH: u16 = 40;
const HERO_COLOR: Color = Color::Rgb(35, 35, 255);
const OTHER_HERO_COLOR: Color = Color::Rgb(3, 255, 3);
const MINOTAUR_COLOR: Color = Color::Rgb(225, 203, 3);
const CHASING_MINOTAUR_COLOR: Color = Color::Rgb(255, 15, 0);
const POWER_UP_COLOR: Color = Color::Rgb(255, 180, 244);

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, _usernames: &UsernameLookup<'_>) {
    if area.height < 10 || area.width < MAZE_MIN_WIDTH + SIDEBAR_WIDTH {
        draw_compact(frame, area, state);
        return;
    }
    let content =
        draw_game_frame_with_info_sidebar(frame, area, "Asterion", info_lines(state), true);
    draw_maze(frame, content, state);
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let lines = state.lines();
    if lines.is_empty() {
        let private = state.private();
        let msg = if private.rejected {
            i18n::tr("chat.asterion_full")
        } else if private.seated {
            i18n::tr("chat.asterion_rendering")
        } else {
            i18n::tr("chat.asterion_joining")
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(theme::TEXT_DIM())))
                .alignment(Alignment::Center),
            area,
        );
        return;
    }
    frame.render_widget(Paragraph::new(lines.to_vec()), area);
    draw_maze_overlays(frame, area, state);
}

fn draw_maze(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(maze_border_color(state));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = state.lines();
    if lines.is_empty() {
        let private = state.private();
        let (msg, color) = if private.rejected {
            (i18n::tr("chat.asterion_full"), theme::ERROR())
        } else if private.seated {
            (i18n::tr("chat.asterion_rendering_short"), theme::TEXT_DIM())
        } else {
            (i18n::tr("chat.asterion_joining_short"), theme::TEXT_DIM())
        };
        frame.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(color)))
                .alignment(Alignment::Center),
            inner,
        );
        return;
    }
    frame.render_widget(Paragraph::new(lines.to_vec()), inner);
    draw_maze_overlays(frame, inner, state);
}

fn maze_border_color(state: &State) -> Style {
    let private = state.private();
    if private.has_won {
        Style::default().fg(theme::AMBER_GLOW())
    } else if private.is_dead {
        Style::default().fg(theme::ERROR())
    } else if private.alarm_level == AlarmLevel::ChasingHero {
        Style::default()
            .fg(theme::ERROR())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::BORDER())
    }
}

fn draw_maze_overlays(frame: &mut Frame, area: Rect, state: &State) {
    let private = state.private();
    if private.has_won {
        let payout_str = ASTERION_DAILY_ESCAPE_PAYOUT.to_string();
        let text: String = if private.daily_prize_claimed {
            i18n::tr("chat.asterion_escaped").to_string()
        } else {
            i18n::trf("chat.asterion_escaped_chips", &[("n", &payout_str)])
        };
        draw_flash_line(frame, area, &text, theme::AMBER_GLOW());
        return;
    }
    if private.is_dead {
        draw_flash_line(frame, area, i18n::tr("chat.asterion_killed"), theme::ERROR());
        return;
    }
    if let Some(flash) = state.power_up_flash() {
        draw_flash_line(frame, area, flash.label(), theme::SUCCESS());
    }
}

fn draw_flash_line(frame: &mut Frame, area: Rect, text: &str, color: Color) {
    if area.height == 0 {
        return;
    }
    let strip = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(Span::styled(
            text.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        strip,
    );
}

fn info_lines(state: &State) -> Vec<Line<'static>> {
    let private = state.private();
    let public = state.public();

    let mut lines: Vec<Line<'static>> = Vec::new();

    let alarm_color = alarm_color(private.alarm_level);
    let radar = radar_bars(
        private.nearest_minotaur_distance_sq,
        private.minotaurs_in_maze,
    );
    let alert = alarm_label(private.alarm_level);
    let current_maze = (private.maze_id + 1).min(MAX_MAZE_ID);
    let payout_str = ASTERION_DAILY_ESCAPE_PAYOUT.to_string();
    let prize = if private.daily_prize_claimed {
        i18n::tr("chat.asterion_claimed").to_string()
    } else {
        i18n::trf("chat.asterion_per_day", &[("n", &payout_str)])
    };

    lines.extend([
        section_header(i18n::tr("chat.asterion_objective")),
        info_label_value(
            i18n::tr("chat.asterion_progress"),
            format!("{current_maze}/{MAX_MAZE_ID}"),
            theme::AMBER(),
        ),
        info_label_value(i18n::tr("chat.asterion_prize"), prize, theme::SUCCESS()),
        info_label_value(
            i18n::tr("chat.asterion_heroes"),
            public.hero_count.to_string(),
            theme::TEXT_BRIGHT(),
        ),
        Line::raw(""),
        section_header(i18n::tr("chat.asterion_maze")),
        info_label_value(i18n::tr("chat.asterion_level"), current_maze.to_string(), theme::TEXT_BRIGHT()),
        info_label_value(
            i18n::tr("chat.asterion_pos"),
            format!("({}, {})", private.position.0, private.position.1),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            i18n::tr("chat.asterion_minotaurs"),
            private.minotaurs_in_maze.to_string(),
            theme::TEXT_BRIGHT(),
        ),
        Line::from(vec![
            Span::styled(
                format!("{:<11}", i18n::tr("chat.asterion_alert")),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(alert, Style::default().fg(alarm_color)),
            Span::raw(" "),
            Span::styled(
                radar,
                Style::default()
                    .fg(alarm_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::raw(""),
        section_header(i18n::tr("chat.asterion_legend")),
        legend_pair_line(
            (i18n::tr("chat.asterion_you"), HERO_COLOR),
            (i18n::tr("chat.asterion_ally"), OTHER_HERO_COLOR),
        ),
        legend_pair_line(
            (i18n::tr("chat.asterion_power"), POWER_UP_COLOR),
            (i18n::tr("chat.asterion_minotaur"), MINOTAUR_COLOR),
        ),
        legend_line(i18n::tr("chat.asterion_chasing"), CHASING_MINOTAUR_COLOR),
        Line::raw(""),
        section_header(i18n::tr("chat.asterion_powerups")),
        info_label_value(
            i18n::tr("chat.asterion_speed"),
            format!("{}/{} move delay", private.speed, Hero::MAX_SPEED),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            i18n::tr("chat.asterion_vision"),
            format!("{}/{} sight", private.vision, Hero::MAX_VISION),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            i18n::tr("chat.asterion_memory"),
            format!("{} seen tiles", private.memory),
            theme::TEXT_BRIGHT(),
        ),
        info_label_value(
            i18n::tr("chat.asterion_pickups"),
            format!(
                "{}/{} pink tile",
                private.power_ups_collected, POWER_UPS_PER_ROOM
            ),
            theme::TEXT_BRIGHT(),
        ),
        Line::raw(""),
        section_header(i18n::tr("chat.asterion_controls")),
        key_hint("arrows/wasd", "move"),
        key_hint(",/. Esc/q", "turn/leave"),
    ]);
    if private.rejected {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            i18n::tr("chat.asterion_full_esc"),
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )));
    }
    lines
}

fn alarm_color(level: AlarmLevel) -> Color {
    match level {
        AlarmLevel::NoMinotaurs => theme::TEXT_DIM(),
        AlarmLevel::NotChasing => theme::AMBER_DIM(),
        AlarmLevel::ChasingOtherHero => theme::AMBER(),
        AlarmLevel::ChasingHero => theme::ERROR(),
    }
}

fn alarm_label(level: AlarmLevel) -> &'static str {
    match level {
        AlarmLevel::NoMinotaurs => i18n::tr("chat.asterion_clear"),
        AlarmLevel::NotChasing => i18n::tr("chat.asterion_near"),
        AlarmLevel::ChasingOtherHero => i18n::tr("chat.asterion_hunt"),
        AlarmLevel::ChasingHero => i18n::tr("chat.asterion_chased"),
    }
}

fn radar_bars(distance_sq: usize, minotaurs_in_maze: usize) -> &'static str {
    if minotaurs_in_maze == 0 {
        return "";
    }
    let raw = (16 * 16 / distance_sq.max(1)).min(RADAR_PREFIXES.len() - 1);
    RADAR_PREFIXES[raw.max(1)]
}

fn section_header(label: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        label,
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

fn legend_line(label: &'static str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled("■", Style::default().fg(color).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {label}"), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn legend_pair_line(first: (&'static str, Color), second: (&'static str, Color)) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            "■",
            Style::default().fg(first.1).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {:<8}", first.0),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(
            "■",
            Style::default().fg(second.1).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("  {}", second.0),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}
