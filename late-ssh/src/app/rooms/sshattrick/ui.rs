use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Paragraph},
};

use sshattrick_core::GameSide;

use crate::app::{
    common::theme,
    rooms::{
        game_ui::{draw_game_frame_with_info_sidebar, info_label_value, key_hint},
        sshattrick::{
            big_text::{
                blue_scored, blue_won, dash, disconnection, draw as draw_banner, palette_colors,
                red_scored, red_won, BigNumberFont,
            },
            state::State,
            svc::{Phase, SshattrickPublicSnapshot},
        },
    },
};
use crate::usernames::UsernameLookup;

const SCORE_BANNER_WIDTH: u16 = 88;
const SCORE_BANNER_HEIGHT: u16 = 6;
const WIN_BANNER_WIDTH: u16 = 72;
const DISCONNECT_BANNER_WIDTH: u16 = 102;
const DISCONNECT_BANNER_HEIGHT: u16 = 6;
const DISCONNECT_BANNER_Y_OFFSET: u16 = 12;
const SCORELINE_DIGIT_SLOT: u16 = 18;
const SCORELINE_DASH_SLOT: u16 = 12;
const SCORELINE_WIDTH: u16 = SCORELINE_DIGIT_SLOT + SCORELINE_DASH_SLOT + SCORELINE_DIGIT_SLOT;
const SCORELINE_HEIGHT: u16 = 6;
const SCORELINE_GAP: u16 = 1;
const COUNTDOWN_DIGIT_WIDTH: u16 = 10;
const COUNTDOWN_DIGIT_HEIGHT: u16 = 6;

// The pitch image is 160 pixels wide × 86 tall, rendered as half-blocks
// (160 cols × 43 rows). Overlay positioning anchors on this rect, not the
// surrounding bordered area, so banners stay centred on the visible pitch.
const PITCH_RENDER_WIDTH: u16 = 160;
const PITCH_RENDER_HEIGHT: u16 = 43;

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
            Paragraph::new("Terminal too small for ssHattrick").centered(),
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
            .centered(),
            inner,
        );
        return;
    }
    let pitch_rect = Rect {
        x: inner.x + inner.width.saturating_sub(PITCH_RENDER_WIDTH) / 2,
        y: inner.y + inner.height.saturating_sub(PITCH_RENDER_HEIGHT) / 2,
        width: PITCH_RENDER_WIDTH.min(inner.width),
        height: PITCH_RENDER_HEIGHT.min(inner.height),
    };
    frame.render_widget(Paragraph::new(lines.to_vec()), pitch_rect);
    draw_overlays(frame, pitch_rect, state.public());
}

fn draw_overlays(frame: &mut Frame, area: Rect, public: &SshattrickPublicSnapshot) {
    let (color_1, color_2) = palette_colors(public.palette);
    match public.phase {
        Phase::Starting => {
            if let Some(remaining_ms) = public.starting_remaining_ms
                && let Some(rect) =
                    centered_rect(area, COUNTDOWN_DIGIT_WIDTH, COUNTDOWN_DIGIT_HEIGHT)
            {
                let digit = remaining_ms.div_ceil(1000) as u8;
                frame.render_widget(Clear, rect);
                frame.render_widget(digit.big_font_styled(color_1, color_2), rect);
            }
        }
        Phase::AfterGoal => {
            let widget = match public.scored {
                Some(GameSide::Red) => red_scored(color_1, color_2),
                Some(GameSide::Blue) => blue_scored(color_1, color_2),
                None => return,
            };
            draw_banner_with_scoreline(
                frame,
                area,
                widget,
                SCORE_BANNER_WIDTH,
                public.red_score,
                public.blue_score,
            );
        }
        Phase::Ending => {
            if public.by_disconnect
                && let Some(rect) = offset_centered_rect(
                    area,
                    DISCONNECT_BANNER_WIDTH,
                    DISCONNECT_BANNER_HEIGHT,
                    DISCONNECT_BANNER_Y_OFFSET,
                )
            {
                frame.render_widget(Clear, rect);
                frame.render_widget(disconnection(color_1, color_2), rect);
            }
            let widget = match public.winner {
                Some(GameSide::Red) => red_won(color_1, color_2),
                Some(GameSide::Blue) => blue_won(color_1, color_2),
                None => draw_banner(color_1, color_2),
            };
            draw_banner_with_scoreline(
                frame,
                area,
                widget,
                WIN_BANNER_WIDTH,
                public.red_score,
                public.blue_score,
            );
        }
        _ => {}
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Option<Rect> {
    if area.width < width || area.height < height {
        return None;
    }
    Some(Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    })
}

fn draw_banner_with_scoreline(
    frame: &mut Frame,
    area: Rect,
    banner: Paragraph<'static>,
    banner_width: u16,
    red_score: u8,
    blue_score: u8,
) {
    let combined_width = banner_width.max(SCORELINE_WIDTH);
    let combined_height = SCORE_BANNER_HEIGHT + SCORELINE_GAP + SCORELINE_HEIGHT;
    let Some(outer) = centered_rect(area, combined_width, combined_height) else {
        return;
    };
    let rows = Layout::vertical([
        Constraint::Length(SCORE_BANNER_HEIGHT),
        Constraint::Length(SCORELINE_GAP),
        Constraint::Length(SCORELINE_HEIGHT),
    ])
    .split(outer);
    let banner_rect = Rect {
        x: outer.x + (outer.width.saturating_sub(banner_width)) / 2,
        y: rows[0].y,
        width: banner_width.min(outer.width),
        height: rows[0].height,
    };
    frame.render_widget(Clear, banner_rect);
    frame.render_widget(banner, banner_rect);
    let scoreline_rect = Rect {
        x: outer.x + (outer.width.saturating_sub(SCORELINE_WIDTH)) / 2,
        y: rows[2].y,
        width: SCORELINE_WIDTH.min(outer.width),
        height: rows[2].height,
    };
    frame.render_widget(Clear, scoreline_rect);
    let cols = Layout::horizontal([
        Constraint::Length(SCORELINE_DIGIT_SLOT),
        Constraint::Length(SCORELINE_DASH_SLOT),
        Constraint::Length(SCORELINE_DIGIT_SLOT),
    ])
    .split(scoreline_rect);
    frame.render_widget(
        red_score
            .big_font_styled(Color::Red, Color::Yellow)
            .right_aligned(),
        cols[0],
    );
    frame.render_widget(dash(theme::TEXT_DIM()), cols[1]);
    frame.render_widget(
        blue_score
            .big_font_styled(Color::Blue, Color::LightMagenta)
            .left_aligned(),
        cols[2],
    );
}

fn offset_centered_rect(area: Rect, width: u16, height: u16, y_offset: u16) -> Option<Rect> {
    let mut rect = centered_rect(area, width, height)?;
    rect.y = rect.y.saturating_sub(y_offset).max(area.y);
    Some(rect)
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
        Phase::Running => "playing".to_string(),
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
