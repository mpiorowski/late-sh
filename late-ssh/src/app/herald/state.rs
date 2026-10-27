//! The herald's pure rules: which day is due, what fills the day's slot
//! (the week's schedule of the kinds), the words that travel with the
//! picture, and how each network marks what the look saw in it. Reads no
//! clock and does no I/O; `svc.rs` feeds it rows and posts what comes
//! back.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc, Weekday};
use dartboard_core::{Canvas, CellValue};
use late_core::models::herald::HeraldKind;
use late_core::models::leaderboard::{WeekBest, week_first_day};
use late_core::models::paper::PaperSectionKind;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

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

/// The tags the board and the gallery's pieces carry, four at most: a
/// post that is a link and a pile of tags reads as spam to instance rules
/// and readers alike. The art tags are the communities that boost
/// strangers' pictures.
const ART_TAGS: [&str; 4] = ["#ASCIIart", "#MastoArt", "#tildeverse", "#terminal"];

/// The weekly scoreboard's: the puzzle crowd, then the member tags.
const SCOREBOARD_TAGS: [&str; 4] = ["#puzzles", "#sudoku", "#tildeverse", "#terminal"];

/// The paper's: never `#MastoArt`, since art communities dislike
/// model-written text.
const PAPER_TAGS: [&str; 3] = ["#tildeverse", "#terminal", "#smolweb"];

/// A Mastodon note in place of a marked picture: no art tags on a post
/// with no art.
const NOTE_TAGS: [&str; 2] = ["#tildeverse", "#terminal"];

/// The tag a marked Bluesky post carries in place of its kind's last:
/// people there hide posts by muting it.
const NSFW_TAG: &str = "#nsfw";

/// The longest alt text the look keeps, before the `ASCII art: ` prefix.
/// Mastodon allows 1,500; the model is asked for 800.
pub(crate) const ALT_MAX_CHARS: usize = 1000;

/// The longest an announcement's words run: under Bluesky's 300 with what
/// `status_text` puts after them, the edition's page
/// (`https://late.sh/paper/YYYY-MM-DD`, 32), two blank lines (4) and
/// `ANNOUNCEMENT_TAGS` (21). The brief is asked for this many.
pub(crate) const ANNOUNCEMENT_MAX_CHARS: usize = 240;

/// An announcement's: no art tags on a post with no art.
const ANNOUNCEMENT_TAGS: [&str; 2] = ["#tildeverse", "#terminal"];

/// Blank cells the board's crop keeps around what is drawn.
pub(crate) const CROP_MARGIN: usize = 2;

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
/// the Artboard's rollover archived at midnight and the paper's edition
/// dated `day` reads.
pub(crate) fn shown_day(day: NaiveDate) -> NaiveDate {
    day - Duration::days(1)
}

/// The last day of the week a post dated `day` shows on the scoreboard:
/// on Sundays the week to yesterday, Saturday; no scoreboard otherwise.
pub(crate) fn scoreboard_week(day: NaiveDate) -> Option<NaiveDate> {
    match day.weekday() {
        Weekday::Sun => Some(shown_day(day)),
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri | Weekday::Sat => {
            None
        }
    }
}

/// The week's schedule: the kinds a day dated `day` tries, in order, the
/// first with something to show being the post. Art most days (the piece
/// over the door, else yesterday's board), the paper on Wednesday and
/// Saturday, the week's scoreboard on Sunday; a day whose kind has
/// nothing falls through to the next, so a quiet paper day still shows a
/// piece and a Sunday nobody played still shows something.
pub(crate) fn schedule(day: NaiveDate) -> &'static [HeraldKind] {
    match day.weekday() {
        Weekday::Mon | Weekday::Tue | Weekday::Thu | Weekday::Fri => {
            &[HeraldKind::Piece, HeraldKind::Artboard, HeraldKind::Paper]
        }
        Weekday::Wed | Weekday::Sat => {
            &[HeraldKind::Paper, HeraldKind::Piece, HeraldKind::Artboard]
        }
        Weekday::Sun => &[
            HeraldKind::Scoreboard,
            HeraldKind::Paper,
            HeraldKind::Piece,
            HeraldKind::Artboard,
        ],
    }
}

