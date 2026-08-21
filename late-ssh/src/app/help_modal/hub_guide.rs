use asterion_core::MAX_MAZE_ID;

use crate::app::lobby::house::{
    ssnake::settings::{
        SSNAKE_BONUS_FOOD_MULTIPLIER, SSNAKE_CLEAR_CHIPS, SSNAKE_CRASH_CHIPS,
        SSNAKE_CRASH_LENGTH_PENALTY_PCT, SSNAKE_EDGE_BONUS_CHIPS, SSNAKE_FOOD_CHIPS,
        SSNAKE_SKIP_COOLDOWN,
    },
    tron::svc::{TRON_WIN_CHIPS, TRON_WIN_PAYOUT_COOLDOWN},
};
use late_core::models::{
    asterion::ASTERION_DAILY_ESCAPE_PAYOUT,
    drinks::{DRINK_PRICE_MAX, DRINK_PRICE_MIN, DRUNK_DECAY_PER_HOUR},
    quest::{DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL, MAX_DAILY_QUEST_STREAK_BONUS_LEVEL},
};

pub(crate) fn bot_context_lines() -> Vec<String> {
    let mut lines = Vec::new();
    for section in guide_sections() {
        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(section.title.to_string());
        lines.extend(section.body.into_iter().map(|line| format!("  {line}")));
    }
    lines
}

struct GuideSection {
    title: &'static str,
    body: Vec<String>,
}

fn guide_sections() -> Vec<GuideSection> {
    let mut sections = Vec::new();
    sections.extend(chip_sections());
    sections.extend(bar_sections());
    sections.extend(quest_sections());
    sections.extend(leaderboard_sections());
    sections.extend(arcade_sections());
    sections.extend(room_game_sections());
    sections
}

fn chip_sections() -> Vec<GuideSection> {
    vec![
        // Earning lives on the Chips tab, which lists every payout in the app.
        // Keeping a second, shorter list here only invited the two to drift.
        GuideSection {
            title: "Earn Chips",
            body: vec![
                "The Chips tab lists every way to earn chips, with amounts.".to_string(),
                "New accounts start with 1,000 chips.".to_string(),
            ],
        },
        GuideSection {
            title: "Top Chips",
            body: vec![
                "Monthly Top Chips counts net chip delta.".to_string(),
                "Betting losses offset betting wins; Shop spending does not lower your rank."
                    .to_string(),
                "Floor restores are excluded from the board.".to_string(),
            ],
        },
    ]
}

fn bar_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "The Bar",
            body: vec![
                "Mention @bartender in the Lounge to order; press t at the bar.".to_string(),
                "There is no menu. He invents the drink and prices it".to_string(),
                format!(
                    "{DRINK_PRICE_MIN}-{DRINK_PRICE_MAX} chips, never more than you can spend."
                ),
                "Your first ever drink is on the house.".to_string(),
                "He only pours for you; use /gift @user <n> to send someone else chips."
                    .to_string(),
            ],
        },
        GuideSection {
            title: "Last Call",
            body: vec![
                "Drinks build a buzz: tipsy, buzzed, sloshed, wasted.".to_string(),
                "Your level shows beside your name wherever you talk.".to_string(),
                format!(
                    "It wears off at {DRUNK_DECAY_PER_HOUR} points an hour, online or not, so a big night is gone by morning."
                ),
                "Wasted is last call: water and coffee only after that.".to_string(),
                "A buzz also comes out in your typing, in public rooms only.".to_string(),
                "Letters inside a word shuffle, more of them the drunker you".to_string(),
                "are, but every word keeps its first and last letter so it".to_string(),
                "stays readable. Handles, links, and code are never touched.".to_string(),
                "What you typed is saved that way; sobering up will not fix it.".to_string(),
            ],
        },
    ]
}

