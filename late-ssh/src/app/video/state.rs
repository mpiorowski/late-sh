//! The watch modal: a YouTube video (a News story, or a link in a chat
//! message) played inside the user's own terminal by their paired CLI's
//! `mpv`, or, when their setup cannot, the setup box naming what is
//! missing. Pure state: the player holds the play as the CLI last reported
//! it and the rectangle its last draw reserved; `sync` turns that into the
//! pair messages the CLI needs, and `svc.rs` sends them. Not a frame of
//! video passes through the server.

use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use late_core::models::article_read::WatchPosition;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use uuid::Uuid;

use crate::app::audio::client_state::{
    ClientAudioState, ClientPlatform, ClientSshMode, VideoBlocker,
};
use crate::app::files::terminal_image::TerminalImageProtocol;
use crate::paired_clients::{PairControlMessage, VideoControl, VideoReport};

/// A playing or paused play with no `video_state` for this long has lost
/// its CLI. A starting one is exempt: the CLI reports `starting` once and
/// nothing more until mpv has a position, and a yt-dlp resolve can take
/// longer than this; a start that fails is reported as `failed` by the CLI
/// itself.
pub(crate) const WATCH_LOST_AFTER: Duration = Duration::from_secs(10);
/// A resume lands this much before the saved position, so it opens on the
/// sentence that was playing rather than the middle of the next.
pub(crate) const WATCH_RESUME_REWIND_S: u64 = 3;
/// A play stopped inside its first seconds is not worth resuming.
pub(crate) const WATCH_SAVE_MIN_S: u64 = 10;
/// A play stopped this close to the end was watched: the next one starts
/// from the top.
pub(crate) const WATCH_SAVE_END_MARGIN_S: u64 = 30;
/// How often a playing position is saved, so a dropped session or a dead
/// CLI still resumes close to where it was.
pub(crate) const WATCH_SAVE_EVERY: Duration = Duration::from_secs(15);
/// How long the footer says `resumed` once a resumed play has a position.
pub(crate) const WATCH_RESUME_NOTE_FOR: Duration = Duration::from_secs(4);
const WINDOW_WIDTH_PERMILLE: u16 = 900;
const WINDOW_HEIGHT_PERMILLE: u16 = 900;
const WINDOW_MAX_WIDTH: u16 = 160;
/// The window's header: title and meta on one row, then a rule.
const WINDOW_HEADER_ROWS: u16 = 2;

/// The session's video: the modal on screen, what the paired CLI was last
/// told, and where this session left the chat links it watched.
#[derive(Debug, Default)]
pub(crate) struct State {
    modal: Option<WatchModal>,
    /// What the CLI was last told to play, kept past the modal so closing
    /// it still sends the stop (`sync`).
    live_play: Option<LivePlay>,
    /// The last `play_id` handed out this session.
    last_play_id: u64,
    /// Where each chat link was left partway through, by URL. Kept for the
    /// session only: a chat link has no row to store it on.
    link_positions: HashMap<String, WatchPosition>,
}

impl State {
    pub(crate) fn modal(&self) -> Option<&WatchModal> {
        self.modal.as_ref()
    }

    pub(crate) fn is_open(&self) -> bool {
        self.modal.is_some()
    }

    pub(crate) fn player_mut(&mut self) -> Option<&mut Player> {
        match &mut self.modal {
            Some(WatchModal::Player(player)) => Some(player),
            Some(WatchModal::Setup(_)) | None => None,
        }
    }

    /// The player on `request` under a fresh `play_id`, in place of the
    /// modal that was up (`svc` closes that first, so its position is
    /// kept). It starts from `resume_from` when the video was left
    /// partway through. Returns the play's id.
    pub(crate) fn open_player(
        &mut self,
        request: WatchRequest,
        resume_from: Option<WatchPosition>,
        now: Instant,
    ) -> u64 {
        self.last_play_id += 1;
        let player = Player::new(request, self.last_play_id, now, resume_from);
        self.modal = Some(WatchModal::Player(Box::new(player)));
        self.last_play_id
    }

