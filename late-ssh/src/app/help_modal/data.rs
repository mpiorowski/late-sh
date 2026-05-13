use crate::app::ai::ghost::GRAYBEARD_MENTION_COOLDOWN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpTopic {
    Overview,
    Architecture,
    Chat,
    Social,
    Music,
    News,
    Arcade,
    Rooms,
    Artboard,
    Bonsai,
    Settings,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 11] = [
        HelpTopic::Overview,
        HelpTopic::Chat,
        HelpTopic::Social,
        HelpTopic::Music,
        HelpTopic::News,
        HelpTopic::Arcade,
        HelpTopic::Rooms,
        HelpTopic::Artboard,
        HelpTopic::Bonsai,
        HelpTopic::Settings,
        HelpTopic::Architecture,
    ];

    pub fn title(self) -> &'static str {
        match self {
            HelpTopic::Overview => "Overview",
            HelpTopic::Architecture => "Architecture",
            HelpTopic::Chat => "Chat",
            HelpTopic::Social => "Social",
            HelpTopic::Music => "Music",
            HelpTopic::News => "News",
            HelpTopic::Arcade => "Arcade",
            HelpTopic::Rooms => "Rooms",
            HelpTopic::Artboard => "Artboard",
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Settings => "Settings",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            HelpTopic::Overview => "Overview",
            HelpTopic::Architecture => "Arch",
            HelpTopic::Chat => "Chat",
            HelpTopic::Social => "Social",
            HelpTopic::Music => "Music",
            HelpTopic::News => "News",
            HelpTopic::Arcade => "Arcade",
            HelpTopic::Rooms => "Rooms",
            HelpTopic::Artboard => "Art",
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Settings => "Settings",
        }
    }

    pub fn index(self) -> usize {
        match self {
            HelpTopic::Overview => 0,
            HelpTopic::Chat => 1,
            HelpTopic::Social => 2,
            HelpTopic::Music => 3,
            HelpTopic::News => 4,
            HelpTopic::Arcade => 5,
            HelpTopic::Rooms => 6,
            HelpTopic::Artboard => 7,
            HelpTopic::Bonsai => 8,
            HelpTopic::Settings => 9,
            HelpTopic::Architecture => 10,
        }
    }
}

pub fn lines_for(topic: HelpTopic) -> Vec<String> {
    match topic {
        HelpTopic::Overview => overview_lines(),
        HelpTopic::Architecture => architecture_lines(),
        HelpTopic::Chat => chat_help_lines(),
        HelpTopic::Social => social_help_lines(),
        HelpTopic::Music => music_help_lines(),
        HelpTopic::News => news_help_lines(),
        HelpTopic::Arcade => arcade_help_lines(),
        HelpTopic::Rooms => rooms_help_lines(),
        HelpTopic::Artboard => artboard_help_lines(),
        HelpTopic::Bonsai => bonsai_help_lines(),
        HelpTopic::Settings => settings_help_lines(),
    }
}

pub fn bot_app_context() -> String {
    let mut out = String::from(
        "APP CONTEXT:\n\
        CRITICAL FACTS:\n\
        - The glyph/icon next to a chat username is only the user's bonsai stage/state. It is not a country flag or custom contributor icon.\n\
        - There is no separate top-level Chat screen. Home/Dashboard owns the chat room rail and chat center; top-level screens are Home, The Arcade, Rooms, and Artboard.\n",
    );
    for topic in HelpTopic::ALL {
        out.push_str(&format!("## {}\n", topic.title()));
        for line in lines_for(topic) {
            if line.trim().is_empty() {
                continue;
            }
            out.push_str("- ");
            out.push_str(line.trim());
            out.push('\n');
        }
    }
    out
}

