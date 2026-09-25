use super::state::*;
use crate::app::common::worldmap::view::{BLANK_LAND_COLOR, WATER_COLOR};

/// Perceptual distance (CIELAB ΔE76), the same measure realm's palette test
/// uses: "these two look alike" as a number rather than an opinion.
fn delta_e(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    fn lab(c: (u8, u8, u8)) -> (f64, f64, f64) {
        fn lin(u: u8) -> f64 {
            let u = f64::from(u) / 255.0;
            if u <= 0.04045 {
                u / 12.92
            } else {
                ((u + 0.055) / 1.055).powf(2.4)
            }
        }
        let (r, g, b) = (lin(c.0), lin(c.1), lin(c.2));
        let x = (r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047;
        let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
        let z = (r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883;
        fn f(t: f64) -> f64 {
            if t > 0.008856 {
                t.cbrt()
            } else {
                7.787 * t + 16.0 / 116.0
            }
        }
        let (fx, fy, fz) = (f(x), f(y), f(z));
        (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
    }
    let (a, b) = (lab(a), lab(b));
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
}

/// A heat map that cannot be read is decoration. Every rung has to be
/// tellable from its neighbour, and none of them may be mistaken for the sea
/// or for a country nobody is in — which is the failure that matters, because
/// it turns "nobody here" into "somebody here".
#[test]
fn the_shading_ladder_is_readable() {
    for pair in HEAT.windows(2) {
        let step = delta_e(pair[0], pair[1]);
        assert!(
            step >= 8.0,
            "{:?} and {:?} are {step:.1} apart",
            pair[0],
            pair[1]
        );
    }
    for rung in HEAT {
        assert!(
            delta_e(rung, BLANK_LAND_COLOR) >= 15.0,
            "{rung:?} could be mistaken for a country nobody is in"
        );
        assert!(
            delta_e(rung, WATER_COLOR) >= 15.0,
            "{rung:?} could be mistaken for the sea"
        );
    }
    // Lightness climbs with the count, so "more people" reads without the
    // legend having to be consulted.
    let lightness = |c: (u8, u8, u8)| {
        0.2126 * f64::from(c.0) + 0.7152 * f64::from(c.1) + 0.0722 * f64::from(c.2)
    };
    for pair in HEAT.windows(2) {
        assert!(
            lightness(pair[1]) > lightness(pair[0]),
            "the ladder has to get brighter, not just different"
        );
    }
}

/// The profile country picker and the map speak the same alphabet — ISO-3166
/// alpha-2 — which is the whole reason this feature is a lookup rather than a
/// translation table. If that ever drifts, the map goes blank and nothing
/// else complains.
#[test]
fn profile_country_codes_find_their_countries_on_the_map() {
    for (code, expected) in [
        ("PL", "Poland"),
        ("RO", "Romania"),
        ("US", "United States of America"),
        ("JP", "Japan"),
        ("NZ", "New Zealand"),
    ] {
        let id = territory_for_code(code).unwrap_or_else(|| panic!("{code} should be on the map"));
        let map = crate::app::common::worldmap::data::map_by_id("earth").unwrap();
        assert_eq!(map.territory(id).unwrap().name, expected);
        assert_eq!(country_name(code), expected);
    }
    // Lowercase is what a sloppy setting would store; it still resolves.
    assert_eq!(territory_for_code("pl"), territory_for_code("PL"));

    // A code the map has no country for is not an error — it is a country
    // without a shape here, and it still gets a name for the list.
    assert_eq!(territory_for_code("ZZ"), None);
    assert_eq!(country_name("ZZ"), "ZZ");
    assert!(
        !country_name("AQ").is_empty(),
        "every code the picker offers has to have a name to show"
    );
}

/// The overwhelming majority of the picker's codes should exist on the map,
/// or the feature quietly under-reports where people are.
#[test]
fn nearly_every_country_the_picker_offers_is_on_the_map() {
    let offered = crate::app::settings_modal::data::filter_countries("");
    let missing: Vec<&str> = offered
        .iter()
        .filter(|c| territory_for_code(c.code).is_none())
        .map(|c| c.code)
        .collect();
    // Natural Earth folds a number of small dependencies into their parent,
    // so a handful will always be absent; a third of them would mean the two
    // lists had drifted apart.
    assert!(
        missing.len() < offered.len() / 4,
        "{} of {} picker countries are not on the map: {missing:?}",
        missing.len(),
        offered.len()
    );
}