/// Cells carrying a glyph that is not whitespace.
pub(crate) fn ink_cells(canvas: &Canvas) -> usize {
    canvas.iter().filter(|(_, cell)| drawn(cell)).count()
}

fn drawn(cell: &CellValue) -> bool {
    match cell {
        CellValue::Narrow(ch) | CellValue::Wide(ch) => !ch.is_whitespace(),
        CellValue::WideCont => false,
    }
}

/// A rectangle of the board in cells, as late-web's `?crop=` reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Crop {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

/// The drawn area of `canvas` with `CROP_MARGIN` around it, inside the
/// board; `None` on a board with nothing drawn. A sparse board early in
/// the month is a few strokes, not a dark field with a few strokes in it.
pub(crate) fn drawn_area(canvas: &Canvas) -> Option<Crop> {
    let mut cells = canvas
        .iter()
        .filter(|(_, cell)| drawn(cell))
        .map(|(pos, cell)| {
            let wide = match cell {
                CellValue::Wide(_) => 1,
                CellValue::Narrow(_) | CellValue::WideCont => 0,
            };
            (pos.x, pos.y, pos.x + wide)
        });
    let (x, y, right) = cells.next()?;
    let (left, top, right, bottom) = cells.fold((x, y, right, y), |(l, t, r, b), (x, y, right)| {
        (l.min(x), t.min(y), r.max(right), b.max(y))
    });
    let left = left.saturating_sub(CROP_MARGIN);
    let top = top.saturating_sub(CROP_MARGIN);
    let right = (right + CROP_MARGIN).min(canvas.width - 1);
    let bottom = (bottom + CROP_MARGIN).min(canvas.height - 1);
    Some(Crop {
        x: left,
        y: top,
        width: right - left + 1,
        height: bottom - top + 1,
    })
}

/// Yesterday's board, once it has passed the ink bar, and the part of it
/// that is drawn.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BoardCandidate {
    /// The snapshot's key, `daily:YYYY-MM-DD`.
    pub(crate) key: String,
    /// The day the board shows.
    pub(crate) shown: NaiveDate,
    /// What the post shows of it.
    pub(crate) crop: Crop,
}

/// What a board's canvas holds: its drawn cells and where they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BoardInk {
    pub(crate) cells: usize,
    /// `None` only when nothing is drawn.
    pub(crate) area: Option<Crop>,
}

impl BoardInk {
    pub(crate) fn of(canvas: &Canvas) -> Self {
        Self {
            cells: ink_cells(canvas),
            area: drawn_area(canvas),
        }
    }
}

/// Yesterday's board as found: the key looked up, and its ink when the
/// rollover archived one. The ink bar decides whether it is a post; what
/// the picture shows is the look's business (`Look`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BoardFound {
    pub(crate) key: String,
    pub(crate) shown: NaiveDate,
    /// `None` when no snapshot exists under the key.
    pub(crate) ink: Option<BoardInk>,
}

impl BoardFound {
    /// The post this board makes, if it has enough ink.
    pub(crate) fn picture(&self) -> Option<Picture> {
        match self.ink {
            Some(BoardInk {
                cells,
                area: Some(crop),
            }) if cells >= BOARD_MIN_INK => Some(Picture::Board(BoardCandidate {
                key: self.key.clone(),
                shown: self.shown,
                crop,
            })),
            Some(_) | None => None,
        }
    }

    /// Why a bare day is bare, for the admin's banner.
    pub(crate) fn describe(&self) -> String {
        match self.ink {
            Some(ink) => format!(
                "{} has {} drawn cells, needs {BOARD_MIN_INK}",
                self.key, ink.cells
            ),
            None => format!("no board archived for {}", self.shown),
        }
    }
}

/// The piece over the door today: the splash wall's piece for the day
/// (`ArtboardPiece::splash_for_day`, hang order, one a day), credited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PieceCandidate {
    pub(crate) id: Uuid,
    pub(crate) title: String,
    pub(crate) artist: String,
}

/// The week's best in every game (the arcade, the Lobby, the doors), one
/// line per game anyone played, nobody named: the card is public and the
/// names stay on the terminal's boards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScoreboardCandidate {
    /// The week's last day; it runs `WEEK_DAYS` back from here.
    pub(crate) last: NaiveDate,
    pub(crate) lines: Vec<ScoreLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScoreLine {
    pub(crate) game: String,
    /// `6 wins`, `12,340`.
    pub(crate) best: String,
}