fn quest_sections() -> Vec<GuideSection> {
    vec![GuideSection {
        title: "Quests",
        body: vec![
            "Two daily quests and one weekly quest are drawn on UTC boundaries; they render at the top of The Arcade (page 2).".to_string(),
            "Daily slot 1 is an easy Arcade quest; slot 2 is a medium one.".to_string(),
            "The weekly slot is a hard Arcade quest.".to_string(),
            "Quest rewards pay automatically when the progress target completes.".to_string(),
            "Finishing any one daily quest advances your daily streak.".to_string(),
            format!(
                "Streak bonuses start on the second consecutive streak day: +{} chips.",
                DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL
            ),
            format!(
                "The bonus climbs by {} chips per day up to +{} chips.",
                DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL,
                i64::from(MAX_DAILY_QUEST_STREAK_BONUS_LEVEL)
                    * DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL
            ),
            "Weekly quests do not count toward the daily streak.".to_string(),
        ],
    }]
}

fn leaderboard_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "Arcade Wins",
            body: vec![
                "Counts daily Sudoku, Nonograms, Solitaire, Minesweeper, Le Word, Rubik's Cube, and Sliding Puzzle."
                    .to_string(),
                "Each completed daily adds monthly points:".to_string(),
                "easy / draw-1  1 pt".to_string(),
                "medium         3 pts".to_string(),
                "hard / draw-3  5 pts".to_string(),
                "Le Word daily  1 pt".to_string(),
                "Rubik's Cube   3 pts".to_string(),
                "Sliding Puzzle 1 / 3 / 5 pts by difficulty".to_string(),
                "More hard dailies across more games wins the board.".to_string(),
            ],
        },
        GuideSection {
            title: "Score Games",
            body: vec![
                "Lateris, 2048, Snake, and Traffic record run scores.".to_string(),
                "Monthly boards use scores recorded this month.".to_string(),
                "All-time boards use each user's saved best score.".to_string(),
                "Traffic's saved best is the sum of your per-track bests.".to_string(),
            ],
        },
        GuideSection {
            title: "Daily Win Boards",
            body: vec![
                "Every daily puzzle has its own board on Leaderboards (page 6).".to_string(),
                "Monthly and all-time columns count solved daily boards.".to_string(),
            ],
        },
        GuideSection {
            title: "Timing",
            body: vec![
                "Monthly boards reset on the 1st, UTC.".to_string(),
                "All-time score boards persist.".to_string(),
                "Leaderboards refresh from the server about every 5 minutes.".to_string(),
            ],
        },
    ]
}

