//! Pure parsers for NetHack's bottom status line, scraped from the vt100 screen.
//!
//! NetHack renders the dungeon depth as `Dlvl:N` (botl.c: `Sprintf(buf,
//! "%s:%-2d", "Dlvl", depth(...))`), shrinking to `Dl:N` only on very narrow
//! terminals (wintty `shrink_dlvl`). The value is the *absolute* depth, so it
//! stays meaningful inside branches like the Gnomish Mines. We read it as a
//! plain value (not an event) and let `state.rs` track the deepest seen — a
//! state comparison, which avoids the "can't count repeated events" problem that
//! plagues message-line scraping.

/// Parse the current dungeon depth (`Dlvl:N` / `Dl:N`) from the rendered screen.
/// Returns `None` when the field is absent or non-numeric (e.g. some special
/// branches that print a name instead of a number).
pub fn parse_dlvl(screen_text: &str) -> Option<i32> {
    // Check the long form first: "Dlvl:" does not contain "Dl:" as a substring,
    // but searching the short form first could match a truncated render oddly.
    for prefix in ["Dlvl:", "Dl:"] {
        if let Some(idx) = screen_text.find(prefix) {
            let rest = &screen_text[idx + prefix.len()..];
            let digits: String = rest
                .trim_start()
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(n) = digits.parse::<i32>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