impl ScoreboardCandidate {
    /// The week ending `last`, if anyone played in it.
    pub(crate) fn of(last: NaiveDate, bests: &[WeekBest]) -> Option<Self> {
        if bests.is_empty() {
            return None;
        }
        Some(Self {
            last,
            lines: bests
                .iter()
                .map(|best| ScoreLine {
                    game: best.game.title().to_string(),
                    best: best.shown_value(),
                })
                .collect(),
        })
    }
}

/// The Late Edition dated the post's day, which reads the day before, once
/// a public column of it printed: the whole of YESTERDAY (its lines are
/// the numbers, and few) and the first paragraph of the other two, for
/// the alt text (the picture is late-web's card of the same columns).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaperCandidate {
    pub(crate) edition: NaiveDate,
    /// The column's lines joined with `; `.
    #[serde(default)]
    pub(crate) yesterday: Option<String>,
    pub(crate) outside: Option<String>,
    pub(crate) reading: Option<String>,
}

impl PaperCandidate {
    /// The edition, if any public column printed. A column's paragraphs
    /// are its non-empty lines, as the web prints them.
    pub(crate) fn of(
        edition: NaiveDate,
        yesterday: Option<&str>,
        outside: Option<&str>,
        reading: Option<&str>,
    ) -> Option<Self> {
        let paragraphs = |text: &str| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        let head = |text: Option<&str>| text.and_then(|text| paragraphs(text).into_iter().next());
        let whole = |text: Option<&str>| {
            text.map(|text| paragraphs(text).join("; "))
                .filter(|joined| !joined.is_empty())
        };
        match (whole(yesterday), head(outside), head(reading)) {
            (None, None, None) => None,
            (yesterday, outside, reading) => Some(Self {
                edition,
                yesterday,
                outside,
                reading,
            }),
        }
    }
}

/// The operator's `#announcements` posts of the day an edition covers, as
/// written, once the edition is swept: what the brief compresses
/// (`brief.rs`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AnnouncementFound {
    pub(crate) edition: NaiveDate,
    pub(crate) text: String,
}

/// Where an announcement's words came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Brief {
    /// The model compressed the note.
    Model,
    /// The note as written, cut at the limit: the model is off or failed.
    Cut,
}

impl Brief {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Cut => "cut",
        }
    }
}

/// The day's announcement as it goes out: one text, no picture, the
/// edition's page under it for the full note.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AnnouncementCandidate {
    pub(crate) edition: NaiveDate,
    pub(crate) text: String,
    pub(crate) brief: Brief,
}

impl AnnouncementCandidate {
    /// The words made safe for the account and cut to
    /// `ANNOUNCEMENT_MAX_CHARS`, whoever wrote them: markdown markers
    /// dropped (`plain_lines`), dashes made commas (`no_dashes`), every
    /// `@`, `#` and `scheme:` dead (`dead_words`), so a note as written
    /// cannot tag or mention anyone and the model cannot either.
    pub(crate) fn of(edition: NaiveDate, text: &str, brief: Brief) -> Self {
        Self {
            edition,
            text: cut_to(
                &dead_words(&no_dashes(&plain_lines(text))),
                ANNOUNCEMENT_MAX_CHARS,
            ),
            brief,
        }
    }
}

/// `text` with every em or en dash used as punctuation made a comma: the
/// house style has none, and the model reaches for them whatever the
/// prompt says.
pub(crate) fn no_dashes(text: &str) -> String {
    text.replace(" — ", ", ")
        .replace(" – ", ", ")
        .replace(['—', '–'], ", ")
}

