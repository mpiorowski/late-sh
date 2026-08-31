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
