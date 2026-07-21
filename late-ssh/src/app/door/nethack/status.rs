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
