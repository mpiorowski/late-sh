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

fn viz_frame(bands: [f32; VIZ_BANDS]) -> VizFrame {
    VizFrame {
        bands,
        rms: 0.5,
        track_pos_ms: 0,
    }
}

/// Bands alternating between two values, low band first.
fn alternating(even: f32, odd: f32) -> [f32; VIZ_BANDS] {
    std::array::from_fn(|band| match band % 2 {
        0 => even,
        _ => odd,
    })
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
#[expect(
    clippy::approx_constant,
    reason = "0.318 is an eased band level, not 1/pi"
)]
fn spectrum_eases_levels_and_caps_hold_where_the_bars_struck() {
    // Drive a run of three frames at one instant, loud bass then loud treble
    // then silence, and check the whole smoothed state after each. No time
    // passes, so the level stays where the first frame set it (mean 0.5,
    // swing 0.5: a full band meters 0.55, an empty one 0.15). Rising bands
    // attack, falling bands release, and every cap stays where its bar
    // last struck.
    let start = Instant::now();
    let halves = |low: f32, high: f32| -> [f32; VIZ_BANDS] {
        std::array::from_fn(|band| match band < VIZ_BANDS / 2 {
            true => low,
            false => high,
        })
    };
    let bass = viz_frame(halves(1.0, 0.0));
    let treble = viz_frame(halves(0.0, 1.0));
    let silence = viz_frame([0.0; VIZ_BANDS]);

    let first = Spectrum::next(None, &bass, start);
    assert_bands_near(
        first.bands(),
        LiveBands {
            levels: halves(0.55, 0.15),
            peaks: halves(0.55, 0.15),
        },
    );

    let second = Spectrum::next(Some(first), &treble, start);
    assert_bands_near(
        second.bands(),
        LiveBands {
            levels: halves(0.43, 0.39),
            peaks: halves(0.55, 0.39),
        },
    );

    let third = Spectrum::next(Some(second), &silence, start);
    assert_bands_near(
        third.bands(),
        LiveBands {
            levels: halves(0.346, 0.318),
            peaks: halves(0.55, 0.39),
        },
    );
}

#[test]
fn a_cap_hangs_through_its_hold_then_falls_faster_and_lands_on_its_bar() {
    // The bands trade places: the even ones strike, then drop while the odd
    // ones rise. Every frame keeps mean 0.5 and swing 0.5, so the level
    // never moves and a band meters 0.55 or 0.15. The even caps hold for
    // CAP_HOLD_SECS, fall under gravity (0.2s past the hold they have
    // dropped 0.5 * 3.0 * 0.2^2 = 0.06), and once they would pass below
    // their bars they land on them.
    let start = Instant::now();
    let even_up = viz_frame(alternating(1.0, 0.0));
    let odd_up = viz_frame(alternating(0.0, 1.0));

    let hit = Spectrum::next(None, &even_up, start);
    assert_bands_near(
        hit.bands(),
        LiveBands {
            levels: alternating(0.55, 0.15),
            peaks: alternating(0.55, 0.15),
        },
    );

    let holding = Spectrum::next(Some(hit), &odd_up, start + Duration::from_millis(400));
    assert_bands_near(
        holding.bands(),
        LiveBands {
            levels: alternating(0.43, 0.39),
            peaks: alternating(0.55, 0.39),
        },
    );

    let falling = Spectrum::next(Some(holding), &odd_up, start + Duration::from_millis(700));
    assert_bands_near(
        falling.bands(),
        LiveBands {
            levels: alternating(0.346, 0.486),
            peaks: alternating(0.49, 0.486),
        },
    );

    let landed = Spectrum::next(Some(falling), &odd_up, start + Duration::from_millis(1000));
    assert_bands_near(
        landed.bands(),
        LiveBands {
            levels: alternating(0.2872, 0.5244),
            peaks: alternating(0.2872, 0.5244),
        },
    );
}

#[test]
fn the_level_spreads_a_flat_passage_low_and_lets_a_hit_jump() {
    // A flat passage like the one on screen: every band at 0.5 or 0.6 of the
    // CLI meter. Metered against its own level (mean 0.55, swing 0.05) it
    // spreads a swing either side of the low center: 0.55 and 0.15.
    let start = Instant::now();
    let flat = Spectrum::next(None, &viz_frame(alternating(0.6, 0.5)), start);
    assert_bands_near(
        flat.bands(),
        LiveBands {
            levels: alternating(0.55, 0.15),
            peaks: alternating(0.55, 0.15),
        },
    );

    // A hit lifts every band to 0.8 before the level can settle: five swings
    // above the mean, so it meters full and the bars attack toward the top.
    let hit = Spectrum::next(Some(flat), &viz_frame([0.8; VIZ_BANDS]), start);
    assert_bands_near(
        hit.bands(),
        LiveBands {
            levels: alternating(0.82, 0.66),
            peaks: alternating(0.82, 0.66),
        },
    );

    // A louder passage held long enough to settle meters where the flat one
    // did (0.55 and 0.15), so the bars ease back down toward it: the meter
    // follows how the music moves, not how loud it is.
    let settled = Spectrum::next(
        Some(hit),
        &viz_frame(alternating(0.9, 0.8)),
        start + Duration::from_secs(60),
    );
    assert_bands_near(
        settled.bands(),
        LiveBands {
            levels: alternating(0.739, 0.507),
            peaks: alternating(0.739, 0.507),
        },
    );
}