/// `text` line by line, trimmed, without the markdown markers a note is
/// written with (`#` headings, `-` and `*` bullets, `>` quotes), blank
/// lines dropped. The note reads as sentences on a network that shows
/// markdown as typed and reads `#word` as a tag.
pub(crate) fn plain_lines(text: &str) -> String {
    text.lines()
        .map(|line| line.trim().trim_start_matches(['#', '-', '*', '>']).trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Everything a day could post, as found: the day's choice is the first
/// with something (`choice`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    /// The edition's announcements, when the operator spoke.
    pub(crate) announcement: Option<AnnouncementFound>,
    /// Sundays only, and only when anyone played.
    pub(crate) scoreboard: Option<ScoreboardCandidate>,
    /// The day's piece over the door, when the wall has one.
    pub(crate) piece: Option<PieceCandidate>,
    pub(crate) board: BoardFound,
    pub(crate) paper: Option<PaperCandidate>,
}

/// What a day posts: a picture, or an announcement still to be briefed
/// (the brief is a model call, `svc.rs`'s to make).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Picture(Picture),
    Announcement(AnnouncementFound),
}

impl Found {
    /// The day's post: an announcement jumps the schedule, else the first
    /// kind of the day's `schedule` that has something, else nothing.
    pub(crate) fn choice(&self, day: NaiveDate) -> Option<Choice> {
        [HeraldKind::Announcement]
            .iter()
            .chain(schedule(day))
            .find_map(|kind| self.of(*kind))
    }

    /// What was found of one kind.
    fn of(&self, kind: HeraldKind) -> Option<Choice> {
        let picture = match kind {
            HeraldKind::Announcement => {
                return self.announcement.clone().map(Choice::Announcement);
            }
            HeraldKind::Scoreboard => self
                .scoreboard
                .as_ref()
                .map(|scoreboard| Picture::Scoreboard(scoreboard.clone())),
            HeraldKind::Piece => self
                .piece
                .as_ref()
                .map(|piece| Picture::Piece(piece.clone())),
            HeraldKind::Artboard => self.board.picture(),
            HeraldKind::Paper => self
                .paper
                .as_ref()
                .map(|paper| Picture::Paper(paper.clone())),
        };
        picture.map(Choice::Picture)
    }

    /// Why a bare day is bare, for the admin's banner.
    pub(crate) fn describe(&self) -> String {
        format!(
            "no announcement, no piece over the door, {}, no edition printed",
            self.board.describe()
        )
    }
}

/// What the day's post shows. Serialized onto the day's row once picked,
/// so a retry delivers the same picture to the networks still missing.
/// A new kind is a variant here, and every match below has to learn it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Picture {
    Board(BoardCandidate),
    Piece(PieceCandidate),
    Scoreboard(ScoreboardCandidate),
    Paper(PaperCandidate),
    /// Words only: no picture, no look.
    Announcement(AnnouncementCandidate),
}

impl Picture {
    pub(crate) fn kind(&self) -> HeraldKind {
        match self {
            Self::Board(_) => HeraldKind::Artboard,
            Self::Piece(_) => HeraldKind::Piece,
            Self::Scoreboard(_) => HeraldKind::Scoreboard,
            Self::Paper(_) => HeraldKind::Paper,
            Self::Announcement(_) => HeraldKind::Announcement,
        }
    }

    /// What the row records: what the post is of.
    pub(crate) fn subject(&self) -> String {
        match self {
            Self::Board(board) => board.key.clone(),
            Self::Piece(piece) => format!("piece:{}", piece.id),
            Self::Scoreboard(scoreboard) => format!("week:{}", scoreboard.last),
            Self::Paper(paper) => format!("paper:{}", paper.edition),
            Self::Announcement(announcement) => format!("announcement:{}", announcement.edition),
        }
    }

    /// The picture's route on late-web (`late-web/CONTEXT.md`, the PNG
    /// routes); `None` for a post of words alone.
    pub(crate) fn picture_path(&self) -> Option<String> {
        match self {
            Self::Board(board) => Some(format!(
                "/gallery/{}/image.png?crop={},{},{},{}",
                board.key, board.crop.x, board.crop.y, board.crop.width, board.crop.height
            )),
            Self::Piece(piece) => Some(format!("/gallery/pieces/{}/image.png", piece.id)),
            Self::Scoreboard(scoreboard) => {
                Some(format!("/games/week/{}/image.png", scoreboard.last))
            }
            Self::Paper(paper) => Some(format!("/paper/{}/image.png", paper.edition)),
            Self::Announcement(_) => None,
        }
    }

