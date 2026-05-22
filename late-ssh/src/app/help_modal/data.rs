use crate::app::ai::ghost::GRAYBEARD_MENTION_COOLDOWN;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelpTopic {
    Overview,
    Architecture,
    Chat,
    Social,
    Music,
    News,
    Games,
    Bonsai,
    Settings,
}

impl HelpTopic {
    pub const ALL: [HelpTopic; 9] = [
        HelpTopic::Overview,
        HelpTopic::Chat,
        HelpTopic::Social,
        HelpTopic::Music,
        HelpTopic::News,
        HelpTopic::Games,
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
            HelpTopic::Games => "Games",
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
            HelpTopic::Games => "Games",
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
            HelpTopic::Games => 5,
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
        HelpTopic::Social => social_help_lines(),
        HelpTopic::Music => music_help_lines(),
        HelpTopic::News => news_help_lines(),
        HelpTopic::Games => games_help_lines(),
        HelpTopic::Bonsai => bonsai_help_lines(),
        HelpTopic::Settings => settings_help_lines(),
    }
}

pub fn bot_app_context() -> String {
    let mut out = String::from(
        "APP CONTEXT:\n\
        CRITICAL FACTS:\n\
        - The glyph/icon next to a chat username is only the user's bonsai stage/state. It is not a country flag or custom contributor icon.\n\
        - There is no separate top-level Chat screen. Home/Dashboard owns the chat room rail and chat center; top-level screens are Home, The Arcade, Rooms, and Artboard.\n\
        - Artboard exists as a top-level shared canvas, but its detailed editing keybinds live only in the Artboard page help, not this app guide.\n",
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
    out.push_str("## Hub Guide\n");
    for line in crate::app::hub::guide::bot_context_lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str("- ");
        out.push_str(line.trim());
        out.push('\n');
    }
    out.push_str("## Terminal FAQ\n");
    for line in crate::app::terminal_help_modal::data::bot_context_lines() {
        if line.trim().is_empty() {
            continue;
        }
        out.push_str("- ");
        out.push_str(line.trim());
        out.push('\n');
    }
    out
}

