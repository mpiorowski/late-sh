#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpTopic {
    Overview,
    Architecture,
    Chat,
    Music,
    News,
    Arcade,
    Bonsai,
    Profile,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 8] = [
        HelpTopic::Overview,
        HelpTopic::Chat,
        HelpTopic::Music,
        HelpTopic::News,
        HelpTopic::Arcade,
        HelpTopic::Bonsai,
        HelpTopic::Profile,
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
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Profile => "Profile",
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
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Profile => "Profile",
        }
    }

    pub fn index(self) -> usize {
        match self {
            HelpTopic::Overview => 0,
            HelpTopic::Chat => 1,
            HelpTopic::Music => 2,
            HelpTopic::News => 3,
            HelpTopic::Arcade => 4,
            HelpTopic::Bonsai => 5,
            HelpTopic::Profile => 6,
            HelpTopic::Architecture => 7,
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
        HelpTopic::Bonsai => bonsai_help_lines(),
        HelpTopic::Profile => profile_help_lines(),
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
        "  /join #room        join a room (creates it if new, solo)",
        "  /create #room      create a room and add everyone",
        "  /leave             leave the current room",
        "  /dm @user          open a direct message",
        "  /active            list active users",
        "  /list              list users in this private room",
        "  /ignore [@user]    ignore a user, or list ignored users",
        "  /unignore [@user]  remove a user from your ignore list",
        "  /music             explain how music works",
        "  /help              open this guide",
        "",
        "Messages",
        "  j / k              select older / newer message",
        "  ↑ / ↓              same as j / k",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  half page up / down",
        "  End                jump to most recent",
        "  g / G              clear selection (back to live view)",
        "  p                  open selected user's profile",
        "  r                  reply to selected message",
        "  e                  edit selected message",
        "  d                  delete selected message",
        "",
        "Rooms",
        "  h / l              previous / next room",
        "  Space              room jump hints",
        "  Enter / i          start composing",
        "  c                  copy a web-chat link to this session",
        "",
        "Compose",
        "  Enter              send and exit",
        "  Ctrl+Enter         send and keep open",
        "  Alt+Enter          newline",
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
        "  q / Esc            close overlay",
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
        "late.sh is a terminal clubhouse over SSH: chat, music, news, games, profiles, and shared presence in one session.",
        "",
        "Primary screens",
        "  1 Dashboard       stream status, voting, and chat snapshot",
        "  2 Chat            public rooms, DMs, mentions, web-chat links",
        "  3 Profile         read-only identity card + edit settings modal",
        "  4 The Arcade      daily puzzles, endless games, leaderboard",
        "",
        "There is also a dedicated Architecture slide if you need system-level context.",
        "",
        "Global keys",
        "  Tab / Shift+Tab   next / previous screen",
        "  1-4               jump straight to a screen",
        "  ?                 open this guide",
        "  q                 quit",
        "  m                 mute paired client",
        "  + / -             paired client volume",
        "  P                 show browser pairing QR",
        "",
        "This modal",
        "  h / l / ← / →     previous / next slide",
        "  j / k / ↑ / ↓     scroll current slide",
        "  ? / q / Esc       close",
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
        "  Dashboard, Chat, News, Profile, The Arcade, and the persistent bonsai sidebar",
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
        "  2048, Tetris, Sudoku, Nonograms, Minesweeper, Solitaire",
        "  Blackjack is admin-gated",
        "",
        "Hub controls",
        "  j / k             browse games",
        "  Enter             play selected game",
        "  Esc               leave current game",
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

fn profile_help_lines() -> Vec<String> {
    [
        "Profile and identity",
        "",
        "Profile is now intentionally lightweight in-page: the page shows your public identity, and editing happens in the welcome/profile modal.",
        "",
        "What you can set",
        "  username",
        "  theme",
        "  notifications, bell, cooldown",
        "  multiline bio",
        "  country via picker, with Unicode flag rendering",
        "  timezone via picker",
        "",
        "How to open it",
        "  on login, the welcome/profile modal opens automatically",
        "  from Profile, use the edit action to reopen it",
        "",
        "Why country matters",
        "",
        "The saved ISO country code can later render a flag in chat and other user surfaces.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
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
