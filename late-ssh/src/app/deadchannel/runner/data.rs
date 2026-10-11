//! The runner's copy: what the wire says to a person the moment they
//! become one. Placeholder content at feed-template standards, design
//! review pending with the rest of the phase 2 copy (GAME.md). Every noun
//! passes the screenshot test (static, signal, city, runner, glyph, wire,
//! channel; never a Unix internal), lowercase in the voice's register.
//! The keys and the rules of the street live in the undercity guide
//! (`guide/data.rs`), not here: the welcome is the story and the way down.

/// The welcome the voice posts on the wire for a runner whose row was
/// just created (`RunnerOrigin::Created`, once per person by
/// construction): the story so far, the one key down to the city, the way
/// out, and who is talking. Story first and short: a new runner reads the
/// wire, not a room description. The keys on the street are
/// the undercity guide's (`guide/data.rs`), which opens by itself on the
/// first descent, so the welcome names none of them. The runner is
/// mentioned by name, so the message lands in their mentions too. One
/// message, one paragraph per line; the chat renderer keeps the breaks.
/// The join's conditional insert is the once-only claim, so this needs
/// none of its own.
pub(crate) fn welcome(username: &str) -> String {
    [
        format!("@{username}. you got through. welcome to the wire."),
        "there's a city under the clubhouse. the rain down there falls as static, the alleys \
         are dead channels, and out in the dark are the glyphs: things made of the same \
         characters your terminal draws. at the bottom of it all something old is still \
         broadcasting. it's been trying names for weeks. it tried yours, and you answered. \
         you're a runner now: the version of you that stays down there when you log off."
            .to_string(),
        "0, then 0 again. the street explains the rest.".to_string(),
        "too loud? /leave in here shuts the door and your runner waits. /join #deadchannel \
         opens it again. i'm afterglow. i'll be on the wire."
            .to_string(),
    ]
    .join("\n")
}