pub fn chat_help_lines() -> Vec<String> {
    [
        "Commands",
        "  /binds             open this guide",
        "  /public #room      open/create opt-in public room",
        "  /private #room     create a private room",
        "  /invite @user      add a user to the current room",
        "  /leave             leave the current room",
        "  /dm @user          open a direct message",
        "  /active            list active users",
        "  /members           list users in this room",
        "  /list              list public rooms",
        "  /paste-image       upload image from paired CLI clipboard",
        "  /ignore [@user]    ignore a user, or list ignored users",
        "  /unignore [@user]  remove a user from your ignore list",
        "  /music             explain how music works",
        "  /settings          open your settings modal",
        "  /exit              open quit confirm",
        "  Ctrl+O             open your settings modal anywhere",
        "  Ctrl+G             open Hub",
        "  Ctrl+/             search and jump to a room or DM",
        "",
        "Messages",
        "  j / k              select older / newer message",
        "  ↑ / ↓              same as j / k",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  half page up / down",
        "  End                jump to most recent",
        "  g / G              clear selection (back to live view)",
        "  p                  open selected user's profile",
        "  f then 1-8",
        "                     react to selected message on any layout",
        "  Enter              jump to loaded original for selected reply",
        "  r                  reply to selected message",
        "  e                  edit selected message",
        "  d                  delete selected message",
        "  c                  copy selected message to clipboard",
        "",
        "Rooms",
        "  h / l  or  ← / →   previous / next room",
        "  Space              room jump hints",
        "  Enter / i          start composing",
        "  C                  show web-chat QR/link for this session",
        "  Ctrl+N / Ctrl+P    next / previous room while preserving draft",
        "",
        "Compose",
        "  Enter              send and exit",
        "  Alt+S              send and keep open",
        "  Alt+Enter / Ctrl+J newline",
        "  Esc                exit compose",
        "  Backspace          delete char",
        "  Ctrl+W / Ctrl+Backspace",
        "                     delete word left",
        "  Ctrl+Delete        delete word right",
        "  Ctrl+U             clear composer",
        "  Ctrl+← / Ctrl+→    move cursor by word",
        "  @user              mention (Tab/Enter to confirm)",
        "  Ctrl+]             open emoji / nerd font picker",
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
        "",
        "Icon picker",
        "  ↑/↓ or Ctrl+K/J    move selection",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  jump a page",
        "  type to filter     search by name",
        "  Enter              insert and close",
        "  Alt+Enter          insert and keep open",
        "  click / wheel      select / scroll",
        "  double-click       insert and keep open",
        "  Esc                close",
        "",
        "Overlay windows",
        "  Esc / q            close overlay",
        "  j / k              scroll overlay",
        "",
        "Synthetic entries",
        "  Home room rail also contains Feeds, News, Showcase, Work, Mentions, and Discover.",
        "  Their detailed keys and fields live in the Social and News tabs.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn music_help_lines() -> Vec<String> {
    MUSIC_HELP_TEXT.lines().map(str::to_string).collect()
}

fn social_help_lines() -> Vec<String> {
    [
        "Social surfaces",
        "",
        "These are chat-adjacent feeds and profile surfaces. They appear in the Home room rail but are not normal chat rooms.",
        "",
        "Feeds",
        "  Private per-user RSS/Atom inbox.",
        "  Manage subscriptions in Settings > Feeds.",
        "  Entries stay private until shared.",
        "  j / k or ↑ / ↓   navigate entries",
        "  Enter             copy selected entry URL",
        "  s                 share selected entry through News processing",
        "  d                 dismiss selected entry",
        "  r                 refresh feeds now",
        "  After sharing, the URL becomes a public News article and #general announcement.",
        "",
        "Showcase",
        "  Public project-link feed; separate from chat messages.",
        "  List is newest first.",
        "  j / k or ↑ / ↓   navigate showcases",
        "  Enter             copy selected URL",
        "  i                 create a showcase",
        "  e                 edit your own showcase",
        "  d                 delete your own showcase",
        "  /                 toggle filter to only your showcases",
        "  Fields            title, URL, tags, description",
        "  Required          title, http(s) URL, description",
        "  Limits            title 120 chars, description 800 chars",
        "  Tags              max 8, max 24 chars each, lowercase, # stripped",
        "  Tab / Shift+Tab   cycle composer fields",
        "  Enter             submit",
        "  Ctrl+J            newline in description",
        "  Esc               cancel compose",
        "",
        "Work",
        "  Public work-profile feed; one profile per user.",
        "  Creating again updates your existing profile and preserves its public w_ slug.",
        "  Public pages live at /profiles and /profiles/{slug}.",
        "  List is recently updated first.",
        "  Public pages automatically include your profile bio, late.fetch fields,",
        "  and Showcase projects when available.",
        "  j / k or ↑ / ↓   navigate profiles",
        "  Enter / c         copy selected public profile link",
        "  i                 create/edit your own profile",
        "  e                 edit your own profile",
        "  d                 delete your own profile",
        "  /                 toggle filter to only your profile",
        "  Fields            headline, status, type, location, contact, links, skills, summary",
        "  Status            open, casual, or not-looking",
        "  Limits            headline 120 chars, contact 200 chars, summary 1000 chars",
        "  Links             http(s) only, max 6",
        "  Skills            max 12, max 24 chars each, lowercase, # stripped",
        "  Tab / Shift+Tab   cycle composer fields",
        "  Enter             submit",
        "  Ctrl+J            newline in summary",
        "  Esc               cancel compose",
        "",
        "Mentions",
        "  User-targeted notification feed for @user mentions.",
        "  Selecting Mentions marks it read.",
        "  j / k or ↑ / ↓   navigate notifications",
        "  Enter             jump to referenced room/message when possible",
        "  Rules             actor excluded; DMs notify participants; private rooms notify members",
        "  Game-room chat does not create Mentions feed notifications.",
        "",
        "Discover",
        "  Lists public topic rooms you have not joined.",
        "  Loads only when selected.",
        "  j / k or ↑ / ↓   navigate rooms",
        "  Enter             join selected public room",
        "",
        "Profiles",
        "  p                 open selected chat author's read-only profile",
        "  j / k, arrows     scroll profile modal",
        "  PageUp/PageDown   page profile modal",
        "  Esc / q           close profile modal",
        "  Profiles show username, country, timezone/current time, markdown bio, bonsai,",
        "  late.fetch fields, and the user's showcases when available.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn rooms_help_lines() -> Vec<String> {
    [
        "Multiplayer Rooms",
        "",
        "Rooms are persistent table-game rooms. Each room has a paired game chat pane, while the game runtime itself is process-local and resets if the SSH server restarts.",
        "",
        "Directory",
        "  3                 open Rooms",
        "  j / k or ↑ / ↓   navigate rooms",
        "  h / l or ← / →   cycle filters",
        "  Filters           All, Blackjack, Poker, Tic-Tac-Toe",
        "  /                 search by room name",
        "  Enter             enter selected room",
        "  n                 create a new room",
        "  Esc               clears create/search/query/filter before leaving room state",
        "  Directory rows show name, game, seats, pace, stakes, and status.",
        "",
        "Create rooms",
        "  n                 open game picker",
        "  j / k or ↑ / ↓   choose game kind",
        "  Enter             open selected create form",
        "  first letter      shortcut to a game kind",
        "  Esc               cancel picker/form",
        "  Room name         max 48 chars",
        "  Search query      max 32 chars",
        "  Cap               3 open tables per user per game kind",
        "  Blackjack         name, pace, stake",
        "  Poker / TTT       name only",
        "",
        "Active room",
        "  Layout            game on top, embedded game chat below",
        "  `                 return to Dashboard; backtick on Dashboard returns to last game",
        "  Esc               clears selected embedded-chat message first",
        "  q / Esc           game backend may leave the active room",
        "  i                 compose in embedded chat",
        "  j / k             embedded-chat message selection unless game claims the key",
        "  PageUp/PageDown   scroll embedded chat",
        "  End               jump embedded chat to most recent",
        "  r/e/d/p/c/f       reply, edit, delete, profile, copy, react selected chat message",
        "  Arrows            game gets first chance; otherwise embedded chat handles them",
        "",
        "Home shortcuts",
        "  3                 open Rooms",
        "  b then 1-3         enter one of the hot room shortcuts in the right rail",
        "",
        "Blackjack",
        "  4 seats, chips, 6-deck shoe, dealer stands soft 17, blackjack pays 3:2.",
        "  Pace              Quick 2m, Standard 5m, Chill 10m",
        "  Stakes            10 / 50 / 100 / 500 chips; max bet is 10x stake",
        "  s / Enter         sit in first open seat",
        "  l                 leave seat when safe",
        "  [ / a             previous chip",
        "  ] / d             next chip",
        "  Space             throw selected chip",
        "  Backspace         pull one chip",
        "  c / Ctrl+W        clear pending bet",
        "  Enter / s         lock bet",
        "  h / Space         hit",
        "  s                 stand",
        "  d / D             double down when eligible",
        "  First locked bet starts a fixed 30s betting cap.",
        "",
        "Poker",
        "  Four-seat fixed-blind Texas Hold'em with 10/20 blinds, private hole cards,",
        "  shared board, side pots, showdown ranking, and chip settlement.",
        "  s / Enter         sit in first open seat",
        "  n                 deal next hand",
        "  c / Space / Enter check or call",
        "  b / r             bet or raise",
        "  [ / ] or - / +    adjust selected bet/raise amount",
        "  a                 all-in",
        "  x                 toggle auto check/fold",
        "  f                 fold",
        "  l                 leave seat",
        "",
        "Tic-Tac-Toe",
        "  Two seats, X and O, no chips.",
        "  s / Space / Enter sit as viewer",
        "  1-9               place directly",
        "  w/a/s/d or arrows move cursor while seated",
        "  Space / Enter     place on cursor",
        "  n                 new round",
        "  l                 leave seat and reset board",
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
        "Primary screens",
        "  1 Home            chat, rooms, music, and live activity",
        "  2 The Arcade      daily puzzles, endless games, leaderboard",
        "  3 Rooms           persistent table-game rooms",
        "  4 Artboard        shared persistent ASCII canvas",
        "",
        "There is also a dedicated Architecture slide if you need system-level context.",
        "",
        "Global keys",
        "  Tab / Shift+Tab   next / previous screen",
        "  1-4               jump straight to a screen",
        "  ?                 open this guide",
        "  q                 open quit confirm (press q again to leave)",
        "  m                 mute paired client",
        "  + / -             paired client volume",
        "  v then v          cycle Home panels: left, right, lounge info, combos",
        "",
        "Home",
        "  P                 install CLI · pair browser (curl / nix / source + QR)",
        "",
        "Room favorites",
        "  f                 favorite / unfavorite the selected room",
        "  favorites appear first in the room rail and room picker",
        "  `                 toggle Dashboard / last game",
        "",
        "Home room shortcuts",
        "  3                 open Rooms",
        "  b then 1-3         enter one of the hot room shortcuts in the right rail",
        "",
        "This modal",
        "  Tab / Shift+Tab   next / previous tab",
        "  j / k / ↑ / ↓     scroll current tab",
        "  Esc / q / ?       close",
        "",
        "Use /binds and /music in chat if you want to jump directly to those slides from the composer.",
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
        "  users currently vote lofi / classic / ambient",
        "  the winning genre streams for everyone",
        "  Icecast serves audio and Liquidsoap manages playlists",
        "  paired browser or CLI clients handle actual audio output and visualizer data",
        "",
        "User-facing areas",
        "  Home/Dashboard with chat rail, The Arcade, Rooms, Artboard, and the persistent bonsai sidebar",
        "  Home chat includes synthetic feeds: Feeds, News, Showcase, Work, Mentions, Discover",
        "  Rooms are persistent DB rows with paired chat_rooms(kind='game')",
        "  Room game runtime state is process-local and can reset on SSH server restart",
        "",
        "Important characteristics",
        "  terminal-first, always-on, social, and zero-signup",
        "  SSH key fingerprint is the identity anchor",
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
    [
        "News processing",
        "",
        "The News room is a shared feed for links worth keeping around. It is built for URL drop-ins, AI summaries, and quick scanning from the terminal.",
        "",
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
        "",
        "Good inputs",
        "  tech articles, launch posts, docs, YouTube links, tweets/x links",
        "  private RSS/Atom entries from Feeds when you press s there",
        "",
        "Feeds relationship",
        "  Feeds is a private inbox in the Home room rail.",
        "  RSS/Atom subscriptions are managed in Settings > Feeds.",
        "  Sharing a feed entry sends its URL through this News pipeline.",
        "  Only shared entries become public News articles and #general announcements.",
        "",
        "Notes",
        "  summaries are intentionally compact for terminal reading",
        "  thumbnails only render when they fit the layout",
        "  the room acts like a curated backlog, not high-speed chat",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn arcade_help_lines() -> Vec<String> {
    [
        "The Arcade and leaderboard",
        "",
        "The Arcade mixes daily puzzle runs with endless score chases. Ctrl+G opens Hub with monthly leaderboards.",
        "",
        "Games in rotation",
        "  High score: 2048, Tetris, Snake",
        "  Daily: Sudoku, Nonograms, Minesweeper, Solitaire",
        "",
        "Arcade controls",
        "  j / k             browse games",
        "  Enter             play selected game",
        "  Esc               leave current game",
        "",
        "Artboard",
        "  4                 open dedicated Artboard page",
        "  i / Enter         enter active mode",
        "  Esc               return Artboard to view mode",
        "",
        "What matters",
        "  top chips tracks positive chip earnings this month",
        "  arcade champion weights daily puzzle wins by difficulty",
        "  Tetris and 2048 boards track monthly score runs",
        "  wins can award Late Chips",
        "  leaderboard tracks Arcade Wins, all-time highs, and chip balances",
        "",
        "Why it exists",
        "",
        "It gives the app a slower social loop than chat: drop in, play a run, show up on the board, come back tomorrow.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn artboard_help_lines() -> Vec<String> {
    [
        "Artboard",
        "",
        "The Artboard is a shared, persistent ASCII canvas. Everyone paints on the same live board from the dedicated screen.",
        "",
        "Where to find it",
        "  4                 open the Artboard screen",
        "  Tab / Shift+Tab   cycle to it from other screens",
        "  https://late.sh/gallery",
        "                    web gallery for Artboard snapshots",
        "",
        "Modes",
        "  view mode         inspect and pan without editing",
        "  active mode       type, erase, select, stamp, and draw",
        "  i / Enter         enter active mode from live view",
        "  Esc               return to view mode or dismiss local editor state",
        "",
        "Important keys",
        "  ?                 toggle Artboard page help in view mode",
        "  Ctrl+P            toggle Artboard help while editing",
        "  g                 open daily/monthly snapshot browser",
        "  Ctrl+\\           toggle owner overlay",
        "  Ctrl+]            open emoji / Unicode glyph picker",
        "  Ctrl+U / Ctrl+Y   previous / next paint color",
        "",
        "Drawing basics",
        "  arrows            move cursor / focus",
        "  Home / End        jump to line edges",
        "  PgUp / PgDn       jump vertically",
        "  <type>            draw a character",
        "  Space             erase",
        "  Shift+arrows      start or extend a selection",
        "  Ctrl+C / Ctrl+X   copy or cut selection into swatches",
        "  Ctrl+A/S/D/F/G    activate swatch slots 1..5",
        "  Enter / Ctrl+V    stamp the floating brush",
        "",
        "Snapshots and gallery",
        "  live board saves every 5 minutes and on shutdown",
        "  daily snapshots are archived as daily:YYYY-MM-DD",
        "  the newest 7 daily snapshots are kept",
        "  monthly snapshots are archived as monthly:YYYY-MM",
        "  on UTC month rollover, the prior daily snapshot becomes the monthly archive and the live board resets blank",
        "  Artboard view mode g opens the terminal snapshot gallery",
        "  web gallery is public at https://late.sh/gallery",
        "",
        "What is shared",
        "  canvas cells, connected peers, your assigned color, and cell ownership provenance",
        "",
        "What stays local",
        "  cursor, viewport, selections, swatches, selected paint color, brush previews, glyph search, and help scroll",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn settings_help_lines() -> Vec<String> {
    let graybeard_mention_cooldown_sec = GRAYBEARD_MENTION_COOLDOWN.as_secs();

    vec![
        "Settings and identity".to_string(),
        "".to_string(),
        "Your identity and preferences live in the settings modal.".to_string(),
        "".to_string(),
        "Tabs".to_string(),
        "  Settings          username, country, timezone, notifications, layout toggles"
            .to_string(),
        "  Bio               multiline markdown bio and late.fetch fields".to_string(),
        "  Themes            theme browser and background color".to_string(),
        "  Feeds             private RSS/Atom subscriptions".to_string(),
        "  Account           account deletion flow".to_string(),
        "".to_string(),
        "What you can set".to_string(),
        "  username".to_string(),
        "  theme and background color".to_string(),
        "  notifications, bell, cooldown, notification format".to_string(),
        "  multiline bio".to_string(),
        "  country via picker, with Unicode flag rendering".to_string(),
        "  timezone via picker".to_string(),
        "  IDE, terminal, OS, and languages for profile/late.fetch surfaces".to_string(),
        "  background color, right sidebar, room list, and lounge info visibility".to_string(),
        "  private RSS/Atom feed subscriptions".to_string(),
        "".to_string(),
        "How to open it".to_string(),
        "  on login, the settings modal opens automatically".to_string(),
        "  press Ctrl+O anywhere in the app".to_string(),
        "  or use /settings from chat".to_string(),
        "".to_string(),
        "Feeds tab".to_string(),
        "  j / k or arrows move through feed rows".to_string(),
        "  Enter / a on the add row starts URL input".to_string(),
        "  d / Delete removes the selected feed".to_string(),
        "  r refreshes feeds".to_string(),
        "  Feed URLs must be http(s) and are capped at 2000 chars".to_string(),
        "".to_string(),
        "Why country matters".to_string(),
        "".to_string(),
        "The saved ISO country code belongs to profile/settings identity surfaces; it is not the chat username badge."
            .to_string(),
        "".to_string(),
        "Notifications".to_string(),
        "".to_string(),
        "Terminal notifications run through OSC 777 / OSC 9.".to_string(),
        "Best support today: kitty, Ghostty, rxvt-unicode, foot, wezterm, konsole, and iTerm2."
            .to_string(),
        "tmux is not supported here, so notification escape sequences can get mangled or dropped."
            .to_string(),
        "Notifications can fire for DMs, mentions, and game events.".to_string(),
        "Bell and cooldown decide how loud and how often they show up.".to_string(),
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

fn bonsai_help_lines() -> Vec<String> {
    [
        "Bonsai",
        "",
        "The bonsai is your slow-burn presence artifact. It grows while you keep showing up, and its state is persistent.",
        "",
        "Controls",
        "  w                 water or replant",
        "  hjkl / arrows     move the pruning cursor",
        "  x                 cut the branch under the cursor",
        "  p                 prune hard: -1 stage, new shape",
        "  s                 copy the bonsai to clipboard",
        "  ?                 open this help section",
        "",
        "How growth works",
        "  watering gives +10 growth",
        "  it also grows slowly while connected",
        "  after 7 dry days it dies",
        "  missed daily wrong-branch cuts cost -10 growth",
        "  cutting the wrong spot costs -10 growth immediately",
        "  cutting all wrong branches preserves the current shape",
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
        "  the only glyph/icon next to a chat username is that user's bonsai stage/state",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

const MUSIC_HELP_TEXT: &str = "\
How music works on late.sh

SSH is a terminal protocol - it carries text, not audio. To hear music you need a second audio channel that pairs with your SSH session.

Option 1 (recommended): Install the CLI

  curl -fsSL https://cli.late.sh/install.sh | bash

Then run `late` instead of `ssh late.sh`. It launches SSH + local audio playback in one process - no browser needed. The CLI decodes the MP3 stream locally, plays through your system audio, and pairs with the TUI over WebSocket for visualizer + controls.

Don't trust the install script? Build from source:

  git clone https://github.com/mpiorowski/late-sh
  cargo build --release --bin late

You can also use the Nix option shown in the Home pair modal.

Option 2: Browser pairing

On Home, press `P` to open CLI install options plus the browser pairing QR/link. The browser connects to your session via a token-based WebSocket, streams audio, and feeds visualizer frames back to the sidebar.

Both options give you:
  m = mute | +/- = volume | visualizer in the sidebar
  Vote for genres on Home: v1 v2 v3
  Cycle Home panels: vv

The stream is 128kbps MP3 from Icecast, fed by Liquidsoap playlists of CC0/CC-BY music. The winning genre switches every hour based on votes.";