    /// The page the post links to on late-web, `None` for the front page.
    /// The board links the gallery itself, not the day's snapshot: the
    /// Artboard keeps only the newest seven dailies
    /// (`app/artboard/CONTEXT.md`), and a post outlives that by years. The
    /// scoreboard has no page; the boards live in the terminal. An
    /// announcement links the edition, which carries the note in full.
    pub(crate) fn page_path(&self) -> Option<String> {
        match self {
            Self::Board(_) => Some("/gallery".to_string()),
            Self::Piece(piece) => Some(format!("/gallery/pieces/{}", piece.id)),
            Self::Scoreboard(_) => None,
            Self::Paper(paper) => Some(format!("/paper/{}", paper.edition)),
            Self::Announcement(announcement) => Some(format!("/paper/{}", announcement.edition)),
        }
    }
}

/// What the look labelled a picture with: the one warning a post carries.
/// Closed, so a new warning breaks the build until each network's map
/// below knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Warning {
    /// Bodies, not sexual.
    Nudity,
    Suggestive,
    Explicit,
    Gore,
    /// A slur, a hate symbol or harassment drawn on the board.
    Offensive,
}

impl Warning {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Nudity => "nudity",
            Self::Suggestive => "suggestive",
            Self::Explicit => "explicit",
            Self::Gore => "gore",
            Self::Offensive => "offensive",
        }
    }
}

/// What the look saw in the day's picture. Stored on the day's row with
/// the picture (migration 255), so a retry posts the same alt text and
/// the same mark and never asks the model again. The look never skips a
/// post: it decides the alt text and the warning, and with them how each
/// network shows it (`mastodon_post`, `bluesky_mark`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Look {
    /// The model described it. `alt` is the description without the
    /// prefix, at most `ALT_MAX_CHARS`; `warning` is `None` for a clean
    /// picture; `reason` names the piece that set the warning, for the
    /// operator (empty when clean).
    Seen {
        alt: String,
        warning: Option<Warning>,
        reason: String,
    },
    /// Nobody looked: the model failed, or AI is off. Posted with the
    /// template alt text and marked not reviewed, never unmarked.
    Unseen,
    /// A card late.sh drew from its own words (the scoreboard, the paper):
    /// clean by construction, its alt text written from the same words, no
    /// model asked.
    Drawn,
}

impl Look {
    /// One word for the log line and the banner.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Seen { warning: None, .. } => "clean",
            Self::Seen {
                warning: Some(warning),
                ..
            } => warning.as_str(),
            Self::Unseen => "unseen",
            Self::Drawn => "drawn",
        }
    }
}

/// How a Mastodon post goes out. Fosstodon, the herald's instance, bans
/// adult and violent content outright, a content warning or not (its
/// rules, `CONTEXT.md`, Gotchas), so only a clean look carries the
/// picture. Anything else is a note behind the warning that points at the
/// front page, whose preview card is the site's own picture: a link to
/// the gallery would unfurl into the very picture the note leaves out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MastodonPost {
    Picture,
    Note { warning: &'static str },
}

pub(crate) fn mastodon_post(look: &Look) -> MastodonPost {
    let note = |warning| MastodonPost::Note { warning };
    match look {
        Look::Seen { warning, .. } => match warning {
            None => MastodonPost::Picture,
            Some(Warning::Nudity) => note("nsfw: nudity"),
            Some(Warning::Suggestive) => note("nsfw: suggestive"),
            Some(Warning::Explicit) => note("nsfw: explicit"),
            Some(Warning::Gore) => note("cw: gore"),
            Some(Warning::Offensive) => note("cw: offensive"),
        },
        Look::Unseen => note("cw: not reviewed"),
        Look::Drawn => MastodonPost::Picture,
    }
}

/// How the words under the picture carry a mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Words {
    /// The words as they are: a clean post.
    Plain,
    /// `#nsfw` in place of the kind's last tag, so the post keeps its tag
    /// count, and `first_line` above everything when the network has no
    /// label for what the warning is.
    Marked { first_line: Option<&'static str> },
}

/// How a Bluesky post is marked. Bluesky has self-labels, not warnings:
/// the label (`com.atproto.label.defs#selfLabels`) blurs the image for
/// whoever hides that kind, and every marked post carries `#nsfw` in its
/// words for whoever mutes the tag. Where no label fits (offensive, not
/// reviewed) a first line says what it is, and the image is not blurred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BlueskyMark {
    pub(crate) label: Option<&'static str>,
    pub(crate) words: Words,
}

