//! The stores/pack sidebar is the one place where authored label text meets a
//! fixed column width, so it is the one place a new resource or building can
//! silently collide with its own count or lose characters off the right edge.

use super::data::{Building, Resource};
use super::ui::{SIDEBAR_LABEL_PAD, SIDEBAR_LABELS, SIDEBAR_WIDTH, sidebar_label};

/// Every sidebar row is `"  " + {label:<PAD} + value`. A label as long as the
/// pad leaves no gutter and renders as "trading post1".
#[test]
fn every_label_leaves_a_gutter_before_its_value() {
    let collides: Vec<&str> = Resource::ALL
        .iter()
        .map(|resource| sidebar_label(*resource))
        .chain(Building::ALL.iter().map(|building| building.label()))
        // The traps row splits into a synthetic label upstream doesn't have.
        .chain(std::iter::once("baited trap"))
        .filter(|label| label.chars().count() >= SIDEBAR_LABEL_PAD)
        .collect();

    assert!(
        collides.is_empty(),
        "labels run into their value column: {collides:?}"
    );
}

/// The widest row a long run produces is a five-figure store with an income
/// suffix ("10000 +100/5s"). If it does not fit, the trailing "s" is clipped.
#[test]
fn the_widest_stores_row_fits_the_sidebar() {
    let widest = 2 + SIDEBAR_LABEL_PAD + "10000 +100/5s".chars().count();

    assert!(
        widest <= SIDEBAR_WIDTH as usize,
        "widest stores row is {widest} cols but the sidebar is {SIDEBAR_WIDTH}"
    );
}

/// The abbreviation list is an exception list, so an entry that is no shorter
/// than the name it replaces is dead weight pretending to be a fix.
#[test]
fn every_sidebar_abbreviation_is_shorter_than_the_name_it_replaces() {
    let useless: Vec<&str> = SIDEBAR_LABELS
        .iter()
        .filter(|(resource, short)| short.chars().count() >= resource.label().chars().count())
        .map(|(_, short)| *short)
        .collect();

    assert!(
        useless.is_empty(),
        "these abbreviations buy nothing: {useless:?}"
    );
}
