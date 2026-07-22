use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn render_inline_idle(height: u16) -> String {
    let width = 24;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let viz = Visualizer::new();

    terminal
        .draw(|frame| viz.render_inline(frame, Rect::new(0, 0, width, height)))
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
fn idle_inline_visualizer_shows_pair_shortcut() {
    let rendered = render_inline_idle(6);

    assert!(rendered.contains("no audio paired"));
    assert!(rendered.contains("? guide"));
    assert!(rendered.contains("pair"));
    assert!(rendered.contains("v+x"));
    assert!(rendered.contains("source"));
}

#[test]
fn idle_inline_visualizer_drops_shortcut_when_too_short() {
    let rendered = render_inline_idle(4);

    assert!(rendered.contains("no audio paired"));
    assert!(!rendered.contains("remote"));
}

#[test]
fn resample_same_size() {
    let viz = Visualizer::new();
    let input = vec![1.0, 2.0, 3.0];
    let result = viz.resample(&input, 3);
    assert_eq!(result, input);
}

#[test]
fn resample_upsample() {
    let viz = Visualizer::new();
    let input = vec![0.0, 1.0];
    let result = viz.resample(&input, 3);
    assert_eq!(result.len(), 3);
    assert_eq!(result[0], 0.0);
    assert_eq!(result[2], 1.0);
    assert!((result[1] - 0.5).abs() < 0.001);
}

#[test]
fn resample_downsample() {
    let viz = Visualizer::new();
    let input = vec![0.0, 0.5, 1.0];
    let result = viz.resample(&input, 2);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0], 0.0);
    assert_eq!(result[1], 1.0);
}

#[test]
fn resample_empty() {
    let viz = Visualizer::new();
    let result = viz.resample(&[], 5);
    assert!(result.is_empty());
}

#[test]
fn resample_zero_target() {
    let viz = Visualizer::new();
    let result = viz.resample(&[1.0, 2.0], 0);
    assert!(result.is_empty());
}

#[test]
fn tilt_clamps_output() {
    let result = Visualizer::tilt(2.0, 0, 8);
    assert!(result <= 1.0);
}

#[test]
fn tilt_single_element() {
    let result = Visualizer::tilt(0.5, 0, 1);
    assert!((0.0..=1.0).contains(&result));
}

#[test]
fn tilt_increases_with_index() {
    let low = Visualizer::tilt(0.5, 0, 8);
    let high = Visualizer::tilt(0.5, 7, 8);
    assert!(high > low);
}

#[test]
fn tick_idle_decays_rms() {
    let mut viz = Visualizer::new();
    viz.has_viz = true;
    viz.rms = 1.0;
    viz.bands = [1.0; 8];
    viz.tick_idle();
    assert!(viz.rms < 1.0);
    assert!(viz.rms > 0.0);
    assert!(viz.bands.iter().all(|band| *band < 1.0 && *band > 0.0));
}

#[test]
fn tick_idle_no_op_without_viz() {
    let mut viz = Visualizer::new();
    viz.rms = 1.0;
    viz.tick_idle();
    assert_eq!(viz.rms, 1.0); // unchanged because has_viz is false
}

#[test]
fn tick_procedural_advances_phase_when_active() {
    let mut viz = Visualizer::new();
    viz.set_procedural_active(true);
    let before = viz.procedural_phase;
    viz.tick_procedural();
    assert!(viz.procedural_phase > before);
}

#[test]
fn tick_procedural_no_op_when_inactive() {
    let mut viz = Visualizer::new();
    viz.tick_procedural();
    assert_eq!(viz.procedural_phase, 0.0);
}

#[test]
fn procedural_bands_stay_in_range() {
    let viz = Visualizer::new();
    for h in viz.procedural_bands() {
        assert!((0.0..=1.0).contains(&h));
    }
}

