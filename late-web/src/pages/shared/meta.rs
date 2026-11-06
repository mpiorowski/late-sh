//! The site's own preview-card picture, the Rust half of `meta.html`: the
//! one place its path and alt text live. The shell's default card reads
//! these constants, and a page with a card of its own falls back to
//! [`site_image`] when it has no picture to show.

/// Where the site picture is served, under the public base URL.
pub(crate) const SITE_IMAGE_PATH: &str = "/static/og-image.png";

/// What the site picture shows, for readers that cannot see it.
pub(crate) const SITE_IMAGE_ALT: &str =
    "A steaming coffee cup drawn in braille dots beside the prompt: ssh late.sh";

/// The site picture's URL under `base`, and its alt text.
pub(crate) fn site_image(base: &str) -> (String, String) {
    (
        format!("{base}{SITE_IMAGE_PATH}"),
        SITE_IMAGE_ALT.to_string(),
    )
}
