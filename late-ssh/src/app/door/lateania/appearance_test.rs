use crate::app::door::lateania::appearance::*;

#[test]
fn portrait_renders_non_empty_and_varies_with_choices() {
    // Every class key yields a full, non-empty portrait.
    for key in [
        "warrior",
        "mage",
        "cleric",
        "rogue",
        "ranger",
        "druid",
        "necromancer",
        "bard",
        "monk",
        "paladin",
        "warlock",
        "berserker",
        "beastlord",
        "skald",
        "runemaster",
        "valewalker",
        "spiritmaster",
    ] {
        let rows = portrait(key, &[0; N_FIELDS]);
        assert_eq!(rows.len(), PORTRAIT_ROWS, "{key} bust height");
        assert!(rows.iter().all(|r| !r.is_empty()), "{key} rows non-empty");
    }
    // Different appearance choices produce a visibly different portrait.
    let plain = portrait("warrior", &[0, 0, 0, 0, 0, 0, 0]);
    let fancy = portrait("warrior", &[3, 2, 4, 7, 0, 0, 0]);
    assert_ne!(plain, fancy, "features change the portrait");
    // ...and a different class changes the headpiece too.
    let mage = portrait("mage", &[0, 0, 0, 0, 0, 0, 0]);
    assert_ne!(mage[0], plain[0], "class changes the head adornment");
}

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
