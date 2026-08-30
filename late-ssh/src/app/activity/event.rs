use std::time::Instant;

use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

use crate::metrics;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityCategory {
    Session,
    Game,
    Bonsai,
    Quest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActivityKind {
    UserJoined,
    GameWon {
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
    },
    GameScored {
        game: ActivityGame,
        score: i32,
        level: Option<i32>,
    },
    /// A notable in-game moment that is neither a win nor a score: started a
    /// session, descended a level, died. `detail` is the full action phrase.
    /// Shown in the dashboard feed (category `Game`).
    GameEvent {
        game: ActivityGame,
        detail: String,
    },
    /// A player entered a game world (door games): the "come join me"
    /// invitation shown in #lounge.
    GameStarted {
        game: ActivityGame,
    },
    /// A boss or sub-boss died to this player. `boss` is the full mob name
    /// as the game renders it (e.g. "the Archdemon Mal'gareth").
    BossSlain {
        game: ActivityGame,
        boss: String,
    },
    /// A player took a seat at a multiplayer table. Fired on sitting, not on
    /// playing, so open seats become visible in #lounge.
    SatDown {
        game: ActivityGame,
    },
    /// A finished daily correspondence match. `action` carries the full
    /// match-level phrase ("won a game of Chess" / "drew with bob at Connect
    /// Four"); `game` and `match_id` exist only for #lounge repeat-throttling:
    /// keying on the match lets one player finish two same-game matches back
    /// to back (one line per match) while a re-emit of the same match dedupes.
    /// Fired only on a finish (win/loss or draw), never on posting or claiming.
    DailyResult {
        game: String,
        match_id: Uuid,
    },
    /// A bought username effect went live ("mat is glowing (24h)"). Shown in
    /// #lounge: the whole point of the purchase is being seen.
    UsernameEffectApplied {
        effect: late_core::models::username_effect::UsernameEffect,
    },
    /// A rented chat badge or flag went live ("mat rented 🐱 (24h)"). Same
    /// reasoning as the username effect: it is bought to be seen next to the
    /// name in every message.
    BadgeRented {
        emoji: String,
    },
    /// A rented title went live ("mat is now the insufferable (30d)"). The
    /// most be-seen thing the Shop sells, so it announces.
    TitleApplied {
        title: String,
    },
    /// Someone burned a six-figure sum for a permanent glyph ("mira burned
    /// 150,000 chips for the Fuse"). The badge is the receipt, so the line
    /// names the price: everyone watching is the product.
    BurnMilestone {
        name: String,
        price: i64,
    },
    /// A chat message reached the gild threshold. Names the author only: the
    /// buyers stay out of it, because the story is that a room paid for
    /// something someone said, not who has chips. `message_id` keys the
    /// #lounge repeat throttle, the way `DailyResult` keys on its match, so
    /// two of one author's messages crossing the line in the same half hour
    /// both post.
    MessageGilded {
        message_id: Uuid,
        count: i64,
        room_slug: Option<String>,
    },
    /// Someone took the crown. Both players are named because that is the
    /// whole story: a takeover is one person outbidding another in public.
    /// `reign_id` keys the #lounge repeat throttle, so back-to-back
    /// takeovers each post (the 1.5x price ladder is the real throttle).
    CrownTaken {
        reign_id: Uuid,
        price: i64,
        /// What the next take costs, so the #lounge headline can quote it
        /// without every reader re-deriving the ladder.
        next_price: i64,
        /// The deposed holder, absent when the crown was vacant.
        from: Option<String>,
    },
    /// Someone bought the house a round. The buyer is the whole story, so the
    /// line names them and the size of their gesture, never the patrons who
    /// got a drink out of it. `round_id` keys the #lounge repeat throttle;
    /// a second round minutes later reaches almost nobody and refuses, so
    /// there is nothing to collapse.
    RoundBought {
        round_id: Uuid,
        patrons: i64,
        total_chips: i64,
    },
    /// The weekly pot drew. Names the winner and the odds they beat, because
    /// the odds are the story: a three-ticket win off three hundred reads
    /// very differently from a fifty-ticket one. `pot_id` keys the #lounge
    /// repeat throttle; there is one of these a week anyway.
    PotDrawn {
        pot_id: Uuid,
        payout: i64,
        winner_tickets: i64,
        total_tickets: i64,
    },
    /// A linked user published an entry on cyberspace.online from late.sh.
    /// Announces our user's own action, never cyberspace content.
    CyberspacePosted {
        title: Option<String>,
    },
    /// A streamer's go-live page reported media flowing: their "watch me"
    /// stream room is on. Fired on the pending -> live transition only,
    /// never at `/golive` command time, so no line ever points at a black
    /// screen. There is no matching "stream ended" event (noise).
    WentLive {
        title: Option<String>,
    },
    /// A named late.sh user arrived at someone's live stream, through
    /// `/watch @user` or by opening the stream room. `streamer` is the
    /// broadcaster's username (the event itself is attributed to the
    /// viewer). Fires once per viewer per stream; the anonymous watch-page
    /// audience behind the "N watching" count has no identity and is never
    /// named.
    WatchingStream {
        streamer: String,
    },
    BonsaiWatered,
    BonsaiLost {
        survived_days: i32,
    },
}

impl ActivityKind {
    pub fn category(&self) -> ActivityCategory {
        match self {
            Self::UserJoined
            | Self::UsernameEffectApplied { .. }
            | Self::BadgeRented { .. }
            | Self::TitleApplied { .. }
            | Self::BurnMilestone { .. }
            | Self::MessageGilded { .. }
            | Self::CrownTaken { .. }
            | Self::RoundBought { .. }
            | Self::PotDrawn { .. }
            | Self::CyberspacePosted { .. }
            | Self::WentLive { .. }
            | Self::WatchingStream { .. } => ActivityCategory::Session,
            Self::GameWon { .. }
            | Self::GameEvent { .. }
            | Self::GameStarted { .. }
            | Self::BossSlain { .. }
            | Self::SatDown { .. }
            | Self::DailyResult { .. } => ActivityCategory::Game,
            Self::GameScored { .. } => ActivityCategory::Quest,
            Self::BonsaiWatered | Self::BonsaiLost { .. } => ActivityCategory::Bonsai,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityGame {
    Asterion,
    Blackjack,
    Brogue,
    Chess,
    Darkroom,
    Dcss,
    GreenDragon,
    LeWord,
    Minesweeper,
    Mud,
    Nethack,
    Nonogram,
    Poker,
    RubiksCube,
    SlidingPuzzle,
    Sshattrick,
    Ssnake,
    Solitaire,
    Sudoku,
    TicTacToe,
    Lateris,
    TwentyFortyEight,
    Tron,
    Snake,
    Traffic,
}

impl ActivityGame {
    pub fn key(self) -> &'static str {
        match self {
            Self::Asterion => "asterion",
            Self::Blackjack => "blackjack",
            Self::Brogue => "brogue",
            Self::Chess => "chess",
            Self::Darkroom => "darkroom",
            Self::Dcss => "dcss",
            Self::GreenDragon => "greendragon",
            Self::LeWord => "le_word",
            Self::Minesweeper => "minesweeper",
            Self::Mud => "mud",
            Self::Nethack => "nethack",
            Self::Nonogram => "nonogram",
            Self::Poker => "poker",
            Self::RubiksCube => "rubiks_cube",
            Self::SlidingPuzzle => "sliding_puzzle",
            Self::Sshattrick => "sshattrick",
            Self::Ssnake => "ssnake",
            Self::Solitaire => "solitaire",
            Self::Sudoku => "sudoku",
            Self::TicTacToe => "tictactoe",
            Self::Lateris => "tetris",
            Self::TwentyFortyEight => "2048",
            Self::Tron => "tron",
            Self::Snake => "snake",
            Self::Traffic => "traffic",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Asterion => "Asterion",
            Self::Blackjack => "Blackjack",
            Self::Brogue => "Brogue",
            Self::Chess => "Chess",
            Self::Darkroom => "A Dark Room",
            Self::Dcss => "DCSS",
            Self::GreenDragon => "Green Dragon",
            Self::LeWord => "Le Word",
            Self::Minesweeper => "Minesweeper",
            Self::Mud => "Lateania",
            Self::Nethack => "NetHack",
            Self::Nonogram => "Nonogram",
            Self::Poker => "Poker",
            Self::RubiksCube => "Rubik's Cube",
            Self::SlidingPuzzle => "Sliding Puzzle",
            Self::Sshattrick => "ssHattrick",
            Self::Ssnake => "Super Snake",
            Self::Solitaire => "Solitaire",
            Self::Sudoku => "Sudoku",
            Self::TicTacToe => "Tic-Tac-Toe",
            Self::Lateris => "Lateris",
            Self::TwentyFortyEight => "2048",
            Self::Tron => "Tron",
            Self::Snake => "Snake",
            Self::Traffic => "Traffic",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActivityEvent {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub username: String,
    pub action: String,
    pub kind: ActivityKind,
    pub at: Instant,
    pub occurred_at: DateTime<Utc>,
}

impl ActivityEvent {
    pub fn occurred_on_utc_date(date: NaiveDate) -> DateTime<Utc> {
        date.and_hms_opt(12, 0, 0)
            .expect("noon is a valid time")
            .and_utc()
    }

    pub fn joined(user_id: Uuid, username: impl Into<String>) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::UserJoined,
            "joined".to_string(),
        )
    }

    pub fn game_won(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
    ) -> Self {
        Self::game_won_at(user_id, username, game, detail, score, Utc::now())
    }

    pub fn game_won_at(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        detail: Option<String>,
        score: Option<i32>,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        metrics::record_game_win(game);
        let base_action = match game {
            ActivityGame::Asterion => "escaped the Asterion maze",
            ActivityGame::Blackjack => "won Blackjack hand",
            ActivityGame::Brogue => "conquered Brogue",
            ActivityGame::Chess => "won Chess game",
            ActivityGame::Darkroom => "flew out of A Dark Room",
            ActivityGame::Dcss => "escaped DCSS with the Orb of Zot",
            ActivityGame::GreenDragon => "prevailed in the Green Dragon",
            ActivityGame::LeWord => "solved Le Word",
            ActivityGame::Minesweeper => "cleared Minesweeper",
            ActivityGame::Mud => "triumphed in Lateania",
            ActivityGame::Nethack => "conquered NetHack",
            ActivityGame::Nonogram => "solved Nonogram",
            ActivityGame::Poker => "won Poker hand",
            ActivityGame::RubiksCube => "solved Rubik's Cube",
            ActivityGame::SlidingPuzzle => "solved Sliding Puzzle",
            ActivityGame::Sshattrick => "won ssHattrick match",
            ActivityGame::Ssnake => "won Super Snake match",
            ActivityGame::Solitaire => "won Solitaire",
            ActivityGame::Sudoku => "solved Sudoku",
            ActivityGame::TicTacToe => "won Tic-Tac-Toe",
            ActivityGame::Lateris => "won Lateris",
            ActivityGame::TwentyFortyEight => "won 2048",
            ActivityGame::Tron => "won Tron round",
            ActivityGame::Snake => "won Snake",
            ActivityGame::Traffic => "finished a Traffic track",
        };
        let action = match detail.as_deref() {
            Some(detail) if !detail.is_empty() => format!("{base_action} ({detail})"),
            _ => base_action.to_string(),
        };
        Self::new_at(
            Some(user_id),
            username,
            ActivityKind::GameWon {
                game,
                detail,
                score,
            },
            action,
            occurred_at,
        )
    }

    /// A notable in-game moment (start/descend/death). `action` is the full verb
    /// phrase shown in the feed, e.g. "descended to NetHack dungeon level 5".
    pub fn game_event(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        action: String,
    ) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::GameEvent {
                game,
                detail: action.clone(),
            },
            action,
        )
    }

    /// A player entered a game world. Copy lives here, not at call sites.
    pub fn game_started(user_id: Uuid, username: impl Into<String>, game: ActivityGame) -> Self {
        let action = match game {
            ActivityGame::Mud => "set out into Lateania".to_string(),
            ActivityGame::Nethack => "descended into NetHack".to_string(),
            ActivityGame::Dcss => "delved into the Dungeon Crawl".to_string(),
            ActivityGame::Brogue => "descended into Brogue".to_string(),
            ActivityGame::GreenDragon => "walked into the Green Dragon".to_string(),
            ActivityGame::Darkroom => "woke up in A Dark Room".to_string(),
            ActivityGame::Asterion
            | ActivityGame::Blackjack
            | ActivityGame::Chess
            | ActivityGame::LeWord
            | ActivityGame::Minesweeper
            | ActivityGame::Nonogram
            | ActivityGame::Poker
            | ActivityGame::RubiksCube
            | ActivityGame::SlidingPuzzle
            | ActivityGame::Sshattrick
            | ActivityGame::Ssnake
            | ActivityGame::Solitaire
            | ActivityGame::Sudoku
            | ActivityGame::TicTacToe
            | ActivityGame::Lateris
            | ActivityGame::TwentyFortyEight
            | ActivityGame::Tron
            | ActivityGame::Snake
            | ActivityGame::Traffic => format!("started {}", game.label()),
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::GameStarted { game },
            action,
        )
    }

    /// A boss or sub-boss fell. `boss` is the mob name as the game renders it.
    pub fn boss_slain(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        boss: impl Into<String>,
    ) -> Self {
        let boss = boss.into();
        let action = format!("slew {} in {}", boss, game.label());
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BossSlain { game, boss },
            action,
        )
    }

    /// A player took a seat at a multiplayer table.
    pub fn sat_down(user_id: Uuid, username: impl Into<String>, game: ActivityGame) -> Self {
        let action = format!("sat down at {}", game.label());
        Self::new(
            Some(user_id),
            username,
            ActivityKind::SatDown { game },
            action,
        )
    }

    /// A bought username effect went live. The action names the style, not
    /// the color: "is glowing (24h)" reads as a story, and the name itself
    /// shows the color everywhere it renders. The tag carries the bought
    /// tier's window, so a month purchase reads "(30d)".
    pub fn username_effect_applied(
        user_id: Uuid,
        username: impl Into<String>,
        effect: late_core::models::username_effect::UsernameEffect,
        duration_secs: i64,
    ) -> Self {
        use late_core::models::rental::duration_tag;
        use late_core::models::username_effect::UsernameEffect;
        let style = match effect {
            UsernameEffect::Glow(_) => "is glowing",
            UsernameEffect::Gradient(_) => "went gradient",
            UsernameEffect::Shimmer => "is shimmering",
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::UsernameEffectApplied { effect },
            format!("{style} ({})", duration_tag(duration_secs)),
        )
    }

    /// A rented chat badge went live. The line names the emoji, since that is
    /// exactly what everyone is about to see next to the name, and carries the
    /// rented window as a tag.
    pub fn badge_rented(
        user_id: Uuid,
        username: impl Into<String>,
        emoji: impl Into<String>,
        duration_secs: i64,
    ) -> Self {
        use late_core::models::rental::duration_tag;
        let emoji = emoji.into();
        let action = format!("rented {emoji} ({})", duration_tag(duration_secs));
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BadgeRented { emoji },
            action,
        )
    }

    /// A rented title went live. The line reads as the title does in chat:
    /// "mira is now the insufferable (30d)".
    pub fn title_applied(
        user_id: Uuid,
        username: impl Into<String>,
        title: impl Into<String>,
        duration_secs: i64,
    ) -> Self {
        use late_core::models::rental::duration_tag;
        let title = title.into();
        let action = format!("is now {title} ({})", duration_tag(duration_secs));
        Self::new(
            Some(user_id),
            username,
            ActivityKind::TitleApplied { title },
            action,
        )
    }

    /// A burn milestone was unlocked. The emoji rides the action so the line
    /// shows exactly what is about to appear next to the name, and the price
    /// is spelled out with separators because six digits run together
    /// otherwise.
    pub fn burn_milestone(
        user_id: Uuid,
        username: impl Into<String>,
        name: impl Into<String>,
        emoji: impl Into<String>,
        price: i64,
    ) -> Self {
        let name = name.into();
        let emoji = emoji.into();
        let action = format!(
            "burned {} chips for the {name} {emoji}",
            crate::app::common::primitives::thousands(price)
        );
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BurnMilestone { name, price },
            action,
        )
    }

    /// A message crossed the gild threshold. Written from the author's side
    /// ("mira got a message gilded 3 times in #lounge") because the feed line
    /// is a compliment, not a receipt: nobody who paid is named, and the room
    /// is, so people can go and read it.
    pub fn message_gilded(
        author_id: Uuid,
        author: impl Into<String>,
        message_id: Uuid,
        count: i64,
        room_slug: Option<String>,
    ) -> Self {
        let room = room_slug
            .as_deref()
            .map(|slug| format!(" in #{slug}"))
            .unwrap_or_default();
        let action = format!("got a message gilded {count} times{room}");
        Self::new(
            Some(author_id),
            author,
            ActivityKind::MessageGilded {
                message_id,
                count,
                room_slug,
            },
            action,
        )
    }

    /// A takeover. Unlike the gild line this one names the loser on purpose:
    /// the crown is a single slot, so "stole it from mira" is the event, and
    /// nothing about it is a private embarrassment. A vacant crown reads
    /// "claimed the vacant crown for 500". This is the ticker line; the
    /// full #lounge headline is `filter::lounge_headline`.
    pub fn crown_taken(
        taker_id: Uuid,
        taker: impl Into<String>,
        reign_id: Uuid,
        price: i64,
        next_price: i64,
        from: Option<String>,
    ) -> Self {
        let price_text = crate::app::common::primitives::thousands(price);
        let action = match &from {
            Some(from) => format!("stole the crown from {from} for {price_text}"),
            None => format!("claimed the vacant crown for {price_text}"),
        };
        Self::new(
            Some(taker_id),
            taker,
            ActivityKind::CrownTaken {
                reign_id,
                price,
                next_price,
                from,
            },
            action,
        )
    }

    /// Someone bought the house a round. The line quotes both numbers because
    /// each says something different: how many people it reached, and what it
    /// cost the one who bought it.
    pub fn round_bought(
        buyer_id: Uuid,
        buyer: impl Into<String>,
        round_id: Uuid,
        patrons: i64,
        total_chips: i64,
    ) -> Self {
        let action = format!(
            "bought the house a round, {patrons} drinks for {} chips",
            crate::app::common::primitives::thousands(total_chips)
        );
        Self::new(
            Some(buyer_id),
            buyer,
            ActivityKind::RoundBought {
                round_id,
                patrons,
                total_chips,
            },
            action,
        )
    }

    /// The pot drew. The line quotes what the winner actually received (the
    /// fifth that was burned is not theirs to be congratulated for) and the
    /// odds behind it.
    pub fn pot_drawn(
        winner_id: Uuid,
        winner: impl Into<String>,
        pot_id: Uuid,
        payout: i64,
        winner_tickets: i64,
        total_tickets: i64,
    ) -> Self {
        use crate::app::common::primitives::thousands;
        let action = format!(
            "won {} chips from the pot on {} of {} tickets",
            thousands(payout),
            thousands(winner_tickets),
            thousands(total_tickets)
        );
        Self::new(
            Some(winner_id),
            winner,
            ActivityKind::PotDrawn {
                pot_id,
                payout,
                winner_tickets,
                total_tickets,
            },
            action,
        )
    }

    /// A finished daily match with a winner. The line names only the winner and
    /// the game — "{winner} won a game of {game}" — never the loser: a friendly
    /// clubhouse feed, not a scoreboard that shames whoever lost. `match_id`
    /// keys the #lounge repeat throttle.
    pub fn daily_win(
        winner_id: Uuid,
        winner: impl Into<String>,
        game_label: &str,
        match_id: Uuid,
    ) -> Self {
        Self::new(
            Some(winner_id),
            winner,
            ActivityKind::DailyResult {
                game: game_label.to_string(),
                match_id,
            },
            format!("won a game of {game_label}"),
        )
    }

    /// A finished daily match that ended in a draw. Attributed to `player_a`
    /// (arbitrary — the line names both): "{player_a} drew with {player_b} at
    /// {game}". Unlike [`Self::daily_win`], a draw shames no one, so naming both
    /// players is fair game.
    pub fn daily_draw(
        player_a_id: Uuid,
        player_a: impl Into<String>,
        player_b: impl AsRef<str>,
        game_label: &str,
        match_id: Uuid,
    ) -> Self {
        Self::new(
            Some(player_a_id),
            player_a,
            ActivityKind::DailyResult {
                game: game_label.to_string(),
                match_id,
            },
            format!("drew with {} at {game_label}", player_b.as_ref()),
        )
    }

    /// A linked user published an entry on cyberspace.online. Names the title
    /// when the entry has one; the story is the action, not the content.
    pub fn cyberspace_posted(
        user_id: Uuid,
        username: impl Into<String>,
        title: Option<String>,
    ) -> Self {
        let action = match feed_safe_title(title.as_deref()) {
            Some(title) => format!("published \"{title}\" on cyberspace"),
            None => "published an entry on cyberspace".to_string(),
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::CyberspacePosted { title },
            action,
        )
    }

    /// A stream went on air: "mat is live: refactoring the render loop".
    /// The line is the invitation; the room row is where the party moves.
    pub fn went_live(user_id: Uuid, username: impl Into<String>, title: Option<String>) -> Self {
        let action = match feed_safe_title(title.as_deref()) {
            Some(title) => format!("is live: {title}"),
            None => "is live".to_string(),
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::WentLive { title },
            action,
        )
    }

    /// Someone showed up to watch: "bob is watching mat's stream". The
    /// event is attributed to the viewer, and `streamer` needs no
    /// `feed_safe_title` pass: usernames cannot contain `@` (DB constraint),
    /// so the #lounge body stays mention-free.
    pub fn watching_stream(viewer_id: Uuid, viewer: impl Into<String>, streamer: String) -> Self {
        let action = format!("is watching {streamer}'s stream");
        Self::new(
            Some(viewer_id),
            viewer,
            ActivityKind::WatchingStream { streamer },
            action,
        )
    }

    pub fn game_scored(
        user_id: Uuid,
        username: impl Into<String>,
        game: ActivityGame,
        score: i32,
        level: Option<i32>,
    ) -> Self {
        let action = match level {
            Some(level) => format!("scored {score} in {} (level {level})", game.label()),
            None => format!("scored {score} in {}", game.label()),
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::GameScored { game, score, level },
            action,
        )
    }

    pub fn bonsai_watered(user_id: Uuid, username: impl Into<String>) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BonsaiWatered,
            "watered their bonsai".to_string(),
        )
    }

    pub fn bonsai_lost(user_id: Uuid, username: impl Into<String>, survived_days: i32) -> Self {
        Self::new(
            Some(user_id),
            username,
            ActivityKind::BonsaiLost { survived_days },
            format!("lost their bonsai ({survived_days}d)"),
        )
    }

    fn new(
        user_id: Option<Uuid>,
        username: impl Into<String>,
        kind: ActivityKind,
        action: String,
    ) -> Self {
        Self::new_at(user_id, username, kind, action, Utc::now())
    }

    fn new_at(
        user_id: Option<Uuid>,
        username: impl Into<String>,
        kind: ActivityKind,
        action: String,
        occurred_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            user_id,
            username: username.into(),
            action,
            kind,
            at: Instant::now(),
            occurred_at,
        }
    }

    pub fn category(&self) -> ActivityCategory {
        self.kind.category()
    }
}

/// Free-text titles (a `/golive` title, a cyberspace entry title) that end up
/// in an action string, made safe for the #lounge feed. Lounge lines become
/// persisted chat messages and the send path runs the mention pipeline on
/// every body (`chat/svc.rs`), so a title containing `@alice` would mint a
/// real mention notification from a system-authored line; `@` is stripped
/// here. `None` when nothing printable is left.
fn feed_safe_title(title: Option<&str>) -> Option<String> {
    let cleaned = title?.replace('@', "");
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned.to_string())
    }
}