fn arcade_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "Arcade Overview",
            body: vec![
                "The Arcade mixes daily puzzle runs, daily challenges, and endless score chases."
                    .to_string(),
                "Open The Arcade with 2.".to_string(),
                "High-score games: 2048, Lateris, Snake, Traffic.".to_string(),
                "Daily games: Rubik's Cube, Sliding Puzzle, Sudoku, Nonograms, Minesweeper, Solitaire, Le Word."
                    .to_string(),
            ],
        },
        GuideSection {
            title: "Arcade Lobby",
            body: vec![
                "j/k or arrows browse games.".to_string(),
                "Enter plays the selected game.".to_string(),
                "Esc/q leaves the current game.".to_string(),
                "` returns to Dashboard while a run is active.".to_string(),
            ],
        },
        GuideSection {
            title: "2048",
            body: vec![
                "hjkl or arrows slide tiles.".to_string(),
                "r restarts after game over.".to_string(),
            ],
        },
        GuideSection {
            title: "Lateris",
            body: vec![
                "h/j/k/l or arrows move, soft-drop, rotate.".to_string(),
                "WASD also moves, soft-drops, and rotates.".to_string(),
                "Space hard drops.".to_string(),
                "p pauses; r/n restarts.".to_string(),
            ],
        },
        GuideSection {
            title: "Snake",
            body: vec![
                "hjkl, WASD, or arrows steer.".to_string(),
                "p pauses; r/n restarts.".to_string(),
            ],
        },
        GuideSection {
            title: "Traffic",
            body: vec![
                "Top-down driving: pick a track, then drive as far as you can through traffic without crashing out."
                    .to_string(),
                "Six tracks: Batin, Route 66, Eurotrip, The Realm, Cosmic Highway, Chaos Highway."
                    .to_string(),
                "Picker: j/k or arrows choose a track; Enter or Space starts it.".to_string(),
                "w/W or up arrow accelerates; s/S or down arrow brakes.".to_string(),
                "a/d or left/right arrow changes lane.".to_string(),
                "Space is the handbrake.".to_string(),
                "p pauses; r restarts the current track; t returns to the track picker.".to_string(),
                "Each track keeps your best score; the leaderboard total is the sum of your per-track bests."
                    .to_string(),
            ],
        },
        GuideSection {
            title: "Rubik's Cube",
            body: vec![
                "Everyone gets the same UTC daily scramble.".to_string(),
                "u/d/l/r/f/b turns a face clockwise.".to_string(),
                "Uppercase turns the same face inverse.".to_string(),
                "s or 0 resets today's scramble.".to_string(),
                "v or any arrow rotates the view.".to_string(),
            ],
        },
        GuideSection {
            title: "Sliding Puzzle",
            body: vec![
                "Daily and personal boards: easy 3x3, medium 4x4, hard 5x5."
                    .to_string(),
                "hjkl or arrows slide a tile in the indicated direction.".to_string(),
                "Click an adjacent tile to slide it into the gap.".to_string(),
                "d selects daily; p selects personal; n twice starts a new personal board."
                    .to_string(),
                "Personal boards persist but grant no chips, quest progress, or Arcade Win."
                    .to_string(),
                "[ and ] change difficulty.".to_string(),
                "r or 0 twice resets the current scramble.".to_string(),
            ],
        },
        GuideSection {
            title: "Daily Puzzle Common Keys",
            body: vec![
                "d selects the daily board.".to_string(),
                "p selects a personal board.".to_string(),
                "n starts a new personal board.".to_string(),
                "[ and ] change difficulty.".to_string(),
                "hjkl or arrows move cursor.".to_string(),
                "r resets the board.".to_string(),
            ],
        },
        GuideSection {
            title: "Sudoku",
            body: vec![
                "1-9 fills a digit.".to_string(),
                "0 or Backspace clears a cell.".to_string(),
            ],
        },
        GuideSection {
            title: "Nonograms",
            body: vec![
                "Space fills or un-fills a cell.".to_string(),
                "x marks or unmarks.".to_string(),
                "c, 0, or Backspace clears a cell.".to_string(),
            ],
        },
        GuideSection {
            title: "Minesweeper",
            body: vec![
                "Space or Enter reveals.".to_string(),
                "f or x flags and unflags.".to_string(),
            ],
        },
        GuideSection {
            title: "Solitaire",
            body: vec![
                "hjkl or arrows move focus.".to_string(),
                "Space or Enter activates, selects, or moves.".to_string(),
                "a auto-moves one card.".to_string(),
                "f auto-foundations all possible cards.".to_string(),
                "u undoes.".to_string(),
                "c clears selection.".to_string(),
                "{ and } scroll the board.".to_string(),
            ],
        },
        GuideSection {
            title: "Le Word",
            body: vec![
                "a-z types letters.".to_string(),
                "Enter submits a guess.".to_string(),
                "Backspace deletes.".to_string(),
                "! opens rules.".to_string(),
            ],
        },
    ]
}