    /// The setup box for `url`, in place of the modal that was up.
    pub(crate) fn open_setup(&mut self, url: &str, inputs: &SetupInputs) {
        self.modal = Some(WatchModal::Setup(SetupBox::decide(url, inputs)));
    }

    /// Close the modal. A closing player hands back what it leaves for the
    /// next watch of its video (`Player::memory`); closing is state only,
    /// the next draw's `cli_messages` sends the stop.
    pub(crate) fn close(&mut self) -> Option<ClosedPlayer> {
        match self.modal.take() {
            Some(WatchModal::Player(player)) => Some(ClosedPlayer {
                source: player.source,
                memory: player.memory(),
                url: player.url,
            }),
            Some(WatchModal::Setup(_)) | None => None,
        }
    }

    /// Where this session left a chat link, if partway through.
    pub(crate) fn link_position(&self, url: &str) -> Option<WatchPosition> {
        self.link_positions.get(url).copied()
    }

    /// Keep what a closed chat link's player left, for its next watch.
    pub(crate) fn remember_link(&mut self, url: String, memory: WatchMemory) {
        match memory {
            WatchMemory::Save(position) => {
                self.link_positions.insert(url, position);
            }
            WatchMemory::Clear => {
                self.link_positions.remove(&url);
            }
            WatchMemory::Nothing => {}
        }
    }

    /// The pair messages that bring the CLI in line with the open player:
    /// a play, a move, a stop. Called after every draw, which is what
    /// records the player's body.
    pub(crate) fn cli_messages(&mut self) -> Vec<PairControlMessage> {
        let wanted = match &self.modal {
            Some(WatchModal::Player(player)) => player.wanted(),
            Some(WatchModal::Setup(_)) | None => None,
        };
        sync(&mut self.live_play, wanted)
    }
}

/// Who asked for a video, and so who keeps its position between watches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchSource {
    /// A News story: the position lives on the reader's `article_reads` row
    /// (`news::State::watch_position`).
    NewsArticle(Uuid),
    /// A YouTube link in a chat message: the position lives on this
    /// session's `State` and goes with it.
    ChatLink,
}

/// Everything the player needs to open on a video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WatchRequest {
    pub source: WatchSource,
    /// What mpv is handed.
    pub url: String,
    pub title: String,
    /// Who shared it, for the header.
    pub author: String,
    pub shared_at: DateTime<Utc>,
}

/// What a closed player leaves for the next watch of its video.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClosedPlayer {
    pub source: WatchSource,
    pub url: String,
    pub memory: WatchMemory,
}

#[derive(Debug)]
pub(crate) enum WatchModal {
    /// Boxed: a play's state dwarfs the setup box.
    Player(Box<Player>),
    Setup(SetupBox),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchSize {
    /// A centered box over the page, the TUI visible around it.
    Window,
    /// The whole terminal but the footer row.
    Full,
}

/// The play as the CLI last reported it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchStatus {
    /// Asked for, nothing played yet.
    Starting,
    Playing(WatchPosition),
    Paused(WatchPosition),
    Ended,
    Failed,
    /// No word from the CLI for [`WATCH_LOST_AFTER`] while playing or
    /// paused, or a send reached nobody. Final: the stop already went out,
    /// so a late report must not bring the play back and replay it.
    Lost,
}

impl WatchStatus {
    /// Whether mpv is (as far as the server knows) up on the CLI.
    fn is_live(self) -> bool {
        match self {
            Self::Starting | Self::Playing(_) | Self::Paused(_) => true,
            Self::Ended | Self::Failed | Self::Lost => false,
        }
    }
}

/// How a watch went, for `metrics::record_video_watch`. Every watch press
/// ends in `Opened` or `Setup`; a player then reports `Played` at most
/// once and one of `Ended`, `Failed`, `Lost` at most once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchOutcome {
    Opened,
    /// The user asked to watch and their setup cannot: the setup box.
    Setup,
    Played,
    Ended,
    Failed,
    Lost,
}

/// Where the player's parts land in the terminal, in 0-based cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WatchLayout {
    pub popup: Rect,
    pub header: Rect,
    /// The reserved rectangle: painted black and sent to the CLI.
    pub body: Rect,
    pub footer: Rect,
    pub close: Rect,
}

