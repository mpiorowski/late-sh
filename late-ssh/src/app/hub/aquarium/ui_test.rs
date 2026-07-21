use super::*;
use crate::app::dashboard::ui::MIN_CHAT_HEIGHT_WITH_LOUNGE;

const TALL_ENOUGH: u16 = TOP_TRAY_HEIGHT + MIN_CHAT_HEIGHT_WITH_LOUNGE;

#[test]
fn carves_a_full_height_tray_when_the_lounge_still_fits() {
    let (tray, rest) = carve_top_tray(Rect::new(0, 0, 80, TALL_ENOUGH));
    let tray = tray.expect("tray fits");
    assert_eq!(tray.height, TOP_TRAY_HEIGHT);
    assert_eq!(rest.y, tray.bottom());
    assert_eq!(rest.height, MIN_CHAT_HEIGHT_WITH_LOUNGE);
}

#[test]
fn skips_the_tray_rather_than_squeezing_the_lounge_below_its_minimum() {
    // One row short: the composer must survive, since `/aquarium` typed
    // into it is the only way back out of the tray.
    let area = Rect::new(0, 0, 80, TALL_ENOUGH - 1);
    let (tray, rest) = carve_top_tray(area);
    assert!(tray.is_none());
    assert_eq!(rest, area);
}

#[test]
fn skips_the_tray_on_a_terminal_too_short_to_hold_it() {
    let area = Rect::new(0, 0, 80, TOP_TRAY_HEIGHT - 1);
    let (tray, rest) = carve_top_tray(area);
    assert!(tray.is_none());
    assert_eq!(rest, area);
}
