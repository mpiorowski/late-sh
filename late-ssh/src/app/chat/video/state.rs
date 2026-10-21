//! Chat's inline video: a YouTube link in a room plays inside its own
//! thumbnail, in the user's terminal, through the paired CLI's `mpv`
//! (`late-cli/VIDEO.md`). Not a frame of video passes through the server.
//! Pure state: the link and thumbnail rules, where the video sits among a
//! frame's chat rows, and the one inline video with what the last frame
//! showed of its rectangle. `ui.rs` paints and checks the rectangle;
//! `svc.rs` decides what the CLI hears.

use std::cell::Cell;
use std::time::Instant;

use late_core::models::article_read::WatchPosition;
use ratatui::layout::Rect;
use uuid::Uuid;

use crate::app::chat::news::watch::state::{Play, WantedPlay, WatchOutcome, WatchStatus};
use crate::app::chat::ui::{ChatRowHit, ChatRowKind};
use crate::app::chat::ui_text::INLINE_PREVIEW_INDENT;
use crate::paired_clients::{PairControlMessage, VideoControl, VideoReport};

/// The row under a YouTube thumbnail until its video plays there; then it
/// is the play's caption (`InlineVideo::caption`).
pub(crate) const WATCH_HINT: &str = "▶ YouTube · Enter to watch";

/// A YouTube video link in a chat message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct YoutubeLink {
    /// The canonical watch link for the video: what mpv is handed.
    pub url: String,
    pub video_id: String,
}

impl YoutubeLink {
    /// The 16:9 thumbnail YouTube serves for every video. The inline
    /// preview is drawn from it, and the video then plays in its place.
    pub(crate) fn thumbnail_url(&self) -> String {
        format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", self.video_id)
    }
}

/// The first YouTube video link in `body`, trimmed the way inline image
/// links are (`chat::state::inline_image_url_in_body`).
pub(crate) fn youtube_link_in_body(body: &str) -> Option<YoutubeLink> {
    let words = body.split(|c: char| {
        c.is_ascii_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>')
    });
    for word in words {
        let Some(start) = word.find("http") else {
            continue;
        };
        let url = word[start..].trim_end_matches(['.', ',', ';', '!', '?']);
        if let Some(video_id) = youtube_video_id(url) {
            return Some(YoutubeLink {
                url: format!("https://www.youtube.com/watch?v={video_id}"),
                video_id,
            });
        }
    }
    None
}

/// The video id in a YouTube watch, short, embed, shorts or live link: the
/// shapes the booth takes (`audio/youtube.rs`). Channels, playlists and
/// anything else carry no one video and answer `None`.
pub(crate) fn youtube_video_id(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return None,
    }
    let host = parsed.host_str()?.to_ascii_lowercase();
    let id = if host == "youtu.be" || host == "www.youtu.be" {
        parsed.path_segments()?.next()?.to_string()
    } else if host == "youtube.com" || host.ends_with(".youtube.com") {
        match parsed.path() {
            "/watch" => parsed
                .query_pairs()
                .find(|(key, _)| key == "v")?
                .1
                .into_owned(),
            _ => {
                let mut segments = parsed.path_segments()?;
                match segments.next() {
                    Some("embed" | "shorts" | "live") => segments.next()?.to_string(),
                    _ => return None,
                }
            }
        }
    } else {
        return None;
    };
    let valid = id.len() == 11
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    match valid {
        true => Some(id),
        false => None,
    }
}

/// Where the video goes in a frame's chat rows: the thumbnail's picture,
/// `picture` cells (cols, rows), [`INLINE_PREVIEW_INDENT`] in from `area`'s
/// left edge. The thumbnail is the message's run of `Image` rows, the
/// picture and the hint row under it. `None` unless all of it is on screen
/// and the picture fits the width: mpv draws the whole picture or nothing.
pub(crate) fn video_rect(
    area: Rect,
    hits: &[ChatRowHit],
    message_id: Uuid,
    picture: (u16, u16),
) -> Option<Rect> {
    let (cols, rows) = picture;
    if cols == 0 || rows == 0 {
        return None;
    }
    let first = hits
        .iter()
        .position(|hit| is_preview_row(hit, message_id))?;
    let run = hits[first..]
        .iter()
        .take_while(|hit| is_preview_row(hit, message_id))
        .count();
    let whole = run == usize::from(rows) + 1;
    let fits = INLINE_PREVIEW_INDENT.saturating_add(cols) <= area.width;
    match (whole, fits) {
        (true, true) => Some(Rect::new(
            area.x + INLINE_PREVIEW_INDENT,
            area.y + u16::try_from(first).ok()?,
            cols,
            rows,
        )),
        (true, false) | (false, _) => None,
    }
}