/// A play the CLI was told about, and the rectangle it was last given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LivePlay {
    pub play_id: u64,
    pub rect: Rect,
}

/// What the open player wants the CLI to be doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WantedPlay<'a> {
    pub play_id: u64,
    pub url: &'a str,
    pub rect: Rect,
    /// Where to begin: zero, or the saved position less the rewind.
    pub start_s: u64,
}

/// What the player leaves behind for the next watch of its video, decided
/// when it closes (`Player::memory`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WatchMemory {
    /// Partway through: resume here next time.
    Save(WatchPosition),
    /// Watched to the end, or stopped back at the top with a position
    /// stored: the next watch starts over.
    Clear,
    /// Nothing worth writing: no position ever came in, or a play with
    /// nothing stored stopped inside its first seconds.
    Nothing,
}

#[derive(Debug)]
pub(crate) struct Player {
    source: WatchSource,
    url: String,
    title: String,
    author: String,
    shared_at: DateTime<Utc>,
    size: WatchSize,
    play_id: u64,
    status: WatchStatus,
    /// Whether `Played` was already reported for this play.
    played: bool,
    last_state_at: Instant,
    /// Where this video was left last time, if partway through.
    resume_from: Option<WatchPosition>,
    /// Whether a position is stored for this video: the one it resumed
    /// from, or a checkpoint since. Taken back to the top, the close then
    /// has a position to clear.
    position_stored: bool,
    /// The last position the CLI reported, kept past a failure or a loss
    /// so the close still saves it.
    last_seen: Option<WatchPosition>,
    /// When the position was last saved (`checkpoint`); opening counts.
    last_saved_at: Instant,
    /// While set and in the future, the footer says the play was resumed.
    resume_note_until: Option<Instant>,
    /// The layout the last draw produced: the `[x]` hit target and the
    /// rectangle `wanted` offers the CLI.
    drawn: Cell<WatchLayout>,
}

impl Player {
    /// A player for `request`, resuming from `resume_from` when the video
    /// was left partway through last time.
    pub(crate) fn new(
        request: WatchRequest,
        play_id: u64,
        now: Instant,
        resume_from: Option<WatchPosition>,
    ) -> Self {
        Self {
            source: request.source,
            url: request.url,
            title: request.title,
            author: request.author,
            shared_at: request.shared_at,
            size: WatchSize::Window,
            play_id,
            status: WatchStatus::Starting,
            played: false,
            last_state_at: now,
            resume_from,
            position_stored: resume_from.is_some(),
            last_seen: None,
            last_saved_at: now,
            resume_note_until: None,
            drawn: Cell::new(WatchLayout::default()),
        }
    }

    pub(crate) fn source(&self) -> WatchSource {
        self.source
    }

