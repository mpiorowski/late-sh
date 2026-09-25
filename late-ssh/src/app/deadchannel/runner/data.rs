//! The runner's copy: what the wire says to a person the moment they
//! become one. Placeholder content at feed-template standards, design
//! review pending with the rest of the phase 2 copy (GAME.md). Every noun
//! passes the screenshot test (static, signal, city, runner, glyph, wire,
//! channel; never a Unix internal), lowercase in the voice's register.

/// The welcome the voice posts on the wire for a runner whose row was
/// just created (`RunnerOrigin::Created`, once per person by
/// construction): who is talking, where they are, the story so far, the
/// keys down to the city and back, the rules of the row, and the way out.
/// One message, one paragraph per line; the chat renderer keeps the
/// breaks. The join's conditional insert is the once-only claim, so this
/// needs none of its own.
pub(crate) fn welcome(username: &str) -> String {
    [
        format!("{username}. you got through. welcome to the wire."),
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
        "getting down: 0 puts you in the clubhouse, 0 again takes you under it. that's \
         static row. arrows or hjkl walk, shift with them runs, enter at a door goes in. the \
         armorer sells gear for bits. the tailor's mirror changes your face. the static at \
         the end of the row is where the glyphs come from: enter there spends a ration, a \
         attacks, r runs. 0 brings you back up, or the stairs by the wire."
            .to_string(),
        "the rules of the row: ten rations a day, and they roll at midnight utc. nothing \
         refills before then. signal at zero puts you off the row until the roll, and the \
         street keeps the bits you were carrying. bits stay down here; chips never buy \
         anything that hits. the people upstairs can't see this channel or hear the static. \
         if they ask what you're on about, tell them to fill in their bio."
            .to_string(),
        "if it gets too loud, /leave while you're in here shuts the door. your runner keeps \
         its face and waits, and /join #deadchannel opens it again."
            .to_string(),
        "i'm afterglow. i'll be on the wire.".to_string(),
    ]
    .join("\n")
}
