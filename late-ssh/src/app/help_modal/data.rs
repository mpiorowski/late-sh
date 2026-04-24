use crate::app::ai::ghost::{GRAYBEARD_CHAT_INTERVAL, GRAYBEARD_MENTION_COOLDOWN};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpTopic {
    Overview,
    Architecture,
    Chat,
    Music,
    News,
    Arcade,
    Artboard,
    Bonsai,
    Settings,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 9] = [
        HelpTopic::Overview,
        HelpTopic::Chat,
        HelpTopic::Music,
        HelpTopic::News,
        HelpTopic::Arcade,
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
            HelpTopic::Music => "Music",
            HelpTopic::News => "News",
            HelpTopic::Arcade => "Arcade",
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
            HelpTopic::Music => "Music",
            HelpTopic::News => "News",
            HelpTopic::Arcade => "Arcade",
            HelpTopic::Artboard => "Art",
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Settings => "Settings",
        }
    }

    pub fn index(self) -> usize {
        match self {
            HelpTopic::Overview => 0,
            HelpTopic::Chat => 1,
            HelpTopic::Music => 2,
            HelpTopic::News => 3,
            HelpTopic::Arcade => 4,
            HelpTopic::Artboard => 5,
            HelpTopic::Bonsai => 6,
            HelpTopic::Settings => 7,
            HelpTopic::Architecture => 8,
        }
    }
}

pub fn lines_for(topic: HelpTopic) -> Vec<String> {
    match topic {
        HelpTopic::Overview => overview_lines(),
        HelpTopic::Architecture => architecture_lines(),
        HelpTopic::Chat => chat_help_lines(),
        HelpTopic::Music => music_help_lines(),
        HelpTopic::News => news_help_lines(),
        HelpTopic::Arcade => arcade_help_lines(),
        HelpTopic::Artboard => artboard_help_lines(),
        HelpTopic::Bonsai => bonsai_help_lines(),
        HelpTopic::Settings => settings_help_lines(),
    }
}