    /// Where the CLI is told to begin: the saved position less the rewind,
    /// zero for a fresh play.
    pub(crate) fn start_s(&self) -> u64 {
        match self.resume_from {
            Some(position) => position.position_s.saturating_sub(WATCH_RESUME_REWIND_S),
            None => 0,
        }
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn title(&self) -> &str {
        &self.title
    }

    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    pub(crate) fn shared_at(&self) -> DateTime<Utc> {
        self.shared_at
    }

    pub(crate) fn play_id(&self) -> u64 {
        self.play_id
    }

    pub(crate) fn size(&self) -> WatchSize {
        self.size
    }

    #[cfg(test)]
    pub(crate) fn status(&self) -> WatchStatus {
        self.status
    }

    /// `f`: Window and Full. The next draw's body differs, and `sync`
    /// turns that into a `video_place`.
    pub(crate) fn toggle_size(&mut self) {
        self.size = match self.size {
            WatchSize::Window => WatchSize::Full,
            WatchSize::Full => WatchSize::Window,
        };
    }

    pub(crate) fn drawn(&self) -> WatchLayout {
        self.drawn.get()
    }

    pub(crate) fn set_drawn(&self, layout: WatchLayout) {
        self.drawn.set(layout);
    }

    /// The parts of the player at `size` over `area`, a pure function of
    /// both. The body is what goes over the wire, so a terminal too small
    /// to hold one has an empty body and nothing plays.
    pub(crate) fn layout(size: WatchSize, area: Rect) -> WatchLayout {
        match size {
            WatchSize::Window => {
                let width = permille(area.width, WINDOW_WIDTH_PERMILLE).min(WINDOW_MAX_WIDTH);
                let height = permille(area.height, WINDOW_HEIGHT_PERMILLE);
                let popup = centered_rect(width, height, area);
                let close = close_rect(popup);
                // Inside the border: header, body, footer.
                if popup.width < 3 || popup.height < WINDOW_HEADER_ROWS + 4 {
                    return WatchLayout {
                        popup,
                        close,
                        ..WatchLayout::default()
                    };
                }
                let inner = Rect::new(popup.x + 1, popup.y + 1, popup.width - 2, popup.height - 2);
                WatchLayout {
                    popup,
                    header: Rect::new(inner.x, inner.y, inner.width, WINDOW_HEADER_ROWS),
                    body: Rect::new(
                        inner.x,
                        inner.y + WINDOW_HEADER_ROWS,
                        inner.width,
                        inner.height - WINDOW_HEADER_ROWS - 1,
                    ),
                    footer: Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
                    close,
                }
            }
            WatchSize::Full => {
                if area.width == 0 || area.height < 2 {
                    return WatchLayout {
                        popup: area,
                        ..WatchLayout::default()
                    };
                }
                WatchLayout {
                    popup: area,
                    header: Rect::default(),
                    body: Rect::new(area.x, area.y, area.width, area.height - 1),
                    footer: Rect::new(area.x, area.bottom() - 1, area.width, 1),
                    close: Rect::default(),
                }
            }
        }
    }

    /// The play the CLI should be running: this one, in the last drawn
    /// body, while mpv is up and there is a body to hold it.
    pub(crate) fn wanted(&self) -> Option<WantedPlay<'_>> {
        let body = self.drawn.get().body;
        match (self.status.is_live(), body.is_empty()) {
            (true, false) => Some(WantedPlay {
                play_id: self.play_id,
                url: &self.url,
                rect: body,
                start_s: self.start_s(),
            }),
            (true, true) | (false, _) => None,
        }
    }

    /// The control message for a key, while mpv is up.
    pub(crate) fn control(&self, control: VideoControl) -> Option<PairControlMessage> {
        match self.status.is_live() {
            true => Some(PairControlMessage::VideoControl {
                play_id: self.play_id,
                control,
            }),
            false => None,
        }
    }

    /// One `video_state` from the CLI, already matched to this play.
    /// `Ended`, `Failed` and `Lost` settle the play for good: a report
    /// after any of them is late and changes nothing.
    pub(crate) fn apply(&mut self, report: &VideoReport, now: Instant) -> Option<WatchOutcome> {
        match self.status {
            WatchStatus::Ended | WatchStatus::Failed | WatchStatus::Lost => return None,
            WatchStatus::Starting | WatchStatus::Playing(_) | WatchStatus::Paused(_) => {}
        }
        self.last_state_at = now;
        match report {
            VideoReport::Starting => {
                self.status = WatchStatus::Starting;
                None
            }
            VideoReport::Playing {
                position_s,
                duration_s,
            } => {
                let position = WatchPosition {
                    position_s: *position_s,
                    duration_s: *duration_s,
                };
                self.status = WatchStatus::Playing(position);
                self.last_seen = Some(position);
                match self.played {
                    true => None,
                    false => {
                        self.played = true;
                        self.resume_note_until =
                            self.resume_from.map(|_| now + WATCH_RESUME_NOTE_FOR);
                        Some(WatchOutcome::Played)
                    }
                }
            }
            VideoReport::Paused {
                position_s,
                duration_s,
            } => {
                let position = WatchPosition {
                    position_s: *position_s,
                    duration_s: *duration_s,
                };
                self.status = WatchStatus::Paused(position);
                self.last_seen = Some(position);
                None
            }
            VideoReport::Ended => {
                self.status = WatchStatus::Ended;
                Some(WatchOutcome::Ended)
            }
            VideoReport::Failed { .. } => {
                self.status = WatchStatus::Failed;
                Some(WatchOutcome::Failed)
            }
            // Answers a stop the server sent; the modal that asked is
            // already gone or lost, and stays as it is.
            VideoReport::Stopped => None,
        }
    }

