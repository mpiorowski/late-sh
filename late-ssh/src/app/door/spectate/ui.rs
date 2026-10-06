// The watch view, drawn by the Games hub in place of its sidebar and landing
// while this session spectates. One header row (who, which game, how long,
// who else is watching, the keys), then the player's screen. The screen is
// the player's size, not ours: a smaller one is centered, a larger one is
// cropped to a window that follows the cursor (crawl parks it on the `@`).

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::proxy::{LiveGame, WatchStatus};
use super::state::State;
use crate::app::common::theme;
use crate::app::door::rebels::render::blit_screen_from;

pub struct SpectateView<'a> {
    pub state: &'a State,
    /// The watched game's roster, for the position and watcher count.
    pub roster: &'a [LiveGame],
}

/// One axis of the fit: where to start reading the player's screen, how far
/// in to start drawing, and how many cells to draw.
#[derive(Debug, PartialEq, Eq)]
pub struct AxisFit {
    pub src: u16,
    pub dst: u16,
    pub len: u16,
}

/// Fit `screen_len` cells of the player's screen into `view_len` cells of
/// ours: centered when it fits, otherwise a window that keeps `cursor` in
/// the middle, clamped to the screen's edges.
pub fn fit_axis(screen_len: u16, view_len: u16, cursor: u16) -> AxisFit {
    if screen_len <= view_len {
        return AxisFit {
            src: 0,
            dst: (view_len - screen_len) / 2,
            len: screen_len,
        };
    }
    AxisFit {
        src: cursor
            .saturating_sub(view_len / 2)
            .min(screen_len - view_len),
        dst: 0,
        len: view_len,
    }
}

pub fn draw(frame: &mut Frame, area: Rect, view: &SpectateView<'_>) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    let body = layout[1];

    let (rows, cols) = view.state.with_screen(|screen| screen.size());
    let cropped = cols > body.width || rows > body.height;
    draw_header(frame, layout[0], view, cropped.then_some((cols, rows)));

    let playname = view.state.playname();
    match view.state.status() {
        WatchStatus::Connecting => {
            draw_notice(frame, body, &format!("Joining {playname}'s game..."))
        }
        WatchStatus::Ended => draw_notice(frame, body, &format!("{playname}'s game has ended.")),
        WatchStatus::Watching => {
            let buf = frame.buffer_mut();
            view.state.with_screen(|screen| {
                let (cursor_row, cursor_col) = screen.cursor_position();
                let x = fit_axis(cols, body.width, cursor_col);
                let y = fit_axis(rows, body.height, cursor_row);
                let target = Rect {
                    x: body.x + x.dst,
                    y: body.y + y.dst,
                    width: x.len,
                    height: y.len,
                };
                blit_screen_from(buf, target, screen, y.src, x.src);
            });
        }
    }
}

fn draw_header(
    frame: &mut Frame,
    area: Rect,
    view: &SpectateView<'_>,
    cropped: Option<(u16, u16)>,
) {
    let playname = view.state.playname();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut spans = vec![
        Span::styled(" watching ", dim),
        Span::styled(
            playname.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" \u{b7} {}", view.state.game().label()), dim),
    ];
    if let Some(game) = view.roster.iter().find(|g| g.playname == playname) {
        spans.push(Span::styled(
            format!(
                " \u{b7} {} in",
                duration_label(minutes_since(game.started_unix))
            ),
            dim,
        ));
        // The roster counts this session too.
        let others = game.watchers.saturating_sub(1);
        if others > 0 {
            spans.push(Span::styled(format!(" \u{b7} {others} also watching"), dim));
        }
    }
    if let Some((cols, rows)) = cropped {
        spans.push(Span::styled(
            format!(" \u{b7} their screen is {cols}x{rows}, yours is smaller"),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    let mut keys = Vec::new();
    if view.roster.len() > 1 {
        keys.push(Span::styled("\u{2190}/\u{2192} switch \u{b7} ", dim));
    }
    keys.push(Span::styled("Esc back ", dim));
    frame.render_widget(
        Paragraph::new(Line::from(keys)).alignment(Alignment::Right),
        area,
    );
}

fn draw_notice(frame: &mut Frame, area: Rect, text: &str) {
    let y = area.y + area.height / 2;
    if y >= area.y + area.height {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        Rect {
            y,
            height: 1,
            ..area
        },
    );
}

/// Whole minutes since a game started, by the wall clock.
pub fn minutes_since(started_unix: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock after the unix epoch")
        .as_secs();
    now.saturating_sub(started_unix) / 60
}

/// `12m`, `3h 05m`: how long a game has been running, for the header and the
/// hub's watch list.
pub fn duration_label(minutes: u64) -> String {
    if minutes < 60 {
        format!("{minutes}m")
    } else {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
