//! The herald's pure rules: which day is due, what fills the day's slot,
//! and the words that travel with the picture. Reads no clock and does no
//! I/O; `svc.rs` feeds it rows and posts what comes back.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use dartboard_core::{Canvas, CellValue};
use late_core::models::artboard::Snapshot;
use late_core::models::herald::{HeraldKind, PosterCandidate};
use serde::{Deserialize, Serialize};

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

/// The tags each kind carries, four at most: a post that is a link and a
/// pile of tags reads as spam to instance rules and readers alike. The
/// art tags are the communities that boost strangers' pictures.
const BOARD_TAGS: &str = "#ASCIIart #MastoArt #tildeverse #ssh";
const POSTER_TAGS: &str = "#ASCIIart #tildeverse #ssh #terminal";

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

/// The Artboard key for the board as it stood at the end of `shown`.
pub(crate) fn board_key(shown: NaiveDate) -> String {
    format!("{}{}", Snapshot::DAILY_PREFIX, shown.format("%Y-%m-%d"))
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

/// What the day's post shows. Serialized onto the day's row once picked,
/// so a retry delivers the same picture to the networks still missing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Picture {
    Poster(PosterCandidate),
    Board(BoardCandidate),
}

impl Picture {
    pub(crate) fn kind(&self) -> HeraldKind {
        match self {
            Self::Poster(_) => HeraldKind::Poster,
            Self::Board(_) => HeraldKind::Artboard,
        }
    }

    /// What the row records: the runner's id, the board's key. The id is
    /// what keeps a poster from going out twice.
    pub(crate) fn subject(&self) -> String {
        match self {
            Self::Poster(poster) => poster.user_id.to_string(),
            Self::Board(board) => board.key.clone(),
        }
    }

    /// The picture's route on late-web (`late-web/CONTEXT.md`, the PNG
    /// routes).
    pub(crate) fn picture_path(&self) -> String {
        match self {
            Self::Poster(poster) => format!("/profiles/{}/poster.png", poster.slug),
            Self::Board(board) => format!("/gallery/{}/image.png", board.key),
        }
    }

    /// The page the post links to, on late-web.
    pub(crate) fn page_path(&self) -> String {
        match self {
            Self::Poster(poster) => format!("/profiles/{}", poster.slug),
            Self::Board(board) => format!("/gallery?key={}", board.key),
        }
    }
}

/// The rotation. A waiting poster goes out unless the last post was one,
/// then the board; the board when no poster waits; the poster again when
/// the board is too bare; nothing when both are missing.
pub(crate) fn pick_picture(
    last: Option<HeraldKind>,
    poster: Option<PosterCandidate>,
    board: Option<BoardCandidate>,
) -> Option<Picture> {
    match (last, poster, board) {
        (Some(HeraldKind::Poster), Some(poster), None) => Some(Picture::Poster(poster)),
        (Some(HeraldKind::Poster), _, Some(board)) => Some(Picture::Board(board)),
        (_, Some(poster), _) => Some(Picture::Poster(poster)),
        (_, None, Some(board)) => Some(Picture::Board(board)),
        (_, None, None) => None,
    }
}

/// The words under the picture, the same on every network and under
/// Bluesky's 300 graphemes, the tightest limit among the networks
/// (`state_test` holds it there): a line of plain speech, the page, the
/// tags. The runner
/// is named `~name`, never `@name`, since a bare `@` is a mention on the
/// instance.
pub(crate) fn status_text(picture: &Picture, web_url: &str) -> String {
    let page = format!("{web_url}{}", picture.page_path());
    match picture {
        Picture::Board(board) => format!(
            "The Artboard yesterday: late.sh's shared ASCII canvas, as the room left it on {}. Anyone in the clubhouse can pick up a brush.\n\n{page}\n\n{BOARD_TAGS}",
            board.shown.format("%-d %B %Y"),
        ),
        Picture::Poster(poster) => format!(
            "~{} painted this poster at PRINTS, the print shop under late.sh's night city, and hung it on their profile.\n\n{page}\n\n{POSTER_TAGS}",
            poster.username,
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
        Picture::Poster(poster) => format!(
            "A 32 by 12 ASCII poster painted by ~{} in coloured text glyphs on dark paper.",
            poster.username,
        ),
    }
}
