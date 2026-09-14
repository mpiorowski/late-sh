use super::*;
use ratatui::{Terminal, backend::TestBackend};

const TEST_WIDTH: u16 = 24;
const TEST_HEIGHT: u16 = 3;

fn render_eq_state(wall_tick: usize, state: EqState) -> String {
    let backend = TestBackend::new(TEST_WIDTH, TEST_HEIGHT);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| {
            render_eq(
                frame,
                Rect::new(0, 0, TEST_WIDTH, TEST_HEIGHT),
                wall_tick,
                state,
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
    render_eq_state(wall_tick, EqState::Ambient)
}

fn viz_frame(bands: [f32; 8]) -> VizFrame {
    VizFrame {
        bands,
        rms: 0.5,
        track_pos_ms: 0,
    }
}

fn assert_bands_near(actual: LiveBands, expected: LiveBands) {
    let pairs = actual
        .levels
        .iter()
        .zip(expected.levels)
        .chain(actual.peaks.iter().zip(expected.peaks));
    for (got, want) in pairs {
        assert!(
            (got - want).abs() < 1e-5,
            "got {actual:?}, want {expected:?}"
        );
    }
}

#[test]
fn spectrum_eases_levels_and_caps_hold_where_the_bars_struck() {
    // Drive a run of three frames at the CLI's ~15 Hz, loud bass then loud
    // treble then silence, and check the whole smoothed state after each:
    // rising bands attack, falling bands release, and every cap stays where
    // its bar last struck, since the run is still inside the hold.
    let start = Instant::now();
    let bass = viz_frame([1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
    let treble = viz_frame([0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
    let silence = viz_frame([0.0; 8]);

    let first = Spectrum::next(None, &bass, start);
    assert_bands_near(
        first.bands(),
        LiveBands {
            levels: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
            peaks: [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0],
        },
    );

    let second = Spectrum::next(Some(first), &treble, start + Duration::from_millis(67));
    assert_bands_near(
        second.bands(),
        LiveBands {
            levels: [0.7, 0.7, 0.7, 0.7, 0.6, 0.6, 0.6, 0.6],
            peaks: [1.0, 1.0, 1.0, 1.0, 0.6, 0.6, 0.6, 0.6],
        },
    );

    let third = Spectrum::next(Some(second), &silence, start + Duration::from_millis(134));
    assert_bands_near(
        third.bands(),
        LiveBands {
            levels: [0.49, 0.49, 0.49, 0.49, 0.42, 0.42, 0.42, 0.42],
            peaks: [1.0, 1.0, 1.0, 1.0, 0.6, 0.6, 0.6, 0.6],
        },
    );
}

#[test]
fn a_cap_hangs_through_its_hold_then_falls_faster_and_lands_on_its_bar() {
    // One hit, then silence: the cap holds for CAP_HOLD_SECS, falls under
    // gravity (0.4s past the hold it has dropped 0.5 * 3.0 * 0.4^2 = 0.24),
    // and once it would pass below the bar it lands on it.
    let start = Instant::now();
    let hit = Spectrum::next(None, &viz_frame([1.0; 8]), start);
    let silence = viz_frame([0.0; 8]);

    let holding = Spectrum::next(Some(hit), &silence, start + Duration::from_millis(400));
    assert_bands_near(
        holding.bands(),
        LiveBands {
            levels: [0.7; 8],
            peaks: [1.0; 8],
        },
    );

    let falling = Spectrum::next(Some(holding), &silence, start + Duration::from_millis(900));
    assert_bands_near(
        falling.bands(),
        LiveBands {
            levels: [0.49; 8],
            peaks: [0.76; 8],
        },
    );

    let landed = Spectrum::next(Some(falling), &silence, start + Duration::from_millis(1400));
    assert_bands_near(
        landed.bands(),
        LiveBands {
            levels: [0.343; 8],
            peaks: [0.343; 8],
        },
    );
}

#[test]
fn hostile_frames_land_inside_the_band() {
    // Frames come off the network: NaN, infinities and out-of-range values
    // must not reach the renderer's level math.
    let frame = viz_frame([f32::NAN, f32::INFINITY, -3.0, 7.0, 0.5, 0.0, 1.0, -0.0]);
    let spectrum = Spectrum::next(None, &frame, Instant::now());
    assert_bands_near(
        spectrum.bands(),
        LiveBands {
            levels: [0.0, 0.0, 0.0, 1.0, 0.5, 0.0, 1.0, 0.0],
            peaks: [0.0, 0.0, 0.0, 1.0, 0.5, 0.0, 1.0, 0.0],
        },
    );
}

#[test]
fn spectrum_goes_stale_once_the_client_stops_sending() {
    let start = Instant::now();
    let spectrum = Spectrum::next(None, &viz_frame([0.5; 8]), start);
    assert!(!spectrum.is_stale(start + Duration::from_millis(700)));
    assert!(spectrum.is_stale(start + Duration::from_millis(800)));
}

#[test]
fn live_bars_draw_the_spectrum_not_the_wall_clock() {
    // Loud bass, silent treble: the left bars fill the strip, the right bars
    // keep only their base pixel, and the frame ignores the wall tick.
    let bands = [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
    let live = EqState::Live(LiveBands {
        levels: bands,
        peaks: bands,
    });
    let rendered = render_eq_state(6, live);
    let rows: Vec<Vec<char>> = rendered.lines().map(|row| row.chars().collect()).collect();
    assert_eq!(rows[0][0], '█', "loud bass bar reaches the top row");
    assert_eq!(rows[2][0], '█', "loud bass bar fills its base");
    let last_bar = TEST_WIDTH as usize - BAR_STRIDE;
    assert_eq!(rows[0][last_bar], ' ', "silent treble bar stays low");
    assert_eq!(rows[2][last_bar], '▁', "silent treble bar keeps its base");
    assert_eq!(rendered, render_eq_state(20, live));
}

#[test]
fn a_held_cap_floats_above_its_fallen_bar_at_any_height() {
    // The bars fell silent under caps still hanging: the cap glyph sits in
    // the cell its height reaches, with open air between it and the base.
    let live = LiveBands {
        levels: [0.0; 8],
        peaks: [1.0, 1.0, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5],
    };
    let rendered = render_eq_state(6, EqState::Live(live));
    let rows: Vec<Vec<char>> = rendered.lines().map(|row| row.chars().collect()).collect();
    assert_eq!(rows[0][0], '▁', "a full cap hangs in the top row");
    assert_eq!(rows[1][0], ' ', "above a bar that fell");
    assert_eq!(rows[2][0], '▁', "which keeps its base");

    // A tall visualizer tile: a half-height cap over six rows sits in the
    // third cell from the bottom.
    let tall: Vec<String> = dance_lines(Dance::Live(live), 6, TEST_WIDTH as usize, 6)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    let last_bar = TEST_WIDTH as usize - BAR_STRIDE;
    let column: Vec<char> = tall
        .iter()
        .map(|row| row.chars().nth(last_bar).expect("column"))
        .collect();
    assert_eq!(column, vec![' ', ' ', ' ', '▁', ' ', '▁']);
}

#[test]
fn ambient_bars_stay_inside_the_band() {
    // The synthesized height must stay in (0, 1]: zero would blank a bar
    // (the band must always read as live), above one would overflow the
    // row cell math.
    for frame in 0..500 {
        for bar in 0..12 {
            let unit = ambient_unit(bar, 12, frame);
            assert!(unit > 0.0 && unit <= 1.0, "bar {bar} frame {frame}: {unit}");
        }
    }
}

#[test]
fn ambient_caps_ride_at_or_above_their_bar() {
    // The ambient cap is the highest recent strike still falling, and the
    // current frame is one of those strikes, so it can never sit below the
    // bar.
    for frame in 0..500 {
        for bar in 0..12 {
            assert!(ambient_cap_unit(bar, 12, frame) >= ambient_unit(bar, 12, frame));
        }
    }
}

#[test]
fn left_bars_run_taller_than_right_bars() {
    // The bass-heavy envelope: averaged over time, the leftmost bar
    // outruns the rightmost, the way a real spectrum sits.
    let average = |bar: usize| -> f64 {
        (0..500)
            .map(|f| ambient_unit(bar, 12, f) as f64)
            .sum::<f64>()
            / 500.0
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
    // Every other column is a gap; the bottom row shows the rhythm most
    // clearly since every bar has at least its base pixel there.
    let bottom = rendered.lines().last().expect("rows");
    for (col, glyph) in bottom.chars().enumerate() {
        if col % BAR_STRIDE == BAR_STRIDE - 1 {
            assert_eq!(glyph, ' ', "gap column {col} must stay blank");
        } else {
            assert_ne!(glyph, ' ', "bar column {col} must carry its base");
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
    let rendered = render_eq_state(6, EqState::Muted);
    assert!(rendered.contains("─".repeat(TEST_WIDTH as usize).as_str()));
    assert!(
        BLOCKS[1..].iter().all(|glyph| !rendered.contains(*glyph)),
        "muted band must not show bars"
    );
    assert_eq!(
        render_eq_state(6, EqState::Muted),
        render_eq_state(20, EqState::Muted)
    );
}

#[test]
fn unpaired_session_points_at_the_guide_instead_of_dancing() {
    // Nothing is paired, so no audio can be playing for this session. A
    // dancing band would be claiming playback that does not exist.
    let rendered = render_eq_state(6, EqState::Unpaired);
    assert!(rendered.contains("no audio here yet"));
    assert!(rendered.contains("press ? to listen"));
    assert!(
        BLOCKS[1..].iter().all(|glyph| !rendered.contains(*glyph)),
        "unpaired band must not show bars"
    );
    // Static, like the muted flatline: the animation frame must not leak in.
    assert_eq!(
        render_eq_state(6, EqState::Unpaired),
        render_eq_state(20, EqState::Unpaired)
    );
}