    /// A playing or paused play the CLI has gone quiet on for
    /// [`WATCH_LOST_AFTER`]. A starting one waits for mpv however long it
    /// takes; `Esc` is the way out of a start that never comes.
    pub(crate) fn expire(&mut self, now: Instant) -> Option<WatchOutcome> {
        let quiet = match self.status {
            WatchStatus::Playing(_) | WatchStatus::Paused(_) => true,
            WatchStatus::Starting
            | WatchStatus::Ended
            | WatchStatus::Failed
            | WatchStatus::Lost => false,
        };
        match quiet && now.saturating_duration_since(self.last_state_at) >= WATCH_LOST_AFTER {
            true => self.lose(),
            false => None,
        }
    }

    /// A send that reached no paired CLI: the play is lost at once.
    pub(crate) fn lose(&mut self) -> Option<WatchOutcome> {
        match self.status.is_live() {
            true => {
                self.status = WatchStatus::Lost;
                Some(WatchOutcome::Lost)
            }
            false => None,
        }
    }

    /// A position worth saving now: the last one reported, while the play
    /// is playing or paused, once [`WATCH_SAVE_EVERY`] has passed since the
    /// last save and the play is past its first seconds. Called after every
    /// report; most answer `None`. A play that is over is the close's to
    /// decide (`memory`).
    pub(crate) fn checkpoint(&mut self, now: Instant) -> Option<WatchPosition> {
        match self.status {
            WatchStatus::Playing(_) | WatchStatus::Paused(_) => {}
            WatchStatus::Starting
            | WatchStatus::Ended
            | WatchStatus::Failed
            | WatchStatus::Lost => return None,
        }
        let position = self.last_seen?;
        if position.position_s < WATCH_SAVE_MIN_S {
            return None;
        }
        if now.saturating_duration_since(self.last_saved_at) < WATCH_SAVE_EVERY {
            return None;
        }
        self.last_saved_at = now;
        self.position_stored = true;
        Some(position)
    }

    /// What to remember for the next watch, as the player closes: the last
    /// position while partway through, a clear once the video was watched
    /// to its end (or taken back to the top with a position stored),
    /// nothing when no position ever came in or a play with nothing stored
    /// stopped at once.
    pub(crate) fn memory(&self) -> WatchMemory {
        if self.status == WatchStatus::Ended {
            return WatchMemory::Clear;
        }
        let Some(position) = self.last_seen else {
            return WatchMemory::Nothing;
        };
        let near_the_end = match position.duration_s {
            Some(duration_s) => position.position_s + WATCH_SAVE_END_MARGIN_S >= duration_s,
            None => false,
        };
        if near_the_end {
            return WatchMemory::Clear;
        }
        if position.position_s < WATCH_SAVE_MIN_S {
            return match self.position_stored {
                true => WatchMemory::Clear,
                false => WatchMemory::Nothing,
            };
        }
        WatchMemory::Save(position)
    }

    /// The footer's right edge. A resumed play says so while it starts and
    /// for [`WATCH_RESUME_NOTE_FOR`] once it plays.
    pub(crate) fn status_text(&self, now: Instant) -> String {
        let resumed = match self.resume_note_until {
            Some(until) => until > now,
            None => false,
        };
        match self.status {
            WatchStatus::Starting => match self.resume_from {
                Some(position) => format!("resuming from {}…", clock(position.position_s)),
                None => "starting…".to_string(),
            },
            WatchStatus::Playing(position) => match resumed {
                true => format!("▶ {} · resumed", position_label(position)),
                false => format!("▶ {}", position_label(position)),
            },
            WatchStatus::Paused(position) => format!("⏸ {}", position_label(position)),
            WatchStatus::Ended => "ended".to_string(),
            WatchStatus::Failed => "couldn't play".to_string(),
            WatchStatus::Lost => "lost the CLI".to_string(),
        }
    }

