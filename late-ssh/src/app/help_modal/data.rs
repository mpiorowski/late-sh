use crate::app::ai::ghost::GRAYBEARD_MENTION_COOLDOWN;
use crate::app::common::qr::{Barcode, HalfBlock};
use qrcodegen::{QrCode, QrCodeEcc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpTopic {
    Pair,
    Overview,
    Architecture,
    Chat,
    Social,
    Music,
    News,
    Arcade,
    Tables,
    Doors,
    TerminalCopy,
    TerminalLinks,
    TerminalImages,
    TerminalSelection,
    TerminalNotifications,
    TerminalCliYoutube,
    Economy,
    Bonsai,
    Settings,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 19] = [
        HelpTopic::Pair,
        HelpTopic::Overview,
        HelpTopic::Chat,
        HelpTopic::Social,
        HelpTopic::Music,
        HelpTopic::News,
        HelpTopic::Arcade,
        HelpTopic::Tables,
        HelpTopic::Doors,
        HelpTopic::TerminalCopy,
        HelpTopic::TerminalLinks,
        HelpTopic::TerminalImages,
        HelpTopic::TerminalSelection,
        HelpTopic::TerminalNotifications,
        HelpTopic::TerminalCliYoutube,
        HelpTopic::Economy,
        HelpTopic::Bonsai,
        HelpTopic::Settings,
        HelpTopic::Architecture,
    ];

    pub fn title(self) -> &'static str {
        match self {
            HelpTopic::Pair => "Pair",
            HelpTopic::Overview => "Overview",
            HelpTopic::Architecture => "Architecture",
            HelpTopic::Chat => "Chat",
            HelpTopic::Social => "Social",
            HelpTopic::Music => "Music",
            HelpTopic::News => "News",
            HelpTopic::Arcade => "Arcade",
            HelpTopic::Tables => "Tables",
            HelpTopic::Doors => "Doors",
            HelpTopic::TerminalCopy => "Copy",
            HelpTopic::TerminalLinks => "Links",
            HelpTopic::TerminalImages => "Images",
            HelpTopic::TerminalSelection => "Selection",
            HelpTopic::TerminalNotifications => "Notifications",
            HelpTopic::TerminalCliYoutube => "CLI YouTube",
            HelpTopic::Economy => "Economy",
            HelpTopic::Bonsai => "Bonsai",
            HelpTopic::Settings => "Settings",
        }
    }

    pub fn index(self) -> usize {
        match self {
            HelpTopic::Pair => 0,
            HelpTopic::Overview => 1,
            HelpTopic::Chat => 2,
            HelpTopic::Social => 3,
            HelpTopic::Music => 4,
            HelpTopic::News => 5,
            HelpTopic::Arcade => 6,
            HelpTopic::Tables => 7,
            HelpTopic::Doors => 8,
            HelpTopic::TerminalCopy => 9,
            HelpTopic::TerminalLinks => 10,
            HelpTopic::TerminalImages => 11,
            HelpTopic::TerminalSelection => 12,
            HelpTopic::TerminalNotifications => 13,
            HelpTopic::TerminalCliYoutube => 14,
            HelpTopic::Economy => 15,
            HelpTopic::Bonsai => 16,
            HelpTopic::Settings => 17,
            HelpTopic::Architecture => 18,
        }
    }
}

pub fn lines_for(topic: HelpTopic, keep_composer_focused: bool, pair_url: &str) -> Vec<String> {
    match topic {
        HelpTopic::Pair => pair_help_lines(pair_url),
        HelpTopic::Overview => overview_lines(),
        HelpTopic::Architecture => architecture_lines(),
        HelpTopic::Chat => chat_help_lines(keep_composer_focused),
        HelpTopic::Social => social_help_lines(),
        HelpTopic::Music => music_help_lines(),
        HelpTopic::News => news_help_lines(),
        HelpTopic::Arcade => arcade_help_lines(),
        HelpTopic::Tables => tables_help_lines(),
        HelpTopic::Doors => doors_help_lines(),
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
        HelpTopic::Economy => economy_lines(),
        HelpTopic::Bonsai => bonsai_help_lines(),
        HelpTopic::Settings => settings_help_lines(),
    }
}