pub fn chat_help_lines() -> Vec<String> {
    [
        "Commands",
        "  /binds             open this guide",
        "  /music             explain how music works",
        "  /settings          open your settings modal",
        "  /icons             open emoji / nerd font picker",
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
        "  /paste-image       upload image from paired CLI clipboard (see Ctrl+L Images)",
        "  /upload <url>      download and upload an image URL (see Ctrl+L Images)",
        "  /ignore [@user]    ignore a user, or list ignored users",
        "  /unignore [@user]  unignore a user, or list ignored users",
        "",
        "Global chat keys",
        "  Ctrl+O             open your settings modal anywhere",
        "  Ctrl+G             open Hub",
        "  Ctrl+L             open terminal FAQ: copy, links, images, selection, notifications, CLI YouTube",
        "  Ctrl+/             search and jump to a room, DM, or Home entry",
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
        "  Enter              send and exit",
        "  Alt+S              send and keep open",
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
        "  image modal        Enter/c copy image URL; Esc/q close; see Ctrl+L Images",
        "",
        "Synthetic entries",
        "  Home room rail also contains RSS, News, Showcase, Work, Mentions, and Discover.",
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
        "These are chat-adjacent updates and profile surfaces. They appear in the Home room rail but are not normal chat rooms.",
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
        "  Profiles show username, country, timezone/current time, chips, markdown bio,",
        "  bonsai, late.fetch fields, and the user's showcases when available.",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn games_help_lines() -> Vec<String> {
    [
        "Games",
        "",
        "The game surfaces are The Arcade and Rooms. This page covers getting around; Hub Guide owns per-game controls, scoring, chips, payouts, and leaderboards.",
        "",
        "Arcade",
        "  2                 open The Arcade",
        "  j / k or ↑ / ↓   browse games",
        "  Enter             play selected game",
        "  Esc / q           leave current game",
        "  `                 return to Dashboard while a run is active",
        "",
        "Rooms directory",
        "  3                 open Rooms",
        "  j / k or ↑ / ↓   navigate rooms",
        "  h / l or ← / →   cycle filters",
        "  /                 search by room name",
        "  Enter             enter selected room",
        "  n                 create a new room",
        "  Esc               clears create/search/query/filter before leaving room state",
        "  Directory rows show name, game, seats, pace, stakes, and status.",
        "",
        "Room creation",
        "  n                 open game picker",
        "  j / k or ↑ / ↓   choose game kind",
        "  Enter             open selected create form",
        "  first letter      shortcut to a game kind",
        "  Esc               cancel picker/form",
        "  Game-specific forms and limits live in Hub Guide.",
        "",
        "Active room",
        "  Layout            game on top, embedded game chat below",
        "  `                 return to Dashboard; backtick on Dashboard returns to last game",
        "  Esc               clears selected embedded-chat message first",
        "  q / Esc           game backend may leave the active room",
        "  i                 compose in embedded chat",
        "  j / k             embedded-chat message selection unless game claims the key",
        "  PageUp/PageDown   scroll embedded chat",
        "  r/e/d/p/c/f       reply, edit, delete, profile, copy, react selected chat message",
        "  Ctrl+P            pin / unpin selected embedded-chat message",
        "  Arrows            game gets first chance; otherwise embedded chat handles them",
        "",
        "Home shortcuts",
        "  3                 open Rooms",
        "  b then 1-4         enter one of the hot room shortcuts in lounge",
        "",
        "Hub Guide",
        "  Ctrl+G then 5      open the detailed games/economy guide",
        "  Hub Guide owns Arcade game list, Arcade controls, room-game controls, chips, scoring, and leaderboards.",
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
        "Artboard has its own page help; this guide keeps its detailed editing keys out.",
        "There is also a dedicated Architecture slide if you need system-level context.",
        "",
        "Global keys",
        "  Tab / Shift+Tab   next / previous screen",
        "  1-4               jump straight to a screen",
        "  ?                 open this guide",
        "  q                 open quit confirm (press q again to leave)",
        "  Ctrl+O            open Settings",
        "  Ctrl+G            open Hub",
        "  Ctrl+L            terminal FAQ: copy, links, images, selection, notifications, CLI YouTube",
        "  Ctrl+/            search and jump to a room, DM, or synthetic Home entry",
        "  w                 open Bonsai Care when not composing",
        "  c                 open Cat Companion when not composing; locked users jump to Hub Shop",
        "  m                 mute paired client",
        "  + / -             paired client volume",
        "  v then v          open the Music Booth (submit + queue + votes)",
        "  v then x          swap paired browser between Icecast and YouTube",
        "  v then s          skip-vote the current YouTube track",
        "  v then 1/2/3      vote Lofi / Ambient / Classic genre",
        "",
        "Home",
        "  P                 install CLI · pair browser (curl / nix / source + QR)",
        "  click top bar     jump screens",
        "  click room rail   select room or synthetic entry",
        "  click unread HUD  jump to Mentions",
        "",
        "Room favorites",
        "  f                 favorite / unfavorite the selected room",
        "  [ / ]             move the selected favorite up / down",
        "  favorites appear first in the room rail and room picker",
        "  `                 toggle Dashboard / last game",
        "",
        "Hub",
        "  Ctrl+G            open Leaderboard, Dailies, Shop, Events, Guide",
        "  Tab / Shift+Tab   switch Hub tabs",
        "  1-5               jump to Hub tab",
        "  Shop              j/k select, [/] category, Enter buy with Late Chips",
        "  Guide             chips, payouts, leaderboards, Arcade, room games",
        "",
        "Jump search",
        "  Ctrl+/            open / close jump modal",
        "  type              filter rooms, DMs, RSS, News, Showcase, Work, Mentions, Discover",
        "  @query / #query   bias toward users or rooms",
        "  ↑/↓ or Ctrl+K/J   move selection",
        "  PageUp/PageDown   jump 8 rows",
        "  Backspace         delete query char",
        "  Ctrl+Backspace    delete query word",
        "  Enter             jump to selected destination",
        "  Esc               close",
        "",
        "Home room shortcuts",
        "  3                 open Rooms",
        "  b then 1-4         enter one of the hot room shortcuts in lounge",
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
        "  Home chat includes synthetic entries: RSS, News, Showcase, Work, Mentions, Discover",
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
        "  theme and background color".to_string(),
        "  notifications, bell, cooldown, notification format".to_string(),
        "  multiline bio".to_string(),
        "  country via picker, with Unicode flag rendering".to_string(),
        "  timezone via picker".to_string(),
        "  IDE, terminal, OS, and languages for profile/late.fetch surfaces".to_string(),
        "  background color, room list, and lounge info visibility".to_string(),
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
        "The saved ISO country code belongs to profile/settings identity surfaces; it is not the chat username badge."
            .to_string(),
        "".to_string(),
        "Notifications".to_string(),
        "".to_string(),
        "Terminal notifications run through OSC 777 / OSC 9.".to_string(),
        "Best support today: kitty, Ghostty, rxvt-unicode, foot, wezterm, konsole, and iTerm2."
            .to_string(),
        "tmux strips notification escapes by default; see Ctrl+L terminal FAQ for passthrough setup."
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
        "Bonsai and companions",
        "",
        "The bonsai is your slow-burn presence artifact. It grows while you keep showing up, and its state is persistent.",
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
        "  the only glyph/icon next to a chat username is that user's bonsai stage/state",
        "",
        "Cat Companion",
        "  Unlock            Hub Shop companion bought with Late Chips",
        "  c                 open cat care when not composing; locked users jump to Hub Shop",
        "  f                 feed",
        "  w                 water",
        "  p                 play",
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

    A Nix option is shown in the Home pair modal.

  Option 2: browser pairing

    On Home press P for the pair modal: install hints plus a QR / link. The browser plays whichever source you have selected, including YouTube.

Global keys (work anywhere)
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
    fn all_purpose_guide_merges_game_topics() {
        assert!(HelpTopic::ALL.iter().any(|topic| topic.title() == "Games"));
        assert!(!bot_app_context().contains("## Arcade\n"));
        assert!(!bot_app_context().contains("## Rooms\n"));
        assert!(bot_app_context().contains("## Games\n"));
    }

    #[test]
    fn bot_context_includes_hub_guide_facts() {
        let context = bot_app_context();
        assert!(context.contains("## Hub Guide\n"));
        assert!(context.contains("Monthly Top Chips counts positive earnings only."));
        assert!(context.contains("Tetris, 2048, and Snake record run scores."));
        assert!(context.contains("Blackjack form: name, pace, stake."));
        assert!(context.contains("Four-seat fixed-stack Texas Hold'em"));
    }

    #[test]
    fn bot_context_includes_terminal_faq_and_image_facts() {
        let context = bot_app_context();
        assert!(context.contains("## Terminal FAQ\n"));
        assert!(context.contains("Why copy sometimes silently fails"));
        assert!(context.contains("CLI YouTube playback"));
        assert!(context.contains("/paste-image"));
        assert!(context.contains("This is CLI-only"));
        assert!(context.contains("The original-quality image is the uploaded/copied URL."));
        assert!(context.contains("Kitty protocol: kitty, Ghostty, wezterm, rio, warp, Konsole."));
    }

    #[test]
    fn chat_guide_lists_user_facing_slash_commands() {
        let lines = chat_help_lines().join("\n");
        for expected in [
            "/friend [@user]",
            "/friends",
            "/icons",
            "/profile [@user]",
            "/upload <url>",
        ] {
            assert!(lines.contains(expected), "missing {expected}");
        }
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
        let games = games_help_lines().join("\n");
        assert!(games.contains("Hub Guide owns Arcade game list"));
        assert!(games.contains("Rooms directory"));
        assert!(!games.contains("Tetris"));
        assert!(!games.contains("Sudoku"));
        assert!(!games.contains("Room stack"));
        assert!(!games.contains("Clock presets"));
    }
}
