use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn render_wave_state(wall_tick: usize, muted: bool) -> String {
    let width = 24u16;
    let height = 3u16;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| render_wave(frame, Rect::new(0, 0, width, height), wall_tick, muted))
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

fn render_wave_at(wall_tick: usize) -> String {
    render_wave_state(wall_tick, false)
}

#[test]
fn wave_tile_rows_are_one_period_wide() {
    // The tile is hand-drawn; a stray edit that changes a row's width
    // would shear the scroll wrap.
    for row in WAVE_TILE {
        assert_eq!(row.chars().count(), WAVE_PERIOD_COLS);
    }
}

#[test]
fn wave_renders_a_connected_box_line() {
    let rendered = render_wave_at(0);
    for glyph in ['╭', '╮', '╰', '╯', '─'] {
        assert!(rendered.contains(glyph), "wave is missing '{glyph}'");
    }
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
    // One full period of scroll = WAVE_PERIOD_COLS steps at one step per
    // two wall ticks.
    assert_eq!(render_wave_at(0), render_wave_at(WAVE_PERIOD_COLS * 2));
}

#[test]
fn sub_edge_ticks_render_identically() {
    // Ticks inside the same anim_half period share an offset; the paint
    // gate skips them, and even if painted they would be identical.
    assert_eq!(render_wave_at(4), render_wave_at(5));
}

#[test]
fn muted_client_flattens_the_wave_to_a_steady_line() {
    // Mute is the oscilloscope at rest: a flat line that ignores the
    // scroll offset entirely, so consecutive frames diff to nothing.
    let rendered = render_wave_state(6, true);
    assert!(rendered.contains("─".repeat(24).as_str()));
    assert!(!rendered.contains('╭'), "muted wave must not show crests");
    assert_eq!(render_wave_state(6, true), render_wave_state(20, true));
}
