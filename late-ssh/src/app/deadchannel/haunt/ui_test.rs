use super::*;

/// The cells one static surge paints at `tick`, at full strength.
fn static_at(tick: usize) -> ratatui::buffer::Buffer {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 12)).expect("test terminal");
    terminal
        .draw(|frame| draw_static_surge(frame, frame.area(), tick, 7, 0.0, 0.5))
        .expect("draw static");
    terminal.backend().buffer().clone()
}

#[test]
fn static_holds_each_pattern_for_a_few_frames_then_shifts() {
    let first = static_at(0);
    assert!(
        first.content().iter().any(|cell| cell.symbol() != " "),
        "expected the surge to paint something"
    );
    for tick in 1..STATIC_FRAME_TICKS {
        assert_eq!(static_at(tick), first, "pattern moved early at tick {tick}");
    }
    assert_ne!(static_at(STATIC_FRAME_TICKS), first);
}

#[test]
fn dissolved_hint_is_deterministic_and_ends_gone() {
    let hint = "press Esc to skip";
    // Intact below every threshold.
    assert_eq!(dissolved_hint(hint, 0.0, 7), Some(hint.to_string()));
    // The same (progress, seed) always paints the same corruption.
    let first = dissolved_hint(hint, 0.5, 7);
    let second = dissolved_hint(hint, 0.5, 7);
    assert_eq!(first, second);
    // Mid-dissolve the text is changed but the same width, whitespace kept.
    let mid = first.unwrap();
    assert_ne!(mid, hint);
    assert_eq!(mid.chars().count(), hint.chars().count());
    assert!(
        mid.char_indices().filter(|(_, c)| *c == ' ').count()
            >= hint.char_indices().filter(|(_, c)| *c == ' ').count()
    );
    // Fully dissolved means gone, not garbage.
    assert_eq!(dissolved_hint(hint, 1.0, 7), None);
}

#[test]
fn glitched_clock_swaps_only_time_characters() {
    let clock = "CEST 14:32";
    let first = glitched_clock(clock, 99);
    let second = glitched_clock(clock, 99);
    assert_eq!(first, second);
    assert_eq!(first.chars().count(), clock.chars().count());
    // The timezone label is untouched; only digits/colon may change, and
    // one or two of them did, to glyph-alphabet characters.
    let changed: Vec<(char, char)> = clock
        .chars()
        .zip(first.chars())
        .filter(|(before, after)| before != after)
        .collect();
    assert!((1..=2).contains(&changed.len()), "changed: {changed:?}");
    for (before, after) in changed {
        assert!(before.is_ascii_digit() || before == ':');
        assert!(
            crate::app::deadchannel::glyphs::GLYPH_ALPHABET.contains(&after),
            "swapped in {after:?}"
        );
    }
}

#[test]
fn glitched_name_touches_name_characters_only() {
    let label = "mira 🇵🇱";
    let first = glitched_name(label, 4);
    assert_eq!(first, glitched_name(label, 4));
    assert_eq!(first.chars().count(), label.chars().count());
    let changed: Vec<(char, char)> = label
        .chars()
        .zip(first.chars())
        .filter(|(before, after)| before != after)
        .collect();
    assert!((2..=3).contains(&changed.len()), "changed: {changed:?}");
    for (before, after) in changed {
        assert!(before.is_alphanumeric());
        assert!(crate::app::deadchannel::glyphs::GLYPH_ALPHABET.contains(&after));
    }
    // The second wave of a hit arrives as a different seed, and it has to
    // read as a different corruption, or the wave edge is invisible.
    let second_wave = glitched_name(label, 4 ^ 0x9E37_79B9_7F4A_7C15);
    assert_ne!(second_wave, first);
    assert_ne!(second_wave, label);
}
