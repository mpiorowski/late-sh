use super::*;
use ratatui::{Terminal, backend::TestBackend};

const TEST_WIDTH: u16 = 24;
const TEST_HEIGHT: u16 = 3;

fn render_eq_state(wall_tick: usize, muted: bool) -> String {
    let backend = TestBackend::new(TEST_WIDTH, TEST_HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            render_eq(
                frame,
                Rect::new(0, 0, TEST_WIDTH, TEST_HEIGHT),
                wall_tick,
                muted,
            )
        })
        .expect("draw");
    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..TEST_HEIGHT {
        for x in 0..TEST_WIDTH {
            rendered.push_str(buffer[(x, y)].symbol());
        }
        rendered.push('\n');
    }
    rendered
}

fn render_eq_at(wall_tick: usize) -> String {
    render_eq_state(wall_tick, false)
}

#[test]
fn bar_levels_stay_inside_the_band() {
    // The synthesized level must never leave 1..=MAX_LEVEL: zero would
    // blank a bar (the band must always read as live), above MAX_LEVEL
    // would overflow the row cell math.
    for frame in 0..500 {
        for bar in 0..12 {
            let level = bar_level(bar, 12, frame);
            assert!((1..=MAX_LEVEL).contains(&level), "bar {bar} frame {frame}");
        }
    }
}

#[test]
fn caps_ride_at_or_above_their_bar() {
    // The peak cap is a trailing max, so it can never sit below the live
    // bar level.
    for frame in 0..500 {
        for bar in 0..12 {
            assert!(cap_level(bar, 12, frame) >= bar_level(bar, 12, frame));
        }
    }
}

#[test]
fn left_bars_run_taller_than_right_bars() {
    // The bass-heavy envelope: averaged over time, the leftmost bar
    // outruns the rightmost, the way a real spectrum sits.
    let average = |bar: usize| -> f64 {
        (0..500).map(|f| bar_level(bar, 12, f) as f64).sum::<f64>() / 500.0
    };
    assert!(average(0) > average(11));
}

#[test]
fn eq_renders_block_bars_with_gap_columns() {
    let rendered = render_eq_at(0);
    assert!(
        BLOCKS[1..].iter().any(|glyph| rendered.contains(*glyph)),
        "band is missing block glyphs"
    );
    // Every third column is a gap; the bottom row shows the rhythm most
    // clearly since every bar has at least its base pixel there.
    let bottom: Vec<char> = rendered.lines().last().expect("rows").chars().collect();
    for col in 0..TEST_WIDTH as usize {
        if col % BAR_STRIDE == BAR_STRIDE - 1 {
            assert_eq!(bottom[col], ' ', "gap column {col} must stay blank");
        } else {
            assert_ne!(bottom[col], ' ', "bar column {col} must carry its base");
        }
    }
}

#[test]
fn eq_dances_with_the_wall_clock() {
    // Two wall ticks apart crosses an anim_half edge, so the bars moved.
    assert_ne!(render_eq_at(0), render_eq_at(2));
}

#[test]
fn eq_is_deterministic_for_a_tick() {
    // No hidden state: the wall tick alone decides the frame.
    assert_eq!(render_eq_at(6), render_eq_at(6));
}

#[test]
fn sub_edge_ticks_render_identically() {
    // Ticks inside the same anim_half period share a paid frame; the
    // paint gate skips them, and even if painted they would be identical.
    assert_eq!(render_eq_at(4), render_eq_at(5));
}

#[test]
fn muted_client_flattens_the_band_to_a_steady_line() {
    // Mute is the meter at rest: a flat line that ignores the animation
    // frame entirely, so consecutive frames diff to nothing.
    let rendered = render_eq_state(6, true);
    assert!(rendered.contains("─".repeat(TEST_WIDTH as usize).as_str()));
    assert!(
        BLOCKS[1..].iter().all(|glyph| !rendered.contains(*glyph)),
        "muted band must not show bars"
    );
    assert_eq!(render_eq_state(6, true), render_eq_state(20, true));
}
