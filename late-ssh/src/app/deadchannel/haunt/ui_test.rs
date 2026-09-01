use super::*;

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
}