fn is_preview_row(hit: &ChatRowHit, message_id: Uuid) -> bool {
    let preview = match &hit.kind {
        ChatRowKind::Image => true,
        ChatRowKind::None | ChatRowKind::Body | ChatRowKind::Header(_) => false,
    };
    preview && hit.message_id == Some(message_id)
}

/// What the last frame showed of the video's rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sight {
    /// The thumbnail was not wholly on screen: scrolled off, another room,
    /// another page.
    Gone,
    /// On screen, with something drawn over the rectangle by the end of
    /// the frame: a modal, a popup, the screensaver.
    Covered(Rect),
    /// On screen and nothing over it.
    Clear(Rect),
}

/// What a frame's sight does to the inline video (`InlineVideo::step`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Keep,
    /// Off the screen: the video ends.
    Close,
    /// A live play went under something: the CLI stops it, and it waits
    /// at its position.
    Park,
    /// A parked video's rectangle is clear again: it plays on from there.
    Unpark,
}

/// Where the inline video stands.
#[derive(Debug)]
enum Phase {
    /// Asked of the CLI (or about to be, at the next sync), as it last
    /// reported. The play's own status says whether mpv is still up.
    Play(Play),
    /// Something was drawn over the video, so the CLI was told to stop. It
    /// comes back from here, under a fresh `play_id`, once the rectangle
    /// is clear again.
    Parked { resume_from: Option<WatchPosition> },
}

/// The one YouTube link playing inside its thumbnail in the chat.
#[derive(Debug)]
pub(crate) struct InlineVideo {
    message_id: Uuid,
    url: String,
    phase: Phase,
    /// The rectangle the current frame's chat draw found the picture in;
    /// `None` until it does (`begin_frame` resets it).
    drawn: Cell<Option<Rect>>,
    /// Whether the rectangle was still the video's own black once every
    /// layer of the frame was drawn (`ui::check_clear`).
    clear: Cell<bool>,
}

impl InlineVideo {
    /// A fresh play of `url` inside `message_id`'s thumbnail.
    pub(crate) fn new(message_id: Uuid, url: String, play_id: u64, now: Instant) -> Self {
        Self {
            message_id,
            phase: Phase::Play(Play::new(play_id, url.clone(), now, None)),
            url,
            drawn: Cell::new(None),
            clear: Cell::new(false),
        }
    }

    pub(crate) fn message_id(&self) -> Uuid {
        self.message_id
    }

    /// The play the CLI knows about; none while parked.
    pub(crate) fn play_id(&self) -> Option<u64> {
        match &self.phase {
            Phase::Play(play) => Some(play.play_id()),
            Phase::Parked { .. } => None,
        }
    }

    /// Whether mpv is up for it, or it is parked waiting to come back: the
    /// two ways the rectangle is the video's and not the thumbnail's.
    pub(crate) fn holds_rect(&self) -> bool {
        match &self.phase {
            Phase::Play(play) => play.is_live(),
            Phase::Parked { .. } => true,
        }
    }

    /// Whether a play is running or starting on the CLI.
    pub(crate) fn is_live(&self) -> bool {
        match &self.phase {
            Phase::Play(play) => play.is_live(),
            Phase::Parked { .. } => false,
        }
    }

    /// The top of a frame: nothing drawn yet.
    pub(crate) fn begin_frame(&self) {
        self.drawn.set(None);
        self.clear.set(false);
    }

    pub(crate) fn drawn(&self) -> Option<Rect> {
        self.drawn.get()
    }

    pub(crate) fn set_drawn(&self, rect: Option<Rect>) {
        self.drawn.set(rect);
    }

    pub(crate) fn set_clear(&self, clear: bool) {
        self.clear.set(clear);
    }

    pub(crate) fn sight(&self) -> Sight {
        match (self.drawn.get(), self.clear.get()) {
            (None, _) => Sight::Gone,
            (Some(rect), false) => Sight::Covered(rect),
            (Some(rect), true) => Sight::Clear(rect),
        }
    }

