use crate::app::settings_modal::gem::*;

#[test]
fn duplicate_consecutive_keys_are_deduped() {
    let mut gem = GemState::new();
    let initial = gem.color();

    gem.handle_key(GemKey::Space);
    let after_first = gem.color();
    assert_ne!(initial, after_first, "first press must change color");

    // Second identical press is dropped — color stays put.
    gem.handle_key(GemKey::Space);
    assert_eq!(gem.color(), after_first);

    // A different key counts again.
    gem.handle_key(GemKey::J);
    assert_ne!(gem.color(), after_first);
}

#[test]
fn mouse_click_resets_key_dedupe() {
    let mut gem = GemState::new();
    gem.handle_key(GemKey::Space);
    let after_key = gem.color();

    gem.handle_click();
    let after_click = gem.color();
    assert_ne!(after_key, after_click);

    // Same key as before the click now counts because the click reset
    // the dedupe state.
    gem.handle_key(GemKey::Space);
    assert_ne!(gem.color(), after_click);
}
