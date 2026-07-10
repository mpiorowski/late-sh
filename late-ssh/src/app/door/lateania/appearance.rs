// Character appearance & bio for Lateania.
//
// The terminal door has no free-text prompt, so a player customises how their
// character looks and reads by choosing among preset options for a handful of
// traits. The choices compose into a short bio sentence shown on the character
// sheet and to anyone who profiles them. Selections are stored as one small
// index per field (`[u8; N_FIELDS]`) and persisted.

/// The customisable traits, each with its menu of options. Order is stable
/// (persisted by index) - append options, never reorder.
pub const FIELDS: &[(&str, &[&str])] = &[
    (
        "Build",
        &[
            "lean",
            "broad-shouldered",
            "wiry",
            "towering",
            "compact",
            "willowy",
            "heavyset",
            "unremarkable",
            "rangy",
            "barrel-chested",
            "slight",
            "iron-hard",
        ],
    ),
    (
        "Hair",
        &[
            "close-cropped",
            "long and braided",
            "wild and unkempt",
            "silver-streaked",
            "shaven-headed",
            "raven-dark",
            "fire-red",
            "sun-bleached",
            "ash-blond",
            "salt-and-pepper",
            "tightly curled",
            "topknotted",
        ],
    ),
    (
        "Eyes",
        &[
            "keen grey",
            "warm brown",
            "pale blue",
            "amber",
            "scarred and one-eyed",
            "mismatched",
            "storm-dark",
            "glass-green",
            "coal-black",
            "hazel",
            "ice-pale",
            "gold-flecked",
        ],
    ),
    (
        "Bearing",
        &[
            "watchful",
            "easy and grinning",
            "grim",
            "restless",
            "courtly",
            "haunted",
            "bold",
            "quiet",
            "weary",
            "sly",
            "gentle",
            "dangerous",
        ],
    ),
    (
        "Origin",
        &[
            "of Embergate",
            "from the harbour-towns",
            "born in the highlands",
            "a child of the desert",
            "out of the Frontier",
            "from far over the sea",
            "of no fixed home",
            "raised in the Sundered Reaches",
            "of the fishing villages",
            "born on the road",
            "from a drowned island",
            "of forgotten stock",
        ],
    ),
    (
        "Mark",
        &[
            "unmarked",
            "a long jaw-scar",
            "a burned hand",
            "a faded tattoo",
            "a broken nose",
            "a missing finger",
            "a braided beard",
            "a raider's brand",
            "a lucky charm at the throat",
            "a limp from an old wound",
            "war-paint half worn off",
            "a clan-mark on the cheek",
        ],
    ),
    (
        "Manner",
        &[
            "says little",
            "laughs too loud",
            "hums old tunes",
            "never sits still",
            "weighs every word",
            "quick to anger",
            "unfailingly polite",
            "always joking",
            "watches the doors",
            "speaks in proverbs",
            "cool under fire",
            "kind to strangers",
        ],
    ),
];

/// Number of customisable fields.
pub const N_FIELDS: usize = FIELDS.len();

/// The label of field `i`.
pub fn field_label(i: usize) -> &'static str {
    FIELDS[i].0
}

/// How many options field `i` offers.
pub fn option_count(i: usize) -> usize {
    FIELDS[i].1.len()
}

/// The chosen option text for field `i` at index `idx` (clamped).
pub fn option(i: usize, idx: u8) -> &'static str {
    let opts = FIELDS[i].1;
    opts[(idx as usize).min(opts.len() - 1)]
}

/// Compose the bio sentence from a full set of selections.
pub fn compose_bio(sel: &[u8; N_FIELDS]) -> String {
    let mark = option(5, sel[5]);
    // "unmarked" (index 0) reads oddly in a sentence; drop that clause.
    let mark_clause = if sel[5] == 0 {
        String::new()
    } else {
        format!(" bearing {mark},")
    };
    format!(
        "A {build}, {origin} adventurer, {hair} of hair and {eyes} of eye,{mark} of a {bearing} bearing who {manner}.",
        build = option(0, sel[0]),
        hair = option(1, sel[1]),
        eyes = option(2, sel[2]),
        bearing = option(3, sel[3]),
        origin = option(4, sel[4]),
        mark = mark_clause,
        manner = option(6, sel[6]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_has_options_and_composes() {
        assert_eq!(N_FIELDS, 7);
        for (i, field) in FIELDS.iter().enumerate() {
            assert!(option_count(i) >= 2, "{} has choices", field_label(i));
            // Out-of-range indices clamp rather than panic.
            assert_eq!(option(i, 250), field.1[field.1.len() - 1]);
        }
        let bio = compose_bio(&[1, 2, 3, 4, 5, 2, 3]);
        assert!(bio.contains("broad-shouldered") && bio.contains("from far over the sea"));
        assert!(bio.contains("a burned hand") && bio.contains("never sits still"));
        // An "unmarked" character drops the mark clause cleanly.
        let plain = compose_bio(&[0, 0, 0, 0, 0, 0, 0]);
        assert!(!plain.contains("unmarked"), "no dangling 'unmarked' clause");
        assert!(plain.contains("says little"), "manner still reads");
    }
}
