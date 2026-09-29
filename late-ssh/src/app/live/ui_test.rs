use super::*;

#[test]
fn the_card_picks_the_form_and_keeps_rows_for_the_messages() {
    let card = |width, height| Rect {
        x: 0,
        y: 0,
        width,
        height,
    };
    let (size, strip, rest) = fit_live_strip(card(80, 30)).unwrap();
    assert_eq!(size, StripSize::Full);
    assert_eq!(strip.height, LIVE_STRIP_HEIGHT);
    assert_eq!(rest.height, 30 - LIVE_STRIP_HEIGHT);

    let (size, strip, _) = fit_live_strip(card(80, 14)).unwrap();
    assert_eq!(
        size,
        StripSize::Compact,
        "a short card gets the one-row form"
    );
    assert_eq!(strip.height, LIVE_STRIP_COMPACT_HEIGHT);

    let (size, _, _) = fit_live_strip(card(40, 30)).unwrap();
    assert_eq!(size, StripSize::Compact, "a narrow card too");

    assert!(fit_live_strip(card(80, 4)).is_none(), "no room at all");
}
