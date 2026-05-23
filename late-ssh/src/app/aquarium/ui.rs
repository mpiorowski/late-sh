use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
};

use super::state::{POS_SCALE, Species, WaterState};
use crate::app::common::theme;

pub fn draw_water_band(frame: &mut Frame, area: Rect, state: &WaterState) {
    if area.height < 2 || area.width < 4 {
        return;
    }
    frame.render_widget(Clear, area);

    let width = area.width as usize;

    // Top row: gentle ripples that drift sideways with time.
    let mut wave = String::with_capacity(width);
    let shift = (state.ticks / 3) as usize;
    for x in 0..width {
        let phase = (x + shift) % 6;
        wave.push(match phase {
            0 => '~',
            3 => '-',
            _ => ' ',
        });
    }
    let wave_line = Line::from(Span::styled(
        wave,
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(
        Paragraph::new(wave_line),
        Rect::new(area.x, area.y, area.width, 1),
    );

    // Bottom row: fish swim across the full width.
    let mut row: Vec<(char, Color)> = vec![(' ', Color::Reset); width];
    let width_cells = width as i64;
    if width_cells == 0 {
        return;
    }
    for fish in &state.fish {
        let going_right = fish.speed_milli >= 0;
        let glyphs: &[char] = match (fish.species, going_right) {
            (Species::Common, true) => &['>', '<', '>'],
            (Species::Common, false) => &['<', '>', '<'],
            (Species::Long, true) => &['>', '<', '(', '(', '\'', '>'],
            (Species::Long, false) => &['<', '\'', ')', ')', '>', '<'],
        };
        let pos = fish.x_milli.div_euclid(POS_SCALE).rem_euclid(width_cells);
        for (i, ch) in glyphs.iter().enumerate() {
            let x = (pos + i as i64).rem_euclid(width_cells) as usize;
            row[x] = (*ch, fish.colour);
        }
    }
    let spans: Vec<Span> = row
        .into_iter()
        .map(|(ch, colour)| Span::styled(ch.to_string(), Style::default().fg(colour)))
        .collect();
    frame.render_widget(
        Paragraph::new(Line::from(spans)),
        Rect::new(area.x, area.y + 1, area.width, 1),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};
    use uuid::Uuid;

    fn render(width: u16, height: u16, state: &WaterState) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                draw_water_band(frame, Rect::new(0, 0, width, height), state);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut row = String::new();
                for x in 0..width {
                    row.push_str(buffer[(x, y)].symbol());
                }
                row
            })
            .collect()
    }

    #[test]
    fn band_contains_wave_and_fish_glyphs_full_width() {
        let state = WaterState::new_for_user(Uuid::from_u128(0xa11ce));
        let rows = render(80, 2, &state);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].chars().count(), 80);
        assert_eq!(rows[1].chars().count(), 80);
        assert!(rows[0].contains('~'), "wave row should contain ripples");
        let fish_chars = ['>', '<', '(', ')', '\''];
        assert!(
            rows[1].chars().any(|c| fish_chars.contains(&c)),
            "fish row should contain at least one fish glyph, got {:?}",
            rows[1]
        );
    }

    #[test]
    fn fish_advance_across_the_row_between_ticks() {
        let mut state = WaterState::new_for_user(Uuid::from_u128(0xb0b));
        let before = render(80, 2, &state)[1].clone();
        for _ in 0..40 {
            state.tick();
        }
        let after = render(80, 2, &state)[1].clone();
        assert_ne!(before, after, "fish positions should shift after ticking");
    }

    #[test]
    fn band_does_not_panic_on_narrow_areas() {
        let state = WaterState::new_for_user(Uuid::from_u128(7));
        let _ = render(3, 2, &state); // below min width
        let _ = render(80, 1, &state); // below min height
    }
}

