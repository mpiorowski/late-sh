use crate::app::worldcup::flags::*;

#[test]
fn known_names_map_to_flags() {
    assert_eq!(flag_emoji("Mexico"), "🇲🇽");
    assert_eq!(flag_emoji("South Korea"), "🇰🇷");
    assert_eq!(flag_emoji("Ivory Coast"), "🇨🇮");
}

#[test]
fn handles_whitespace_and_aliases() {
    assert_eq!(flag_emoji("  Germany  "), "🇩🇪");
    assert_eq!(flag_emoji("Czech Republic"), "🇨🇿");
    assert_eq!(flag_emoji("United States"), "🇺🇸");
}

#[test]
fn unknown_and_placeholders_return_empty() {
    assert_eq!(flag_emoji("Winner SF 1"), "");
    assert_eq!(flag_emoji("Atlantis"), "");
}
