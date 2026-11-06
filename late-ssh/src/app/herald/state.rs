//! The herald's pure rules: which day each of the two posts is due, what
//! fills each (the words slot's schedule, the art slot's day), the words that
//! travel with a post, and which looks go out at all. Reads no clock and does
//! no I/O; `svc.rs` feeds it rows and posts what comes back.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Utc, Weekday};
use dartboard_core::{Canvas, CellValue};
use late_core::models::herald::{HeraldKind, HeraldPicked, HeraldSlot};
use late_core::models::leaderboard::{ArcadeGame, DayTally, week_first_day};
use late_core::models::paper::PaperSectionKind;
use late_core::models::showcase::AuthoredShowcase;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::svc::{HeraldEvent, HeraldService};

/// When the words go out: late afternoon in Europe, the morning on the US
/// east coast, ahead of the room's busiest hour (18:00 to 19:00 UTC,
/// `AUDIENCE.md`), so whoever clicks through finds people in.
pub(crate) const WORDS_POST_TIME: NaiveTime = match NaiveTime::from_hms_opt(15, 0, 0) {
    Some(time) => time,
    None => unreachable!(),
};

/// When the art goes out: the evening in Europe, the afternoon in the US. Five
/// hours after the words, so the two never stack in one timeline.
pub(crate) const ART_POST_TIME: NaiveTime = match NaiveTime::from_hms_opt(20, 0, 0) {
    Some(time) => time,
    None => unreachable!(),
};

pub(crate) fn post_time(slot: HeraldSlot) -> NaiveTime {
    match slot {
        HeraldSlot::Words => WORDS_POST_TIME,
        HeraldSlot::Art => ART_POST_TIME,
    }
}

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
/// model-written text; `#TUI` for the people who browse terminal apps.
const PAPER_TAGS: [&str; 4] = ["#tildeverse", "#terminal", "#TUI", "#smolweb"];

/// A project's: the member tags alone. A project is whatever a member made, so
/// no tag about its subject can be right for all of them.
const PROJECT_TAGS: [&str; 2] = ["#tildeverse", "#terminal"];

/// The longest alt text the look keeps, before the `ASCII art: ` prefix.
/// Mastodon allows 1,500; the model is asked for 800.
pub(crate) const ALT_MAX_CHARS: usize = 1000;

/// The longest an announcement's words run: under Bluesky's 300 with what
/// `status_text` puts after them, `Full notes: ` and the edition's page
/// (`https://late.sh/paper/YYYY-MM-DD`, 44), two blank lines (4) and
/// `ANNOUNCEMENT_TAGS` (26). The brief is asked for this many.
pub(crate) const ANNOUNCEMENT_MAX_CHARS: usize = 225;

/// An announcement's: no art tags on a post with no art, `#TUI` for the
/// people who browse terminal apps, who are who a release note is for.
const ANNOUNCEMENT_TAGS: [&str; 3] = ["#tildeverse", "#terminal", "#TUI"];

/// The longest a project's whole post runs, a little under Bluesky's 300
/// graphemes: its words get whatever the title, the credit and the link leave.
pub(crate) const PROJECT_POST_MAX_CHARS: usize = 295;

/// The fewest characters of words a project post is worth. A title and a link
/// that leave less than this make the project ineligible.
pub(crate) const PROJECT_MIN_WORDS: usize = 60;

/// How much of a project's title the post carries (the column allows 120).
pub(crate) const PROJECT_TITLE_MAX_CHARS: usize = 60;

/// How long before a project may be posted again.
pub(crate) const PROJECT_COOLDOWN: Duration = Duration::days(60);

/// How recently a project's author must have logged in.
pub(crate) const PROJECT_ACTIVE_WINDOW: Duration = Duration::days(7);

/// The site's own picture on late-web, the night-city skyline
/// (`late-web/src/pages/shared/meta.rs`, `SITE_IMAGE_PATH`): the thumb of
/// a post of words on Bluesky, which has no card of its own for a post made
/// through the API (the app builds one by hand).
pub(crate) const SITE_IMAGE_PATH: &str = "/static/og-image.png";

/// Blank cells the board's crop keeps around what is drawn.
pub(crate) const CROP_MARGIN: usize = 2;