pub(crate) fn bluesky_mark(look: &Look) -> BlueskyMark {
    let labelled = |label| BlueskyMark {
        label: Some(label),
        words: Words::Marked { first_line: None },
    };
    let plain = BlueskyMark {
        label: None,
        words: Words::Plain,
    };
    match look {
        Look::Seen { warning, .. } => match warning {
            None => plain,
            Some(Warning::Nudity) => labelled("nudity"),
            Some(Warning::Suggestive) => labelled("sexual"),
            Some(Warning::Explicit) => labelled("porn"),
            Some(Warning::Gore) => labelled("graphic-media"),
            Some(Warning::Offensive) => BlueskyMark {
                label: None,
                words: Words::Marked {
                    first_line: Some("cw: offensive"),
                },
            },
        },
        Look::Unseen => BlueskyMark {
            label: None,
            words: Words::Marked {
                first_line: Some("cw: not reviewed"),
            },
        },
        Look::Drawn => plain,
    }
}

/// `day` as the words write it: `24 October 2026`.
pub(crate) fn long_date(day: NaiveDate) -> String {
    day.format("%-d %B %Y").to_string()
}

/// The week ending `last` as the words write it: `18 October to 24
/// October 2026`.
fn week_span(last: NaiveDate) -> String {
    format!(
        "{} to {}",
        week_first_day(last).format("%-d %B"),
        long_date(last)
    )
}

