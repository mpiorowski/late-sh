//! Hardcoded per-username badges rendered next to the bonsai glyph in chat
//! author labels. Small allowlist; edit and redeploy to change. Each user can
//! have multiple badges; array order determines render order (first = closest
//! to the username).

const MODERATOR: &str = "\u{1F6E1}";
const ARTIST: &str = "\u{1F3A8}";
const WRENCH: &str = "\u{1F527}";

const SPECIAL_BADGES: &[(&str, &[&str])] = &[
    ("mevanlc", &[MODERATOR, WRENCH]),
    ("kirii.md", &[MODERATOR, ARTIST]),
    ("kirii.exe", &[MODERATOR, ARTIST]),
    ("wranglyph", &[MODERATOR]),
    ("tasmania", &[MODERATOR]),
];

pub fn special_badges(username: &str) -> &'static [&'static str] {
    SPECIAL_BADGES
        .iter()
        .find_map(|(u, b)| u.eq_ignore_ascii_case(username).then_some(*b))
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::{ARTIST, MODERATOR, WRENCH, special_badges};

    #[test]
    fn mevanlc_has_mod_and_wrench() {
        assert_eq!(special_badges("mevanlc"), &[MODERATOR, WRENCH]);
    }

    #[test]
    fn kirii_variants_have_mod_and_artist() {
        assert_eq!(special_badges("kirii.md"), &[MODERATOR, ARTIST]);
        assert_eq!(special_badges("kirii.exe"), &[MODERATOR, ARTIST]);
    }

    #[test]
    fn wranglyph_and_tasmania_have_mod_only() {
        assert_eq!(special_badges("wranglyph"), &[MODERATOR]);
        assert_eq!(special_badges("Tasmania"), &[MODERATOR]);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(special_badges("MEVANLC"), special_badges("mevanlc"));
    }

    #[test]
    fn mat_is_not_listed() {
        assert!(special_badges("mat").is_empty());
    }

    #[test]
    fn unknown_usernames_have_no_badges() {
        assert!(special_badges("randomuser").is_empty());
        assert!(special_badges("").is_empty());
    }
}
