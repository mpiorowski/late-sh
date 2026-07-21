use crate::app::door::nethack::status::*;

#[test]
fn parses_standard_status_line() {
    let status = "Dlvl:5  $:120  HP:18(18) Pw:2(2) AC:6  Xp:3/24  T:412";
    assert_eq!(parse_dlvl(status), Some(5));
}

#[test]
fn parses_padded_and_shrunk_forms() {
    // botl.c pads with %-2d, so a one-digit level has a trailing space.
    assert_eq!(parse_dlvl("Dlvl:1  HP:12(12)"), Some(1));
    // Narrow-terminal shrink form.
    assert_eq!(parse_dlvl("Dl:23 HP:5(40)"), Some(23));
    // Deep level, no trailing pad space.
    assert_eq!(parse_dlvl("Dlvl:42 AC:-3"), Some(42));
}

#[test]
fn returns_none_without_a_numeric_dlvl() {
    assert_eq!(parse_dlvl("no status here"), None);
    // Tutorial / named branches print a non-numeric field.
    assert_eq!(parse_dlvl("Tutorial:start"), None);
}