/// Words the words carry dead to every network: a piece's title a member
/// typed, an announcement the operator or the model wrote, and the
/// official account must not mention, tag or link whatever they say. `@`
/// and `#` go, with the full-width forms Mastodon also reads, and a colon
/// always has a space after it, so no `scheme:` survives to open a link
/// (`https: //`, `magnet: ?`).
pub(crate) fn dead_words(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut chars = title.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '@' | '＠' | '#' | '＃' => {}
            ':' => {
                out.push(':');
                match chars.peek() {
                    Some(next) if !next.is_whitespace() => out.push(' '),
                    Some(_) | None => {}
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// What the post is, in a sentence of plain speech, and its tags.
fn speech(picture: &Picture) -> (String, &'static [&'static str]) {
    match picture {
        Picture::Board(board) => (
            format!(
                "The Artboard yesterday: late.sh's shared ASCII canvas, as the room left it on {}. Anyone in the clubhouse can pick up a brush.",
                long_date(board.shown),
            ),
            &ART_TAGS,
        ),
        Picture::Piece(piece) => (
            format!(
                "\"{}\" by ~{}, hung in the late.sh gallery: drawn cell by cell on the shared ASCII canvas, over SSH.",
                dead_words(&piece.title),
                piece.artist,
            ),
            &ART_TAGS,
        ),
        Picture::Scoreboard(scoreboard) => (
            format!(
                "The week's best in every late.sh game, {}: the daily puzzles, the arcade's high scores, chess and pool in the Lobby, the roguelike doors, all played in a terminal.",
                week_span(scoreboard.last),
            ),
            &SCOREBOARD_TAGS,
        ),
        Picture::Paper(paper) => (
            format!(
                "The Late Edition for {}: late.sh's daily paper, with the day before in numbers, what the clubhouse was reading and a look outside.",
                long_date(paper.edition),
            ),
            &PAPER_TAGS,
        ),
        Picture::Announcement(announcement) => (announcement.text.clone(), &ANNOUNCEMENT_TAGS),
    }
}

/// The words under the picture, under Bluesky's 300 graphemes, the
/// tightest limit among the networks (`state_test` holds it there, marks
/// included): a line of plain speech, the page, the tags. A member a kind
/// credits is `~name`, never `@name`, so no bare `@` can ping a stranger
/// on the instance, and a piece's title goes in dead (`title_words`).
/// Every network that shows the picture gets the same
/// words but for the mark (`Words`).
pub(crate) fn status_text(picture: &Picture, web_url: &str, words: Words) -> String {
    let page = match picture.page_path() {
        Some(path) => format!("{web_url}{path}"),
        None => web_url.to_string(),
    };
    let (speech, tags) = speech(picture);
    match words {
        Words::Plain => format!("{speech}\n\n{page}\n\n{}", tags.join(" ")),
        Words::Marked { first_line } => {
            let (kept, _) = tags.split_at(tags.len() - 1);
            let body = format!("{speech}\n\n{page}\n\n{} {NSFW_TAG}", kept.join(" "));
            match first_line {
                Some(line) => format!("{line}\n\n{body}"),
                None => body,
            }
        }
    }
}

/// The words of a Mastodon note (`MastodonPost::Note`), behind the
/// warning: what the post was, that the picture stays off the server,
/// where it is. The link is the front page, never the picture's own page.
pub(crate) fn note_text(picture: &Picture, web_url: &str) -> String {
    let what = match picture {
        Picture::Board(board) => format!(
            "Yesterday's Artboard, as the room left it on {}",
            long_date(board.shown)
        ),
        Picture::Piece(piece) => format!(
            "\"{}\" by ~{}, a new piece in the late.sh gallery",
            dead_words(&piece.title),
            piece.artist
        ),
        Picture::Scoreboard(scoreboard) => format!(
            "The week's best in every late.sh game, to {}",
            long_date(scoreboard.last)
        ),
        Picture::Paper(paper) => format!("The Late Edition for {}", long_date(paper.edition)),
        Picture::Announcement(announcement) => {
            format!("late.sh's news of {}", long_date(announcement.edition))
        }
    };
    format!(
        "{what}, is behind a content warning, so this server gets a note and not the picture. It is on late.sh, and on Bluesky with a label.\n\n{web_url}\n\n{}",
        NOTE_TAGS.join(" ")
    )
}

/// `text` cut to `max` characters, an ellipsis where it was cut.
pub(crate) fn cut_to(text: &str, max: usize) -> String {
    if text.chars().count() > max {
        let mut cut: String = text.chars().take(max - 1).collect();
        cut.push('…');
        cut
    } else {
        text.to_string()
    }
}

/// `text` cut to `ALT_MAX_CHARS`.
pub(crate) fn cut_alt(text: &str) -> String {
    cut_to(text, ALT_MAX_CHARS)
}

/// The picture described for readers who cannot see it: what the look
/// saw, else a template that says what the picture is, if not what is
/// on it. A card's is its own words. `None` for a post with no picture.
pub(crate) fn alt_text(picture: &Picture, look: &Look) -> Option<String> {
    let alt = match (picture, look) {
        (Picture::Announcement(_), _) => return None,
        (Picture::Board(_) | Picture::Piece(_), Look::Seen { alt, .. }) => {
            format!("ASCII art: {alt}")
        }
        (Picture::Board(board), Look::Unseen | Look::Drawn) => format!(
            "ASCII art: the late.sh Artboard, a shared terminal canvas, as it stood at the end of {} UTC. Coloured text glyphs on a dark background.",
            long_date(board.shown),
        ),
        (Picture::Piece(piece), Look::Unseen | Look::Drawn) => format!(
            "ASCII art: \"{}\", a piece hung in the late.sh gallery. Coloured text glyphs on a dark background.",
            piece.title,
        ),
        (Picture::Scoreboard(scoreboard), _) => cut_alt(&format!(
            "A scoreboard in green and grey terminal text: the week's best in every late.sh game, {}. {}.",
            week_span(scoreboard.last),
            scoreboard
                .lines
                .iter()
                .map(|line| format!("{}: {}", line.game, line.best))
                .collect::<Vec<_>>()
                .join("; "),
        )),
        (Picture::Paper(paper), _) => {
            let mut text = format!(
                "The front page of the Late Edition, late.sh's daily paper, for {}, in terminal text.",
                long_date(paper.edition)
            );
            if let Some(yesterday) = &paper.yesterday {
                text.push_str(&format!(
                    " {}: {yesterday}.",
                    PaperSectionKind::Yesterday.title()
                ));
            }
            if let Some(outside) = &paper.outside {
                text.push_str(&format!(
                    " {}: {outside}",
                    PaperSectionKind::Outside.title()
                ));
            }
            if let Some(reading) = &paper.reading {
                text.push_str(&format!(
                    " {}: {reading}",
                    PaperSectionKind::Reading.title()
                ));
            }
            cut_alt(&text)
        }
    };
    Some(alt)
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