pub fn bot_app_context() -> String {
    let mut out = String::from("APP CONTEXT:\n");
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
        "  /public #room      open or create a public room",
        "  /private #room     create a private room",
        "  /invite @user      add a user to the current room",
        "  /leave             leave the current room",
        "  /create-room #room admin: create a permanent public room",
        "  /delete-room #room admin: delete a permanent room",
        "  /fill-room #room   admin: add all users and enable auto-join",
        "  /dm @user          open a direct message",
        "  /active            list active users",
        "  /members           list users in this room",
        "  /list              list public rooms",
        "  /ignore [@user]    ignore a user, or list ignored users",
        "  /unignore [@user]  remove a user from your ignore list",
        "  /music             explain how music works",
        "  /settings          open your settings modal",
        "  /exit              open quit confirm",
        "  Ctrl+O             open your settings modal anywhere",
        "",
        "Messages",
        "  j / k              select older / newer message",
        "  ↑ / ↓              same as j / k",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  half page up / down",
        "  End                jump to most recent",
        "  g / G              clear selection (back to live view)",
        "  p                  open selected user's profile",
        "  f then 1 / 2 / 3 / 4 / 5",
        "                     react to selected message on any layout",
        "  r                  reply to selected message",
        "  e                  edit selected message",
        "  d                  delete selected message",
        "  c                  copy selected message to clipboard",
        "",
        "Rooms",
        "  h / l  or  ← / →   previous / next room",
        "  Space              room jump hints",
        "  Enter / i          start composing",
        "  c                  copy a web-chat link to this session",
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
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn music_help_lines() -> Vec<String> {
    MUSIC_HELP_TEXT.lines().map(str::to_string).collect()
}

fn overview_lines() -> Vec<String> {
    [
        "late.sh in one pass",
        "",
        "late.sh is a terminal clubhouse over SSH: chat, music, news, games, settings, and shared presence in one session.",
        "",
        "Primary screens",
        "  1 Dashboard       stream status, voting, and chat snapshot",
        "  2 Chat            public rooms, DMs, mentions, web-chat links",
        "  3 The Arcade      daily puzzles, endless games, leaderboard",
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
        "  P                 show browser pairing QR",
        "",
        "Dashboard favorites",
        "  Pin rooms in Settings → Favorites so the dashboard's chat card",
        "  can switch between them without leaving the home screen.",
        "  Strip appears once you have 2+ pins.",
        "  [ / ]             cycle prev / next pinned room",
        "  ,                 jump back to the previously-active pin",
        "  g then 1-9        jump directly to favorite slot N",
        "",
        "This modal",
        "  Tab / Shift+Tab   next / previous tab",
        "  j / k / ↑ / ↓     scroll current tab",
        "  Esc / q / ?       close",
        "",
        "Use /help and /music in chat if you want to jump directly to those slides from the composer.",
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
        "  PostgreSQL stores users, chat, profiles, games, chips, and leaderboard data",
        "  services publish watch snapshots and broadcast events into SSH sessions",
        "",
        "Audio stack",
        "  users currently vote lofi / classic / ambient",
        "  the winning genre streams for everyone",
        "  Icecast serves audio and Liquidsoap manages playlists",
        "  paired browser or CLI clients handle actual audio output and visualizer data",
        "",
        "User-facing areas",
        "  Dashboard, Chat, The Arcade, Artboard, and the persistent bonsai sidebar",
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
        "  Enter             submit the link",
        "  Esc               cancel URL entry",
        "  j / k             browse stories",
        "  d                 delete your own story",
        "",
        "What happens after submit",
        "  1. late.sh fetches the article or video page",
        "  2. AI extracts a compact summary",
        "  3. ASCII art / preview is generated when possible",
        "  4. the story lands in the shared feed for everyone",
        "",
        "Good inputs",
        "  tech articles, launch posts, docs, YouTube links, tweets/x links",
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
        "The Arcade mixes daily puzzle runs with endless score chases. Your progress feeds the shared leaderboard and streak system.",
        "",
        "Games in rotation",
        "  High score: 2048, Tetris",
        "  Daily: Sudoku, Nonograms, Minesweeper, Solitaire",
        "  Multiplayer: Blackjack (admin-gated)",
        "",
        "Hub controls",
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
        "  daily puzzles build streaks",
        "  wins can award Late Chips",
        "  leaderboard tracks streak leaders, all-time highs, and chip balances",
        "  chat badges reflect streak tiers",
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
        "  cursor, viewport, selections, swatches, brush previews, glyph search, and help scroll",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn settings_help_lines() -> Vec<String> {
    let graybeard_interval_min = GRAYBEARD_CHAT_INTERVAL.as_secs() / 60;
    let graybeard_mention_cooldown_sec = GRAYBEARD_MENTION_COOLDOWN.as_secs();

    vec![
        "Settings and identity".to_string(),
        "".to_string(),
        "Your identity and preferences live in the settings modal.".to_string(),
        "".to_string(),
        "What you can set".to_string(),
        "  username".to_string(),
        "  theme".to_string(),
        "  notifications, bell, cooldown".to_string(),
        "  multiline bio".to_string(),
        "  country via picker, with Unicode flag rendering".to_string(),
        "  timezone via picker".to_string(),
        "  favorite rooms (dashboard quick-switch strip)".to_string(),
        "".to_string(),
        "How to open it".to_string(),
        "  on login, the settings modal opens automatically".to_string(),
        "  press Ctrl+O anywhere in the app".to_string(),
        "  or use /settings from chat".to_string(),
        "".to_string(),
        "Why country matters".to_string(),
        "".to_string(),
        "The saved ISO country code can later render a flag in chat and other user surfaces."
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
        format!("Lurks in #general every ~{graybeard_interval_min}min."),
        "Burned-out senior who still shows up to heckle modern software.".to_string(),
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
        "  x                 prune and reshape",
        "  s                 copy the bonsai to clipboard",
        "",
        "How growth works",
        "  water it daily to keep it healthy",
        "  it grows while connected",
        "  after 7 dry days it dies",
        "  pruning changes shape but costs some growth",
        "",
        "Why it matters",
        "  it gives the app a calm personal loop outside chat and games",
        "  the tree becomes a little signature of how you inhabit late.sh over time",
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
  cargo install --path late-cli

Option 2: Browser pairing

Press `P` to open a QR code + copy the pairing URL. The browser connects to your session via a token-based WebSocket, streams audio, and feeds visualizer frames back to the sidebar.

Both options give you:
  m = mute | +/- = volume | visualizer in the sidebar
  Vote for genres on the Dashboard: L C A

The stream is 128kbps MP3 from Icecast, fed by Liquidsoap playlists of CC0/CC-BY music. The winning genre switches every hour based on votes.";