    /// What the last frame's sight does to the video. A play that is over
    /// (ended, failed, lost) has nothing to park and stays until it leaves
    /// the screen.
    pub(crate) fn step(&self) -> Step {
        match (self.sight(), &self.phase) {
            (Sight::Gone, Phase::Play(_) | Phase::Parked { .. }) => Step::Close,
            (Sight::Covered(_), Phase::Play(play)) => match play.is_live() {
                true => Step::Park,
                false => Step::Keep,
            },
            (Sight::Covered(_), Phase::Parked { .. }) => Step::Keep,
            (Sight::Clear(_), Phase::Parked { .. }) => Step::Unpark,
            (Sight::Clear(_), Phase::Play(_)) => Step::Keep,
        }
    }

    /// Park at the play's position: the last one reported, or where it
    /// began when none came in yet.
    pub(crate) fn parked(self) -> Self {
        let resume_from = match &self.phase {
            Phase::Play(play) => match play.last_seen() {
                Some(position) => Some(position),
                None => play.resume_from(),
            },
            Phase::Parked { resume_from } => *resume_from,
        };
        Self {
            phase: Phase::Parked { resume_from },
            ..self
        }
    }

    /// Back from parked under `play_id`, from where it was parked.
    pub(crate) fn unparked(self, play_id: u64, now: Instant) -> Self {
        let resume_from = match &self.phase {
            Phase::Parked { resume_from } => *resume_from,
            Phase::Play(play) => play.resume_from(),
        };
        let play = Play::new(play_id, self.url.clone(), now, resume_from);
        Self {
            phase: Phase::Play(play),
            ..self
        }
    }

    /// Enter on a video that is over: watched to the end it starts from
    /// the top, failed or lost it tries again from where it got to.
    pub(crate) fn replayed(self, play_id: u64, now: Instant) -> Self {
        let resume_from = match &self.phase {
            Phase::Play(play) => match play.status() {
                WatchStatus::Ended => None,
                WatchStatus::Starting
                | WatchStatus::Playing(_)
                | WatchStatus::Paused(_)
                | WatchStatus::Failed
                | WatchStatus::Lost => match play.last_seen() {
                    Some(position) => Some(position),
                    None => play.resume_from(),
                },
            },
            Phase::Parked { resume_from } => *resume_from,
        };
        let play = Play::new(play_id, self.url.clone(), now, resume_from);
        Self {
            phase: Phase::Play(play),
            ..self
        }
    }

    /// The play the CLI should be running: this one, in the rectangle the
    /// last frame left clear.
    pub(crate) fn wanted(&self) -> Option<WantedPlay<'_>> {
        match (&self.phase, self.sight()) {
            (Phase::Play(play), Sight::Clear(rect)) => play.wanted(rect),
            (Phase::Play(_), Sight::Gone | Sight::Covered(_)) | (Phase::Parked { .. }, _) => None,
        }
    }

    /// The control message for a key, while mpv is up.
    pub(crate) fn control(&self, control: VideoControl) -> Option<PairControlMessage> {
        match &self.phase {
            Phase::Play(play) => play.control(control),
            Phase::Parked { .. } => None,
        }
    }

    /// One `video_state` from the CLI, already matched to this play.
    pub(crate) fn apply(&mut self, report: &VideoReport, now: Instant) -> Option<WatchOutcome> {
        match &mut self.phase {
            Phase::Play(play) => play.apply(report, now),
            Phase::Parked { .. } => None,
        }
    }

    /// A playing or paused play the CLI has gone quiet on.
    pub(crate) fn expire(&mut self, now: Instant) -> Option<WatchOutcome> {
        match &mut self.phase {
            Phase::Play(play) => play.expire(now),
            Phase::Parked { .. } => None,
        }
    }

    /// A send that reached no paired CLI: a live play is lost at once.
    pub(crate) fn lose(&mut self) -> Option<WatchOutcome> {
        match &mut self.phase {
            Phase::Play(play) => play.lose(),
            Phase::Parked { .. } => None,
        }
    }

    /// The thumbnail's last row while the video is here: where the play
    /// is and what the keys do. A parked video is under something, so the
    /// one frame that shows its caption is the one it comes back on.
    pub(crate) fn caption(&self, now: Instant) -> String {
        match &self.phase {
            Phase::Play(play) => match play.status() {
                WatchStatus::Starting | WatchStatus::Playing(_) => {
                    format!("{} · Enter pause · Esc stop", play.status_text(now))
                }
                WatchStatus::Paused(_) => {
                    format!("{} · Enter play · Esc stop", play.status_text(now))
                }
                WatchStatus::Ended => "ended · Enter to watch again".to_string(),
                WatchStatus::Failed | WatchStatus::Lost => {
                    format!("{} · Enter to try again", play.status_text(now))
                }
            },
            Phase::Parked { .. } => "resuming…".to_string(),
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
