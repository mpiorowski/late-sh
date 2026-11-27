//! The radio nudge where the hand already is: a plain-ssh session that
//! reaches for the music (the Stations modal, the Music Booth, the music
//! keys) is told right there that nothing plays over plain ssh and where
//! it does. A session counts as plain ssh while no client is paired on its
//! token (`App::paired_client_state`); the `late` CLI pairs at startup.

/// Where the hand was. `pub` because `metrics` counts the nudge by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadioNudgeSpot {
    /// The Stations modal opened.
    Stations,
    /// The Music Booth opened.
    Booth,
    /// A station slot or the source toggle pressed outside the modals.
    Keys,
}

/// The line. `listen_url` is the public listen page; its scheme is dropped
/// so the line fits a modal row and a banner.
pub(crate) fn line(listen_url: &str) -> String {
    let listen = listen_url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    format!("Plain ssh plays nothing: install late (? Pair) or open {listen}")
}

#[cfg(test)]
#[path = "nudge_test.rs"]
mod nudge_test;