    /// The body's one line when there is nothing to watch here any more.
    pub(crate) fn fallback_line(&self) -> Option<String> {
        match self.status {
            WatchStatus::Failed | WatchStatus::Lost => {
                Some(format!("Watch on the web: {}", self.url))
            }
            WatchStatus::Starting
            | WatchStatus::Playing(_)
            | WatchStatus::Paused(_)
            | WatchStatus::Ended => None,
        }
    }

    /// `key hint` pairs in the order they drop off when the width runs
    /// short: the last one goes first. `Esc` is drawn after them.
    pub(crate) fn footer_hints(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("Space", "pause"),
            ("←/→", "seek 10s"),
            ("⇧←/→", "60s"),
            ("0", "restart"),
            ("m", "mute"),
            (
                "f",
                match self.size {
                    WatchSize::Window => "full",
                    WatchSize::Full => "window",
                },
            ),
        ]
    }
}

/// The pair messages that bring the CLI from `live` (what it was last
/// told) to `wanted` (what the open player wants), updating `live` to
/// match. One comparison covers opening, `f`, a resize, closing, a page
/// change, and a play that ended: whatever made the player go away or
/// move, the CLI hears about it here.
pub(crate) fn sync(
    live: &mut Option<LivePlay>,
    wanted: Option<WantedPlay<'_>>,
) -> Vec<PairControlMessage> {
    match (*live, wanted) {
        (None, None) => Vec::new(),
        (None, Some(wanted)) => {
            *live = Some(LivePlay {
                play_id: wanted.play_id,
                rect: wanted.rect,
            });
            vec![play_message(wanted)]
        }
        (Some(current), None) => {
            *live = None;
            vec![PairControlMessage::VideoStop {
                play_id: current.play_id,
            }]
        }
        (Some(current), Some(wanted)) if current.play_id == wanted.play_id => {
            match current.rect == wanted.rect {
                true => Vec::new(),
                false => {
                    *live = Some(LivePlay {
                        play_id: wanted.play_id,
                        rect: wanted.rect,
                    });
                    vec![PairControlMessage::VideoPlace {
                        play_id: wanted.play_id,
                        left: wanted.rect.x,
                        top: wanted.rect.y,
                        cols: wanted.rect.width,
                        rows: wanted.rect.height,
                    }]
                }
            }
        }
        (Some(current), Some(wanted)) => {
            *live = Some(LivePlay {
                play_id: wanted.play_id,
                rect: wanted.rect,
            });
            vec![
                PairControlMessage::VideoStop {
                    play_id: current.play_id,
                },
                play_message(wanted),
            ]
        }
    }
}

fn play_message(wanted: WantedPlay<'_>) -> PairControlMessage {
    PairControlMessage::VideoPlay {
        play_id: wanted.play_id,
        url: wanted.url.to_string(),
        left: wanted.rect.x,
        top: wanted.rect.y,
        cols: wanted.rect.width,
        rows: wanted.rect.height,
        start_s: wanted.start_s,
    }
}

/// `1:23 / 12:04`, or just the position for a live stream.
pub(crate) fn position_label(position: WatchPosition) -> String {
    match position.duration_s {
        Some(duration_s) => format!("{} / {}", clock(position.position_s), clock(duration_s)),
        None => clock(position.position_s),
    }
}

/// `m:ss`, or `h:mm:ss` from an hour on.
pub(crate) fn clock(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

/// One thing a setup needs before a video can play here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Requirement {
    /// The Tweaks "Terminal images" row is `Off`.
    ImagesOn,
    Cli,
    /// A `late` that is paired but does not own the tty: OpenSSH or an
    /// old mode.
    CliNative,
    /// A `late` from before `video`: it reports neither the capability nor
    /// what blocks it.
    CliUpdate,
    KittyTerminal,
    NoMultiplexer,
    Mpv,
    /// The `mpv_too_old` blocker: the mpv row says so.
    MpvTooOld,
    YtDlp,
}

