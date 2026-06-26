//! Hardcoded per-username badges rendered next to the bonsai glyph in chat
//! author labels. Small allowlist; edit and redeploy to change. Each user can
//! have multiple badges; keep arrays in canonical render order:
//! moderator, developer, artist.

const MODERATOR: &str = "🛡️";
const ARTIST: &str = "🎨";
const DEVELOPER: &str = "🔨️";

const SPECIAL_BADGES: &[(&str, &[&str])] = &[
    ("mevanlc", &[MODERATOR, DEVELOPER]),
    ("kirii.md", &[MODERATOR, ARTIST]),
    ("kirii.exe", &[MODERATOR, ARTIST]),
    ("ricott1", &[DEVELOPER]),
    ("odd", &[MODERATOR]),
    ("tasmania", &[MODERATOR, DEVELOPER]),
];

pub fn special_badges(username: &str) -> &'static [&'static str] {
    SPECIAL_BADGES
        .iter()
        .find_map(|(u, b)| u.eq_ignore_ascii_case(username).then_some(*b))
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::{ARTIST, DEVELOPER, MODERATOR, special_badges};
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn badges_use_emoji_width_glyphs() {
        for badge in [MODERATOR, ARTIST, DEVELOPER] {
            assert_eq!(UnicodeWidthStr::width(badge), 2);
        }
    }

    #[test]
    fn mevanlc_has_mod_and_developer() {
        assert_eq!(special_badges("mevanlc"), &[MODERATOR, DEVELOPER]);
    }

    #[test]
    fn kirii_variants_have_mod_and_artist() {
        assert_eq!(special_badges("kirii.md"), &[MODERATOR, ARTIST]);
        assert_eq!(special_badges("kirii.exe"), &[MODERATOR, ARTIST]);
    }

    #[test]
    fn ricott1_has_developer() {
        assert_eq!(special_badges("ricott1"), &[DEVELOPER]);
    }

    #[test]
    fn odd_has_mod_only() {
        assert_eq!(special_badges("odd"), &[MODERATOR]);
    }

    #[test]
    fn tasmania_has_mod_and_developer() {
        assert_eq!(special_badges("Tasmania"), &[MODERATOR, DEVELOPER]);
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
