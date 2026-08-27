use super::event::{ActivityCategory, ActivityEvent, ActivityGame, ActivityKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityFilter {
    categories: &'static [ActivityCategory],
}

impl ActivityFilter {
    pub const fn dashboard() -> Self {
        Self {
            categories: &[
                ActivityCategory::Session,
                ActivityCategory::Game,
                ActivityCategory::Bonsai,
            ],
        }
    }

    pub fn includes(&self, event: &ActivityEvent) -> bool {
        self.categories.contains(&event.category())
    }
}

/// THE routing decision for #lounge system lines: invitations and stories in,
/// grind out. Every kind and every game is matched explicitly — when a new
/// event or game is added, the compiler drags you here to decide whether it
/// ships a story into the lounge.
pub fn lounge_includes(event: &ActivityEvent) -> bool {
    match &event.kind {
        // Presence story: someone showed up.
        ActivityKind::UserJoined => true,
        // One person's generosity, in front of the room. The whole point of
        // the mechanic is this line.
        ActivityKind::RoundBought { .. } => true,
        // Invitations: an open seat someone can still claim.
        ActivityKind::SatDown { .. } => true,
        // Door-game stories: entering a world, felling its bosses.
        ActivityKind::GameStarted { .. } | ActivityKind::BossSlain { .. } => true,
        ActivityKind::GameEvent { game, .. } => match game {
            // Door games: their moments are curated at the source
            // (start/descend/die/milestones), so they read as stories. DCSS,
            // NetHack, and Brogue events come from the log pipe (deaths,
            // artifact pickups), already freshness- and recency-gated at
            // ingestion.
            ActivityGame::Mud
            | ActivityGame::Nethack
            | ActivityGame::Dcss
            | ActivityGame::Brogue
            | ActivityGame::GreenDragon => true,
            // A Dark Room never posts in-game moments: the room's story is
            // its own log, and the only thing worth telling the lounge is the
            // ending, which arrives as a win.
            ActivityGame::Darkroom => false,
            ActivityGame::Asterion
            | ActivityGame::Blackjack
            | ActivityGame::Chess
            | ActivityGame::LeWord
            | ActivityGame::Minesweeper
            | ActivityGame::Nonogram
            | ActivityGame::Poker
            | ActivityGame::RubiksCube
            | ActivityGame::Sshattrick
            | ActivityGame::Ssnake
            | ActivityGame::Solitaire
            | ActivityGame::Sudoku
            | ActivityGame::TicTacToe
            | ActivityGame::Lateris
            | ActivityGame::TwentyFortyEight
            | ActivityGame::Tron
            | ActivityGame::Snake
            | ActivityGame::Traffic => false,
        },
        ActivityKind::GameWon { game, .. } => match game {
            // Human-vs-human matches are rare enough to be stories.
            ActivityGame::Asterion
            | ActivityGame::Chess
            | ActivityGame::Sshattrick
            | ActivityGame::Ssnake
            | ActivityGame::TicTacToe
            | ActivityGame::Tron => true,
            // Door-game wins are milestone-gated at the source (dragon
            // kills, NetHack amulet/ascension, a DCSS Orb escape, a Brogue
            // escape/mastery, A Dark Room's one ending) — stories.
            ActivityGame::GreenDragon
            | ActivityGame::Nethack
            | ActivityGame::Dcss
            | ActivityGame::Brogue
            | ActivityGame::Darkroom => true,
            // Lateania fires a win per mob kill; boss kills arrive as
            // `BossSlain` instead.
            ActivityGame::Mud => false,
            // Per-hand gambling wins are pure noise; the sit is the story.
            ActivityGame::Blackjack | ActivityGame::Poker => false,
            // Daily-puzzle solves: `GameWon` fires only in daily mode (never
            // for personal/practice runs), so these are once-per-day-per-board
            // finishes, not high-volume grind — a small "someone beat today's
            // puzzle" story worth sharing.
            ActivityGame::LeWord
            | ActivityGame::Minesweeper
            | ActivityGame::Nonogram
            | ActivityGame::RubiksCube
            | ActivityGame::Solitaire
            | ActivityGame::Sudoku => true,
            // Score-run games have no daily-win concept; their final scores
            // ride the hidden `GameScored` quest signal, not `GameWon`.
            ActivityGame::Lateris
            | ActivityGame::TwentyFortyEight
            | ActivityGame::Snake
            | ActivityGame::Traffic => false,
        },
        // Finished daily correspondence matches: one line per match (win/loss
        // or draw). Rare and human-vs-human, so a genuine story.
        ActivityKind::DailyResult { .. } => true,
        // A bought username effect: being seen is the whole product, so the
        // purchase is a story by design.
        ActivityKind::UsernameEffectApplied { .. } => true,
        // Same for the rest of the name-adjacent rentals: a badge and a title
        // are bought to be read next to the name in every message.
        ActivityKind::BadgeRented { .. } => true,
        ActivityKind::TitleApplied { .. } => true,
        // The dearest thing anyone buys, and it buys nothing but being seen
        // buying it. Once per rung per account forever, so it cannot repeat.
        ActivityKind::BurnMilestone { .. } => true,
        // A room paying for something someone said, three times over. Rare by
        // construction (once per message, at the threshold gild only) and the
        // line points at a room worth reading.
        ActivityKind::MessageGilded { .. } => true,
        // One slot, one holder, and every takeover names both players. Rare
        // by price (each take is 1.5x the last), so this is the story the
        // crown exists to ship.
        ActivityKind::CrownTaken { .. } => true,
        // The pot's one line: once a week when it draws. The size itself
        // rides the status HUD all week, so there is nothing to nudge about
        // before that.
        ActivityKind::PotDrawn { .. } => true,
        // Publishing on cyberspace: our user's own action, rare by their API
        // rate limits (15 entries/day), and the funnel that advertises the
        // integration ("wait, you can post to cyberspace from here?").
        ActivityKind::CyberspacePosted { .. } => true,
        // Going live is the archetypal invitation: the whole point of the
        // line is pulling people into the stream room.
        ActivityKind::WentLive { .. } => true,
        // The other half of the invitation: an audience gathering is the
        // signal that pulls the next person in. Once per viewer per stream,
        // and only in-app viewers exist here, so this cannot become a
        // per-heartbeat drip.
        ActivityKind::WatchingStream { .. } => true,
        // Quest-only grind signals, never surfaced anywhere public.
        ActivityKind::GameScored { .. } => false,
        // The bonsai is a private ritual: neither the daily watering nor the
        // death after N dry days belongs in the public feed.
        ActivityKind::BonsaiWatered => false,
        ActivityKind::BonsaiLost { .. } => false,
    }
}

/// The few events that are a story worth a real chat message, on top of the
/// one-row ticker line every `lounge_includes` survivor gets. The ticker is
/// glanceable and gone; a headline is a message from `system` that sits in
/// #lounge history like anything a person said, so it is reserved for the
/// rare, public, everyone-cares moments. The body carries no
/// `SYSTEM_LINE_PREFIX`, which is exactly what keeps it out of the ticker
/// diversion and in the message list. Same shape as `lounge_includes`: every
/// kind is matched, so a new event has to decide here whether it headlines.
pub fn lounge_headline(event: &ActivityEvent) -> Option<String> {
    use crate::app::common::primitives::thousands;
    use crate::app::common::username_effect::CROWN_GLYPH;
    match &event.kind {
        // One slot changed hands in public. Both names, what it cost, and
        // what unseating the new holder costs now: the whole contest in one
        // line, for the people who were not watching the ticker.
        ActivityKind::CrownTaken {
            price,
            next_price,
            from,
            ..
        } => {
            let taker = &event.username;
            let paid = thousands(*price);
            let next = thousands(*next_price);
            Some(match from {
                Some(from) => format!(
                    "{CROWN_GLYPH} {taker} stole the crown from {from} for {paid} chips. Next price: {next} chips."
                ),
                None => format!(
                    "{CROWN_GLYPH} {taker} claimed the vacant crown for {paid} chips. Next price: {next} chips."
                ),
            })
        }
        // Once a day, and the one person it matters most to is the one most
        // likely to have been offline for it. A ticker line is gone by the
        // time they reconnect; a headline is a row in #lounge history they
        // can still read.
        ActivityKind::PotDrawn {
            payout,
            winner_tickets,
            total_tickets,
            ..
        } => {
            let winner = &event.username;
            Some(format!(
                "\u{1F3B0} {winner} won the pot: {} chips on {} of {} tickets.",
                thousands(*payout),
                thousands(*winner_tickets),
                thousands(*total_tickets)
            ))
        }
        // No headline: @bartender already says it out loud in the room where
        // it was bought, and everyone it reached is online by definition, so a
        // #lounge row would be the third telling of one drink.
        ActivityKind::RoundBought { .. }
        | ActivityKind::UserJoined
        | ActivityKind::GameStarted { .. }
        | ActivityKind::GameWon { .. }
        | ActivityKind::GameScored { .. }
        | ActivityKind::GameEvent { .. }
        | ActivityKind::BossSlain { .. }
        | ActivityKind::SatDown { .. }
        | ActivityKind::DailyResult { .. }
        | ActivityKind::BonsaiWatered
        | ActivityKind::BonsaiLost { .. }
        | ActivityKind::UsernameEffectApplied { .. }
        | ActivityKind::BadgeRented { .. }
        | ActivityKind::TitleApplied { .. }
        | ActivityKind::BurnMilestone { .. }
        | ActivityKind::MessageGilded { .. }
        | ActivityKind::CyberspacePosted { .. }
        | ActivityKind::WentLive { .. }
        | ActivityKind::WatchingStream { .. } => None,
    }
}
