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

// ---- Composed portrait ----------------------------------------------------
//
// A little ASCII bust assembled from the player's own appearance choices plus
// their class, so every character looks distinct and made by them. This produces
// the *plain* rows (glyphs only); `ui.rs::portrait_lines` tints them with the
// class accent and per-feature colours from the theme palette. Kept free of any
// rendering/theme dependency so it stays pure and testable.

/// Field indices into `FIELDS`, named for readability.
const F_BUILD: usize = 0;
const F_HAIR: usize = 1;
const F_EYES: usize = 2;
const F_BEARING: usize = 3;

/// Number of rows in an assembled portrait bust. Every row is non-empty; the
/// class accent and per-feature colours are applied by the renderer.
pub const PORTRAIT_ROWS: usize = 7;

/// A class-flavoured headpiece (top adornment) for the portrait, keyed by the
/// stable class key. Warpaint/helm/hood/circlet give the bust its calling.
fn head_adornment(class_key: &str) -> &'static str {
    match class_key {
        // Iron helm for the plate-wearers.
        "warrior" | "paladin" | "berserker" | "valewalker" => "▟█████▙",
        // Pointed hood/cowl for the shadow callings.
        "rogue" | "necromancer" | "warlock" | "spiritmaster" => "╱▔▔▔▔▔╲",
        // Circlet/wizard's brim for the casters.
        "mage" | "cleric" | "runemaster" => "◇═════◇",
        // Feathered/wild band for the wilds-folk.
        "ranger" | "druid" | "beastlord" => "≈≈≈≈≈≈≈",
        // Singer's laurel.
        "bard" | "skald" => "❀─────❀",
        // Ascetic's bare brow.
        _ => "───────",
    }
}

/// The hair fringe glyphs (just under the adornment), chosen by the Hair field.
/// Length/style reads from the option index.
fn hair_fringe(idx: u8) -> &'static str {
    // 12 hair options; map each to a distinct fringe texture.
    match idx {
        0 => "‚‚‚‚‚‚‚",  // close-cropped
        1 => "≀≀≀≀≀≀≀",  // long and braided
        2 => "ϟϟϟϟϟϟϟ",  // wild and unkempt
        3 => "╌╌╌╌╌╌╌",  // silver-streaked
        4 => "       ",  // shaven-headed (bare)
        5 => "▚▚▚▚▚▚▚",  // raven-dark
        6 => "^^^^^^^",  // fire-red
        7 => "″″″″″″″",  // sun-bleached
        8 => "'''''''",  // ash-blond
        9 => "╍╍╍╍╍╍╍",  // salt-and-pepper
        10 => "ςςςςςςς", // tightly curled
        _ => "⌐‾‾‾‾‾¬",  // topknotted
    }
}

/// The eye glyph pair from the Eyes field.
fn eye_glyphs(idx: u8) -> (&'static str, &'static str) {
    match idx {
        4 => ("x", "◉"), // scarred and one-eyed
        5 => ("◉", "○"), // mismatched
        _ => ("◉", "◉"),
    }
}

/// The mouth/expression glyph from the Bearing field.
fn mouth_glyph(idx: u8) -> &'static str {
    match idx {
        1 | 7 => "◡",  // easy and grinning / bold (a grin)
        2 | 5 => "▔",  // grim / haunted (a hard line)
        9 | 11 => "‿", // sly / dangerous (a slight smirk)
        3 => "~",      // restless
        _ => "─",      // neutral set
    }
}

/// The cheek/frame side glyphs from the Build field: a wider frame reads broader.
fn frame_sides(idx: u8) -> (char, char) {
    // Broad/heavy builds get a heavier jaw frame; slight ones a lighter one.
    match idx {
        1 | 3 | 6 | 9 | 11 => ('█', '█'), // broad/towering/heavyset/barrel/iron-hard
        5 | 10 => ('▏', '▕'),             // willowy / slight
        _ => ('▌', '▐'),                  // the middling builds
    }
}

/// Compose the plain portrait rows (glyphs only) from a class key and the
/// appearance selections. Always returns `PORTRAIT_ROWS` non-empty bust rows.
/// Colour (class accent + feature tints) is layered on by the renderer.
pub fn portrait(class_key: &str, sel: &[u8; N_FIELDS]) -> Vec<String> {
    let adorn = head_adornment(class_key);
    let hair = hair_fringe(sel[F_HAIR]);
    let (le, re) = eye_glyphs(sel[F_EYES]);
    let mouth = mouth_glyph(sel[F_BEARING]);
    let (ls, rs) = frame_sides(sel[F_BUILD]);
    vec![
        format!("  {adorn}  "),
        format!("  {hair}  "),
        format!(" {ls}▁▁▁▁▁{rs} "),
        format!(" {ls} {le} {re} {rs} "),
        format!(" {ls}  ‸  {rs} "),
        format!(" {ls} {mouth}{mouth}{mouth} {rs} "),
        format!("  ╲▁▁▁▁▁╱  "),
    ]
}