#[test]
fn procedural_bands_animate_with_phase() {
    let mut viz = Visualizer::new();
    let first = viz.procedural_bands();
    viz.set_procedural_active(true);
    // Several ticks should produce a different shape.
    for _ in 0..10 {
        viz.tick_procedural();
    }
    let later = viz.procedural_bands();
    assert!(
        first
            .iter()
            .zip(later.iter())
            .any(|(a, b)| (a - b).abs() > 0.01)
    );
}

#[test]
fn update_smooths_real_viz_after_first_frame() {
    let mut viz = Visualizer::new();
    viz.update(&VizFrame {
        bands: [1.0; 8],
        rms: 1.0,
        track_pos_ms: 0,
    });
    assert_eq!(viz.bands, [1.0; 8]);
    assert_eq!(viz.rms, 1.0);

    viz.update(&VizFrame {
        bands: [0.0; 8],
        rms: 0.0,
        track_pos_ms: 100,
    });

    assert!(viz.bands.iter().all(|band| *band < 1.0 && *band > 0.0));
    assert!(viz.rms < 1.0 && viz.rms > 0.0);
}

#[test]
fn render_inline_uses_procedural_path_when_active() {
    let width = 17;
    let height = 6;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut viz = Visualizer::new();
    viz.set_procedural_active(true);
    // Advance once so at least one band sits above the midline.
    viz.tick_procedural();

    terminal
        .draw(|frame| viz.render_inline(frame, Rect::new(0, 0, width, height)))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..height {
        for x in 0..width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
    }
    // Procedural path renders bars, NOT the idle "no audio paired" copy.
    assert!(rendered.contains('█'));
    assert!(!rendered.contains("no audio paired"));
}

#[test]
fn render_inline_uses_real_viz_after_update() {
    let width = 17;
    let height = 4;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut viz = Visualizer::new();
    viz.update(&VizFrame {
        bands: [1.0, 0.8, 0.6, 0.4, 0.2, 0.0, 0.5, 0.9],
        rms: 0.7,
        track_pos_ms: 1234,
    });

    terminal
        .draw(|frame| viz.render_inline(frame, Rect::new(0, 0, width, height)))
        .expect("draw");

    let buffer = terminal.backend().buffer();
    let mut rendered = String::new();
    for y in 0..height {
        for x in 0..width {
            rendered.push_str(buffer[(x, y)].symbol());
        }
    }
    assert!(rendered.contains('█'));
    assert!(!rendered.contains("no audio paired"));
}

#[test]
fn procedural_takes_priority_over_real_viz() {
    // If both real viz frames AND procedural are active, procedural wins —
    // user pinned to YouTube source should not see stale Icecast bars.
    let width = 17;
    let height = 4;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    let mut viz = Visualizer::new();
    viz.has_viz = true;
    viz.bands = [1.0; 8];
    viz.set_procedural_active(true);

    terminal
        .draw(|frame| viz.render_inline(frame, Rect::new(0, 0, width, height)))
        .expect("draw");

    // Procedural bars peak around 0.75; full-height column from real bands
    // would fill every row. Top row should be empty under the procedural path.
    let buffer = terminal.backend().buffer();
    let mut top_row = String::new();
    for x in 0..width {
        top_row.push_str(buffer[(x, 0)].symbol());
    }
    assert!(!top_row.contains('█'));
}

#[test]
fn idle_decay_settles_and_stops_reporting_change() {
    let mut viz = Visualizer::new();

    // Untouched visualizer: nothing to decay, nothing to repaint.
    assert!(!viz.tick_idle());

    viz.update(&late_core::audio::VizFrame {
        bands: [1.0; 8],
        rms: 1.0,
        track_pos_ms: 0,
    });
    assert!(viz.tick_idle(), "fresh energy must animate the decay");

    // Decay runs out within a bounded number of ticks and then goes quiet.
    let mut settled_at = None;
    for tick in 0..500 {
        if !viz.tick_idle() {
            settled_at = Some(tick);
            break;
        }
    }
    assert!(settled_at.is_some(), "idle decay never settled");
    assert!(!viz.tick_idle(), "settled visualizer must stay quiet");
    assert_eq!(viz.rms(), 0.0);
}