impl Requirement {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::ImagesOn => "terminal images on",
            Self::Cli => "the late CLI",
            Self::CliNative => "late in native mode",
            Self::CliUpdate => "an up-to-date late CLI",
            Self::KittyTerminal => "a Kitty-graphics terminal",
            Self::NoMultiplexer => "no tmux or screen",
            Self::Mpv => "mpv 0.36 or newer",
            Self::MpvTooOld => "mpv is too old",
            Self::YtDlp => "yt-dlp",
        }
    }

    pub(crate) fn detail(self) -> &'static str {
        match self {
            Self::ImagesOn => "Ctrl+O, Tweaks, Terminal images",
            Self::Cli => "run late.sh through `late`, not plain ssh",
            Self::CliNative => "`late` must own the terminal, not hand it to ssh",
            Self::CliUpdate => "this `late` predates video; reinstall it",
            Self::KittyTerminal => "kitty, Ghostty, WezTerm, Konsole, Rio",
            Self::NoMultiplexer => "the video is drawn straight to the terminal",
            Self::Mpv | Self::MpvTooOld => "plays the video",
            Self::YtDlp => "fetches it for mpv",
        }
    }
}

/// What the server knows about one requirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Holds,
    Fails,
    /// No paired CLI said: nothing to go on.
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SetupRow {
    pub requirement: Requirement,
    pub mark: Mark,
}

/// The one install line under the rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstallHint {
    CliInstaller,
    /// The forwarded env says Windows Terminal.
    CliInstallerWindows,
    LinuxPackages,
    Brew,
    WindowsUnsupported,
}

impl InstallHint {
    pub(crate) fn text(self) -> &'static str {
        match self {
            Self::CliInstaller => "curl -fsSL https://cli.late.sh/install.sh | bash",
            Self::CliInstallerWindows => "irm https://cli.late.sh/install.ps1 | iex",
            Self::LinuxPackages => {
                "Install mpv and yt-dlp with your package manager (apt, dnf, pacman, nix)"
            }
            Self::Brew => "brew install mpv yt-dlp",
            Self::WindowsUnsupported => "Windows is not supported yet",
        }
    }
}

/// Everything the server holds about why a watch might not play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SetupInputs {
    /// The paired CLI's last `client_state`; `None` is plain SSH.
    pub cli: Option<ClientAudioState>,
    /// The protocol the server would draw images with, the Tweaks row
    /// applied except `Off`, which is `images_off`. A hint for the terminal
    /// row until a CLI reports; the CLI's own blockers decide after that.
    pub protocol: Option<TerminalImageProtocol>,
    /// The session's `TERM` is tmux or screen.
    pub multiplexer_term: bool,
    /// The forwarded env says Windows Terminal.
    pub windows_terminal: bool,
    /// The Tweaks "Terminal images" row is `Off`.
    pub images_off: bool,
}

#[derive(Debug)]
pub(crate) struct SetupBox {
    url: String,
    rows: Vec<SetupRow>,
    install: Option<InstallHint>,
    close: Cell<Rect>,
}

