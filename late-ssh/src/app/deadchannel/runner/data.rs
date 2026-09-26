//! The runner's copy: what the wire says to a person the moment they
//! become one. Placeholder content at feed-template standards, design
//! review pending with the rest of the phase 2 copy (GAME.md). Every noun
//! passes the screenshot test (static, signal, city, runner, glyph, wire,
//! channel; never a Unix internal), lowercase in the voice's register.
//! The keys and the rules of the street live in the undercity guide
//! (`guide/data.rs`), not here: the welcome is the story and the way down.

/// The welcome the voice posts on the wire for a runner whose row was
/// just created (`RunnerOrigin::Created`, once per person by
/// construction): who is talking, where they are, the story so far, the
/// one key down to the city, and the way out. The keys on the street are
/// the undercity guide's (`guide/data.rs`), which opens by itself on the
/// first descent, so the welcome names none of them. The runner is
/// mentioned by name, so the message lands in their mentions too. One
/// message, one paragraph per line; the chat renderer keeps the breaks.
/// The join's conditional insert is the once-only claim, so this needs
/// none of its own.
pub(crate) fn welcome(username: &str) -> String {
    [
        format!("@{username}. you got through. welcome to the wire."),
        "this is #deadchannel: the back room under the clubhouse and the game's own log. \
         what happens to runners posts here as it happens, and between the lines it's just \
         us talking. half the messages are the game, half are people. that's the point."
            .to_string(),
        "the story so far. there's a city behind the screen. the rain down there falls as \
         static, the alleys are dead channels, and out in the dark are the glyphs: things \
         made of the same characters your terminal draws. at the bottom of it all something \
         old is still broadcasting. the static has been trying names for weeks. it tried \
         yours, and you answered. you're a runner now: the version of you that stays down \
         there when you log off."
            .to_string(),
        "getting down: 0 puts you in the clubhouse, 0 again takes you under it. the street \
         explains itself the first time you land on it, and ? down there says it all again."
            .to_string(),
        "the people upstairs can't see this channel or hear the static. if they ask what \
         you're on about, tell them to fill in their bio. if it gets too loud, /leave while \
         you're in here shuts the door. your runner keeps its face and waits, and \
         /join #deadchannel opens it again."
            .to_string(),
        "i'm afterglow. i'll be on the wire.".to_string(),
    ]
    .join("\n")
}
