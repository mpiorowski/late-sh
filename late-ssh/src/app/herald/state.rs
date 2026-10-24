//! The herald's pure rules: which day is due, what fills the day's slot,
//! and the words that travel with the picture. Reads no clock and does no
//! I/O; `svc.rs` feeds it rows and posts what comes back.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use dartboard_core::{Canvas, CellValue};
use late_core::models::herald::HeraldKind;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use super::svc::{HeraldEvent, HeraldService};

/// When in the UTC day the post goes out: late afternoon in Europe, noon
/// on the US east coast, the hours the Fediverse reads. Long after the
/// Artboard's midnight rollover archived yesterday's board.
pub(crate) const HERALD_POST_TIME: NaiveTime = match NaiveTime::from_hms_opt(16, 0, 0) {
    Some(time) => time,
    None => unreachable!(),
};

/// The fewest drawn cells yesterday's board needs to be worth a post. The
/// board is wiped on the first of the month and the days after it are a
/// few strokes on black; two hundred cells is a small picture, not a
/// smudge.
pub(crate) const BOARD_MIN_INK: usize = 200;

/// The tags the board carries, four at most: a post that is a link and a
/// pile of tags reads as spam to instance rules and readers alike. The
/// art tags are the communities that boost strangers' pictures.
const BOARD_TAGS: &str = "#ASCIIart #MastoArt #tildeverse #ssh";

/// The day whose post is due right now: today once the post time has
/// passed, yesterday before it. A replica that was down at the hour
/// catches up at its next check and stamps the day the picture belongs
/// to.
pub(crate) fn post_due_day(now: DateTime<Utc>) -> NaiveDate {
    let today = now.date_naive();
    if now.time() >= HERALD_POST_TIME {
        today
    } else {
        today - Duration::days(1)
    }
}

/// The day a post dated `day` shows: the one before it, the whole UTC day
/// the Artboard's rollover archived at midnight.
pub(crate) fn shown_day(day: NaiveDate) -> NaiveDate {
    day - Duration::days(1)
}

/// Cells carrying a glyph that is not whitespace.
pub(crate) fn ink_cells(canvas: &Canvas) -> usize {
    canvas
        .iter()
        .filter(|(_, cell)| match cell {
            CellValue::Narrow(ch) | CellValue::Wide(ch) => !ch.is_whitespace(),
            CellValue::WideCont => false,
        })
        .count()
}

/// Yesterday's board, once it has passed the ink bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BoardCandidate {
    /// The snapshot's key, `daily:YYYY-MM-DD`.
    pub(crate) key: String,
    /// The day the board shows.
    pub(crate) shown: NaiveDate,
}

/// Yesterday's board as found: the key looked up, and its ink when the
/// rollover archived one. The ink bar decides whether it is a post.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoardLook {
    pub(crate) key: String,
    pub(crate) shown: NaiveDate,
    /// Drawn cells; `None` when no snapshot exists under the key.
    pub(crate) ink: Option<usize>,
}

impl BoardLook {
    /// The post this board makes, if it has enough ink.
    pub(crate) fn picture(&self) -> Option<Picture> {
        match self.ink {
            Some(ink) if ink >= BOARD_MIN_INK => Some(Picture::Board(BoardCandidate {
                key: self.key.clone(),
                shown: self.shown,
            })),
            Some(_) | None => None,
        }
    }

    /// Why a bare day is bare, for the admin's banner.
    pub(crate) fn describe(&self) -> String {
        match self.ink {
            Some(ink) => format!("{} has {ink} drawn cells, needs {BOARD_MIN_INK}", self.key),
            None => format!("no board archived for {}", self.shown),
        }
    }
}

/// What the day's post shows. Serialized onto the day's row once picked,
/// so a retry delivers the same picture to the networks still missing.
/// One kind today; the next ones (`AUDIENCE.md`, the herald's future
/// work) are variants here, and every match below has to learn them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Picture {
    Board(BoardCandidate),
}

impl Picture {
    pub(crate) fn kind(&self) -> HeraldKind {
        match self {
            Self::Board(_) => HeraldKind::Artboard,
        }
    }

    /// What the row records: the board's key.
    pub(crate) fn subject(&self) -> String {
        match self {
            Self::Board(board) => board.key.clone(),
        }
    }

    /// The picture's route on late-web (`late-web/CONTEXT.md`, the PNG
    /// routes).
    pub(crate) fn picture_path(&self) -> String {
        match self {
            Self::Board(board) => format!("/gallery/{}/image.png", board.key),
        }
    }

    /// The page the post links to, on late-web. The gallery itself, not
    /// the day's snapshot: the Artboard keeps only the newest seven dailies
    /// (`app/artboard/CONTEXT.md`), and a post outlives that by years.
    pub(crate) fn page_path(&self) -> String {
        match self {
            Self::Board(_) => "/gallery".to_string(),
        }
    }
}

/// The words under the picture, the same on every network and under
/// Bluesky's 300 graphemes, the tightest limit among the networks
/// (`state_test` holds it there): a line of plain speech, the page, the
/// tags. Nobody is named, so no bare `@` can ping a stranger on the
/// instance.
pub(crate) fn status_text(picture: &Picture, web_url: &str) -> String {
    let page = format!("{web_url}{}", picture.page_path());
    match picture {
        Picture::Board(board) => format!(
            "The Artboard yesterday: late.sh's shared ASCII canvas, as the room left it on {}. Anyone in the clubhouse can pick up a brush.\n\n{page}\n\n{BOARD_TAGS}",
            board.shown.format("%-d %B %Y"),
        ),
    }
}

/// The picture described for readers who cannot see it.
pub(crate) fn alt_text(picture: &Picture) -> String {
    match picture {
        Picture::Board(board) => format!(
            "ASCII art: the late.sh Artboard, a shared terminal canvas, as it stood at the end of {} UTC. Coloured text glyphs on a dark background.",
            board.shown.format("%-d %B %Y"),
        ),
    }
}

/// What `/herald` asked for. Every subcommand is admin-only, refused with
/// a banner for anyone else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeraldCommand {
    /// `/herald on`: the row on, after a due day the switch slept through
    /// is settled unposted; the next 16:00 UTC check posts.
    On,
    /// `/herald off`: the row off; the next check does nothing.
    Off,
    /// `/herald status`: the row, and today's day as it stands.
    Status,
    /// `/herald preview`: what `/herald post` would send, and where,
    /// without sending it.
    Preview,
    /// `/herald post`: work today's day now, whatever the row says and
    /// whatever the hour. The 16:00 check then finds it settled.
    Post,
}

/// `None`: not a `/herald` line. `Some(None)`: `/herald` alone or with
/// junk after it.
pub(crate) fn parse_herald_command(body: &str) -> Option<Option<HeraldCommand>> {
    let rest = body.trim().strip_prefix("/herald")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    Some(match words.as_slice() {
        ["on"] => Some(HeraldCommand::On),
        ["off"] => Some(HeraldCommand::Off),
        ["status"] => Some(HeraldCommand::Status),
        ["preview"] => Some(HeraldCommand::Preview),
        ["post"] => Some(HeraldCommand::Post),
        _ => None,
    })
}

/// The session's end of the herald: the service handle for `/herald`
/// and the answers coming back. Built at session start.
pub(crate) struct HeraldState {
    pub(super) service: HeraldService,
    pub(super) rx: broadcast::Receiver<HeraldEvent>,
}

impl HeraldState {
    pub(crate) fn new(service: HeraldService) -> Self {
        let rx = service.subscribe();
        Self { service, rx }
    }
}