impl SetupBox {
    /// The box for `inputs`, every row decided here and nowhere else.
    pub(crate) fn decide(url: &str, inputs: &SetupInputs) -> Self {
        let blockers: &[VideoBlocker] = match &inputs.cli {
            Some(cli) => &cli.video_blockers,
            None => &[],
        };
        let blocked = |blocker: VideoBlocker| blockers.contains(&blocker);
        // A CLI that knows about video reports either the capability or
        // what blocks it; one that reports neither predates it.
        let reports = match &inputs.cli {
            Some(cli) => cli.supports_video() || !cli.video_blockers.is_empty(),
            None => false,
        };

        let mut rows = Vec::new();
        if inputs.images_off {
            rows.push(row(Requirement::ImagesOn, Mark::Fails));
        }
        rows.push(match &inputs.cli {
            None => row(Requirement::Cli, Mark::Fails),
            Some(cli)
                if cli.ssh_mode != ClientSshMode::Native || blocked(VideoBlocker::NotNative) =>
            {
                row(Requirement::CliNative, Mark::Fails)
            }
            Some(_) if !reports => row(Requirement::CliUpdate, Mark::Fails),
            Some(_) => row(Requirement::Cli, Mark::Holds),
        });
        // The CLI sits in the terminal and is the authority on it; the
        // server's image detection serves images (WezTerm reads as iTerm2
        // there) and can only confirm a Kitty terminal, never rule one out.
        let kitty = match reports {
            true => holds(
                !blocked(VideoBlocker::NotKitty)
                    && !blocked(VideoBlocker::NoPixelSize)
                    && !blocked(VideoBlocker::Platform),
            ),
            false => match inputs.protocol {
                Some(TerminalImageProtocol::Kitty) => Mark::Holds,
                Some(TerminalImageProtocol::Iterm2 | TerminalImageProtocol::Sixel) | None => {
                    Mark::Unknown
                }
            },
        };
        rows.push(row(Requirement::KittyTerminal, kitty));
        let multiplexer = inputs.multiplexer_term || blocked(VideoBlocker::Multiplexer);
        rows.push(row(Requirement::NoMultiplexer, holds(!multiplexer)));
        rows.push(match reports {
            false => row(Requirement::Mpv, Mark::Unknown),
            true if blocked(VideoBlocker::NoMpv) => row(Requirement::Mpv, Mark::Fails),
            true if blocked(VideoBlocker::MpvTooOld) => row(Requirement::MpvTooOld, Mark::Fails),
            true => row(Requirement::Mpv, Mark::Holds),
        });
        rows.push(match reports {
            false => row(Requirement::YtDlp, Mark::Unknown),
            true => row(Requirement::YtDlp, holds(!blocked(VideoBlocker::NoYtDlp))),
        });

        let tools_missing = blocked(VideoBlocker::NoMpv)
            || blocked(VideoBlocker::MpvTooOld)
            || blocked(VideoBlocker::NoYtDlp);
        let install = match (&inputs.cli, reports) {
            (None, _) | (Some(_), false) => match inputs.windows_terminal {
                true => Some(InstallHint::CliInstallerWindows),
                false => Some(InstallHint::CliInstaller),
            },
            (Some(cli), true) => match (cli.platform, tools_missing) {
                (ClientPlatform::Windows, _) => Some(InstallHint::WindowsUnsupported),
                (
                    ClientPlatform::Linux
                    | ClientPlatform::Macos
                    | ClientPlatform::Android
                    | ClientPlatform::Unknown,
                    false,
                ) => None,
                (ClientPlatform::Macos, true) => Some(InstallHint::Brew),
                (
                    ClientPlatform::Linux | ClientPlatform::Android | ClientPlatform::Unknown,
                    true,
                ) => Some(InstallHint::LinuxPackages),
            },
        };

        Self {
            url: url.to_string(),
            rows,
            install,
            close: Cell::new(Rect::default()),
        }
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn rows(&self) -> &[SetupRow] {
        &self.rows
    }

    pub(crate) fn install(&self) -> Option<InstallHint> {
        self.install
    }

    pub(crate) fn close(&self) -> Rect {
        self.close.get()
    }

    pub(crate) fn set_close(&self, close: Rect) {
        self.close.set(close);
    }
}

fn row(requirement: Requirement, mark: Mark) -> SetupRow {
    SetupRow { requirement, mark }
}

fn holds(holds: bool) -> Mark {
    match holds {
        true => Mark::Holds,
        false => Mark::Fails,
    }
}

/// The `[x]` on a popup's top border, or nothing on one too small for it.
pub(crate) fn close_rect(popup: Rect) -> Rect {
    match popup.width >= 5 && popup.height > 0 {
        true => Rect::new(popup.right() - 4, popup.y, 3, 1),
        false => Rect::default(),
    }
}

/// `value * permille / 1000` without the `u16` overflow a wide terminal
/// would hit.
fn permille(value: u16, permille: u16) -> u16 {
    (u32::from(value) * u32::from(permille) / 1000) as u16
}

pub(crate) fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