/// The day whose post of `slot` is due right now: today once the slot's time
/// has passed, yesterday before it. A replica that was down at the hour catches
/// up at its next check and stamps the day the post belongs to.
pub(crate) fn post_due_day(now: DateTime<Utc>, slot: HeraldSlot) -> NaiveDate {
    let today = now.date_naive();
    if now.time() >= post_time(slot) {
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

/// What the words slot can post. Closed, so its schedule cannot name an art kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WordsKind {
    Announcement,
    Project,
    Scoreboard,
    Paper,
}

/// The words slot's week: the kinds a day dated `day` tries after the operator's
/// announcement, in order, the first with something to show being the post. A
/// project on Monday, and while it is owed through to Friday (an announcement or
/// a failed write-up moves it along; `WordsFound::projects` is empty once the
/// week has had one); the paper on Wednesday and Saturday, and on Monday when no
/// project can run; the week's scoreboard on Sunday, else the paper. A day whose
/// kinds all have nothing posts no words: the art still goes out.
pub(crate) fn words_schedule(day: NaiveDate) -> &'static [WordsKind] {
    match day.weekday() {
        Weekday::Mon | Weekday::Wed => &[WordsKind::Project, WordsKind::Paper],
        Weekday::Tue | Weekday::Thu | Weekday::Fri => &[WordsKind::Project],
        Weekday::Sat => &[WordsKind::Paper],
        Weekday::Sun => &[WordsKind::Scoreboard, WordsKind::Paper],
    }
}

/// What the art slot shows on a day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtDay {
    /// Yesterday's Artboard.
    Board,
    /// The piece over the door.
    Piece,
}

