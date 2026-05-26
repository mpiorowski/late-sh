use nes::frame::{NTSC_HEIGHT, NTSC_WIDTH};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
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
        ]),
        keys: keys_line(vec![
            ("WASD/Arrows", "d-pad"),
            ("K", "B"),
            ("L", "A"),
            ("Space", "select"),
            ("Enter", "start"),
            ("[/]", "rom"),
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

    let max_w = play_area.width.min(120);
    let max_h = play_area.height;
    let target_w_by_h = max_h.saturating_mul(NTSC_WIDTH as u16) / ((NTSC_HEIGHT as u16) / 2);
    let target_w = max_w.min(target_w_by_h.max(1)).max(1);
    let target_h = max_h
        .min(target_w.saturating_mul((NTSC_HEIGHT as u16) / 2) / NTSC_WIDTH as u16)
        .max(1);
    let render_area = Rect {
        x: play_area.x + play_area.width.saturating_sub(target_w) / 2,
        y: play_area.y + play_area.height.saturating_sub(target_h) / 2,
        width: target_w,
        height: target_h,
    };

    let mut lines = Vec::with_capacity(render_area.height as usize);
    for cell_y in 0..render_area.height {
        let mut spans = Vec::with_capacity(render_area.width as usize);
        for cell_x in 0..render_area.width {
            let src_x = cell_x as usize * NTSC_WIDTH / render_area.width as usize;
            let top_y = (cell_y as usize * 2) * NTSC_HEIGHT / (render_area.height as usize * 2);
            let bottom_y = ((cell_y as usize * 2 + 1) * NTSC_HEIGHT
                / (render_area.height as usize * 2))
                .min(NTSC_HEIGHT - 1);
            let top = sample_rgb(&frame_rgb, src_x, top_y);
            let bottom = sample_rgb(&frame_rgb, src_x, bottom_y);
            spans.push(Span::styled(
                "▀",
                Style::default().fg(rgb(top)).bg(rgb(bottom)),
            ));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), render_area);
    draw_rom_tabs(frame, play_area, state);
}

fn sample_rgb(frame: &[u8], x: usize, y: usize) -> (u8, u8, u8) {
    let i = ((y * NTSC_WIDTH) + x) * 3;
    (frame[i], frame[i + 1], frame[i + 2])
}

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::Rgb(r, g, b)
}

fn draw_rom_tabs(frame: &mut Frame, area: Rect, state: &State) {
    if area.height < 3 {
        return;
    }
    let mut spans = Vec::new();
    for (idx, rom) in super::state::ROMS.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::styled("  ", Style::default()));
        }
        let style = if idx == state.selected_rom() {
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_MUTED())
        };
        spans.push(Span::styled(rom.title, style));
    }
    let line = Line::from(spans).alignment(Alignment::Center);
    let tab_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    frame.render_widget(Paragraph::new(line), tab_area);
}
