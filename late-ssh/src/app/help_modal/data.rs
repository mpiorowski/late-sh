use crate::app::ai::ghost::GRAYBEARD_MENTION_COOLDOWN;
use crate::app::common::primitives::thousands;
use crate::app::common::qr::{Barcode, HalfBlock};
use crate::app::common::username_effect::CROWN_GLYPH;
use late_core::models::{
    article::{NEWS_SHARE_MAX_PAID_PER_DAY, NEWS_SHARE_REWARD_CHIPS},
    asterion::ASTERION_DAILY_ESCAPE_PAYOUT,
    chat_message_gild::GildTier,
    chips::{CHIP_FLOOR, Difficulty, INITIAL_CHIP_BALANCE},
    crown::CROWN_MIN_PRICE,
    pot::{POT_MAX_TICKETS_PER_DAY, POT_TICKET_PRICE},
    quest::{DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL, MAX_DAILY_QUEST_STREAK_BONUS_LEVEL},
};
use qrcodegen::{QrCode, QrCodeEcc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpTopic {
    Pair,
    Overview,
    Architecture,
    Chat,
    Irc,
    Social,
    Profiles,
    News,
    Arcade,
    Lobby,
    Lateania,
    TerminalCopy,
    TerminalLinks,
    TerminalImages,
    TerminalSelection,
    TerminalNotifications,
    TerminalCliYoutube,
    Chips,
    Economy,
    Bonsai,
    Settings,
    Voice,
    Streaming,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 23] = [
        HelpTopic::Pair,
        HelpTopic::Overview,
        HelpTopic::Chat,
        HelpTopic::Irc,
        HelpTopic::Social,
        HelpTopic::Profiles,
        HelpTopic::News,
        HelpTopic::Arcade,
        HelpTopic::Lobby,
        HelpTopic::Lateania,
        HelpTopic::TerminalCopy,
        HelpTopic::TerminalLinks,
        HelpTopic::TerminalImages,
        HelpTopic::TerminalSelection,
        HelpTopic::TerminalNotifications,
        HelpTopic::TerminalCliYoutube,
        HelpTopic::Chips,
        HelpTopic::Economy,
        HelpTopic::Bonsai,
        HelpTopic::Settings,
        HelpTopic::Voice,
        HelpTopic::Streaming,
        HelpTopic::Architecture,
    ];

    pub fn title(self) -> &'static str {
        match self {
            HelpTopic::Pair => "Pair",
            HelpTopic::Overview => "Overview",
            HelpTopic::Architecture => "Architecture",
            HelpTopic::Chat => "Chat",
            HelpTopic::Irc => "IRC",
            HelpTopic::Social => "Social",
            HelpTopic::Profiles => "Profiles",
            HelpTopic::News => "News",
            HelpTopic::Arcade => "Arcade",
            HelpTopic::Lobby => "Lobby",
            HelpTopic::Lateania => "Lateania",
            HelpTopic::TerminalCopy => "Copy",
            HelpTopic::TerminalLinks => "Links",
            HelpTopic::TerminalImages => "Images",
            HelpTopic::TerminalSelection => "Selection",
            HelpTopic::TerminalNotifications => "Notifications",
            HelpTopic::TerminalCliYoutube => "CLI YouTube",
            HelpTopic::Chips => "Chips",
            HelpTopic::Economy => "Economy",
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Settings => "Settings",
            HelpTopic::Voice => "Voice",
            HelpTopic::Streaming => "Streaming",
        }
    }

    pub fn index(self) -> usize {
        match self {
            HelpTopic::Pair => 0,
            HelpTopic::Overview => 1,
            HelpTopic::Chat => 2,
            HelpTopic::Irc => 3,
            HelpTopic::Social => 4,
            HelpTopic::Profiles => 5,
            HelpTopic::News => 6,
            HelpTopic::Arcade => 7,
            HelpTopic::Lobby => 8,
            HelpTopic::Lateania => 9,
            HelpTopic::TerminalCopy => 10,
            HelpTopic::TerminalLinks => 11,
            HelpTopic::TerminalImages => 12,
            HelpTopic::TerminalSelection => 13,
            HelpTopic::TerminalNotifications => 14,
            HelpTopic::TerminalCliYoutube => 15,
            HelpTopic::Chips => 16,
            HelpTopic::Economy => 17,
            HelpTopic::Bonsai => 18,
            HelpTopic::Settings => 19,
            HelpTopic::Voice => 20,
            HelpTopic::Streaming => 21,
            HelpTopic::Architecture => 22,
        }
    }
}

pub(crate) fn lines_for(
    topic: HelpTopic,
    keep_composer_focused: bool,
    listen_url: &str,
) -> Vec<String> {
    match topic {
        HelpTopic::Pair => pair_help_lines(listen_url),
        HelpTopic::Overview => overview_lines(),
        HelpTopic::Architecture => architecture_lines(),
        HelpTopic::Chat => chat_help_lines(keep_composer_focused),
        HelpTopic::Irc => irc_help_lines(),
        HelpTopic::Social => social_help_lines(),
        HelpTopic::Profiles => directory_help_lines(),
        HelpTopic::News => news_help_lines(),
        HelpTopic::Arcade => arcade_help_lines(),
        HelpTopic::Lobby => lobby_help_lines(),
        HelpTopic::Lateania => lateania_help_lines(),
        HelpTopic::TerminalCopy => {
            terminal_faq_topic_lines(crate::app::help_modal::terminal_faq::TerminalHelpTopic::Copy)
        }
        HelpTopic::TerminalLinks => {
            terminal_faq_topic_lines(crate::app::help_modal::terminal_faq::TerminalHelpTopic::Links)
        }
        HelpTopic::TerminalImages => terminal_faq_topic_lines(
            crate::app::help_modal::terminal_faq::TerminalHelpTopic::Images,
        ),
        HelpTopic::TerminalSelection => terminal_faq_topic_lines(
            crate::app::help_modal::terminal_faq::TerminalHelpTopic::Selection,
        ),
        HelpTopic::TerminalNotifications => terminal_faq_topic_lines(
            crate::app::help_modal::terminal_faq::TerminalHelpTopic::Notifications,
        ),
        HelpTopic::TerminalCliYoutube => terminal_faq_topic_lines(
            crate::app::help_modal::terminal_faq::TerminalHelpTopic::CliYoutube,
        ),
        HelpTopic::Chips => chips_help_lines(),
        HelpTopic::Economy => economy_lines(),
        HelpTopic::Bonsai => bonsai_help_lines(),
        HelpTopic::Settings => settings_help_lines(),
        HelpTopic::Voice => voice_help_lines(),
        HelpTopic::Streaming => streaming_help_lines(),
    }
}