/// The art slot's week: the board on Monday, the piece over the door every
/// other day. One candidate a day and no fallback: a flagged piece's board is
/// nearly always flagged too, since the board keeps its drawings all month.
pub(crate) fn art_day(day: NaiveDate) -> ArtDay {
    match day.weekday() {
        Weekday::Mon => ArtDay::Board,
        Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri | Weekday::Sat | Weekday::Sun => {
            ArtDay::Piece
        }
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

/// The week in every game (the arcade, the Lobby, the doors): one line
/// per game anyone played with everyone's plays added up, nobody named:
/// the card is public and the names stay on the terminal's boards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScoreboardCandidate {
    /// The week's last day; it runs `WEEK_DAYS` back from here.
    pub(crate) last: NaiveDate,
    pub(crate) lines: Vec<ScoreLine>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ScoreLine {
    pub(crate) game: String,
    /// `1 play`, `1,234 plays`. Rows picked before the week counted
    /// plays stored the field as `best`.
    #[serde(alias = "best")]
    pub(crate) plays: String,
}

impl ScoreboardCandidate {
    /// The week ending `last`, if anyone played in it.
    pub(crate) fn of(last: NaiveDate, tallies: &[DayTally]) -> Option<Self> {
        if tallies.is_empty() {
            return None;
        }
        Some(Self {
            last,
            lines: tallies
                .iter()
                .map(|tally| ScoreLine {
                    game: tally.game.title().to_string(),
                    plays: tally.shown_plays(),
                })
                .collect(),
        })
    }
}

/// The Late Edition dated the post's day, which reads the day before, once
/// a public column of it printed: the covered day's most played arcade
/// game, the whole of YESTERDAY (its lines are the numbers, and few) and
/// the first paragraph of the other two, for the alt text (the picture is
/// late-web's card of the same, in the same order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PaperCandidate {
    pub(crate) edition: NaiveDate,
    /// The card's headline: the game and its finishes, as the card
    /// writes them.
    #[serde(default)]
    pub(crate) most_played: Option<String>,
    /// The column's lines joined with `; `.
    #[serde(default)]
    pub(crate) yesterday: Option<String>,
    pub(crate) reading: Option<String>,
    pub(crate) outside: Option<String>,
}

impl PaperCandidate {
    /// The edition, if any public column printed (the most played game
    /// alone is no edition). A column's paragraphs are its non-empty
    /// lines, as the web prints them.
    pub(crate) fn of(
        edition: NaiveDate,
        most_played: Option<(ArcadeGame, i64)>,
        yesterday: Option<&str>,
        reading: Option<&str>,
        outside: Option<&str>,
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
        match (whole(yesterday), head(reading), head(outside)) {
            (None, None, None) => None,
            (yesterday, reading, outside) => Some(Self {
                edition,
                most_played: most_played.map(|(game, plays)| match plays {
                    1 => format!("{}, 1 game finished", game.title()),
                    plays => format!("{}, {plays} games finished", game.title()),
                }),
                yesterday,
                reading,
                outside,
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
    /// The edition's first line of OUTSIDE, else of WHAT WE WERE READING,
    /// for the link card, the way the web's own card reads it.
    pub(crate) summary: Option<String>,
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
    #[serde(default)]
    pub(crate) summary: Option<String>,
}

/// The link card a Bluesky post of words alone carries
/// (`app.bsky.embed.external`): the edition's page, named and described
/// as the web's own preview card is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LinkCard {
    pub(crate) uri: String,
    pub(crate) title: String,
    pub(crate) description: String,
}

/// Words made safe for the official account and cut to `limit`, whoever wrote
/// them: markdown markers dropped and paragraphs kept (`plain_lines`), dashes
/// made commas (`no_dashes`), every `@`, `#` and `scheme:` dead (`dead_words`),
/// so neither a member, the operator nor the model can tag or mention anyone.
fn outgoing_words(text: &str, limit: usize) -> String {
    cut_to(&dead_words(&no_dashes(&plain_lines(text))), limit)
}

impl AnnouncementCandidate {
    /// The note or the model's brief as it goes out.
    pub(crate) fn of(
        edition: NaiveDate,
        text: &str,
        brief: Brief,
        summary: Option<String>,
    ) -> Self {
        Self {
            edition,
            text: outgoing_words(text, ANNOUNCEMENT_MAX_CHARS),
            brief,
            summary,
        }
    }

    /// The card under the words on Bluesky.
    pub(crate) fn link_card(&self, web_url: &str) -> LinkCard {
        LinkCard {
            uri: format!("{web_url}/paper/{}", self.edition),
            title: format!("The Late Edition, {}", long_date(self.edition)),
            description: match &self.summary {
                Some(summary) => summary.clone(),
                None => "late.sh's daily paper.".to_string(),
            },
        }
    }
}

/// A member's project as drawn, before the write-up: what the model is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectFound {
    pub(crate) id: Uuid,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) description: String,
    pub(crate) tags: Vec<String>,
    pub(crate) artist: String,
}

impl ProjectFound {
    /// The project as a candidate, if the post can carry it: a link a network
    /// will make live (`linkable`), and room for words after it.
    pub(crate) fn of(authored: AuthoredShowcase) -> Option<Self> {
        let AuthoredShowcase { showcase, username } = authored;
        let url = showcase.url.trim().to_string();
        if !linkable(&url) {
            return None;
        }
        let found = Self {
            id: showcase.id,
            title: showcase.title,
            url,
            description: showcase.description,
            tags: showcase.tags,
            artist: username,
        };
        (found.words_limit() >= PROJECT_MIN_WORDS).then_some(found)
    }

    /// How many characters the write-up may run: the post's limit less what
    /// `status_text` puts around the words.
    pub(crate) fn words_limit(&self) -> usize {
        let around = project_head(&self.title, &self.artist).chars().count()
            + self.url.chars().count()
            + PROJECT_TAGS.join(" ").chars().count()
            + "\n\n".len() * 3;
        PROJECT_POST_MAX_CHARS.saturating_sub(around)
    }
}

/// A link the official account can carry: http or https, a real host with a
/// dot in it, no credentials and no whitespace (a word is whatever sits between
/// whitespace to `bluesky::facets`).
fn linkable(url: &str) -> bool {
    if url.contains(char::is_whitespace) {
        return false;
    }
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return false,
    }
    parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.domain().is_some_and(|domain| domain.contains('.'))
}

/// The line above a project's words: where it comes from and whose it is. The
/// title is a member's, so it goes in dead and cut.
fn project_head(title: &str, artist: &str) -> String {
    format!(
        "\"{}\" by ~{artist}, from late.sh.",
        cut_to(&dead_words(title), PROJECT_TITLE_MAX_CHARS)
    )
}

/// Why the write-up turned a project down. Closed: the prompt's verdicts, one
/// for one (`writeup::parse`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectRefusal {
    /// Fosstodon bans commercial promotion.
    Commercial,
    /// Adult content, hate, a scam.
    Unfit,
    /// Too little to say what it is.
    Unclear,
}

