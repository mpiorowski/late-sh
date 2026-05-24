use std::collections::HashMap;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};
use uuid::Uuid;

use asterion_core::{AlarmLevel, Hero, MAX_MAZE_ID, POWER_UPS_PER_ROOM};
use late_core::models::asterion::ASTERION_DAILY_ESCAPE_PAYOUT;

use crate::app::{common::theme, rooms::asterion::state::State};

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

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, _usernames: &HashMap<Uuid, String>) {
    if area.height < 10 || area.width < 60 {
        draw_compact(frame, area, state);
        return;
    }
    let columns = Layout::horizontal([Constraint::Min(40), Constraint::Length(28)]).split(area);
    draw_maze(frame, columns[0], state);
    draw_sidebar(frame, columns[1], state);
}

fn draw_compact(frame: &mut Frame, area: Rect, state: &State) {
    let lines = state.lines();
    if lines.is_empty() {
        let private = state.private();
        let msg = if private.rejected {
            "Asterion room is full. Press Esc to leave."
        } else if private.seated {
            "Asterion - rendering..."
        } else {
            "Asterion - joining..."
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
            ("Room is full. Press Esc to leave.", theme::ERROR())
        } else if private.seated {
            ("Rendering...", theme::TEXT_DIM())
        } else {
            ("Joining maze...", theme::TEXT_DIM())
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
        let text = if private.daily_prize_claimed {
            "ESCAPED - DAILY PRIZE CLAIMED"
        } else {
            "ESCAPED - 500 CHIPS"
        };
        draw_flash_line(frame, area, text, theme::AMBER_GLOW());
        return;
    }
    if private.is_dead {
        draw_flash_line(frame, area, "KILLED BY A MINOTAUR", theme::ERROR());
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

fn draw_sidebar(frame: &mut Frame, area: Rect, state: &State) {
    let private = state.private();
    let public = state.public();

    let status = if private.has_won {
        "ESCAPED"
    } else if private.is_dead {
        "Knocked out"
    } else if private.rejected {
        "Room full"
    } else if private.seated {
        "Alive"
    } else {
        "Joining..."
    };

    let status_color = if private.has_won {
        theme::AMBER_GLOW()
    } else if private.is_dead || private.rejected {
        theme::ERROR()
    } else {
        theme::AMBER()
    };

    let alarm_color = alarm_color(private.alarm_level);
    let radar = radar_bars(
        private.nearest_minotaur_distance_sq,
        private.minotaurs_in_maze,
    );
    let current_maze = (private.maze_id + 1).min(MAX_MAZE_ID);
    let prize = if private.daily_prize_claimed {
        "claimed today".to_string()
    } else {
        format!("{ASTERION_DAILY_ESCAPE_PAYOUT}/day")
    };

    let lines = vec![
        Line::from(Span::styled(
            "ASTERION",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        section_header("Objective"),
        line_kv("Goal", "Escape maze 10", None),
        line_kv("Progress", &format!("{current_maze}/{MAX_MAZE_ID}"), None),
        line_kv("Prize", &prize, None),
        Line::from(""),
        line_kv("Status", status, Some(status_color)),
        line_kv("Maze", &format!("{}", private.maze_id), None),
        line_kv(
            "Pos",
            &format!("({}, {})", private.position.0, private.position.1),
            None,
        ),
        line_kv("Heroes", &format!("{}", public.hero_count), None),
        Line::from(""),
        section_header("Radar"),
        Line::from(vec![
            Span::styled(
                format!(" {:<8}", "Alert"),
                Style::default().fg(theme::TEXT_DIM()),
            ),
            Span::styled(
                radar,
                Style::default()
                    .fg(alarm_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        line_kv("In maze", &format!("{}", private.minotaurs_in_maze), None),
        Line::from(""),
        section_header("Power-ups"),
        line_kv(
            "Speed",
            &format!("{}/{}", private.speed, Hero::MAX_SPEED),
            None,
        ),
        line_kv(
            "Vision",
            &format!("{}/{}", private.vision, Hero::MAX_VISION),
            None,
        ),
        line_kv("Memory", &format!("{}", private.memory), None),
        line_kv(
            "Pickups",
            &format!("{}/{}", private.power_ups_collected, POWER_UPS_PER_ROOM),
            None,
        ),
        control_line(" pink tile: auto-pickup"),
        control_line(" speed lowers move delay"),
        control_line(" vision widens view"),
        control_line(" memory keeps seen tiles"),
        Line::from(""),
        section_header("Controls"),
        control_line(" arrows move"),
        control_line(" w/s/a/l move"),
        control_line(" , . turn"),
        control_line(" Esc/q leave"),
    ];

    let block = Block::bordered().border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(lines), inner);
}

fn alarm_color(level: AlarmLevel) -> Color {
    match level {
        AlarmLevel::NoMinotaurs => theme::TEXT_DIM(),
        AlarmLevel::NotChasing => theme::AMBER_DIM(),
        AlarmLevel::ChasingOtherHero => theme::AMBER(),
        AlarmLevel::ChasingHero => theme::ERROR(),
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

fn control_line(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(text, Style::default().fg(theme::TEXT_FAINT())))
}

fn line_kv(label: &str, value: &str, value_color: Option<Color>) -> Line<'static> {
    let value_style = if let Some(c) = value_color {
        Style::default().fg(c).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    Line::from(vec![
        Span::styled(
            format!(" {label:<8}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::styled(value.to_string(), value_style),
    ])
}