#[test]
fn hostile_frames_land_inside_the_band() {
    // Frames come off the network: NaN, infinities and out-of-range values
    // are clamped before the level sees them (clamped, the frame is
    // 0,0,0,1,0.5,0,1,0 twice: mean 0.3125, swing 0.390625), so every band
    // meters inside the band.
    let frame = viz_frame(std::array::from_fn(|band| {
        [f32::NAN, f32::INFINITY, -3.0, 7.0, 0.5, 0.0, 1.0, -0.0][band % 8]
    }));
    let spectrum = Spectrum::next(None, &frame, Instant::now());
    let metered: [f32; VIZ_BANDS] =
        std::array::from_fn(|band| [0.19, 0.19, 0.19, 0.702, 0.446, 0.19, 0.702, 0.19][band % 8]);
    assert_bands_near(
        spectrum.bands(),
        LiveBands {
            levels: metered,
            peaks: metered,
        },
    );
}

#[test]
fn spectrum_goes_stale_once_the_client_stops_sending() {
    let start = Instant::now();
    let spectrum = Spectrum::next(None, &viz_frame([0.5; VIZ_BANDS]), start);
    assert!(!spectrum.is_stale(start + Duration::from_millis(700)));
    assert!(spectrum.is_stale(start + Duration::from_millis(800)));
}

#[test]
fn live_bars_draw_the_spectrum_not_the_wall_clock() {
    // Loud bass, silent treble: the left bars fill the strip, the right bars
    // keep only their base pixel, and the frame ignores the wall tick.
    let bands: [f32; VIZ_BANDS] = std::array::from_fn(|band| match band < VIZ_BANDS / 2 {
        true => 1.0,
        false => 0.0,
    });
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

/// Every cell of drawn lines as its glyph and style, top row first.
fn cells(lines: &[Line<'static>]) -> Vec<Vec<(char, Style)>> {
    lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .flat_map(|span| span.content.chars().map(move |glyph| (glyph, span.style)))
                .collect()
        })
        .collect()
}

#[test]
fn a_bar_that_fell_leaves_its_ghost_standing_up_to_the_peak() {
    // The bars fell silent under peaks still hanging: the ghost fills the
    // air from the bar's head up to its peak, the head cell carries the
    // ghost behind its glyph, and the gap columns stay bare.
    let ghost = theme::EQ_GHOST();
    let live = LiveBands {
        levels: [0.0; VIZ_BANDS],
        peaks: std::array::from_fn(|band| match band {
            0 | 1 => 1.0,
            _ => 0.5,
        }),
    };
    let ghost_cell = ('█', Style::default().fg(ghost));
    let base_under_ghost = ('▁', row_style(2, 3).bg(ghost));

    // The music stage: a full peak over three rows ghosts the whole column.
    let stage = cells(&dance_lines(Dance::Live(live), 6, TEST_WIDTH as usize, 3));
    let first_bar: Vec<(char, Style)> = stage.iter().map(|row| row[0]).collect();
    assert_eq!(first_bar, vec![ghost_cell, ghost_cell, base_under_ghost]);
    assert!(
        stage.iter().all(|row| row[1].1.bg.is_none()),
        "a gap column never picks up the ghost"
    );

    // A tall visualizer tile: a half-height peak over six rows ghosts the
    // bottom three cells and leaves open air above.
    let tall = cells(&dance_lines(Dance::Live(live), 6, TEST_WIDTH as usize, 6));
    let last_bar = TEST_WIDTH as usize - BAR_STRIDE;
    let column: Vec<(char, Style)> = tall.iter().map(|row| row[last_bar]).collect();
    let air: Vec<char> = column[..3].iter().map(|(glyph, _)| *glyph).collect();
    assert_eq!(air, vec![' ', ' ', ' '], "open air above the peak");
    assert_eq!(
        column[3..],
        [ghost_cell, ghost_cell, ('▁', row_style(5, 6).bg(ghost))]
    );
}

#[test]
fn a_peak_inside_the_bars_top_cell_paints_no_ghost_above_it() {
    // Over three rows a bar at 0.1 fills 2 of its bottom cell's 8 eighths
    // and a peak at 0.25 ends at 6, inside that same cell. One cell holds one
    // background, so a ghost there would fill to the cell's top, a third of
    // the strip, and stand far above where the bar ever reached. The cell
    // draws the bar alone.
    let live = LiveBands {
        levels: [0.1; VIZ_BANDS],
        peaks: [0.25; VIZ_BANDS],
    };
    let stage = cells(&dance_lines(Dance::Live(live), 6, TEST_WIDTH as usize, 3));
    let first_bar: Vec<char> = stage.iter().map(|row| row[0].0).collect();
    assert_eq!(first_bar, vec![' ', ' ', '▂']);
    assert_eq!(stage[2][0].1, row_style(2, 3), "no ghost behind the head");
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