impl ProjectRefusal {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Commercial => "commercial",
            Self::Unfit => "unfit",
            Self::Unclear => "unclear",
        }
    }
}

/// The week's project as it goes out: the model's words under the title and the
/// credit, the member's own link under them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProjectCandidate {
    pub(crate) id: Uuid,
    pub(crate) title: String,
    pub(crate) url: String,
    pub(crate) artist: String,
    pub(crate) text: String,
}

impl ProjectCandidate {
    /// The model's words as they go out, cut to what the post has room for.
    pub(crate) fn of(project: &ProjectFound, words: &str) -> Self {
        Self {
            id: project.id,
            title: project.title.clone(),
            url: project.url.clone(),
            artist: project.artist.clone(),
            text: outgoing_words(words, project.words_limit()),
        }
    }
}

/// Which projects a week's draw leaves out, from the picks of the last
/// `PROJECT_COOLDOWN`: everything posted in that time. `None` when the week of
/// `day` (Monday on) already had its project, so nothing is owed. An unreadable
/// subject is an error: every project row is written by `Picture::subject`.
pub(crate) fn project_exclusions(
    day: NaiveDate,
    picked: &[HeraldPicked],
) -> anyhow::Result<Option<Vec<Uuid>>> {
    let monday = day - Duration::days(i64::from(day.weekday().num_days_from_monday()));
    if picked.iter().any(|pick| pick.day >= monday) {
        return Ok(None);
    }
    let mut excluded = Vec::with_capacity(picked.len());
    for pick in picked {
        let Some(id) = pick.subject.strip_prefix("project:") else {
            anyhow::bail!("project subject {:?} has no prefix", pick.subject);
        };
        excluded.push(Uuid::parse_str(id)?);
    }
    Ok(Some(excluded))
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
/// written with (`#` headings, `-` and `*` bullets, `>` quotes); a run of
/// blank lines is one paragraph break, and none open or close the text.
/// The note reads as paragraphs on a network that shows markdown as typed
/// and reads `#word` as a tag.
pub(crate) fn plain_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_break = false;
    for line in text.lines() {
        let line = line.trim().trim_start_matches(['#', '-', '*', '>']).trim();
        if line.is_empty() {
            pending_break = !out.is_empty();
            continue;
        }
        if pending_break {
            out.push_str("\n\n");
            pending_break = false;
        } else if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

/// Everything the words slot could post on a day, as found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WordsFound {
    /// The edition's announcements, when the operator spoke.
    pub(crate) announcement: Option<AnnouncementFound>,
    /// The projects drawn for the week, in the order to try them. Empty when
    /// the day has no project on its schedule, the week already had one, or
    /// nothing is eligible.
    pub(crate) projects: Vec<ProjectFound>,
    /// Sundays only, and only when anyone played.
    pub(crate) scoreboard: Option<ScoreboardCandidate>,
    pub(crate) paper: Option<PaperCandidate>,
}

impl WordsFound {
    /// What was found of one kind.
    fn of(&self, kind: WordsKind) -> Option<Choice> {
        match kind {
            WordsKind::Announcement => self.announcement.clone().map(Choice::Announcement),
            WordsKind::Project => match self.projects.is_empty() {
                true => None,
                false => Some(Choice::Projects(self.projects.clone())),
            },
            WordsKind::Scoreboard => self
                .scoreboard
                .clone()
                .map(|scoreboard| Choice::Picture(Picture::Scoreboard(scoreboard))),
            WordsKind::Paper => self
                .paper
                .clone()
                .map(|paper| Choice::Picture(Picture::Paper(paper))),
        }
    }
}

/// What the art slot could post on a day: the one candidate its `art_day` names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ArtFound {
    Board(BoardFound),
    /// `None` when the wall has no piece for the day.
    Piece(Option<PieceCandidate>),
}

impl ArtFound {
    fn picture(&self) -> Option<Picture> {
        match self {
            Self::Board(board) => board.picture(),
            Self::Piece(piece) => piece.clone().map(Picture::Piece),
        }
    }
}

/// What one slot could post on a day, as found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Found {
    Words(WordsFound),
    Art(ArtFound),
}

