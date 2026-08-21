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
    /// A bought 24h username effect went live ("mat is glowing (24h)").
    /// Shown in #lounge: the whole point of the purchase is being seen.
    UsernameEffectApplied {
        effect: late_core::models::username_effect::UsernameEffect,
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

    /// A bought 24h username effect went live. The action names the style,
    /// not the color: "is glowing (24h)" reads as a story, and the name
    /// itself shows the color everywhere it renders.
    pub fn username_effect_applied(
        user_id: Uuid,
        username: impl Into<String>,
        effect: late_core::models::username_effect::UsernameEffect,
    ) -> Self {
        use late_core::models::username_effect::UsernameEffect;
        let action = match effect {
            UsernameEffect::Glow(_) => "is glowing (24h)",
            UsernameEffect::Gradient(_) => "went gradient (24h)",
            UsernameEffect::Shimmer => "is shimmering (24h)",
        };
        Self::new(
            Some(user_id),
            username,
            ActivityKind::UsernameEffectApplied { effect },
            action.to_string(),
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
