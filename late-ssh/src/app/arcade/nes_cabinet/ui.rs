use nes::frame::{NTSC_HEIGHT, NTSC_WIDTH};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    arcade::ui::{GameBottomBar, draw_game_frame, keys_line, status_line, tip_line},
    common::theme,
};

use super::state::State;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, show_bottom_bar: bool) {
    let bottom = GameBottomBar {
        status: status_line(vec![
            ("ROM", state.rom().title.to_string(), theme::SUCCESS()),
            ("Mode", "Potatis".to_string(), theme::AMBER()),
            (
                "View",
                if state.zoomed() { "zoom" } else { "fit" }.to_string(),
                theme::TEXT_BRIGHT(),
            ),
        ]),
        keys: keys_line(vec![
            ("WASD", "d-pad"),
            ("K", "B"),
            ("L", "A"),
            ("Space", "select"),
            ("Enter", "start"),
            ("Z", "zoom"),
            ("Arrows/Shift+hjkl", "pan zoom"),
            ("R", "reset"),
            ("Q", "quit"),
        ]),
        tip: Some(tip_line(state.rom().subtitle)),
    };
    let play_area = draw_game_frame(frame, area, "NES Cabinet", bottom, show_bottom_bar);

    if play_area.height < 8 || play_area.width < 24 {
        frame.render_widget(
            Paragraph::new("Terminal too small for NES Cabinet").alignment(Alignment::Center),
            play_area,
        );
        return;
    }

    if let Some(error) = state.last_error() {
        frame.render_widget(
            Paragraph::new(error.to_string()).alignment(Alignment::Center),
            play_area,
        );
        return;
    }

    let frame_rgb = state.frame();
    if frame_rgb.len() < NTSC_WIDTH * NTSC_HEIGHT * 3 {
        frame.render_widget(
            Paragraph::new("Warming up...").alignment(Alignment::Center),
            play_area,
        );
        return;
    }

    let viewport = if state.zoomed() {
        zoom_viewport(play_area, state.pan())
    } else {
        fit_viewport()
    };
    let (target_w, target_h) = target_dims(play_area, viewport, state.zoomed());
    let render_area = centered_area(play_area, target_w, target_h);

    let mut lines = Vec::with_capacity(render_area.height as usize);
    let src_cols = render_area.width as usize;
    let src_half_rows = render_area.height as usize * 2;
    for cell_y in 0..render_area.height {
        let mut spans = Vec::with_capacity(render_area.width as usize);
        for cell_x in 0..render_area.width {
            let cell_x = cell_x as usize;
            let top_row = cell_y as usize * 2;
            let bottom_row = top_row + 1;
            let x0 = viewport.x + cell_x * viewport.width / src_cols;
            let x1 = (viewport.x + (cell_x + 1) * viewport.width / src_cols)
                .max(x0 + 1)
                .min(NTSC_WIDTH);
            let top_y0 = viewport.y + top_row * viewport.height / src_half_rows;
            let top_y1 = (viewport.y + (top_row + 1) * viewport.height / src_half_rows)
                .max(top_y0 + 1)
                .min(NTSC_HEIGHT);
            let bottom_y0 = viewport.y + bottom_row * viewport.height / src_half_rows;
            let bottom_y1 = (viewport.y + (bottom_row + 1) * viewport.height / src_half_rows)
                .max(bottom_y0 + 1)
                .min(NTSC_HEIGHT);
            let top = average_rgb(&frame_rgb, x0, x1, top_y0, top_y1);
            let bottom = average_rgb(&frame_rgb, x0, x1, bottom_y0, bottom_y1);
            spans.push(Span::styled(
                "▀",
                Style::default().fg(rgb(top)).bg(rgb(bottom)),
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), render_area);
}

#[derive(Clone, Copy)]
struct SourceViewport {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

fn fit_viewport() -> SourceViewport {
    SourceViewport {
        x: 0,
        y: 0,
        width: NTSC_WIDTH,
        height: NTSC_HEIGHT,
    }
}

fn zoom_viewport(area: Rect, pan: (usize, usize)) -> SourceViewport {
    let width = (area.width as usize).min(NTSC_WIDTH);
    let height = ((area.height as usize) * 2).min(NTSC_HEIGHT);
    let max_x = NTSC_WIDTH.saturating_sub(width);
    let max_y = NTSC_HEIGHT.saturating_sub(height);
    SourceViewport {
        x: pan.0.min(max_x),
        y: pan.1.min(max_y),
        width,
        height,
    }
}

fn target_dims(area: Rect, viewport: SourceViewport, zoomed: bool) -> (u16, u16) {
    if zoomed {
        return (
            (viewport.width as u16).min(area.width).max(1),
            ((viewport.height / 2) as u16).min(area.height).max(1),
        );
    }

    let max_w = area.width;
    let max_h = area.height;
    let target_w_by_h =
        max_h.saturating_mul(viewport.width as u16) / ((viewport.height as u16) / 2);
    let target_w = max_w.min(target_w_by_h.max(1)).max(1);
    let target_h = max_h
        .min(target_w.saturating_mul((viewport.height as u16) / 2) / viewport.width as u16)
        .max(1);
    (target_w, target_h)
}

fn centered_area(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn average_rgb(frame: &[u8], x0: usize, x1: usize, y0: usize, y1: usize) -> (u8, u8, u8) {
    let mut r = 0usize;
    let mut g = 0usize;
    let mut b = 0usize;
    let mut count = 0usize;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * NTSC_WIDTH) + x) * 3;
            r += frame[i] as usize;
            g += frame[i + 1] as usize;
            b += frame[i + 2] as usize;
            count += 1;
        }
    }
    if count == 0 {
        return (0, 0, 0);
    }
    ((r / count) as u8, (g / count) as u8, (b / count) as u8)
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}