/// One thing a slot could post: a picture, an announcement still to be briefed,
/// or the week's projects still to be written up (both model calls, `svc.rs`'s
/// to make).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Choice {
    Picture(Picture),
    Announcement(AnnouncementFound),
    Projects(Vec<ProjectFound>),
}

impl Found {
    /// What the slot could post on `day`, best first: for words an announcement
    /// ahead of the day's schedule, for art the day's one candidate. `svc.rs`
    /// walks it, since a project the model turns down falls to the next.
    pub(crate) fn choices(&self, day: NaiveDate) -> Vec<Choice> {
        match self {
            Self::Words(words) => [WordsKind::Announcement]
                .iter()
                .chain(words_schedule(day))
                .filter_map(|kind| words.of(*kind))
                .collect(),
            Self::Art(art) => art.picture().map(Choice::Picture).into_iter().collect(),
        }
    }

    /// Why a bare slot is bare, for the admin's banner.
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Words(_) => {
                "no announcement and nothing on the day's schedule (project, scoreboard, paper)"
                    .to_string()
            }
            Self::Art(ArtFound::Board(board)) => board.describe(),
            Self::Art(ArtFound::Piece(_)) => "no piece over the door".to_string(),
        }
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
    /// Words only, under the member's own link.
    Project(ProjectCandidate),
}

impl Picture {
    pub(crate) fn kind(&self) -> HeraldKind {
        match self {
            Self::Board(_) => HeraldKind::Artboard,
            Self::Piece(_) => HeraldKind::Piece,
            Self::Scoreboard(_) => HeraldKind::Scoreboard,
            Self::Paper(_) => HeraldKind::Paper,
            Self::Announcement(_) => HeraldKind::Announcement,
            Self::Project(_) => HeraldKind::Project,
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
            Self::Project(project) => format!("project:{}", project.id),
        }
    }

    /// What the post carries: a picture, or words alone. The one place that says
    /// which kind is which, so a picture without alt text or words without a
    /// card cannot be asked for.
    pub(crate) fn shape(&self) -> Shape<'_> {
        match self {
            Self::Board(board) => Shape::Image(Image::Board(board)),
            Self::Piece(piece) => Shape::Image(Image::Piece(piece)),
            Self::Scoreboard(scoreboard) => Shape::Image(Image::Scoreboard(scoreboard)),
            Self::Paper(paper) => Shape::Image(Image::Paper(paper)),
            Self::Announcement(announcement) => {
                Shape::Words(WordsAlone::Announcement(announcement))
            }
            Self::Project(project) => Shape::Words(WordsAlone::Project(project)),
        }
    }

    /// The page the post links to. The board links the gallery itself, not the
    /// day's snapshot: the Artboard keeps only the newest seven dailies
    /// (`app/artboard/CONTEXT.md`), and a post outlives that by years. The
    /// scoreboard links the front page; the boards live in the terminal. An
    /// announcement links the edition, which carries the note in full, and a
    /// project its member's own link, the one address not on late.sh.
    pub(crate) fn page(&self, web_url: &str) -> String {
        match self {
            Self::Board(_) => format!("{web_url}/gallery"),
            Self::Piece(piece) => format!("{web_url}/gallery/pieces/{}", piece.id),
            Self::Scoreboard(_) => web_url.to_string(),
            Self::Paper(paper) => format!("{web_url}/paper/{}", paper.edition),
            Self::Announcement(announcement) => {
                format!("{web_url}/paper/{}", announcement.edition)
            }
            Self::Project(project) => project.url.clone(),
        }
    }
}