fn room_game_sections() -> Vec<GuideSection> {
    vec![
        GuideSection {
            title: "House Tables",
            body: vec![
                "Ctrl+G opens the Lobby; house tables sit below the daily matches.".to_string(),
                "There is one fixed table per game: no creating tables, no settings forms."
                    .to_string(),
                "Poker, Blackjack, Asterion, Tron, and Super Snake.".to_string(),
                "j/k or arrows move; Enter sits at the selected table.".to_string(),
                "The row shows live occupancy; empty tables are always joinable.".to_string(),
                "q or Esc leaves the table screen; your seat follows that game's rules."
                    .to_string(),
                "Only Poker and Blackjack put your chips at risk, and Super Snake docks a small fee per crash. Everywhere else the winner is paid by the house and losers lose nothing."
                    .to_string(),
            ],
        },
        GuideSection {
            title: "Daily Matches",
            body: vec![
                "Press c (or C for a directed challenge) in the Lobby to post a daily correspondence match, open to anyone or aimed at one user."
                    .to_string(),
                "Chess, battleship, connect4, reversi, checkers, and backgammon.".to_string(),
                "24h per move; Enter in the Lobby claims an open match or opens one of yours."
                    .to_string(),
                "Boards live outside the Tab cycle; Esc returns to the Lobby.".to_string(),
                "` hops Home chat, boards waiting on your move, seated tables, and unfinished dailies."
                    .to_string(),
                "Nothing is staked and a draw pays nobody; only the winner is paid, once per match."
                    .to_string(),
            ],
        },
        GuideSection {
            title: "Active Table",
            body: vec![
                "Game is on top; embedded game chat is below.".to_string(),
                "` cycles Dashboard and tables where you are seated.".to_string(),
                "i composes in embedded chat.".to_string(),
                "Esc clears selected embedded-chat message first.".to_string(),
                "j/k selects embedded-chat messages unless the game claims the key.".to_string(),
                "PageUp/PageDown scroll embedded chat.".to_string(),
                "r/e/d/p/c/f reply, edit, delete, profile, copy, react selected chat message.".to_string(),
                "g jumps to a reply's original message even when it contains an image.".to_string(),
                "Arrows go to the game first; otherwise embedded chat handles them.".to_string(),
            ],
        },
        GuideSection {
            title: "Asterion",
            body: vec![
                "Up to 12 heroes share a real-time labyrinth.".to_string(),
                format!(
                    "Escape {MAX_MAZE_ID} mazes to claim {ASTERION_DAILY_ESCAPE_PAYOUT} chips once per UTC day."
                ),
                "Arrows move; w/s/a/l also moves.".to_string(),
                "Comma and period rotate your view.".to_string(),
                "Pink power-ups auto-collect when you walk onto them.".to_string(),
                "Esc or q leaves the maze and frees your hero slot.".to_string(),
            ],
        },
        GuideSection {
            title: "Blackjack",
            body: vec![
                "Four seats, chips, 6-deck shoe, dealer stands soft 17, blackjack pays 3:2.".to_string(),
                "The house table is fixed: 10-chip stake, standard pace (5m action timer)."
                    .to_string(),
                "Chip buttons are 10, 20, 50, and 100; the table max is 100 chips a hand."
                    .to_string(),
                "s or Enter sits in first open seat.".to_string(),
                "l leaves seat when safe.".to_string(),
                "[/a previous chip; ]/d next chip.".to_string(),
                "Space throws selected chip.".to_string(),
                "Backspace pulls one chip.".to_string(),
                "c or Ctrl+W clears pending bet.".to_string(),
                "Enter or s locks bet.".to_string(),
                "h or Space hits; s stands; d/D doubles down when eligible.".to_string(),
                "First locked bet starts a fixed 30s betting cap.".to_string(),
            ],
        },
        GuideSection {
            title: "Poker",
            body: vec![
                "Four-seat fixed-stack Texas Hold'em with private hole cards, shared board, side pots, showdown ranking, and chip settlement.".to_string(),
                "The house table is fixed: 1000-chip starting stack, 10/20 blinds, standard pace."
                    .to_string(),
                "The pot is what you win; it varies with how many players bet and how much."
                    .to_string(),
                "s or Enter sits in first open seat.".to_string(),
                "n deals next hand.".to_string(),
                "c, Space, or Enter checks or calls.".to_string(),
                "b or r bets or raises.".to_string(),
                "[/] or -/+ adjusts selected bet/raise amount.".to_string(),
                "a goes all-in.".to_string(),
                "x toggles auto check/fold.".to_string(),
                "f folds; l leaves seat.".to_string(),
            ],
        },
        GuideSection {
            title: "Tron",
            body: vec![
                "Two to four riders on the fixed house table: quick speed, glitch mode."
                    .to_string(),
                format!(
                    "Wins pay {TRON_WIN_CHIPS} chips whatever the rider count, one payout per {} minutes.",
                    TRON_WIN_PAYOUT_COOLDOWN.as_secs() / 60
                ),
                "s, Space, or Enter sits when not seated.".to_string(),
                "n starts when at least two riders are seated.".to_string(),
                "w/a/s/d or arrows steer while seated.".to_string(),
                "l leaves seat.".to_string(),
            ],
        },
        GuideSection {
            title: "Super Snake",
            body: vec![
                "A five-seat snake arena that never stops: nobody starts it, nobody wins it, and there is no round to wait for."
                    .to_string(),
                "Sit down mid-flight and you spawn well clear of the other snakes; stand up any time.".to_string(),
                format!(
                    "Every food is worth {SSNAKE_FOOD_CHIPS} chips times the number of snakes MOVING when you eat it."
                ),
                "What the arena owes you runs up in the seat row as a pending figure; it reaches your balance when you stand up, and the idle kick banks it for you if you just disconnect."
                    .to_string(),
                "Snakes that are seated but not moving count for nobody, so idling at a seat pays zero and inflates nothing."
                    .to_string(),
                format!(
                    "Food touching an arena wall pays +{SSNAKE_EDGE_BONUS_CHIPS} per wall before any multiplier, so a corner pickup is worth more than one in open floor."
                ),
                format!("Pink food pays {SSNAKE_BONUS_FOOD_MULTIPLIER}x the usual rate."),
                format!(
                    "The orange food is the arena's last: eating it pays {SSNAKE_CLEAR_CHIPS} chips times the same multiplier and reshuffles the board to a new random level."
                ),
                "The new board counts 3, 2, 1, GO on screen before anyone can steer, so a key pressed on the old arena cannot drive you into a wall you have not seen."
                    .to_string(),
                format!(
                    "Crashing costs {SSNAKE_CRASH_CHIPS} chips and respawns you {SSNAKE_CRASH_LENGTH_PENALTY_PCT}% shorter; there are no lives to lose, and shedding length is how a snake too long to steer gets back under control."
                ),
                format!(
                    "v votes to skip the arena. It changes only once every seated player has voted, and no more than once every {}s — on a table of one you are the whole vote, so the cooldown is what stops anyone rerolling until they get the level they farm fastest.",
                    SSNAKE_SKIP_COOLDOWN.as_secs()
                ),
                "s, Space, or Enter sits when not seated.".to_string(),
                "w/a/s/d, h, or arrows steer while seated.".to_string(),
                format!(
                    "l leaves your seat, but standing up while your snake is moving costs the same {SSNAKE_CRASH_CHIPS} chips as a crash — you cannot bail out of one for free. Stand up parked and it is free."
                ),
                "q or Esc leaves the table screen; your snake keeps its seat and keeps going.".to_string(),
            ],
        },
        GuideSection {
            title: "Daily Boards",
            body: vec![
                "Chess and the other daily games are correspondence matches now, not house tables."
                    .to_string(),
                "One move per turn, 24h to reply, played from the Lobby.".to_string(),
                "w/a/s/d or arrows move the cursor; Space or Enter selects and moves.".to_string(),
                "Esc returns to the Lobby; the match keeps waiting on whoever is to move."
                    .to_string(),
            ],
        },
    ]
}
