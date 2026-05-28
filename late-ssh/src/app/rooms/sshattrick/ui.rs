use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph},
};

use sshattrick_core::GameSide;

use crate::app::{
    common::theme,
    rooms::{
        game_ui::{draw_game_frame_with_info_sidebar, info_label_value, key_hint},
        sshattrick::{
            state::State,
            svc::{Phase, SshattrickPublicSnapshot},
        },
    },
};
use crate::usernames::UsernameLookup;

// The pitch image is 160 wide × 86 tall (→ 43 rows of half-blocks). Plus the
// 28-cell info sidebar and 2 cells of border around the pitch.
const PITCH_MIN_WIDTH: u16 = 160;
const SIDEBAR_WIDTH: u16 = 28;
const MIN_WIDTH: u16 = PITCH_MIN_WIDTH + SIDEBAR_WIDTH;
const MIN_HEIGHT: u16 = 45;
const RED_COLOR: Color = Color::Red;
const BLUE_COLOR: Color = Color::LightBlue;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, _usernames: &UsernameLookup<'_>) {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new("Terminal too small for ssHattrick").alignment(Alignment::Center),
            area,
        );
        return;
    }
    let content =
        draw_game_frame_with_info_sidebar(frame, area, "ssHattrick", info_lines(state), true);
    draw_pitch(frame, content, state);
}

fn draw_pitch(frame: &mut Frame, area: Rect, state: &State) {
    let block = Block::bordered()
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = state.lines();
    if lines.is_empty() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                placeholder_text(state.public()),
                Style::default().fg(theme::TEXT_DIM()),
            ))
            .alignment(Alignment::Center),
            inner,
        );
        return;
    }
    frame.render_widget(Paragraph::new(lines.to_vec()), inner);
}

fn placeholder_text(public: &SshattrickPublicSnapshot) -> &'static str {
    match public.phase {
        Phase::Waiting => "Waiting for both seats to fill. Press SPACE to sit.",
        Phase::Starting => "Match starting...",
        Phase::Running | Phase::AfterGoal => "Rendering...",
        Phase::Ending => "Match over. Press N for a rematch, Esc to leave.",
    }
}

fn info_lines(state: &State) -> Vec<Line<'static>> {
    let public = state.public();
    let private = state.private();

    let mut lines = Vec::with_capacity(16);
    lines.push(Line::from(Span::styled(
        "Score",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(info_label_value(
        "Red",
        format!(
            "{} {}",
            public.red_score,
            public
                .red
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "<open>".to_string())
        ),
        RED_COLOR,
    ));
    lines.push(info_label_value(
        "Blue",
        format!(
            "{} {}",
            public.blue_score,
            public
                .blue
                .as_ref()
                .map(|s| s.name.clone())
                .unwrap_or_else(|| "<open>".to_string())
        ),
        BLUE_COLOR,
    ));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        "Match",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    let time_left = public.time_left_ms / 1000;
    lines.push(info_label_value(
        "Time",
        format!("{}:{:02}", time_left / 60, time_left % 60),
        theme::TEXT(),
    ));
    let status = match public.phase {
        Phase::Waiting => "waiting".to_string(),
        Phase::Starting => "starting".to_string(),
        Phase::Running => "running".to_string(),
        Phase::AfterGoal => "after goal".to_string(),
        Phase::Ending => match public.winner {
            Some(GameSide::Red) => "red wins".to_string(),
            Some(GameSide::Blue) => "blue wins".to_string(),
            None => "draw".to_string(),
        },
    };
    lines.push(info_label_value("Status", status, theme::AMBER()));
    let your_seat = match private.seated_as {
        Some(GameSide::Red) => "Red",
        Some(GameSide::Blue) => "Blue",
        None => "spectator",
    };
    lines.push(info_label_value(
        "You",
        your_seat.to_string(),
        theme::AMBER_GLOW(),
    ));
    lines.push(Line::from(""));

    lines.push(Line::from(Span::styled(
        "Controls",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    )));
    if private.seated_as.is_some() {
        if public.phase == Phase::Ending {
            lines.push(key_hint("N/space", "rematch"));
        } else {
            lines.push(key_hint("arrows/wasd", "move"));
            lines.push(key_hint("space", "shoot"));
        }
    } else {
        lines.push(key_hint("space", "sit"));
        if public.phase == Phase::Ending {
            lines.push(key_hint("N", "rematch"));
        }
    }
    lines.push(key_hint("Esc/q", "leave"));

    lines
}