/// What a post carries besides its words (`Picture::shape`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape<'a> {
    Image(Image<'a>),
    Words(WordsAlone<'a>),
}

/// A post with a picture: what members drew, for the look to read, or one of
/// late.sh's cards.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Image<'a> {
    Board(&'a BoardCandidate),
    Piece(&'a PieceCandidate),
    Scoreboard(&'a ScoreboardCandidate),
    Paper(&'a PaperCandidate),
}

impl Image<'_> {
    /// The picture's route on late-web (`late-web/CONTEXT.md`, the PNG routes).
    pub(crate) fn path(self) -> String {
        match self {
            Self::Board(board) => format!(
                "/gallery/{}/image.png?crop={},{},{},{}",
                board.key, board.crop.x, board.crop.y, board.crop.width, board.crop.height
            ),
            Self::Piece(piece) => format!("/gallery/pieces/{}/image.png", piece.id),
            Self::Scoreboard(scoreboard) => format!("/games/week/{}/image.png", scoreboard.last),
            Self::Paper(paper) => format!("/paper/{}/image.png", paper.edition),
        }
    }

    /// The picture described for readers who cannot see it: what the look saw,
    /// else a template that says what the picture is, if not what is on it. A
    /// card's is its own words.
    pub(crate) fn alt_text(self, look: &Look) -> String {
        match (self, look) {
            (Self::Board(_) | Self::Piece(_), Look::Seen { alt, .. }) => {
                format!("ASCII art: {alt}")
            }
            (Self::Board(board), Look::Unseen | Look::Drawn) => format!(
                "ASCII art: the late.sh Artboard at the end of {} UTC. Coloured text on a dark background.",
                long_date(board.shown),
            ),
            (Self::Piece(piece), Look::Unseen | Look::Drawn) => format!(
                "ASCII art: \"{}\", from the late.sh gallery. Coloured text on a dark background.",
                piece.title,
            ),
            (Self::Scoreboard(scoreboard), _) => cut_alt(&format!(
                "A scoreboard in green and grey terminal text. What got played on late.sh, {}, game by game. {}.",
                week_span(scoreboard.last),
                scoreboard
                    .lines
                    .iter()
                    .map(|line| format!("{}: {}", line.game, line.plays))
                    .collect::<Vec<_>>()
                    .join("; "),
            )),
            (Self::Paper(paper), _) => {
                let mut text = format!(
                    "Front page of the Late Edition, late.sh's daily paper, {}, in terminal text.",
                    long_date(paper.edition)
                );
                if let Some(most_played) = &paper.most_played {
                    text.push_str(&format!(" Most played yesterday: {most_played}."));
                }
                if let Some(yesterday) = &paper.yesterday {
                    text.push_str(&format!(
                        " {}: {yesterday}.",
                        PaperSectionKind::Yesterday.title()
                    ));
                }
                if let Some(reading) = &paper.reading {
                    text.push_str(&format!(
                        " {}: {reading}",
                        PaperSectionKind::Reading.title()
                    ));
                }
                if let Some(outside) = &paper.outside {
                    text.push_str(&format!(
                        " {}: {outside}",
                        PaperSectionKind::Outside.title()
                    ));
                }
                cut_alt(&text)
            }
        }
    }
}

/// A post of words alone: an announcement or a project.
#[derive(Clone, Copy, Debug)]
pub(crate) enum WordsAlone<'a> {
    Announcement(&'a AnnouncementCandidate),
    Project(&'a ProjectCandidate),
}

impl WordsAlone<'_> {
    /// The card under the words on Bluesky.
    pub(crate) fn link_card(self, web_url: &str) -> LinkCard {
        match self {
            Self::Announcement(announcement) => announcement.link_card(web_url),
            Self::Project(project) => LinkCard {
                uri: project.url.clone(),
                title: cut_to(&project.title, PROJECT_TITLE_MAX_CHARS),
                description: project.text.clone(),
            },
        }
    }
}

/// What the look labelled a picture with: the one warning a post carries.
/// Closed, so a new warning breaks the build until `look::parse` knows it.
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
/// the same verdict and never asks the model again. It decides whether a
/// member's drawing goes out at all (`goes_out`) and writes its alt text.
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
    /// Nobody looked: AI is off. Never goes out. A call that fails is no look at
    /// all: the post fails and the next check asks again.
    Unseen,
    /// Nothing for the look to see, no model asked: a card late.sh drew from its
    /// own words (the scoreboard, the paper), clean by construction, or words
    /// alone, with no picture.
    Drawn,
}

impl Look {
    /// Whether a post with this look goes out. Only a clean look or a post with
    /// nothing to look at does: Fosstodon bans adult and violent content even
    /// behind a warning, so anything flagged, or that nobody could check, stays
    /// off every network and the slot rests for the day.
    pub(crate) fn goes_out(&self) -> bool {
        match self {
            Self::Seen { warning: None, .. } | Self::Drawn => true,
            Self::Seen {
                warning: Some(_), ..
            }
            | Self::Unseen => false,
        }
    }

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
/// that closes a word has a space after it, so no `scheme:` survives to
/// open a link (`https: //`, `magnet: ?`). Every scheme a network links
/// ends in a letter, so a colon after a digit stays: a time (`16:00`) or
/// a ratio reads as written.
pub(crate) fn dead_words(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut chars = title.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '@' | '＠' | '#' | '＃' => {}
            ':' => {
                let closes_a_word = out.chars().next_back().is_some_and(char::is_alphabetic);
                out.push(':');
                match (closes_a_word, chars.peek()) {
                    (true, Some(next)) if !next.is_whitespace() => out.push(' '),
                    (true | false, Some(_) | None) => {}
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
                "Yesterday's Artboard on late.sh, {}. A shared ASCII canvas, anyone logged in can draw on it.",
                long_date(board.shown),
            ),
            &ART_TAGS,
        ),
        Picture::Piece(piece) => (
            format!(
                "\"{}\" by ~{}, from the late.sh gallery. ASCII art drawn on the shared canvas over SSH.",
                dead_words(&piece.title),
                piece.artist,
            ),
            &ART_TAGS,
        ),
        Picture::Scoreboard(scoreboard) => (
            format!(
                "What got played on late.sh, {}. Daily puzzles, the arcade, chess and pool, the doors, all in a terminal.",
                week_span(scoreboard.last),
            ),
            &SCOREBOARD_TAGS,
        ),
        Picture::Paper(paper) => (
            format!(
                "The Late Edition for {}, late.sh's daily paper. Yesterday in numbers, what people were reading, and some news from outside.",
                long_date(paper.edition),
            ),
            &PAPER_TAGS,
        ),
        Picture::Announcement(announcement) => (announcement.text.clone(), &ANNOUNCEMENT_TAGS),
        Picture::Project(project) => (
            format!(
                "{}\n\n{}",
                project_head(&project.title, &project.artist),
                project.text
            ),
            &PROJECT_TAGS,
        ),
    }
}

/// The words of a post, under Bluesky's 300 graphemes, the tightest limit among
/// the networks (`state_test` holds it there): a line of plain speech (an
/// announcement's paragraphs, a project's credit and write-up), the page (the
/// paper's and an announcement's named, `Full edition:`, `Full notes:`, so the
/// link reads as what it is), the tags. A member a kind credits is `~name`,
/// never `@name`, so no bare `@` can ping a stranger on the instance, and what a
/// member or the model wrote goes in dead (`dead_words`). Every network gets the
/// same words.
pub(crate) fn status_text(picture: &Picture, web_url: &str) -> String {
    let page = picture.page(web_url);
    let page = match picture {
        Picture::Board(_) | Picture::Piece(_) | Picture::Scoreboard(_) | Picture::Project(_) => {
            page
        }
        Picture::Paper(_) => format!("Full edition: {page}"),
        Picture::Announcement(_) => format!("Full notes: {page}"),
    };
    let (speech, tags) = speech(picture);
    format!("{speech}\n\n{page}\n\n{}", tags.join(" "))
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

/// What `/herald` asked for. Every subcommand is admin-only, refused with
/// a banner for anyone else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeraldCommand {
    /// `/herald on`: the row on, after any post the switch slept through is
    /// settled unposted; each slot's next hour posts.
    On,
    /// `/herald off`: the row off; the next check does nothing.
    Off,
    /// `/herald status`: the row, and today's two posts as they stand.
    Status,
    /// `/herald preview words|art`: what `/herald post` would send of that
    /// slot, and where, without sending it.
    Preview(HeraldSlot),
    /// `/herald post words|art`: work that slot of today now, whatever the row
    /// says and whatever the hour. The slot's check then finds it settled.
    Post(HeraldSlot),
}

/// `None`: not a `/herald` line. `Some(None)`: `/herald` alone or with
/// junk after it. `preview` and `post` name their slot: there are two posts a
/// day and neither is the default.
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
        ["preview", "words"] => Some(HeraldCommand::Preview(HeraldSlot::Words)),
        ["preview", "art"] => Some(HeraldCommand::Preview(HeraldSlot::Art)),
        ["post", "words"] => Some(HeraldCommand::Post(HeraldSlot::Words)),
        ["post", "art"] => Some(HeraldCommand::Post(HeraldSlot::Art)),
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
