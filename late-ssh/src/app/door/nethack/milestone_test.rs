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
fn pet_name_led_plines_do_not_spoof_markers() {
    // A pet named after a marker (rc DOGNAME=, or the in-game C-call command)
    // leads its own plines, landing the marker at the start of row 0 with the
    // rest of the sentence after it. Real marker plines end the line, end with
    // the terminal --More--, or concatenate the next message after TWO spaces;
    // pet plines continue after a single space, and name munging collapses
    // double spaces, so these must all be rejected.
    assert!(!has_ascension_line(
        "You ascend to the status of Demigod bites the newt!"
    ));
    assert!(!has_ascension_line(
        "You ascend to the status of Demigod... bites the newt!"
    ));
    assert!(!has_ascension_prelude(
        "An invisible choir sings, and you are bathed in radiance... bites the newt!"
    ));
    assert!(!has_amulet_pickup(
        "The Amulet is bestowing a wish upon you! misses the newt."
    ));
    // A name embedding a fake --More-- cannot fake the terminal prompt: the
    // real one ends the line.
    assert!(!has_ascension_line(
        "You ascend to the status of Demigod...--More-- bites the newt!"
    ));
    // The real forms still match: bare, --More--, and two-space concatenation
    // with the next queued message.
    assert!(has_ascension_line("You ascend to the status of Demigod..."));
    assert!(has_ascension_line(
        "You ascend to the status of Demigoddess...--More--"
    ));
    assert!(has_ascension_line(
        "You ascend to the status of Demigod...  Do you want your possessions identified?"
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