pub fn bot_app_context() -> String {
    let mut out = String::from(
        "APP CONTEXT:\n\
        CRITICAL FACTS:\n\
        - Chat username badges render in this order: special role badges, bonsai stage, equipped badge, equipped flag, then the /brb moon.\n\
        - There is no separate top-level Chat screen. Home/Dashboard owns the chat room rail and chat center; top-level screens are Home, The Arcade, Tables, Door Games, Artboard, and Directory.\n\
        - Directory page 6 owns Profiles, Projects, and Pinstar tabs. Artboard and Pinstar have detailed page-local editing keybinds.\n",
    );
    for topic in HelpTopic::ALL {
        out.push_str(&format!("## {}\n", topic.title()));
        // Bot context is per-app, not per-user — describe the default Enter/
        // Alt+S binding rather than any one user's `keep_composer_focused`
        // tweak state.
        for line in lines_for(topic, false, "") {
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

const SHELL_INSTALL_COMMAND: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";
const WINDOWS_INSTALL_COMMAND: &str = "irm https://cli.late.sh/install.ps1 | iex";
const NIX_COMMAND: &str = "nix run github:mpiorowski/late-sh#late";
const SOURCE_URL: &str = "https://github.com/mpiorowski/late-sh";
const QR_QUIET_ZONE: i32 = 4;

fn pair_help_lines(pair_url: &str) -> Vec<String> {
    let pair_url = pair_url.trim();
    let pair_url = if pair_url.is_empty() {
        "your pairing link appears here in-session"
    } else {
        pair_url
    };
    let mut lines = vec![
        "Install `late` / Pair Browser".to_string(),
        "".to_string(),
        "Recommended: install the native CLI and run `late` instead of `ssh late.sh`.".to_string(),
        "That gives one process for SSH, local Icecast audio, YouTube webview fallback, and OS clipboard image reads.".to_string(),
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
        "  controls    m mute, +/- volume, v+x source, v+v Music Booth".to_string(),
        "".to_string(),
        "Browser pairing".to_string(),
        "  Open this link on any device, or scan the QR below.".to_string(),
        "  The browser plays your selected source, including YouTube.".to_string(),
        "  A real browser takes over YouTube from the CLI webview helper while it is paired.".to_string(),
        "".to_string(),
    ];

    lines.extend(qr_lines(pair_url));
    lines.extend([
        "".to_string(),
        pair_url.to_string(),
        "scan with your phone or open the link on any device".to_string(),
        "".to_string(),
        "Trouble?".to_string(),
        "  The terminal-specific tabs below cover copy, links, images, selection, notifications, and CLI YouTube.".to_string(),
    ]);
    lines
}

fn qr_lines(pair_url: &str) -> Vec<String> {
    if !(pair_url.starts_with("https://") || pair_url.starts_with("http://")) {
        return Vec::new();
    }
    let Ok(qr) = QrCode::encode_text(pair_url, QrCodeEcc::Low) else {
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

pub fn chat_help_lines(keep_composer_focused: bool) -> Vec<String> {
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
        "  /music             explain how music works",
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
        "  /private #room     create a private room",
        "  /invite @user      add a user to the current room",
        "  /leave             leave the current room",
        "  /dm @user          open a direct message",
        "  /active            list active users",
        "  /friends           list friends",
        "  /friend [@user]    list friends, or mark a user as a friend",
        "  /unfriend [@user]  list friends, or remove a friend mark",
        "  /members           list users in this room",
        "  /list              list public rooms",
        "  /roll [NdM ...]    roll dice (default d20), e.g. /roll 3d6 2d20",
        "  /paste-image       upload image from paired CLI clipboard (see Images)",
        "  /upload <url>      download and upload an image URL (see Images)",
        "  /ignore [@user]    ignore a user, or list ignored users",
        "  /unignore [@user]  unignore a user, or list ignored users",
        "",
        "Global chat keys",
        "  Ctrl+O             open your settings modal anywhere",
        "  Ctrl+G             open Hub",
        "  Ctrl+Q             toggle your Aquarium tray after unlocking it in Shop",
        "  Ctrl+/             search and jump to a room, DM, or Home entry",
        "  ?                  open this guide; Pair and terminal-specific tabs live here",
        "",
        "Messages",
        "  j / k              select older / newer message",
        "  ↑ / ↓              same as j / k",
        "  Ctrl+U / Ctrl+D    half page up / down",
        "  PageUp / PageDown  half page up / down",
        "  g / G              clear selection (back to live view)",
        "  p                  open selected user's profile",
        "  f then 1-8",
        "                     react to selected message on any layout",
        "  Enter              jump to loaded original for selected reply",
        "  Enter              open selected image or News item when present",
        "  r                  reply to selected message",
        "  e                  edit selected message",
        "  d                  delete selected message",
        "  c                  copy selected message to clipboard",
        "  Ctrl+P             pin / unpin selected message",
        "",
        "Rooms",
        "  h / l  or  ← / →   previous / next room",
        "  Space              room jump hints",
        "  Enter / i          start composing",
        "  Ctrl+N / Ctrl+P    next / previous room while preserving draft",
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
        "  Home room rail also contains RSS, News, Voice, Mentions, and Discover.",
        "  Directory page 6 contains Profiles, Projects, and Pinstar.",
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

pub fn music_help_lines() -> Vec<String> {
    MUSIC_HELP_TEXT.lines().map(str::to_string).collect()
}

fn social_help_lines() -> Vec<String> {
    [
        "Social surfaces",
        "",
        "These are chat-adjacent updates and profile surfaces. RSS stays in Home; Projects and Profiles live on Directory page 6.",
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
        "  After sharing, the URL becomes a public News article and #general announcement.",
        "",
        "Projects",
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
        "Profiles",
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
        "  Profiles show username, birthday, country, timezone/current time, chips, markdown bio,",
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
        "  `                 return to Dashboard while a run is active",
        "",
        "Notes",
        "  Game-specific controls appear inside the Arcade page.",
        "  Daily puzzle completions, run scores, chips, payouts, and leaderboards are covered in Economy.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn tables_help_lines() -> Vec<String> {
    [
        "Tables",
        "",
        "Tables are persistent multiplayer sessions for table-style games with paired embedded chat.",
        "  3                 open Tables",
        "  j / k or ↑ / ↓   navigate tables",
        "  h / l or ← / →   cycle filters",
        "  /                 search by table name",
        "  Enter             enter selected table",
        "  n                 create a new table",
        "  Esc               clears create/search/query/filter before leaving table state",
        "  Directory rows show name, game, creator, seats, pace, stakes, and status.",
        "",
        "Table creation",
        "  n                 open game picker",
        "  j / k or ↑ / ↓   choose game kind",
        "  Enter             open selected create form",
        "  first letter      shortcut to a game kind",
        "  Esc               cancel picker/form",
        "  Game-specific forms and limits live in the Economy tab.",
        "",
        "Active table",
        "  Layout            game on top, embedded game chat below",
        "  `                 cycle Dashboard and tables where you are seated",
        "  Esc               clears selected embedded-chat message first",
        "  q / Esc           game backend may leave the active table",
        "  i                 compose in embedded chat",
        "  j / k             embedded-chat message selection unless game claims the key",
        "  PageUp/PageDown   scroll embedded chat",
        "  r/e/d/p/c/f       reply, edit, delete, profile, copy, react selected chat message",
        "  Ctrl+P            pin / unpin selected embedded-chat message",
        "  Arrows            game gets first chance; otherwise embedded chat handles them",
        "",
        "Home shortcuts",
        "  3                 open Tables",
        "  b then 1-4         enter one of the recent table shortcuts in lounge",
        "",
        "Economy",
        "  Economy tab        Arcade game list, Arcade controls, table-game controls, chips, scoring, and leaderboards.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn doors_help_lines() -> Vec<String> {
    [
        "Doors",
        "",
        "Door Games are BBS-style persistent worlds. Lateania is the first door.",
        "  4                 open Door Games",
        "  Esc / q           leave the door and return Home",
        "",
        "Lateania",
        "  1-5               choose class before your first adventure",
        "  w/a/s/d or arrows move north/west/south/east",
        "  y/u/n/m           diagonal movement",
        "  < / >             move up / down where exits exist",
        "  o                 look around",
        "  Space / Enter / x attack",
        "  1-9               use ability slots after choosing a class",
        "  z                 flee combat",
        "",
        "Panels",
        "  c                 character",
        "  v                 abilities",
        "  t                 inventory",
        "  b                 shop, when a merchant is present",
        "  Enter             activate selected inventory/shop row",
        "  x                 sell selected inventory item at a shop",
        "",
        "Persistence",
        "  Your Lateania character is saved when you leave and periodically while present.",
        "  Reset/restart is not exposed in the UI yet.",
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
        "  1 Home            chat, tables, music, and live activity",
        "  2 The Arcade      daily puzzles, endless games, leaderboard",
        "  3 Tables          persistent table games",
        "  4 Door Games      BBS-style persistent worlds",
        "  5 Artboard        shared persistent ASCII canvas",
        "  6 Directory       Profiles, Projects, and Pinstar",
        "",
        "Artboard and Directory/Pinstar have their own page-local controls; this guide keeps detailed editing keys out.",
        "There is also a dedicated Architecture slide if you need system-level context.",
        "",
        "Global keys",
        "  Tab / Shift+Tab   next / previous screen",
        "  1-5               jump straight to a screen",
        "  ?                 open this guide",
        "  q                 open quit confirm (press q again to leave)",
        "  Ctrl+O            open Settings",
        "  Ctrl+G            open Hub",
        "  Ctrl+Q            toggle Aquarium tray after unlocking it in Shop",
        "  Ctrl+/            search and jump to a room, DM, or synthetic Home entry",
        "  ?                 open this guide; Pair and terminal-specific tabs live here",
        "  w                 open Bonsai Care when not composing",
        "  c                 open Cat Companion after unlocking it",
        "  m                 mute paired client",
        "  + / -             paired client volume",
        "  v then v          open the Music Booth (submit + queue + votes)",
        "  v then x          swap paired browser between Icecast and YouTube",
        "  v then s          skip-vote the current YouTube track",
        "  v then 1/2/3      vote Lofi / Ambient / Classic genre",
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
        "  `                 cycle Dashboard / seated game rooms",
        "",
        "Hub",
        "  Ctrl+G            open Shop, Leaderboard, Quests, Events",
        "  Tab / Shift+Tab   switch Hub tabs",
        "  1-4               jump to Hub tab",
        "  Shop              j/k select, [/] subtab, Enter buy with Late Chips",
        "  Economy tab       chips, payouts, leaderboards, Arcade, table games",
        "",
        "Jump search",
        "  Ctrl+/            open / close jump modal",
        "  type              filter rooms, DMs, RSS, News, Voice, Mentions, Discover",
        "  @query / #query   bias toward users or rooms",
        "  ↑/↓ or Ctrl+K/J   move selection",
        "  PageUp/PageDown   jump 8 rows",
        "  Backspace         delete query char",
        "  Ctrl+Backspace    delete query word",
        "  Enter             jump to selected destination",
        "  Esc               close",
        "",
        "Home room shortcuts",
        "  3                 open Tables",
        "  b then 1-4         enter one of the recent table shortcuts in lounge",
        "",
        "This modal",
        "  Tab / Shift+Tab   next / previous tab",
        "  j / k / ↑ / ↓     scroll current tab",
        "  ? / Esc / q       close",
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
        "  Home/Dashboard with chat rail, The Arcade, Tables, Door Games, Artboard, Directory, and the persistent bonsai sidebar",
        "  Home chat includes synthetic entries: RSS, News, Voice, Mentions, Discover; Directory owns Profiles, Projects, and Pinstar",
        "  Tables are persistent DB rows with paired chat_rooms(kind='game')",
        "  Table game runtime state is process-local and can reset on SSH server restart",
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
        "  private RSS/Atom entries from the RSS room when you press s there",
        "",
        "RSS relationship",
        "  RSS is a private inbox in the Home room rail.",
        "  RSS/Atom subscriptions are managed in Settings > RSS.",
        "  Sharing an RSS entry sends its URL through this News pipeline.",
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
        "  Themes            expanded theme browser".to_string(),
        "  RSS               private RSS/Atom subscriptions".to_string(),
        "  Account           account deletion flow".to_string(),
        "  Special           show-settings-on-connect toggle; unlocks after profile setup"
            .to_string(),
        "".to_string(),
        "What you can set".to_string(),
        "  username".to_string(),
        "  birthday as month/day".to_string(),
        "  theme and background color".to_string(),
        "  notifications, bell, cooldown, notification format".to_string(),
        "  multiline bio".to_string(),
        "  country via picker, with Unicode flag rendering".to_string(),
        "  timezone via picker".to_string(),
        "  IDE, terminal, OS, and languages for profile/late.fetch surfaces".to_string(),
        "  background color, room list, and the Activity boxes toggle".to_string(),
        "  right sidebar mode (on/off/custom) with per-screen visibility".to_string(),
        "  private RSS/Atom subscriptions".to_string(),
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
        "  Custom sidebar: Enter on Custom opens per-screen checklist".to_string(),
        "  Account: Enter opens delete confirmation; type DELETE to confirm".to_string(),
        "  ? opens this guide; Esc / q closes".to_string(),
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
        "The saved ISO country code belongs to profile/settings identity surfaces; equipped chat flags come from Hub Shop."
            .to_string(),
        "".to_string(),
        "Notifications".to_string(),
        "".to_string(),
        "Terminal notifications run through OSC 777 / OSC 9.".to_string(),
        "Best support today: kitty, Ghostty, rxvt-unicode, foot, wezterm, konsole, and iTerm2."
            .to_string(),
        "tmux strips notification escapes by default; see the Notifications tab for passthrough setup."
            .to_string(),
        "Notifications can fire for DMs, mentions, friend joins, and game events.".to_string(),
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
        "Dynamic Bonsai",
        "",
        "Dynamic Bonsai is the living tree. It is not a fixed ladder of pictures: it keeps a real branch graph, and every choice you make is remembered in how it grows next. Water it, steer the tips, cut your mistakes, and pinch foliage, and the silhouette becomes a record of how you tended it.",
        "",
        "Unlock it in the Hub Shop for 1000 chips. While it is equipped, w opens Dynamic Bonsai instead of classic Bonsai; clear the slot to switch back.",
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
        "  Unlock            Hub Shop companion bought with Late Chips",
        "  c                 open pet care after unlocking it",
        "  f                 feed (every 2 days)",
        "  w                 water (daily)",
        "  p                 play (daily; 3-day care streak unlocks happy)",
        "  q / Esc           close",
        "  play mode         hjkl / WASD / arrows move toy",
        "  Space / Enter / p dash toy",
        "  c                 stop play",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

const MUSIC_HELP_TEXT: &str = "\
How music works on late.sh

late.sh has two audio surfaces running at once:

  Icecast    a 24/7 house radio. The room votes on the genre.
  YouTube    a shared queue everyone can submit links to.

You pick which one your paired browser plays. The sidebar shows both at a glance: the one you're actually hearing is highlighted, the other is dimmed.

Get audio paired

  Option 1 (recommended): install the CLI

    macOS / Linux / Termux:
      curl -fsSL https://cli.late.sh/install.sh | bash

    Windows PowerShell:
      irm https://cli.late.sh/install.ps1 | iex

    Then run `late` instead of `ssh late.sh`. One process, SSH + local audio. The CLI plays Icecast directly and can open a small YouTube webview helper when YouTube is selected.

    Build from source instead:
      git clone https://github.com/mpiorowski/late-sh
      cargo build --release --bin late

    A Nix option is shown in the Pair tab of this guide.

  Option 2: browser pairing

    Open the Pair tab in this guide for install hints plus a QR / link. The browser plays whichever source you have selected, including YouTube.

Global keys (work anywhere)
  ?                open this guide, including Pair and terminal-specific tabs
  m                 mute paired client
  + / -             volume up / down

Vote the Icecast genre
  v then 1 / l      Lofi
  v then 2 / a      Ambient
  v then 3 / c      Classic
  The winning genre takes over on the next hourly flip.

Swap which source you hear
  v then x          toggle your paired browser between Icecast and YouTube. Your choice is saved per-user, so a refresh keeps it.

Music Booth (v then v)

  Opens a modal with a URL submit row on top and the queue below.

  Tab               switch focus between submit and queue
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

  The queue is ordered by score, so upvotes pull tracks toward the front. You can't vote on the track that's already playing, but you can skip-vote it.

Skip the current track
  v then s          add your vote to skip. The track skips once enough paired users agree.
  s                 same thing, while you're in the booth queue.

Track length

  Every track is capped at 1 hour. Shorter videos play to their real end; anything longer (long mixes, live streams, the YouTube fallback) gets cut off at the 1h mark and the queue moves on.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_purpose_guide_keeps_artboard_out_of_topic_tabs() {
        assert!(
            !HelpTopic::ALL
                .iter()
                .any(|topic| topic.title() == "Artboard")
        );
        assert!(!bot_app_context().contains("## Artboard\n"));
    }

    #[test]
    fn all_purpose_guide_splits_game_topics() {
        assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Arcade"));
        assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Tables"));
        assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Doors"));
        assert!(!HelpTopic::ALL.iter().any(|topic| topic.title() == "Games"));
        assert!(bot_app_context().contains("## Arcade\n"));
        assert!(bot_app_context().contains("## Tables\n"));
        assert!(bot_app_context().contains("## Doors\n"));
        assert!(!bot_app_context().contains("## Games\n"));
    }

    #[test]
    fn bot_context_includes_hub_guide_facts() {
        let context = bot_app_context();
        assert!(context.contains("## Economy\n"));
        assert!(context.contains("Monthly Top Chips counts positive earnings only."));
        assert!(context.contains("Tetris, 2048, and Snake record run scores."));
        assert!(context.contains("Blackjack form: name, pace, stake."));
        assert!(context.contains("Four-seat fixed-stack Texas Hold'em"));
    }

    #[test]
    fn bot_context_includes_terminal_faq_and_image_facts() {
        let context = bot_app_context();
        assert!(context.contains("## Copy\n"));
        assert!(context.contains("## Images\n"));
        assert!(context.contains("## CLI YouTube\n"));
        assert!(context.contains("Why copy sometimes silently fails"));
        assert!(context.contains("CLI YouTube playback"));
        assert!(context.contains("/paste-image"));
        assert!(context.contains("This is CLI-only"));
        assert!(context.contains("The original-quality image is the uploaded/copied URL."));
        assert!(context.contains("Kitty protocol: kitty, Ghostty, rio, warp, Konsole."));
        assert!(context.contains("iTerm2 inline images: iTerm2, WezTerm, mintty, hterm."));
    }

    #[test]
    fn chat_guide_lists_user_facing_slash_commands() {
        let lines = chat_help_lines(false).join("\n");
        for expected in [
            "/brb [message]",
            "/coffee",
            "/friend [@user]",
            "/friends",
            "/icons",
            "/petname [name]",
            "/profile [@user]",
            "/tea",
            "/upload <url>",
        ] {
            assert!(lines.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn chat_guide_collapses_compose_section_when_keep_composer_focused() {
        let off = chat_help_lines(false).join("\n");
        assert!(off.contains("Enter              send and exit"));
        assert!(off.contains("Alt+S              send and keep open"));
        assert!(!off.contains("<<COMPOSE_SEND_LINES>>"));

        let on = chat_help_lines(true).join("\n");
        assert!(on.contains("Enter              send and keep open"));
        assert!(!on.contains("Alt+S"));
        assert!(!on.contains("send and exit"));
        assert!(!on.contains("<<COMPOSE_SEND_LINES>>"));
    }

    #[test]
    fn bot_context_does_not_leak_restricted_commands() {
        let context = bot_app_context();
        for forbidden in [
            "/audio",
            "/create-room",
            "/delete-room",
            "/fill-room",
            "/mod",
            "staff",
            "admin",
            "moderation",
            "unskippable",
        ] {
            assert!(
                !context.to_lowercase().contains(forbidden),
                "bot context leaked {forbidden}"
            );
        }
    }

    #[test]
    fn global_guide_points_to_hub_for_game_details() {
        let arcade = arcade_help_lines().join("\n");
        let tables = tables_help_lines().join("\n");
        let doors = doors_help_lines().join("\n");
        assert!(arcade.contains("Economy"));
        assert!(tables.contains("Economy tab"));
        assert!(doors.contains("Lateania"));
        assert!(!arcade.contains("Tetris"));
        assert!(!tables.contains("Sudoku"));
        assert!(!doors.contains("Clock presets"));
    }
}
