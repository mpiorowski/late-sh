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
    ("odd", &[MODERATOR, DEVELOPER]),
    ("tasmania", &[MODERATOR, DEVELOPER]),
];

pub(crate) fn special_badges(username: &str) -> &'static [&'static str] {
    SPECIAL_BADGES
        .iter()
        .find_map(|(u, b)| u.eq_ignore_ascii_case(username).then_some(*b))
        .unwrap_or(&[])
}

#[cfg(test)]
#[path = "special_badges_test.rs"]
mod special_badges_test;
