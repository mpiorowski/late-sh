use crate::app::door::nethack::milestone::*;

#[test]
fn detects_real_amulet_pickup() {
    assert!(has_amulet_pickup(
        "  The Amulet is bestowing a wish upon you!--More--"
    ));
    // The inventory pickup line is intentionally NOT a trigger (fakes match).
    assert!(!has_amulet_pickup("f - the Amulet of Yendor."));
    assert!(!has_amulet_pickup("You see here a spellbook."));
}

#[test]
fn detects_ascension_line_both_genders() {
    assert!(has_ascension_line("You ascend to the status of Demigod..."));
    assert!(has_ascension_line(
        "You ascend to the status of Demigoddess..."
    ));
    assert!(!has_ascension_line("You feel like a new man."));
}

#[test]
fn detects_ascension_prelude() {
    assert!(has_ascension_prelude(
        "An invisible choir sings, and you are bathed in radiance...--More--"
    ));
    assert!(!has_ascension_prelude("The door opens."));
}

#[test]
fn markers_must_lead_the_message_line_not_just_appear() {
    // Engraving read-back is prefixed, so it does not start the line.
    assert!(!has_amulet_pickup(
        "You read in the dust: The Amulet is bestowing a wish upon you!"
    ));
    assert!(!has_ascension_line(
        "You read in the dust: You ascend to the status of Demigod"
    ));
    // A named/called creature puts the text mid-sentence, not at the start.
    assert!(!has_ascension_line(
        "You see here a jackal called You ascend to the status of Demigod."
    ));
    // Only row 0 is trusted: a marker sitting in the map/menu body is ignored.
    assert!(!has_amulet_pickup(
        "Dlvl:3\nThe Amulet is bestowing a wish upon you!"
    ));
}

#[test]
fn detects_death_but_not_lifesave_quit_or_save() {
    // End-of-game signals match.
    assert!(has_death("Do you want to see what you had when you died?"));
    assert!(has_death(
        "                     /    REST    \\\n                   /     PEACE      \\"
    ));
    // The pre-life-saving announce alone is NOT treated as death, so an
    // amulet-of-life-saving survivor doesn't get a spurious death event.
    assert!(!has_death("You die...--More--"));
    assert!(!has_death(
        "You die...  But wait... your medallion begins to glow!"
    ));
    // Quit and save are not deaths.
    assert!(!has_death("Do you want to see what you had when you quit?"));
    assert!(!has_death("Be seeing you..."));
    assert!(!has_death("You ascend to the status of Demigoddess..."));
}
