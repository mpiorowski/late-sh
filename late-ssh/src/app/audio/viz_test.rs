use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn render_wave_at(wall_tick: usize) -> String {
    let width = 24u16;
    let height = 3u16;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| render_wave(frame, Rect::new(0, 0, width, height), wall_tick))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..height {
        for x in 0..width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    rendered
}

#[test]
fn wave_renders_a_braille_line() {
    let rendered = render_wave_at(0);
    let dots = rendered
        .chars()
        .filter(|c| ('\u{2801}'..='\u{28FF}').contains(c))
        .count();
    // A thin line across a 24-cell strip needs at least one dotted cell
    // per column's worth of curve; well over the width in practice.
    assert!(dots >= 12, "expected a drawn line, got {dots} dotted cells");
}

#[test]
fn wave_scrolls_with_the_wall_clock() {
    // Two wall ticks apart crosses an anim_half edge, so the offset moved.
    assert_ne!(render_wave_at(0), render_wave_at(2));
}

#[test]
fn wave_is_deterministic_for_a_tick() {
    // No hidden state: the wall tick alone decides the frame.
    assert_eq!(render_wave_at(6), render_wave_at(6));
}

#[test]
fn wave_wraps_cleanly_after_one_period() {
    // One full wavelength of scroll = WAVE_LENGTH_DOTS steps at one step
    // per two wall ticks.
    assert_eq!(render_wave_at(0), render_wave_at(WAVE_LENGTH_DOTS * 2));
}

#[test]
fn sub_edge_ticks_render_identically() {
    // Ticks inside the same anim_half period share an offset; the paint
    // gate skips them, and even if painted they would be identical.
    assert_eq!(render_wave_at(4), render_wave_at(5));
}