pub(crate) fn bot_app_context() -> String {
    let mut out = String::from(
        "APP CONTEXT:\n\
        CRITICAL FACTS:\n\
        - Chat username badges render in this order: bracketed last-month leaderboard awards, special role badges, bonsai stage, chat badge, chat flag, burn milestone, then the /brb moon. The crown prints immediately after the name, and a rented title after that, ahead of the whole stack, as \"name \u{1F451}, the night clerk\".\n\
        - The Clubhouse (page 0, the Late Lounge tavern) is the landing screen: a walkable ASCII room where everyone online is present. Arrows/hjkl walk, i says something (it floats over your head and lands in #lounge), w waves, x dances, Enter interacts with a landmark. This is where you (@bartender) keep the bar.\n\
        - @bartender pours drinks for Late Chips: mention him (or press t at the bar) to order. There is no fixed menu; he invents each drink's name and prices it 100-1000 chips, never more than the patron can spend while keeping a 100-chip floor untouched. A brand-new patron's first-ever drink is free. He only ever pours for the patron who mentioned him: he never charges a drink onto someone else, and points anyone who wants to buy another user a round or a drink at \"/gift @user <n>\" instead, since gifted chips do not carry the drunk-text effect onto someone who did not choose to drink.\n\
        - Drinking builds a buzz that levels up: 0 sober, 1 tipsy, 2 buzzed, 3 sloshed, 4 wasted. Every non-sober level prints its word beside the name. Once wasted, the bartender cuts a patron off to water or coffee instead of more drinks.\n\
        - The buzz sobers up on its own over time, no action needed, whether the patron is online or not: it decays 334 points an hour, so reaching wasted wears off in about six hours and even a maxed-out binge is fully sober again half a day later.\n\
        - A buzz also comes out in your typing, in public rooms only (never DMs or private rooms). Letters inside a word get shuffled, more of them the drunker you are, but the first and last letter of every word stay put so it always stays readable. Tipsy is the odd stumbled word; wasted is most of the sentence, plus the occasional *hic*. Handles, room slugs, links, and code in backticks are never touched. The slurring is saved with the message, so it does not clear up when you sober up later.\n\
        - There is no separate top-level Chat screen. Home/Dashboard owns the chat room rail and chat center; top-level screens are Clubhouse (0), Home (1), The Arcade (2), Games (3), Artboard (4), Profiles (5), and Leaderboards (6).\n\
        - Users constantly ask how to see their mentions. The answer: Mentions is an entry in the Home (page 1) room rail, so press 1 and pick Mentions there; or click the \"N unread mentions\" counter in the top-right corner of the frame; or press Ctrl+/ and type mentions. The unread count lives in the top border, selecting Mentions marks it read, and Enter previews a mention with its surrounding messages (Enter again jumps to it).\n\
        - Users miss their DMs the same way. A DM carrying unread messages is lifted out of the DM list at the bottom of the Home (page 1) room rail into an \"unread dms\" group directly under core, with its unread count beside it; once read it drops back into \"dms\" as soon as the user moves to another room. Favorited DMs stay in favorites instead, and a DM whose peer is ignored appears nowhere. Ctrl+/ also lists DMs unread-first, and /dm @user opens one.\n\
        - The Games hub (page 3) is the dedicated landing for the door games Lateania, NetHack, DCSS, Brogue, Usurper, Green Dragon, A Dark Room, dopewars, CodeKeep, BashQuest, and Rebels; each is launched from there, not from its own top-level page. A Dark Room is the odd one out: it is an incremental, so it grows on its own while you are connected to late.sh (about three hours of village time a day, wherever you are in the app) instead of being played in one sitting. It is also the only door with an ending, and it has two of them: flying the starship out pays 15,000 chips and the [ADE] badge, and doing it while carrying the fleet beacon taken off the immortal wanderer on the ravaged battleship pays 20,000 and the [ADB] badge. They are claimed separately, so one account can earn both. The chips land for every run that gets out, because the save is wiped on the way out and a repeat is the whole arc again; the badge lands once per account. The battleship itself only appears on the map once the account has finished the game at least once, so a first run never meets it.\n\
        - The three roguelikes (NetHack, DCSS, Brogue) support stepping out mid-game: pressing ` inside a running game detaches it (the game keeps running, saved-state intact) and hops along the backtick cycle to the next live dungeon or back to Home chat. Resume from the hub card (a green dot marks a game in progress, Enter resumes) or by pressing ` again from Home. A detached game idle for 20 minutes is closed with a clean save, and it also saves if the session drops. Inside DCSS this costs crawl's own ` repeat-command key.\n\
        - Lateania rides the backtick cycle too, with a twist: pressing ` inside the world hops out like a single-press leave (the character autosaves out of the world, same as a confirmed Esc), and for the next 5 minutes Lateania stays a stop on the cycle, so ` from Home hops straight back into the same character, skipping the character-select gate. While that window is live the Games hub sidebar marks Lateania with the same green dot the roguelikes get. An explicit Esc-Esc leave drops it off the cycle immediately.\n\
        - NetHack and DCSS take a per-account config file (.nethackrc / init.txt): press c on their Games hub card (or their landing page) to open a paste box, paste the whole file to save it, x clears back to defaults. It is stored on the account and applied at every launch, including resumes after a hangup-save. Brogue keeps its config per-player upstream already, so it has no paste box.\n\
        - Profiles page 5 lists people: one row per user who shared a project or posted a work card. Artboard has detailed page-local editing keybinds.\n\
        - Leaderboards page 6 holds every board. The Games section leads: Lateania Adventurers (living characters by level, class shown on the row) and Lateania Frontier (deepest Frontier zone walked), then a board triple for each roguelike door in DCSS, NetHack, Brogue order: Wins (all-time), Deepest Dive, and Top Score (monthly + all-time), fed spoof-proof from the games' own log files, seconds after a game ends. Then Top Chips, Arcade Wins, per-game daily win counts, and per-game high scores, each with monthly and all-time standings. A trailing Badge Guide entry explains what every award code means, how it is earned, and whether it pays chips. Daily quests render at the top of The Arcade (page 2). The Shop opens with the /shop composer command; there is no Hub modal and no shop chord anymore.\n",
    );
    for topic in HelpTopic::ALL {
        out.push_str(&format!("## {}\n", topic.title()));
        // Bot context is per-app, not per-user, so describe the default
        // Enter/Alt+S binding rather than any one user's
        // `keep_composer_focused` tweak state.
        for line in lines_for(topic, false, "") {
            let line = line.trim();
            if line.is_empty() || is_restricted_bot_context_line(line) {
                continue;
            }
            out.push_str("- ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Trimmed app context for @bartender: navigation only, not the full guide.
/// He is house furniture, not the help desk. @bot owns explaining features
/// in depth, so anything past "which screen / which key" should route there.
pub(crate) fn bartender_app_context() -> String {
    "APP CONTEXT (basic navigation):\n\
    - Screens: 0 Clubhouse (this room, the Late Lounge tavern), 1 Home (chat + music), 2 The Arcade (single-player games, daily quests at the top), 3 Games hub (Lateania, NetHack, DCSS, Brogue, Usurper, Green Dragon, A Dark Room, dopewars, CodeKeep, BashQuest, Rebels), 4 Artboard (shared ASCII canvas), 5 Profiles (the people: their projects and open-to-work cards), 6 Leaderboards (every board, monthly and all-time).\n\
    - Tab / Shift+Tab cycles screens; number keys 0-6 jump straight to one.\n\
    - Ctrl+O opens Settings from anywhere. Ctrl+G opens the Lobby (daily correspondence games plus the fixed house tables: Poker, Blackjack, Asterion, Tron, Super Snake). Typing /shop into the composer opens the Shop.\n\
    - Ctrl+/ opens jump search across rooms and DMs; typing ?query searches messages.\n\
    - Home's room rail also holds RSS, News, Cyberspace, Voice, Mentions, and Discover. When a patron asks where their mentions are: press 1, pick Mentions in the rail, or click the \"N unread mentions\" counter in the top-right corner.\n\
    - A DM with unread messages jumps to an \"unread dms\" group directly under core in that rail, so nobody has to scroll to the bottom to find it; it drops back down to \"dms\" once it has been read and you move on.\n\
    - In the Clubhouse: arrows/hjkl walk, i talks (it floats over your head and lands in #lounge), w waves, x dances, Enter interacts with a landmark.\n\
    - Pressing ? anywhere opens the full in-app guide, with a tab per topic.\n\
    - For anything past basic directions (commands, game rules, settings, IRC, account stuff) don't guess: tell the patron to go ask @bot, that's what he's for.\n"
        .to_string()
}

fn is_restricted_bot_context_line(line: &str) -> bool {
    let line = line.to_lowercase();
    [
        "/audio",
        "/create-room",
        "/delete-room",
        "/fill-room",
        "/mod",
        "staff",
        "admin",
        "moderation",
        "unskippable",
    ]
    .iter()
    .any(|forbidden| line.contains(forbidden))
}

const SHELL_INSTALL_COMMAND: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";
const WINDOWS_INSTALL_COMMAND: &str = "irm https://cli.late.sh/install.ps1 | iex";
const NIX_COMMAND: &str = "nix run github:mpiorowski/late-sh#late";
const SOURCE_URL: &str = "https://github.com/mpiorowski/late-sh";
const QR_QUIET_ZONE: i32 = 4;

fn pair_help_lines(listen_url: &str) -> Vec<String> {
    let listen_url = listen_url.trim();
    let listen_url = if listen_url.is_empty() {
        "late.sh/listen"
    } else {
        listen_url
    };
    let mut lines = vec![
        "Install `late` / Listen Anywhere".to_string(),
        "".to_string(),
        "Recommended: install the native CLI and run `late` instead of `ssh late.sh`.".to_string(),
        "That gives one process for SSH, local Icecast audio, YouTube webview fallback, voice rooms, and OS clipboard image reads.".to_string(),
        "".to_string(),
        "Install".to_string(),
        format!("  linux / macos / termux   {SHELL_INSTALL_COMMAND}"),
        format!("  windows powershell       {WINDOWS_INSTALL_COMMAND}"),
        format!("  nixos                    {NIX_COMMAND}"),
        format!("  source                   git clone {SOURCE_URL}"),
        "                           cargo build --release --bin late".to_string(),
        "".to_string(),
        "What `late` unlocks".to_string(),
        "  audio       Icecast playback and visualizer on your machine".to_string(),
        "  youtube     embedded webview hosts the shared queue locally".to_string(),
        "  clipboard   /paste-image reads your OS clipboard image into chat".to_string(),
        "  voice       talk in voice rooms with your mic (linux + windows; plain SSH only shows status)".to_string(),
        "  desktop     now playing shows in your desktop media widget (linux)".to_string(),
        "  controls    m mute, +/- volume, v+x source, v+v Music Booth".to_string(),
        "".to_string(),
        "Listen without the CLI".to_string(),
        "  Open the link below on any device, or scan the QR.".to_string(),
        "  It plays the house streams, Nightride, and the community YouTube".to_string(),
        "  queue in a plain browser tab. No pairing, no session, nothing to".to_string(),
        "  install, so it works from a phone or a locked-down laptop.".to_string(),
        "  Listening only: chat, games and the rest stay in the terminal.".to_string(),
        "".to_string(),
    ];

    lines.extend(qr_lines(listen_url));
    lines.extend([
        "".to_string(),
        listen_url.to_string(),
        "scan with your phone or open the link on any device".to_string(),
        "".to_string(),
        "Trouble?".to_string(),
        "  The terminal-specific tabs below cover copy, links, images, selection, notifications, and CLI YouTube.".to_string(),
    ]);
    lines.push("".to_string());
    lines.extend(music_pair_lines());
    lines
}

fn qr_lines(listen_url: &str) -> Vec<String> {
    if !(listen_url.starts_with("https://") || listen_url.starts_with("http://")) {
        return Vec::new();
    }
    let Ok(qr) = QrCode::encode_text(listen_url, QrCodeEcc::Low) else {
        return Vec::new();
    };
    let size = qr.size();
    let total = size + QR_QUIET_ZONE * 2;
    let module = |x: i32, y: i32| -> bool {
        let mx = x - QR_QUIET_ZONE;
        let my = y - QR_QUIET_ZONE;
        if mx < 0 || my < 0 || mx >= size || my >= size {
            return false;
        }
        qr.get_module(mx, my)
    };

    let mut out = Vec::with_capacity(((total + 1) / 2) as usize);
    let mut y = 0i32;
    while y < total {
        let mut row = String::with_capacity(total as usize);
        for x in 0..total {
            let top = module(x, y);
            let bot = module(x, y + 1);
            let bits = (top as u32) | ((bot as u32) << 1);
            row.push(HalfBlock::glyph(bits));
        }
        out.push(format!("  {row}"));
        y += 2;
    }
    out
}

fn terminal_faq_topic_lines(
    topic: crate::app::help_modal::terminal_faq::TerminalHelpTopic,
) -> Vec<String> {
    crate::app::help_modal::terminal_faq::lines_for(topic)
}

fn economy_lines() -> Vec<String> {
    crate::app::help_modal::hub_guide::bot_context_lines()
}

/// Every way Late Chips enter or leave an account, in one place. "How do I
/// earn chips" is the single most asked question, so this tab is the one
/// source of truth: the Economy tab keeps ranking and per-game rules, and the
/// payout numbers live here. Amounts pull from constants where one exists;
/// the rest come from the seeded reward templates in `late-core/migrations`,
/// so update both together when a payout changes.
fn chips_help_lines() -> Vec<String> {
    let start = INITIAL_CHIP_BALANCE;
    let floor = CHIP_FLOOR;
    let easy = Difficulty::Easy.chips();
    let medium = Difficulty::Medium.chips();
    let hard = Difficulty::Hard.chips();
    let water = crate::app::bonsai::svc::WATER_CHIP_BONUS;
    let asterion = ASTERION_DAILY_ESCAPE_PAYOUT;
    let streak_step = DAILY_QUEST_STREAK_BONUS_CHIPS_PER_LEVEL;
    let streak_max = i64::from(MAX_DAILY_QUEST_STREAK_BONUS_LEVEL) * streak_step;

    vec![
        "Late Chips".to_string(),
        "".to_string(),
        "Late Chips are the single currency: one balance per account, spent in the Shop and at the bar, and ranked monthly on the Top Chips board.".to_string(),
        format!("You start with {start} chips."),
        format!("You can never end up below {floor} chips: after a losing settlement at a betting table the floor is restored for you, so going broke is not a thing."),
        "This tab lists every way to earn chips. If something is not on this list, it does not pay chips.".to_string(),
        "".to_string(),
        "1. Arcade dailies (page 2)".to_string(),
        "  Every daily puzzle pays once per board per UTC day, for everyone who solves it.".to_string(),
        format!("  easy               {easy} chips"),
        format!("  medium             {medium} chips"),
        format!("  hard               {hard} chips"),
        "  Sudoku, Nonograms, and Minesweeper each have all three difficulties.".to_string(),
        format!("  Solitaire draw-1   {medium} chips"),
        format!("  Solitaire draw-3   {hard} chips"),
        format!(
            "  Le Word daily      {} chips",
            crate::app::arcade::le_word::state::DAILY_WIN_REWARD_CHIPS
        ),
        format!(
            "  Rubik's Cube daily {} chips",
            crate::app::arcade::rubiks_cube::state::DAILY_WIN_REWARD_CHIPS
        ),
        "  Personal (non-daily) boards pay nothing; only the daily board pays.".to_string(),
        "  The high-score games (2048, Lateris, Snake, Traffic) pay no chips for a run on their own. They pay through Quests, and they rank you on the monthly leaderboards.".to_string(),
        "".to_string(),
        "2. Quests (top of The Arcade, page 2)".to_string(),
        "  Two daily quests and one weekly quest are drawn for you on UTC boundaries.".to_string(),
        "  Every quest currently drawn is an Arcade quest: daily slot 1 easy, daily slot 2 medium, the weekly slot hard, all from the Arcade pool (daily puzzles plus score runs).".to_string(),
        "  easy daily         150 chips".to_string(),
        "  medium daily       375 chips".to_string(),
        "  hard weekly        750 chips".to_string(),
        "  Rewards pay automatically the moment the target completes; there is nothing to claim.".to_string(),
        "  A quest reward stacks with the daily-puzzle payout above, so one solve can pay twice.".to_string(),
        format!("  Finishing any one daily quest advances your daily streak: +{streak_step} chips on the second consecutive day, climbing by {streak_step} per day up to +{streak_max}."),
        "  Weekly quests do not advance the daily streak.".to_string(),
        "".to_string(),
        "3. Bonsai (w)".to_string(),
        format!("  Watering pays {water} chips once per UTC day."),
        "  Classic and Dynamic Bonsai pay the same; you only get it once per day either way.".to_string(),
        "".to_string(),
        "4. The Lobby (Ctrl+G)".to_string(),
        "  Poker and Blackjack are the only real betting in the Lobby. Super Snake nicks a few chips per crash but pays far more per food; everywhere else the winner is paid out of the house and the losers lose nothing.".to_string(),
        "".to_string(),
        "  Daily correspondence matches, paid per match won:".to_string(),
        "    Chess            500 chips".to_string(),
        "    Chess960         500 chips".to_string(),
        "    Connect Four     400 chips".to_string(),
        "    Reversi          400 chips".to_string(),
        "    Checkers         400 chips".to_string(),
        "    Backgammon       400 chips".to_string(),
        "    Briscola         400 chips".to_string(),
        "    Battleship       300 chips".to_string(),
        "    A draw pays nobody. Only the winner is paid, and each match pays once.".to_string(),
        format!("    A win pays only if at least {} moves were played (both players' moves count),", crate::app::lobby::daily::svc::DAILY_WIN_MIN_MOVES),
        "    and at most one win per opponent per game per day the challenge was posted is paid.".to_string(),
        "    Two long games against the same person can end the same day and both pay:".to_string(),
        "    they were posted on different days.".to_string(),
        "".to_string(),
        "  House tables:".to_string(),
        format!("    Asterion         {asterion} chips for escaping the last maze, once per UTC day"),
        format!("    Super Snake      {} chips per food eaten (+{} per arena wall the food touches),", crate::app::lobby::house::ssnake::settings::SSNAKE_FOOD_CHIPS, crate::app::lobby::house::ssnake::settings::SSNAKE_EDGE_BONUS_CHIPS),
        "                     times the number of snakes moving,".to_string(),
        format!("                     with no cooldown. Clearing an arena pays {} on"
            , crate::app::lobby::house::ssnake::settings::SSNAKE_CLEAR_CHIPS),
        format!("                     the same multiplier; every crash costs {}. The arena runs forever,", crate::app::lobby::house::ssnake::settings::SSNAKE_CRASH_CHIPS),
        "                     so one player alone can farm it and a crowd earns more each.".to_string(),
        "                     The seat's take is pending while you play and lands in your balance".to_string(),
        "                     when you stand up (or when the idle kick reclaims the seat).".to_string(),
        format!("    Tron             {} chips per win, one payout per 5 minutes", crate::app::lobby::house::tron::svc::TRON_WIN_CHIPS),
        "    Poker, Blackjack these are real betting: you put chips in and can lose them.".to_string(),
        "                     Winnings come from the pot or the dealer, not from a fixed payout,".to_string(),
        format!("                     and the {floor}-chip floor is restored if you bust."),
        "".to_string(),
        "5. Games hub (page 3)".to_string(),
        "  Door games pay for big feats, and they are the biggest payouts in the app. Every one of them repeats:".to_string(),
        "    NetHack: claim the Amulet of Yendor           20,000 chips   per run, 7-day gap".to_string(),
        "    NetHack: ascend                               50,000 chips   per run, 7-day gap".to_string(),
        "    DCSS: pick up the Orb of Zot                  20,000 chips   per run, 7-day gap".to_string(),
        "    DCSS: escape the dungeon with the Orb         50,000 chips   per run, 7-day gap".to_string(),
        "    Brogue: escape the Dungeons of Doom           20,000 chips   per run, 7-day gap".to_string(),
        "    Brogue: the super-victory (mastery)           50,000 chips   per run, 7-day gap".to_string(),
        "    Lateania: the Archdemon Mal'gareth            10,000 chips   per character, 7-day gap".to_string(),
        "    Lateania: the King Who Was Promised Nothing   10,000 chips   per character, 7-day gap".to_string(),
        "    Lateania: Yssgar, the Sundering Deep          20,000 chips   per character, 7-day gap".to_string(),
        "    Lateania: Kaethyr Ascendant                   20,000 chips   per character, 7-day gap".to_string(),
        "    Green Dragon: slay the Green Dragon           10,000 chips   every kill".to_string(),
        "    A Dark Room: fly the starship off the rock    15,000 chips   every run".to_string(),
        "    A Dark Room: fly out with the fleet beacon    20,000 chips   every run".to_string(),
        "  \"per run\" means one payout per finished game, so re-reading an old log pays nothing;".to_string(),
        "  the 7-day gap is per milestone per account, so a lucky week still pays once.".to_string(),
        "  Green Dragon and A Dark Room need no gap: the kill resets your character and the ending wipes the save,".to_string(),
        "  so the next payout is a whole run away either way.".to_string(),
        "  Lateania's gate is the character: each crown pays once per character you have, and the 7-day gap".to_string(),
        "  stops a reroll from farming the easy two.".to_string(),
        "  Each of those also grants a permanent profile badge, once per account, on the first grant;".to_string(),
        "  the full code guide is on the Leaderboards page (6).".to_string(),
        "  Lateania gold is its own in-world currency and never converts to chips.".to_string(),
        "  Usurper, dopewars, CodeKeep, BashQuest, and Rebels pay no chips yet. More door games will get payouts as they land.".to_string(),
        "".to_string(),
        "6. Gilds".to_string(),
        format!("  Press g on someone else's message in a public room and pick a tier: Bronze {}, Silver {}, Gold {}.",
            thousands(GildTier::Bronze.price()),
            thousands(GildTier::Silver.price()),
            thousands(GildTier::Gold.price())),
        "  Two thirds of what the buyer pays lands in the author's balance; the last third is destroyed.".to_string(),
        format!("  So an author receives {} / {} / {} chips per gild.",
            thousands(GildTier::Bronze.author_share()),
            thousands(GildTier::Silver.author_share()),
            thousands(GildTier::Gold.author_share())),
        "  The marker stays on the message forever. There is no un-gild, and you cannot gild yourself or a bot.".to_string(),
        "  One gild per message per buyer. Buying a higher tier later raises it at that tier's full price; it never goes down.".to_string(),
        "  Gilds received do NOT count toward Top Chips: the board ranks what you earned, not what you were tipped.".to_string(),
        "".to_string(),
        "7. Sharing news".to_string(),
        format!("  Publishing a link to News pays {NEWS_SHARE_REWARD_CHIPS} chips."),
        "  It pays the same either way: pasting a URL with i in News, or pressing s on an entry in your RSS inbox.".to_string(),
        "  A link that is already in News cannot be shared again, so a story only ever pays its first sharer.".to_string(),
        "  You are paid once per link. Deleting your own story and re-sharing it pays nothing the second time.".to_string(),
        format!("  At most {NEWS_SHARE_MAX_PAID_PER_DAY} shares a day (UTC) are paid. Shares past that still publish, for nothing."),
        "".to_string(),
        "8. The crown".to_string(),
        format!("  One slot, one holder, one {CROWN_GLYPH} after their name in every message they send."),
        "  /crown shows who wears it and what taking it costs. /crown take buys it.".to_string(),
        format!("  A vacant crown costs {}. After that it costs 1.5x whatever the holder paid, rounded up,", thousands(CROWN_MIN_PRICE)),
        "  so the price ratchets on its own and nobody sets it.".to_string(),
        "  Every chip is destroyed: the crown pays nobody, and none of it comes back into the economy.".to_string(),
        "  There is no cooldown: anyone can take it off you the moment you have it, at 1.5x. You cannot take a crown you already wear.".to_string(),
        "  It empties at the end of every UTC month: the crown goes back to vacant, and whoever wore it".to_string(),
        "  when the month ended keeps the permanent [CRWN] badge for that month.".to_string(),
        "  Every takeover posts to #lounge, naming both players.".to_string(),
        "  Like Shop spending, the crown does not count against Top Chips.".to_string(),
        "".to_string(),
        "9. Burn milestones".to_string(),
        format!("  Three permanent badges in the Shop's Ultimates tab: Wick {} \u{1F56F}\u{FE0F}, Fuse {} \u{1F9E8}, Furnace {} \u{1F30B}.",
            thousands(50_000),
            thousands(150_000),
            thousands(500_000)),
        "  They buy nothing but the glyph, and every chip is destroyed.".to_string(),
        "  A milestone never expires and is never equipped: it shows on top of whatever badge and flag you are renting,".to_string(),
        "  so nothing you rent can hide one. Own two and the dearer one shows.".to_string(),
        "  Each unlock posts to #lounge, naming the price.".to_string(),
        "  Like the crown and Shop spending, they do not count against Top Chips.".to_string(),
        "".to_string(),
        "10. The pot".to_string(),
        format!("  A raffle, drawn once a week, Monday 21:00 UTC. Tickets cost {} chips each.", thousands(POT_TICKET_PRICE)),
        "  /pot shows the pot, the tickets in it, what you hold and paid, how many more you can buy today, and how long is left.".to_string(),
        "  /pot buy N buys N tickets.".to_string(),
        format!("  One player may buy at most {POT_MAX_TICKETS_PER_DAY} tickets a day (UTC), so a full week is {}: the draw is about showing up, not about bank.", 7 * POT_MAX_TICKETS_PER_DAY),
        "  At the draw one ticket is pulled, weighted by how many each player holds.".to_string(),
        "  The holder takes 80% of everything the tickets paid in; the other fifth is destroyed.".to_string(),
        "  Nobody in the pot means nobody is paid: it rolls, and a fresh pot opens either way.".to_string(),
        "  The winner is announced in #lounge, so you can read it when you get back.".to_string(),
        "  Neither the tickets you buy nor the pot you win counts toward Top Chips.".to_string(),
        "".to_string(),
        "11. Gifts".to_string(),
        "  /gift @user <n>    send chips to someone, with an optional note after the amount".to_string(),
        format!("  A gift only goes through while it leaves you at or above {floor} chips."),
        "  Gifts move chips between players; they do not create new ones.".to_string(),
        "".to_string(),
        "What does not pay chips".to_string(),
        "  Chatting, showcases, profiles, voice, and the Artboard pay nothing. Sharing a link to News does pay; see 7 above.".to_string(),
        "  Monthly leaderboard awards are prestige only; the door feats above are the exception,".to_string(),
        "  and those pay again every time their gate reopens.".to_string(),
        "  There is no login bonus, idle income, or daily stipend: chips come from playing, watering, quests, and sharing news.".to_string(),
        "".to_string(),
        "Where chips go".to_string(),
        "  The Shop (/shop) for badge, flag, title and name-effect rentals, Dynamic Bonsai, the pet companion, and the Aquarium.".to_string(),
        "  Badges, flags, titles, and name effects are rented for 24 hours or 30 days, one live at a time per slot.".to_string(),
        "  The one title on sale is Your Own Title (1,000 / 24h, 40,000 / 30d): you write it, up to 20 characters.".to_string(),
        "  It is screened before the chips move, so a refused title costs you nothing.".to_string(),
        "  A rebuy replaces whatever is live in that slot and restarts its clock; when a rental lapses the slot simply empties.".to_string(),
        "  @bartender drinks in the Clubhouse.".to_string(),
        "  Poker and Blackjack bets.".to_string(),
        "  Gifts you send.".to_string(),
        "  Gilds you buy on other people's messages.".to_string(),
        "  The crown (/crown take), which burns the whole price.".to_string(),
        format!("  Pot tickets (/pot buy N) at {} chips each, of which a fifth is burned at the draw.", thousands(POT_TICKET_PRICE)),
        "  Burn milestones and the two ultimate spells (1,000,000 each), the top of the Shop.".to_string(),
        "  Monthly Top Chips ranks net chip delta, so Shop and crown spending never lower your rank; betting losses do.".to_string(),
    ]
}

pub(crate) fn chat_help_lines(keep_composer_focused: bool) -> Vec<String> {
    let compose_send_lines: &[&str] = if keep_composer_focused {
        &["  Enter              send and keep open"]
    } else {
        &[
            "  Enter              send and exit",
            "  Alt+S              send and keep open",
        ]
    };
    let mut lines: Vec<String> = [
        "Commands",
        "  /binds             open this guide",
        "  /settings          open your settings modal",
        "  /icons             open emoji / nerd font picker",
        "  /petname [name]    show or set your pet's name",
        "  /brb [message]     show away badge and mute paired audio",
        "  /coffee            post a coffee cup",
        "  /tea               post a tea cup",
        "  /ultimate          open owned Ultimate Spells",
        "  /profile [@user]   open your profile, or another user's profile",
        "  /exit              open quit confirm",
        "  /public #room      open/create opt-in public room",
        "  /join #room        same as /public",
        "  /private #room     create a private room",
        "  /roominfo          set this room's topic & rules (owner, or a mod)",
        "  /rules             show this room's rules",
        "  /invite @user      add a user to the current room",
        "  /kick @user        remove a user from your private room (or a mod)",
        "  /ban @user         ban from your stream room; add 7d and a reason",
        "  /unban @user       lift a ban you set on your stream room",
        "  /leave             leave the current room",
        "  /dm @user          open a direct message",
        "  /active            list active users",
        "  /gift @user <n>    send chips, with an optional note after the amount",
        "  /crown             who wears the crown; /crown take buys it",
        "  /pot               the weekly pot; /pot buy N buys N tickets",
        "  /friends           list friends",
        "  /friend [@user]    list friends, or mark a user as a friend",
        "  /unfriend [@user]  list friends, or remove a friend mark",
        "  /members           list users in this room",
        "  /list              list public rooms",
        "  /poll              start a Home room poll with 2-3 options",
        "  /pair @user        shared live coding scratchpad (both of you must run it)",
        "  /golive [title]    stream your screen via a browser publisher page",
        "  /golive obs [..]   stream from OBS over WHIP; /golive stop ends either",
        "  /watch @user       open someone's live stream (browser via paired CLI, else QR)",
        "                     setup and OBS details live in the Streaming tab",
        "  /pomodoro [m] [..] focus countdown in the top bar, 1-180 min (default 25)",
        "                     [..] names the block; /pomodoro stop cancels it",
        "  /roll [NdM ...]    roll dice (default d20), e.g. /roll 3d6 2d20",
        "  /sheet [@user]     your character sheet, or another user's (#dnd)",
        "  /paste-image       upload image from paired CLI clipboard (see Images)",
        "  /upload <url>      download and upload an image URL (see Images)",
        "  /ignore [@user]    ignore a user, or list ignored users",
        "  /unignore [@user]  unignore a user, or list ignored users",
        "  /search [query]    search messages (opens the Ctrl+/ modal in ? mode)",
        "  /history           browse this room's full history in a scrollable modal",
        "                     with unread waiting it opens on your first unread message",
        "  /summary           AI catch-up of this public room, the last day or since your last read",
        "  /summary 6h        catch up on exactly that window instead (also /summary 90m)",
        "                     (up to 2 days back; one per room every 10 minutes)",
        "",
        "Global chat keys",
        "  Ctrl+O             open your settings modal anywhere",
        "  Ctrl+G             open / close the Lobby (daily games + house tables)",
        "  Ctrl+L             redraw the screen if something outside late.sh scribbled on it",
        "  /shop              open the Shop",
        "  /aquarium          toggle the Aquarium in the Lounge after unlocking it in the Shop",
        "  /aquarium feed     feed your Aquarium with bought Aquarium Food",
        "  /pet               toggle the pet strip in the Lounge",
        "  Ctrl+/             jump to a room or DM; type ?query to search messages",
        "  ?                  open this guide; Pair and terminal-specific tabs live here",
        "",
        "Messages",
        "  j / k              select older / newer message",
        "                     inside a message taller than the pane they scroll it by rows first",
        "  ↑ / ↓              same as j / k",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  half page up / down",
        "  g / G              clear selection (back to live view)",
        "  p                  open selected user's profile",
        "  f then 1-9        quick-react to selected message",
        "  f then 0          choose any icon-picker reaction",
        "  f then f          list reaction owners",
        "  Enter              jump to loaded original for selected reply",
        "  Enter              open selected image or News item when present",
        "  g                  jump to a reply's original even if it has an image",
        "  r                  reply to selected message",
        "  e                  edit selected message",
        "  dd                 delete selected message (press d twice)",
        "  c                  copy selected message to clipboard",
        "  new messages       a rule marks where you left off, in the room and in /history",
        "  t                  translate selected message (press again to hide)",
        "  g                  gild selected message (three tiers, chips to the author)",
        "",
        "Translation",
        "  t                  translate the selected message into your language",
        "  ↳ line             the translation renders dim under the original",
        "  target language    Ctrl+O → Settings → Translation → Target language",
        "  auto mode          same menu: auto-translate new messages in the open room",
        "  share yours        same menu: translate my messages to English, so English",
        "                     readers see your messages without asking",
        "  history            older messages stay on demand; press t on any of them",
        "  shared             a translation is cached, so everyone after you reads it free",
        "",
        "Rooms",
        "  h / l  or  ← / →   previous / next room",
        "  Space              room jump hints",
        "  Enter / i          start composing",
        "  Ctrl+N / Ctrl+P    next / previous room while preserving draft",
        "",
        "Polls",
        "  /poll              create a 10/20/30-minute poll in the selected Home room",
        "  va / vb / vc       vote while a poll is visible",
        "  v1 / v2 / v3       select music streams/stations",
        "  limit              one active poll per room",
        "  author             the strip names who started the poll, when it fits",
        "",
        "Compose",
        // `<<COMPOSE_SEND_LINES>>` marker is replaced after collection so the
        // Enter/Alt+S section can collapse to a single line when the
        // `keep_composer_focused` tweak is on. Keep this token unique.
        "<<COMPOSE_SEND_LINES>>",
        "  Alt+Enter / Ctrl+J newline",
        "  Esc                exit compose",
        "  Backspace          delete char",
        "  Ctrl+W / Ctrl+Backspace",
        "                     delete word left",
        "  Ctrl+Delete        delete word right",
        "  Ctrl+U             delete to start of line",
        "  Ctrl+← / Ctrl+→    move cursor by word",
        "  @user              mention (Tab/Enter to confirm)",
        "  Ctrl+]             open emoji / nerd font picker",
        "  paste image bytes  upload PNG/JPEG/GIF/WebP when file storage is configured",
        "",
        "Markdown",
        "  # / ## / ###       headings",
        "  **bold**           bold",
        "  *italic*           italic",
        "  ***both***         bold + italic",
        "  ~~strike~~         strikethrough",
        "  `code`             inline code",
        "  [text](url)        link",
        "  > quote            blockquote",
        "  - item             unordered list",
        "  1. item            ordered list",
        "  ```                fenced code block (close with ```)",
        "  :shortcode:        GitHub/Discord emoji names expand on screen (:tada: -> 🎉);",
        "                     unknown codes and anything inside code stay exactly as typed",
        "",
        "Icon picker",
        "  ↑/↓ or Ctrl+K/J    move selection",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  jump a page",
        "  type to filter     search by name",
        "  Tab / Shift+Tab    switch icon tabs",
        "  Enter              insert and close",
        "  Alt+Enter          insert and keep open",
        "  click / wheel      select / scroll",
        "  double-click       insert and keep open",
        "  Esc                close",
        "",
        "Overlay windows",
        "  Esc / q            close overlay",
        "  j / k              scroll overlay",
        "  image modal        Enter/c copy image URL; Esc/q close; see Images",
        "",
        "Synthetic entries",
        "  Home room rail also contains RSS, News, Cyberspace, Voice, Mentions, and Discover.",
        "  A DM with unread messages sits under core in an unread dms group,",
        "  and drops back to dms once you open another room.",
        "  Profiles page 5 is the merged feed of projects and work cards.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let marker_idx = lines
        .iter()
        .position(|l| l == "<<COMPOSE_SEND_LINES>>")
        .expect("compose-send marker present");
    lines.splice(
        marker_idx..=marker_idx,
        compose_send_lines.iter().map(|s| s.to_string()),
    );
    lines
}

fn irc_help_lines() -> Vec<String> {
    [
        "IRC access",
        "",
        "late.sh includes an optional IRC surface for the same chat account.",
        "It is not a separate account or a separate chat system: IRC reads and writes the same rooms, DMs, usernames, and bans.",
        "",
        "How to connect",
        "  Create token      Settings > Account > IRC access token",
        "  Server password   paste that token into your IRC client's server password / PASS field",
        "  Nick              any configured nick is accepted, then locked to your late.sh username",
        "  Dev server        localhost:6667 with TLS off when running make start",
        "  Production        irc.late.sh port 6697 with TLS/SSL enabled",
        "  Verify TLS        keep certificate verification on when using irc.late.sh",
        "",
        "WeeChat quick setup",
        "  /server add late irc.late.sh/6697",
        "  /set irc.server.late.tls on",
        "  /set irc.server.late.tls_verify on",
        "  /set irc.server.late.password \"late-irc-...\"",
        "  /connect late",
        "",
        "Connection troubleshooting",
        "  IRC is raw TCP, so irc.late.sh must be DNS-only, not proxied.",
        "  If hostname connects hang, check that DNS resolves to late.sh node IPs.",
        "  Avoid pasting tokens in public logs or chat; reset the token if it leaks.",
        "",
        "Good Arch clients",
        "  WeeChat           terminal-native; pacman -S weechat",
        "  Halloy            GUI client; pacman -S halloy",
        "",
        "Useful IRC commands",
        "  /list             list public channels and private channels you can access",
        "  /join #lounge     join a channel; #lounge is joined automatically",
        "  /msg #room text   send to a late.sh room",
        "  /msg nick text    send a late.sh DM",
        "  /names #room      show online channel users",
        "  /whois nick       show basic user info",
        "  /part #room       detach the IRC view; late.sh membership stays unchanged",
        "",
        "Room mapping",
        "  #lounge, language rooms, and topic rooms with slugs are IRC channels.",
        "  Private topic rooms only appear to members.",
        "  DMs stay direct messages, not channels.",
        "  Game-room chat is not exposed as IRC channels.",
        "",
        "Token behavior",
        "  New users start with no IRC token and cannot connect over IRC.",
        "  Resetting a token shows the new value once and disconnects old IRC clients.",
        "  Revoking the token disables IRC access for the account.",
        "  Deleting or linking accounts disconnects live IRC clients for the affected account.",
        "",
        "Client expectations",
        "  Most clients call the token a server password.",
        "  Nick changes from IRC are refused; change your username in late.sh settings.",
        "  Long late.sh messages may appear as multiple IRC PRIVMSG lines.",
        "  Edited messages arrive with an [edit] prefix.",
        "  Presence is bridged: TUI and IRC users both count as online.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn music_pair_lines() -> Vec<String> {
    MUSIC_PAIR_TEXT.lines().map(str::to_string).collect()
}

fn social_help_lines() -> Vec<String> {
    [
        "Social surfaces",
        "",
        "These are Home-adjacent feeds and notification surfaces. Profiles page 5 has its own guide tab.",
        "",
        "RSS",
        "  Private per-user RSS/Atom inbox.",
        "  Manage subscriptions in Settings > RSS.",
        "  Entries stay private until shared.",
        "  j / k or ↑ / ↓   navigate entries",
        "  Enter             copy selected entry URL",
        "  s                 share selected entry through News processing",
        "  d                 dismiss selected entry",
        "  r                 refresh RSS now",
        "  After sharing, the URL becomes a public News article and #lounge announcement.",
        "",
        "Cyberspace",
        "  cyberspace.online is a small, human social network like ours; late.sh",
        "  acts as your personal client for it. Everything happens as you, under",
        "  your own linked account; what you read there stays on your screen only.",
        "  Open it with /cs (alias /cyberspace). Once you link an account it gets",
        "  its own Home rail section, and /cs unlink takes it back off the rail.",
        "  The rail count is unread notifications plus entries published since your",
        "  last visit; the pane header splits it, and new entries carry a dot.",
        "  /cs link          link your cyberspace account (stores a token, never the password)",
        "  /cs post          publish an entry (also announced in #lounge)",
        "  /cs chat          their chat rooms (cIRC), to read and to pin into your rail",
        "  /cs mail          your c-mail conversations, pinned into the rail the same way",
        "  /cs mail @user    write to someone new: starts (or finds) the conversation,",
        "                    pins it, and walks you straight into it",
        "  /cs unlink        forget the link and token",
        "  j / k or ↑ / ↓   navigate the feed, or scroll an open entry",
        "  g                 back to the top of the feed, entry, or notifications",
        "  Enter             open the selected entry with its replies",
        "  r                 refresh (feed) / reply (open entry)",
        "  c                 copy a link to the selected entry",
        "  n then Enter      open the entry a notification is about",
        "  p                 new entry, n notifications, b back",
        "  /cs chat opens their chat room picker: Enter adds a room to your",
        "  cyberspace rail section as its own entry, Enter again removes it.",
        "  Inside a room: j/k scroll, g newest, i write, Enter send, b or Esc leave.",
        "  /cs chat and /cs mail also work from a room composer: they open the",
        "  picker over the room instead of sending the text to cyberspace.",
        "  A room is live only while you are inside it: nothing is fetched in the",
        "  background, and leaving takes you out of its user list on their side.",
        "",
        "Mentions",
        "  User-targeted notification feed for @user mentions.",
        "  Three ways in: pick Mentions in the Home (1) room rail, click the",
        "  \"N unread mentions\" counter in the top-right frame border, or",
        "  Ctrl+/ and type mentions.",
        "  Selecting Mentions marks it read.",
        "  j / k or ↑ / ↓   navigate notifications",
        "  Enter             preview the mention with surrounding messages; Enter again jumps",
        "  Rules             actor excluded; DMs notify participants; private rooms notify members",
        "  Game-room chat does not create Mentions feed notifications.",
        "",
        "Discover",
        "  Lists public topic rooms you have not joined.",
        "  Loads only when selected.",
        "  j / k or ↑ / ↓   navigate rooms",
        "  Enter             join selected public room",
        "  /                 filter the list by room name",
        "  s                 sort by recent activity or by member count",
        "",
        "Read-only profile modal",
        "  p                 open selected chat author's profile card",
        "  /profile [@user]  open your own profile card, or another user's",
        "  j / k, arrows     scroll",
        "  PageUp/PageDown   page",
        "  Esc / q           close",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn directory_help_lines() -> Vec<String> {
    [
        "Profiles",
        "",
        "Profiles page 5 is the people of late.sh: one row per person who shared a project or posted a work card, sorted by their latest activity. The detail panel (wide terminals) shows the whole person: bio, late.fetch, their work card, and every project.",
        "  5                 open Profiles",
        "  j / k or ↑ / ↓   move between people",
        "  h / l or ← / →   move the detail focus across the person's card and projects",
        "  PageUp/PageDown   jump rows",
        "  Enter / c         copy the focused item's link (public profile link, or project URL)",
        "  o                 open the selected person's profile card",
        "  i                 share a new project",
        "  w                 create/edit your work card",
        "  e                 edit the focused item (yours only)",
        "  d                 delete the focused item (yours only)",
        "  /                 toggle filter to only your row",
        "  s                 search people (matches usernames, cards, and projects)",
        "  Esc               cancel the active form or search",
        "",
        "Work cards",
        "  One card per user; creating again updates it and preserves its public w_ slug.",
        "  Public pages live at /profiles and /profiles/{slug}.",
        "  Detail panel       previews the public page: work fields, Bio, late.fetch, projects",
        "  Fields            headline, status, type, location, contact, links, skills, summary",
        "  Status            open, casual, or not-looking",
        "  Limits            headline 120 chars, contact 200 chars, summary 1000 chars",
        "  Links             http(s) only, max 6",
        "  Skills            max 12, max 24 chars each, lowercase, # stripped",
        "  Tab / Shift+Tab   cycle composer fields",
        "  Enter             submit",
        "  Ctrl+J            newline in summary",
        "",
        "Projects / Showcases",
        "  Public project links; separate from chat messages.",
        "  Detail panel       the focused project shows its full description; the rest stay compact",
        "  Fields            title, URL, tags, description",
        "  Required          title, http(s) URL, description",
        "  Limits            title 120 chars, description 800 chars",
        "  Tags              max 8, max 24 chars each, lowercase, # stripped",
        "  Tab / Shift+Tab   cycle composer fields",
        "  Enter             submit",
        "  Ctrl+J            newline in description",
        "",
        "Public web profile",
        "  /profiles         public index of work profiles; open first, then casual, then not-looking",
        "  /profiles/{slug}  detail page created from your work card's slug",
        "  Profile fields    headline, @username, status, work type, location, summary, skills, contact, links",
        "  Bio               comes from Settings > Bio; rendered as sanitized Markdown when non-empty",
        "  late.fetch        comes from identity settings: created date, theme, IDE, terminal, OS, languages",
        "  Showcases         all your projects appear below the profile when available",
        "                    each shows title, URL, description, tags, and links out to the project",
        "  Index cards       show headline, user/status, type/location, summary preview, and skills",
        "",
        "Read-only profile modal",
        "  p                 open selected chat author's profile card",
        "  /profile [@user]  open your own profile card, or another user's",
        "  j / k, arrows     scroll profile modal",
        "  PageUp/PageDown   page profile modal",
        "  Esc / q           close profile modal",
        "  Profiles show username, country, timezone/current time, chips, markdown bio,",
        "  bonsai, late.fetch fields, and the user's showcases when available.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn arcade_help_lines() -> Vec<String> {
    [
        "Arcade",
        "",
        "The Arcade is for single-player terminal games, daily puzzles, endless runs, and leaderboard play.",
        "  2                 open The Arcade",
        "  j / k or ↑ / ↓   browse games",
        "  Enter             play selected game",
        "  Esc / q           leave current game",
        "  `                 in a daily puzzle: hop games waiting on you (boards, tables, dailies, live dungeons)",
        "",
        "Notes",
        "  Game-specific controls appear inside the Arcade page.",
        "  Daily puzzle completions, run scores, chips, payouts, and leaderboards are covered in Economy.",
        "",
        "Leaderboard badges",
        "  Awarded each month to the previous month's top players. They show",
        "  first in your chat username badge stack, wrapped in brackets.",
        "  The trailing digit is your rank, 1-3 (so [AW1] is that month's #1).",
        "  [CHIP]    Top Chips",
        "  [AW]      Arcade Wins",
        "  [LA]      Lateris (Tetris)",
        "  [24#]     2048",
        "  [SN]      Snake",
        "  [CRWN]    The Crown, to whoever wore it when the month ended.",
        "            It is the one monthly badge with no rank digit: the crown has one holder.",
        "  The door badges are one-off feats, shown with no rank digit. The badge lands the first",
        "  time; the chips land again on the gate shown here. Full guide on the Leaderboards page.",
        "  [LMG]     Lateania Archdemon             10,000 chips  per character, 7-day gap",
        "  [LKN]     Lateania Frontier King         10,000 chips  per character, 7-day gap",
        "  [LYS]     Lateania Sundering Deep        20,000 chips  per character, 7-day gap",
        "  [LKA]     Lateania Kaethyr Ascendant     20,000 chips  per character, 7-day gap",
        "  [NHA]     NetHack Amulet                 20,000 chips  per run, 7-day gap",
        "  [NHY]     NetHack Ascension              50,000 chips  per run, 7-day gap",
        "  [DCO]     DCSS Orb of Zot                20,000 chips  per run, 7-day gap",
        "  [DCW]     DCSS Escape                    50,000 chips  per run, 7-day gap",
        "  [BRE]     Brogue Escape                  20,000 chips  per run, 7-day gap",
        "  [BRM]     Brogue Mastery                 50,000 chips  per run, 7-day gap",
        "  [GDS]     Green Dragon Slayer            10,000 chips  every kill",
        "  [ADE]     A Dark Room Escape             15,000 chips  every run",
        "  [ADB]     A Dark Room Homefleet          20,000 chips  every run",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn lobby_help_lines() -> Vec<String> {
    [
        "Lobby",
        "",
        "The Lobby (Ctrl+G) is the front door for multiplayer play: async daily matches plus the fixed house tables, with paired embedded chat.",
        "  Ctrl+G            open / close the Lobby",
        "  j / k or \u{2191} / \u{2193}   move through matches and house tables",
        "  Enter             claim / open a match, or sit at a house table",
        "  Esc               close the Lobby",
        "",
        "Daily matches",
        "  c / C             post an open or directed chess, chess960, battleship, connect4, reversi, checkers, backgammon, or briscola challenge",
        "  24h per move; boards live outside the Tab cycle, Esc returns to the Lobby",
        "  chess960 shuffles the back rank: same rules, and you castle by moving your king onto your own rook",
        "  briscola holds a hand: yours is drawn face up, theirs never is, and spectators see neither",
        "  `                 hop Home chat, boards on your move, seated tables, unfinished dailies, live door games (inside a roguelike, ` detaches and hops onward; inside Lateania it leaves with an autosave and keeps the door on the cycle for 5 minutes)",
        "",
        "House tables",
        "  Poker, Blackjack, Asterion, Tron, and Super Snake: one fixed table each, no setup forms",
        "  the Lobby row shows live occupancy; empty tables are always joinable",
        "  q / Esc           leave the table screen (your seat follows the game's rules)",
        "",
        "At a table",
        "  Layout            game on top, embedded game chat below",
        "  i                 compose in embedded chat",
        "  j / k             embedded-chat message selection unless game claims the key",
        "  PageUp/PageDown   scroll embedded chat",
        "  r/e/d/p/c/f       reply, edit, delete, profile, copy, react selected chat message",
        "  t                 translate the selected chat message",
        "  g                 jump to a reply's original even if it has an image",
        "  Arrows            game gets first chance; otherwise embedded chat handles them",
        "",
        "Economy",
        "  Economy tab        chips, stakes, payouts, and leaderboards.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn lateania_help_lines() -> Vec<String> {
    [
        "Lateania",
        "",
        "Lateania is the persistent BBS-style world, opened from the Games hub.",
        "  3                 open the Games hub, then select the Lateania card",
        "  Enter             step through the gate from the hub",
        "  d                 reset your Lateania character after confirmation",
        "  Esc               leave the active world back to the Games hub",
        "  ?                 open global guide from the hub or active game",
        "",
        "Rebels in the Sky",
        "  Pirate basketball across the galaxy, proxied live from frittura.org.",
        "  3 then Enter      open the Games hub, select Rebels, connect",
        "  Esc / Ctrl-C      quit the game; you return to the Games hub",
        "  Disconnecting (or the server closing) also returns to the hub.",
        "",
        "Lateania",
        "  w/s + Enter       choose your calling (1-9 quick-pick the first nine)",
        "  w/a/s/d or arrows move north/west/south/east",
        "  < / >             move up / down where exits exist",
        "  o                 look around",
        "  Space / Enter / x attack",
        "  1-9, 0            use ability slots 1-10 after choosing a class",
        "  v then Enter      cast any ability from the panel, however deep the roster",
        "  z                 flee combat",
        "",
        "Getting around",
        "  m                 world map: overhead, biome-coloured, only where you have been",
        "    wasd/arrows     pan the map; < > change level; Enter re-centres on you",
        "    markers         @ you, * a zone boss and its drops, a heart a tameable beast",
        "  i                 the ways: fast-travel between waystones, town to continent gates",
        "  r                 recall to Embergate when out of combat",
        "  ;                 retreat to the nearest safe haven when lost in a maze",
        "",
        "Panels",
        "  c                 character",
        "  v                 abilities",
        "  t                 inventory (Enter equips, or takes off what you wear)",
        "  b                 shop, when a merchant is present",
        "  j                 quest journal",
        "  k                 earned titles",
        "  f                 follow another adventurer in the room",
        "  y                 gather at a resource node",
        "  u                 craft where a station stands",
        "  q                 tame a wild beast where one roams",
        "  n                 housing ledger",
        "  '                 say to your room (local chat)",
        "  Enter             activate selected inventory/shop row",
        "  x                 sell selected inventory item at a shop",
        "",
        "Persistence",
        "  Your Lateania character is saved when you leave and periodically while present.",
        "  Press d on the Lateania card in the Games hub to reset and start over.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn overview_lines() -> Vec<String> {
    [
        "late.sh in one pass",
        "",
        "late.sh is a terminal clubhouse over SSH: chat, music, news, games, settings, and shared presence in one session.",
        "",
        "House rules",
        "  late.sh is 18+ throughout. Consensual adult content is allowed and",
        "  can appear in any room; images render inline, so you may see one",
        "  without opening it. Everyone shown must be an adult and must have",
        "  agreed to it being posted, and it comes down when they ask.",
        "  Terms: https://late.sh/terms. Reports: admin@dwarfforge.io.",
        "",
        "Primary screens",
        "  0 Clubhouse       the Late Lounge: walk around, everyone is live",
        "  1 Home            chat, music, and live activity",
        "  2 The Arcade      daily puzzles, endless games, quests at the top",
        "  3 Games           door games: Lateania, NetHack, DCSS, Brogue, Usurper,",
        "                    Green Dragon, A Dark Room, dopewars, CodeKeep, BashQuest, Rebels",
        "  4 Artboard        shared persistent ASCII canvas",
        "  5 Profiles        the people, one row each: their projects and work cards",
        "  6 Leaderboards    every board, monthly and all-time",
        "",
        "You land in the Clubhouse: hjkl/arrows walk, i talks (your words float",
        "over your head and land in #lounge), w waves, x dances, Enter interacts.",
        "",
        "The Games hub is a grouped sidebar: arrow keys or j/k move between its",
        "games; Enter launches the selected game. CodeKeep needs 108x24 inside the frame.",
        "Any screen too small to draw says so, naming the size it needs and the size you have.",
        "Inside a running roguelike, ` steps out while the game keeps going (a green",
        "dot marks it; ` or Enter on its card resumes). c on the NetHack or DCSS card",
        "opens the config paste box (.nethackrc / init.txt).",
        "",
        "Profiles has its own guide tab; Artboard keeps page-local editing help.",
        "There is also a dedicated Architecture slide if you need system-level context.",
        "",
        "Global keys",
        "  Tab / Shift+Tab   next / previous screen",
        "  0-6               jump straight to a screen",
        "  ?                 open this guide",
        "  q                 open quit confirm (press q again to leave)",
        "  Ctrl+O            open Settings",
        "  Ctrl+G            open / close the Lobby (daily games + house tables)",
        "  Ctrl+L            redraw the screen after outside terminal damage",
        "  /shop             open the Shop",
        "  /aquarium         toggle the Aquarium in the Lounge after unlocking it in the Shop",
        "  /aquarium feed    feed your Aquarium with bought Aquarium Food",
        "  Ctrl+/            jump to a room, DM, or Home entry; ?query searches messages",
        "  ?                 open this guide; Pair and terminal-specific tabs live here",
        "  w                 open Bonsai Care when not composing",
        "  /pet              toggle the pet strip in the Lounge",
        "  /pet feed/water   pet-strip care after unlocking the companion",
        "  m                 mute paired client",
        "  + / -             paired client volume",
        "  v then v          open the Music Booth (submit + queue + votes)",
        "  v then x          cycle audio source: Icecast → YouTube → Radio",
        "  v then s          skip-vote the current YouTube track",
        "  v then 1..5       select stream/station in the active source",
        "",
        "Home",
        "  click top bar     jump screens",
        "  click room rail   select room or synthetic entry",
        "  click unread HUD  jump to Mentions",
        "",
        "Room favorites",
        "  f                 favorite / unfavorite the selected room",
        "  [ / ]             move the selected favorite up / down",
        "  favorites appear first in the room rail and room picker",
        "  `                 hop Home chat and games waiting on you (boards, tables, dailies)",
        "",
        "Shop",
        "  /shop             open the Shop modal from any composer",
        "  Shop              j/k select, h/l subtab, Enter buy with Late Chips",
        "  name effects      Name Glow / Gradient / Shimmer sell by the day or by the month",
        "  badges and flags  rented for 24h or 30 days; a rebuy replaces the live one",
        "  title             your own words after your name in chat, 20 characters, rented the same way",
        "                    and screened before you are charged",
        "  burn milestones   Ultimates tab: permanent glyphs at 50,000 / 150,000 / 500,000,",
        "                    worn on top of a rented badge and flag, dearest one showing",
        "  Economy tab       chips, payouts, leaderboards, Arcade, table games",
        "",
        "Jump search",
        "  Ctrl+/            open / close jump modal",
        "  type              filter rooms, DMs, RSS, News, Cyberspace, Voice, Mentions, Discover",
        "  @query / #query   bias toward users or rooms",
        "  ↑/↓ or Ctrl+K/J   move selection",
        "  PageUp/PageDown   jump 8 rows",
        "  Backspace         delete query char",
        "  Ctrl+Backspace    delete query word",
        "  Enter             jump to selected destination",
        "  Esc               close",
        "",
        "Message search (inside the jump modal)",
        "  ?query            search messages in every room you're in",
        "  ?#room query      search one room; ?@user query searches a DM",
        "  ↑/↓               browse hits; the pane below shows the message with",
        "                    4 messages of surrounding conversation",
        "  Enter             jump to the hit's room and select it when loaded",
        "  Ctrl+Y            copy the selected hit's text",
        "  /search [query]   open message search from the composer",
        "",
                "This modal",
        "  Tab / Shift+Tab   next / previous tab",
        "  j / k / ↑ / ↓     scroll current tab",
        "  ? / Esc / q       close",
        "",
        "Use /binds in chat if you want to jump directly to this slide from the composer.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn architecture_lines() -> Vec<String> {
    [
        "Architecture",
        "",
        "late.sh is a Rust workspace with four crates: late-cli, late-core, late-ssh, and late-web.",
        "",
        "What runs where",
        "  late-ssh          main SSH/TUI runtime",
        "  late-web          browser web UI and pairing flows",
        "  late-core         shared models, database access, infrastructure",
        "  late-cli          local CLI companion for audio playback and controls",
        "",
        "State and persistence",
        "  PostgreSQL stores users, chat, profiles, social feeds, game rooms, chips, and leaderboard data",
        "  services publish watch snapshots and broadcast events into SSH sessions",
        "",
        "Audio stack",
        "  Icecast has chill and classical house streams",
        "  Radio has Nightride guest stations",
        "  Liquidsoap manages the house playlists",
        "  the paired CLI plays audio locally; late.sh/listen plays the same sources in a browser",
        "",
        "User-facing areas",
        "  Home/Dashboard with chat rail, The Arcade, Games (door-game hub: Lateania, the roguelikes, the BBS doors, CodeKeep, Rebels), Artboard, Profiles, and the persistent bonsai sidebar",
        "  Home chat includes synthetic entries: RSS, News, Cyberspace, Voice, Mentions, Discover; Profiles owns the projects and work-card feed",
        "  The Lobby fronts daily matches (DB rows) and fixed house tables with chat_rooms(kind='game')",
        "  House-table runtime state is process-local and can reset on SSH server restart",
        "",
        "Important characteristics",
        "  terminal-first, always-on, social, and zero-signup",
        "  SSH keys are identity/device anchors; linked keys can point to one late.sh identity",
        "",
        "Highest-risk runtime areas are render-loop backpressure, chat sync consistency, connection limiting, and paired-client state drift.",
        "",
        "The project is source-available under FSL-1.1-MIT, converting to MIT after two years.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn news_help_lines() -> Vec<String> {
    // The payout is the one thing people ask about twice, so it leads rather
    // than sitting in a footnote. The amount comes from the constant the
    // service pays out, never from a copy of it.
    let mut lines = vec![
        "News processing".to_string(),
        "".to_string(),
        "The News room is a shared feed for links worth keeping around. It is built for URL drop-ins, AI summaries, and quick scanning from the terminal.".to_string(),
        "".to_string(),
        "What it pays".to_string(),
        format!(
            "  Sharing a link pays you {NEWS_SHARE_REWARD_CHIPS} chips, from the composer here or with s in your RSS inbox."
        ),
        "  A link already in News cannot be shared again, so only the first sharer is paid.".to_string(),
        "  You are paid once per link: deleting your own story and re-sharing it pays nothing.".to_string(),
        format!("  At most {NEWS_SHARE_MAX_PAID_PER_DAY} shares a day (UTC) are paid; the rest still publish, for nothing."),
        "  Full chip rules live in the Chips tab.".to_string(),
        "".to_string(),
    ];
    lines.extend(
        [
            "How it works",
            "  i                 start the URL composer",
            "  Enter             copy selected link",
            "  Enter in composer submit link",
            "  Esc               cancel URL entry",
            "  j / k             browse stories",
            "  d                 delete your own story",
            "  /                 toggle filter to only your stories",
            "  Enter on news msg open the news item modal",
            "  Enter in modal    copy link and close",
            "  N in modal        jump to News with story selected",
            "",
            "What happens after submit",
            "  1. late.sh fetches the article or video page",
            "  2. AI extracts a compact summary",
            "  3. ASCII art / preview is generated when possible",
            "  4. the story lands in the shared feed for everyone",
            "  5. the chips land in your balance",
            "",
            "Good inputs",
            "  tech articles, launch posts, docs, YouTube links, tweets/x links",
            "  private RSS/Atom entries from the RSS room when you press s there",
            "",
            "RSS relationship",
            "  RSS is a private inbox in the Home room rail.",
            "  RSS/Atom subscriptions are managed in Settings > RSS.",
            "  Sharing an RSS entry sends its URL through this News pipeline.",
            "  Only shared entries become public News articles and #lounge announcements.",
            "",
            "Notes",
            "  summaries are intentionally compact for terminal reading",
            "  thumbnails only render when they fit the layout",
            "  the room acts like a curated backlog, not high-speed chat",
        ]
        .into_iter()
        .map(str::to_string),
    );
    lines
}

fn settings_help_lines() -> Vec<String> {
    let graybeard_mention_cooldown_sec = GRAYBEARD_MENTION_COOLDOWN.as_secs();

    vec![
        "Settings and identity".to_string(),
        "".to_string(),
        "Your identity and preferences live in the settings modal.".to_string(),
        "".to_string(),
        "Tabs".to_string(),
        "  Settings          username, late.fetch fields, country, timezone, notifications, layout toggles"
            .to_string(),
        "  Bio               multiline markdown bio".to_string(),
        "  Themes            expanded theme browser; / searches it, f stars a theme into Favorites"
            .to_string(),
        "  Tweaks            power-user toggles for appearance, compose, music, display, and startup"
            .to_string(),
        "  Account           link SSH keys across accounts, reset/revoke your IRC access token, or delete your account"
            .to_string(),
        "  RSS               private RSS/Atom subscriptions".to_string(),
        "".to_string(),
        "What you can set".to_string(),
        "  username".to_string(),
        "  theme and terminal background sync".to_string(),
        "  notifications, bell, cooldown, notification format".to_string(),
        "  multiline bio".to_string(),
        "  country via picker, with Unicode flag rendering".to_string(),
        "  timezone via picker".to_string(),
        "  IDE, terminal, OS, and languages for profile/late.fetch surfaces".to_string(),
        "  Tweaks: terminal background sync, text brightness, right sidebar mode, room list, pet strip, composer send behavior, music mute-on-start, chat flag fallback, land on Home"
            .to_string(),
        "  private RSS/Atom subscriptions".to_string(),
        "  IRC access token for external IRC clients".to_string(),
        "".to_string(),
        "How to open it".to_string(),
        "  on login, the settings modal opens automatically".to_string(),
        "  press Ctrl+O anywhere in the app".to_string(),
        "  or use /settings from chat".to_string(),
        "".to_string(),
        "Modal controls".to_string(),
        "  Tab / Shift+Tab switch settings tabs".to_string(),
        "  j / k or arrows move rows".to_string(),
        "  Left / Right cycle option rows".to_string(),
        "  Enter / e edit text or open pickers".to_string(),
        "  Space quick-cycles simple toggles".to_string(),
        "  Pickers: type to filter, Enter pick, Esc cancel".to_string(),
        "  Custom sidebar: Enter on Custom opens the three-page checklist".to_string(),
        "  Account: Enter opens Link Accounts or Delete Account".to_string(),
        "  ? opens this guide; Esc / q closes".to_string(),
        "".to_string(),
        "Account linking".to_string(),
        "  Use Settings > Account > Link Accounts when two SSH keys created separate late.sh accounts.".to_string(),
        "  Open Link Accounts on both accounts; one side generates a 10-minute link code.".to_string(),
        "  Enter the other account's code to preview its username and created date.".to_string(),
        "  Choose the main account to keep: Current or Other.".to_string(),
        "  Type the main username exactly, then press Enter to link.".to_string(),
        "  Both SSH keys will open the main account after linking.".to_string(),
        "  The other account is abandoned; chips, messages, scores, streaks, settings, and other data are not merged.".to_string(),
        "  Linking is unavailable while either account has an active ban.".to_string(),
        "".to_string(),
        "Account deletion".to_string(),
        "  Settings > Account > Delete Account opens delete confirmation; type DELETE to confirm".to_string(),
        "".to_string(),
        "Tweaks tab".to_string(),
        "  Power-user toggles, grouped by area:".to_string(),
        "  Appearance".to_string(),
        "    Sync terminal background  paint your terminal's background to match the theme; off (or the Terminal theme) leaves your terminal's own background alone. Selections and highlights inside the app are part of the theme itself, not this toggle"
            .to_string(),
        "    Text Brightness         nudge overall text brightness up or down".to_string(),
        "    Right sidebar           on / off / auto for Home and Arcade; Enter opens a panel checklist"
            .to_string(),
        "    Room list               on / off / auto for the Home room-list rail".to_string(),
        "                            auto hides a rail on terminals too narrow to carry it, so one"
            .to_string(),
        "                            account works on both a desktop and a phone".to_string(),
        "                            both rows apply to this device (this SSH key) only, never the"
            .to_string(),
        "                            account default; `\\` on Home cycles the same two".to_string(),
        "    Pet companion strip     show/hide the pet strip above the Lounge chat composer (pet owners only)"
            .to_string(),
        "  Compose".to_string(),
        "    Send and keep open on Enter   Enter sends without closing the composer; while on, Alt+S becomes a no-op"
            .to_string(),
        "  Display".to_string(),
        "    Chat flag text fallback       show text/boxed-letter labels instead of flag emoji in chat badges and Shop Flags"
            .to_string(),
        "  Startup".to_string(),
        "    Land on Home page             land on Home (page 1) instead of the Clubhouse (page 0) when a session starts"
            .to_string(),
        "".to_string(),
        "RSS tab".to_string(),
        "  j / k or arrows move through RSS rows".to_string(),
        "  Enter / a on the add row starts URL input".to_string(),
        "  d / Delete removes the selected RSS source".to_string(),
        "  r refreshes RSS".to_string(),
        "  RSS/Atom URLs must be http(s) and are capped at 2000 chars".to_string(),
        "".to_string(),
        "Why country matters".to_string(),
        "".to_string(),
        "The saved ISO country code belongs to profile/settings identity surfaces; equipped chat flags come from the Shop (/shop)."
            .to_string(),
        "".to_string(),
        "Notifications".to_string(),
        "".to_string(),
        "Terminal notifications run through OSC 777 / OSC 9.".to_string(),
        "Best support today: kitty, Ghostty, rxvt-unicode, foot, wezterm, konsole, and iTerm2."
            .to_string(),
        "tmux strips notification escapes by default; see the Notifications tab for passthrough setup."
            .to_string(),
        "Notifications can fire for DMs, mentions, friend joins, game events, and streams (a friend going live, or someone opening yours).".to_string(),
        "Bell and cooldown decide how loud and how often they show up.".to_string(),
        "".to_string(),
        "Native CLI config file".to_string(),
        "".to_string(),
        "These in-app settings are separate from the native `late` CLI's own config.".to_string(),
        "The CLI config is explicit and optional: nothing is ever created for you, and the CLI runs fine with no file at all.".to_string(),
        "  Path              $XDG_CONFIG_HOME/late/config.toml, or ~/.config/late/config.toml".to_string(),
        "  Override          run `late --config <path>` to point at a different file".to_string(),
        "  Missing file      silently ignored; built-in defaults apply".to_string(),
        "Precedence, lowest to highest: built-in defaults, then the config file, then LATE_* env vars, then CLI flags. A later layer wins.".to_string(),
        "Flat TOML keys mirror the flags: ssh-target, ssh-port, ssh-user, ssh-mode, key, audio-base-url, api-base-url, audio-output-device, verbose.".to_string(),
        "  Example           ssh-target = \"late.example\"".to_string(),
        "  Note              sections like [foo] are rejected; it is a flat key = value file.".to_string(),
        "".to_string(),
        "@bot".to_string(),
        "".to_string(),
        "@bot is the app's AI helper in chat.".to_string(),
        "Mention replies are rate-limited with a 30s cooldown per user.".to_string(),
        "It answers questions about late.sh, product positioning, and high-level architecture."
            .to_string(),
        "It sees recent room history plus compact context about online non-bot members in the active room."
            .to_string(),
        "The exact model depends on the current server configuration.".to_string(),
        "".to_string(),
        "@graybeard".to_string(),
        "".to_string(),
        "Burned-out senior who still shows up to heckle modern software.".to_string(),
        "Only replies when mentioned.".to_string(),
        format!("Replies on mention with a {graybeard_mention_cooldown_sec}s cooldown."),
    ]
}

fn voice_help_lines() -> Vec<String> {
    [
        "Voice rooms",
        "",
        "Voice is live talk attached to a room, backed by LiveKit. It is not a separate call screen: voice rides whatever room you are already in, so you keep chatting, playing, or browsing while connected.",
        "",
        "Where voice shows up",
        "  A two-line voice strip sits at the top of any voice-enabled room: who is connected on top, controls below.",
        "  DMs, private rooms, and game rooms have voice enabled by default.",
        "  Public rooms stay voice-off until a staffer turns them on.",
        "  When voice is off on the server or in this room, the strip says so and no one can join.",
        "  Stream rooms (`/golive`) use the same voice channel; while the stream is on air the strip shows ⦿ ON AIR and joining asks for one extra Ctrl+V confirm, because voice there is audible to anonymous watch-page viewers.",
        "",
        "Joining and controls",
        "  Ctrl+V            join the room's voice, switch to it if you are in another, or leave when already in this one",
        "  Ctrl+T            mute / unmute your microphone",
        "  /voice            same as Ctrl+V from the composer",
        "  /mute             same as Ctrl+T from the composer",
        "  You always join muted; unmute with Ctrl+T when you want to talk.",
        "  Deafen exists in the protocol but has no in-app shortcut yet.",
        "  Artboard keeps Ctrl+V / Ctrl+T for its own editing, so voice chords are ignored there.",
        "",
        "Reading the roster",
        "  🟢 speaking       mic on and currently talking (name turns green)",
        "  ⚪ listening      joined, mic on, silent",
        "  🔇 muted          mic off",
        "  🔕 deafened       not hearing the room",
        "  Your own name is always amber so you can spot yourself.",
        "",
        "Top-right badge",
        "  While you are connected to any voice room, a `mic <room> [status]` badge shows in the top chrome.",
        "  It follows you across screens so you always know you are still live and where.",
        "",
        "Staying connected",
        "  Entering a DM, private room, or game does not auto-join voice; joining is always an explicit Ctrl+V.",
        "  Once joined you stay in voice across room, screen, and game navigation.",
        "  You leave only when you press Ctrl+V to leave, switch to another voice room, disconnect the native CLI, go stale, or a moderator removes you.",
        "",
        "What you need to join",
        "  Voice media runs in the native `late` CLI, so install it and run `late` (see the Pair tab).",
        "  Supported for joining on Linux, macOS, and Windows.",
        "  Raw `ssh late.sh` sessions can see the roster and badge but cannot join or listen yet.",
        "  If no capable CLI is paired, the strip prompts you to run the native late CLI.",
        "",
        "How it works under the hood",
        "  LiveKit carries the actual audio; late.sh never relays voice media through SSH or the music stack.",
        "  late.sh only mints a short-lived LiveKit token per join and tracks who is connected, muted, or speaking for the roster.",
        "  The native CLI captures your mic and plays back the room over LiveKit, and reports its state back so the TUI roster stays in sync.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn streaming_help_lines() -> Vec<String> {
    [
        "Streaming",
        "",
        "/golive puts you live in your own stream room: viewers watch in a browser, chat rides the room, and the room's voice channel is the stream's voice. One stream per account.",
        "",
        "Two ways to publish",
        "  /golive [title]    stream your screen from a browser publisher page;",
        "                     late.sh opens it via your paired CLI, or shows a QR",
        "  /golive obs [..]   stream from OBS over WHIP; shows the server URL and",
        "                     bearer token to paste into OBS",
        "  The two kinds do not mix: to switch, /golive stop first, then start the other.",
        "  Rerunning /golive obs while set up shows the same credentials again.",
        "  /golive stop       end your stream (either kind)",
        "  Either handoff box stays up until you press Esc, so a stray key cannot",
        "  take the URL or the token off your screen while you copy it.",
        "",
        "Watching",
        "  /watch @user       open someone's live stream (browser via paired CLI, else QR)",
        "  Live streams also show under the room rail's `stream` section, and an",
        "  `is live` line hits #lounge once media flows.",
        "  Watch pages are born silent: nothing is audible until the viewer turns sound on.",
        "  When a friend goes live you get a banner and a desktop notification.",
        "",
        "Your audience",
        "  The first time a late.sh user opens your stream, by /watch or by walking",
        "  into your stream room, you get an `is watching` banner and #lounge gets",
        "  an `is watching` line.",
        "  Both stream notifications share one settings row, `Streams`; banners",
        "  show either way.",
        "  Browser viewers who only have the link stay anonymous: they show up in",
        "  the header's watcher count and nowhere else.",
        "",
        "OBS setup: Settings > Stream",
        "  Service           WHIP",
        "  Server            the WHIP URL from /golive obs",
        "  Bearer Token      the token from /golive obs",
        "",
        "OBS setup: Settings > Output",
        "  Video Encoder     H.264: hardware if you have it (NVENC, AMF, VAAPI,",
        "                    Apple VT), otherwise x264",
        "  Audio Encoder     Opus, required: WHIP cannot carry AAC",
        "  Bitrate           whatever your upload handles; 4000-8000 Kbps is plenty",
        "  The server forwards your encoding untouched (no re-encode), so what you",
        "  send is exactly what viewers get.",
        "",
        "If OBS says \"at least one video or audio encoder is not set\"",
        "  Switching Service to WHIP resets any encoder it cannot carry, including",
        "  the Recording ones, and OBS refuses to save while any output has none.",
        "  Simple output mode: set Recording Quality to \"Same as stream\".",
        "  Advanced output mode: check the Recording tab and pick its encoders",
        "  (\"Use stream encoder\" is fine).",
        "  Still complaining with everything set? Restart OBS: older versions do",
        "  not rebind encoders after the service switch until a restart.",
        "",
        "Credentials",
        "  The WHIP URL and bearer token are minted per stream and die with it:",
        "  /golive stop (or the stream ending) invalidates them, and the next",
        "  /golive obs mints a fresh pair to paste in.",
        "",
        "On air and voice",
        "  While you publish, the stream room's voice strip shows ⦿ ON AIR.",
        "  Voice in a stream room is audible to watch-page viewers, and an OBS",
        "  stream counts as on air the whole time it publishes, since program",
        "  audio can carry a microphone.",
        "",
        "Disconnects",
        "  OBS dropping does not kill the stream instantly: after ~30 seconds",
        "  without media it falls to a reconnect grace, and ends ~30 seconds",
        "  later if OBS has not come back. OBS's automatic reconnect picks the",
        "  stream back up within that window.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn bonsai_help_lines() -> Vec<String> {
    [
        "Dynamic Bonsai",
        "",
        "Dynamic Bonsai is the living tree. It is not a fixed ladder of pictures: it keeps a real branch graph, and every choice you make is remembered in how it grows next. Water it, steer the tips, cut your mistakes, and pinch foliage, and the silhouette becomes a record of how you tended it.",
        "",
        "Unlock it in the Shop (/shop) for 1000 chips. While it is equipped, w opens Dynamic Bonsai instead of classic Bonsai; clear the slot to switch back.",
        "",
        "Controls",
        "  w                 water, or replant when the tree has died",
        "  tab / n           select the next live tip",
        "  shift-tab         select the previous live tip",
        "  wheel             scroll-select tips with the mouse",
        "  ←↓↑→ / hjkl       steer the selected tip's future growth",
        "  x                 cut the selected branch and everything above it",
        "  p                 pinch the selected tip toward a leaf pad",
        "  s                 split the selected tip on the next growth",
        "  c                 copy the tree to clipboard",
        "  ?                 open this guide",
        "  q / Esc           close",
        "",
        "The two meters",
        "  vigor             growth strength 0-100; high vigor grows wider, tidier waves",
        "  stress            dry-neglect pressure 0-120; high stress narrows and wilds growth",
        "  watering          +vigor, big -stress, and an immediate growth wave",
        "  a dry day         +stress, -vigor, and messier side shoots",
        "  status line       shows Day, vigor, stress, and mode at a glance",
        "",
        "Watering",
        "  w waters once per UTC day: +18 vigor, -35 stress, and a fresh growth wave.",
        "  It earns the same 200 chips as classic watering, once per day.",
        "  Skip days and stress climbs while vigor falls.",
        "",
        "Selecting a tip",
        "  tab / n and shift-tab cycle only the live tips: the branch ends that can still grow.",
        "  The trunk is never selectable; structure branches are skipped until they become tips.",
        "  Steering, pinching, and splitting all act on the selected tip.",
        "",
        "Steering (wiring)",
        "  Arrows or hjkl lean the selected tip: h/← left, l/→ right, k/↑ reach up, j/↓ droop down.",
        "  Wiring does not move the branch now. It biases where this tip grows next, and new growth keeps the lean.",
        "  Press a direction again to bias harder. A downward wire makes a drooping, cascade look.",
        "  Only live tips wire; pinched, leaf, and dead wood will not.",
        "",
        "Cutting",
        "  x removes the selected branch and every branch above it, cleanly, with no scar.",
        "  Cut where you want the shape to stop; growth resumes from the tips you keep.",
        "  The trunk cannot be cut. Cutting costs a little vigor.",
        "",
        "Pinching into leaf pads",
        "  p pinches the selected tip so it stops extending and stays compact.",
        "  Pinch the same spot three times, each over a separate growth wave, to set a leaf pad of dense foliage.",
        "  After a pinch, wait for the tip to read \"ready to pinch\" again before the next one counts.",
        "  Leaf pads carry the most canopy weight, so pinched tips are how you build a full crown.",
        "",
        "Splitting",
        "  s marks the selected tip to fork into two on the next growth wave.",
        "  It only forks when both new tips have open space; otherwise the mark waits.",
        "  Split-marked tips grow first in the wave. Splits build structure on purpose instead of waiting for random side shoots.",
        "",
        "How a growth wave works",
        "  Growth comes in waves, not one tip at a time: split-marked tips first, then your selected tip, then a spread of other live tips.",
        "  Watering grows the widest wave; high vigor widens it; stress narrows it.",
        "  Healthy growth reaches up and stays tidy; dry, stressed growth throws messy sideways shoots.",
        "  It also creeps a little on its own while you stay connected, as long as vigor is high enough.",
        "  The graph caps at 96 branches total; once a tree reaches that size, growth quietly stops adding new ones, though you can still steer, pinch, cut, and split what's already there.",
        "",
        "When it dies",
        "  Dynamic Bonsai only dies when stress maxes out and vigor hits zero at the same time, so it stays recoverable-but-ugly before then.",
        "  Weak tips harden into grey deadwood.",
        "  The first w after death replants a fresh seedling; water again the next day to feed it.",
        "",
        "Reading the tree",
        "  amber wood        live branches and trunk",
        "  green foliage     leaf pads and a healthy canopy",
        "  bright tip        just pinched, still setting",
        "  green tip         ready to pinch again",
        "  grey and faint    deadwood, or a tree that has died",
        "  dry leaves        the canopy browns out when stress is high",
        "  The sidebar preview is a compact silhouette; denser foliage reads as * and #.",
        "",
        "The chat badge",
        "  Your chat glyph is earned from the live tree: branch length plus leaf-pad weight, scaled by health.",
        "  Ladder: · ⚘ 🌱 🌲 🌳 🌸 🌼.",
        "  Neglect lowers the score, so a big tangled mess is not automatically prestigious. A dead tree shows no badge.",
        "",
        "────────────────────────────────────────",
        "",
        "Classic Bonsai and companions",
        "",
        "Classic Bonsai is the default tree until you equip Dynamic Bonsai. It is your slow-burn presence artifact: it grows while you keep showing up, and its state is persistent.",
        "",
        "Bonsai controls",
        "  w                 open Bonsai Care when not composing",
        "  w                 water or replant inside Bonsai Care",
        "  hjkl / arrows     move the pruning cursor",
        "  x                 cut the branch under the cursor",
        "  p                 prune hard: -1 stage, new shape",
        "  s                 copy the bonsai to clipboard",
        "  ?                 open this help section",
        "  Esc / q           close Bonsai Care",
        "",
        "How growth works",
        "  watering gives +10 growth (+5 streak) and 200 chips once per UTC day",
        "  it also grows slowly while connected",
        "  after 7 dry days it dies",
        "  missed daily branch cuts cost -10 growth once",
        "  cutting the wrong spot costs -10 growth immediately",
        "  cutting all wrong branches preserves the current shape",
        "  daily care is water plus the listed overgrown branches",
        "",
        "Stages",
        "  0-99              Seed",
        "  100-199           Sprout",
        "  200-299           Sapling",
        "  300-399           Young Tree",
        "  400-499           Mature",
        "  500-599           Ancient",
        "  600-700           Blossom",
        "",
        "Why it matters",
        "  it gives the app a calm personal loop outside chat and games",
        "  the tree becomes a little signature of how you inhabit late.sh over time",
        "  the bonsai stage is one part of the chat username badge stack",
        "",
        "Pet Companion",
        "  Unlock            Shop companion bought with Late Chips (/shop)",
        "  strip             lives above the Lounge composer once unlocked",
        "  /pet              show or hide the strip",
        "  /pet feed         spend one pet food, or click the bowl or the pet",
        "  once fed it strolls the screen for 30 minutes; one meal per day",
        "  /pet water        water (daily), or click the water bowl",
        "  pet food is a Shop item; an empty bowl shows ? when you are out",
        "  /petname [name]   show or set your pet's name",
        "  3-day care streak keeps it happy",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

const MUSIC_PAIR_TEXT: &str = "\
Music controls

late.sh has three music sources:

  Icecast    24/7 house radio with chill and classical streams.
  YouTube    a shared queue everyone can submit links to.
  Radio      direct Nightride guest stations.

Your paired client plays the selected source. Use v then 1..5 to select a stream or station inside the active source.

Plain stream, no pairing:
  vlc https://late.sh/stream
  mpv https://late.sh/stream

Direct stream playback is Icecast only. Pair the CLI or browser for source switching, mute/volume keys, visualizer sync, or the shared YouTube queue.

No sound from the paired CLI on Linux?
  The CLI plays audio through ALSA. On a PipeWire system with no ALSA compatibility layer, it finds no output device.
  Install pipewire-alsa (Arch: pacman -S pipewire-alsa, Debian/Ubuntu: apt install pipewire-alsa) and reconnect.

Now playing on your desktop (Linux)
  The paired CLI publishes the current track over MPRIS, the D-Bus standard your desktop already uses for media players.
  GNOME's top bar, KDE's tray, lock screens, and panel applets pick it up on their own. There is nothing to switch on: it appears once `late` is running and paired.
  Every source reports title and artist. YouTube tracks add duration, the video thumbnail, and a watch link; Icecast adds track length.
  Play/pause from the widget, or your keyboard's media keys, mutes and unmutes the paired client, the same as pressing m here. The volume slider works too.
  The controls travel through the server to every paired player, so they cover all sources, YouTube included, and this terminal always agrees with the widget.
  Machines with no session bus (headless boxes, containers, some WSL setups) simply get nothing. Audio and everything else carry on as normal.

Global keys (work anywhere)
  ?                open this guide, including Pair and terminal-specific tabs
  m                 mute paired client
  + / -             volume up / down

Select stream or station
  Icecast active: v then 1 / 2 selects chill / classical
  Radio active:   v then 1..5 selects Chillsynth / Nightride / Datawave / Spacesynth / Ambient

Swap which source you hear
  v then x          cycle your paired client through Icecast → YouTube → Radio. Your choice is saved per-user, so a refresh keeps it.

Music Booth (v then v)

  Opens a modal with a URL submit row on top and Queue/History below.

  Tab               switch focus between submit, queue, and history
  [ or ]            switch Queue / History
  Esc               close

  Submit focus:
    type            paste or type a YouTube URL
    Enter           submit
    ↓ or Ctrl+J     drop into the queue
    Backspace       delete char

  Queue focus:
    ↑ / ↓ or Ctrl+K/J
                     move selection
    PageUp/PageDown jump 8 rows
    + or =          upvote selected item
    - or _          downvote selected item
    0               clear your vote
    s               skip-vote the currently playing track
    d               delete your own queued item
    ↑ at the top    back to the submit row

  History focus:
    ↑ / ↓ or Ctrl+K/J
                     move selection
    PageUp/PageDown jump 8 rows
    /               filter the list
    Enter           queue selected track fresh
    d               delete selected track (staff)

  The queue is ordered by score, so upvotes pull tracks toward the front. You can't vote on the track that's already playing, but you can skip-vote it.
  History keeps up to 200 unique played tracks, most recently played first, so whatever is playing right now sits at the top. There are no history votes. Requeued history tracks start with 0 live queue votes.

Skip the current track
  v then s          add your vote to skip. The track skips once enough active YouTube-source users agree.
  s                 same thing, while you're in the booth queue.

Track length

  Every track is capped at 1 hour. Shorter videos play to their real end; anything longer (long mixes, live streams, the YouTube fallback) gets cut off at the 1h mark and the queue moves on.";

#[cfg(test)]
#[path = "data_test.rs"]
mod data_test;
