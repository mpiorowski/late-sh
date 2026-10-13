//! The house events board and personal calendars: truth and authorization.
//!
//! A board event (`owner_id` NULL) is posted by anyone who is not banned,
//! read by everyone, and anyone may say they are in. A personal event belongs
//! to one account and nobody else ever reads it: the owner scope sits inside
//! every statement that reads or locks an event, so another account's
//! personal event answers `Gone` exactly as a missing one does. Every write
//! locks the actor and the event row, so a stale editor cannot overwrite a
//! newer revision and two posts at once cannot beat the daily cap. The cap
//! counts `calendar_posts`, a log deletes never touch.
use crate::db::Db;
use anyhow::{Result, ensure};
use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use deadpool_postgres::GenericClient;
use tokio_postgres::Row;
use uuid::Uuid;

pub const CALENDAR_CHANGED_CHANNEL: &str = "calendar_changed";
/// Board posts one account may make per UTC day.
pub const BOARD_POSTS_PER_DAY: i64 = 5;
/// How long before its start a timed board post goes live: the strip's
/// nudge, the Live panel's row. An all-day post goes live for its day.
pub const STRIP_LEAD: Duration = Duration::hours(1);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarView {
    #[default]
    Month,
    List,
}
impl CalendarView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Month => "Month",
            Self::List => "List",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Month => Self::List,
            Self::List => Self::Month,
        }
    }
}

/// Where an event lives: on the board for everyone, or in the viewer's own
/// calendar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CalendarSource {
    #[default]
    Board,
    Personal,
}
impl CalendarSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Board => "the board",
            Self::Personal => "just me",
        }
    }
    pub fn toggled(self) -> Self {
        match self {
            Self::Board => Self::Personal,
            Self::Personal => Self::Board,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EventTiming {
    /// The editor displays end_exclusive - one day.
    AllDay {
        start: NaiveDate,
        end_exclusive: NaiveDate,
    },
    Timed {
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
    },
}
impl EventTiming {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::AllDay {
                start,
                end_exclusive,
            } => ensure!(
                end_exclusive > start,
                "End date must be on or after the start date"
            ),
            Self::Timed { start, end } => {
                ensure!(end.is_none_or(|e| e > *start), "End must be after start")
            }
        }
        Ok(())
    }
    /// The event as instants. A start-only timed event lasts an hour.
    pub fn bounds(&self, tz: Tz) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
        self.validate()?;
        match self {
            Self::AllDay {
                start,
                end_exclusive,
            } => Ok((day_boundary(*start, tz)?, day_boundary(*end_exclusive, tz)?)),
            Self::Timed { start, end } => Ok((*start, end.unwrap_or(*start + Duration::hours(1)))),
        }
    }
    /// The civil dates the event covers in `tz`, end exclusive.
    pub fn dates(&self, tz: Tz) -> (NaiveDate, NaiveDate) {
        match self {
            Self::AllDay {
                start,
                end_exclusive,
            } => (*start, *end_exclusive),
            Self::Timed { start, end } => {
                let e = end.unwrap_or(*start + Duration::hours(1));
                (
                    start.with_timezone(&tz).date_naive(),
                    (e - Duration::nanoseconds(1))
                        .with_timezone(&tz)
                        .date_naive()
                        .succ_opt()
                        .unwrap(),
                )
            }
        }
    }
}
pub fn effective_timezone(value: Option<&str>) -> Tz {
    value
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(chrono_tz::UTC)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occurrence {
    Earlier,
    Later,
}
pub fn local_instant(
    local: NaiveDateTime,
    tz: Tz,
    occurrence: Option<Occurrence>,
) -> Result<DateTime<Utc>> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(t) => Ok(t.with_timezone(&Utc)),
        LocalResult::None => anyhow::bail!("This local time does not exist in {tz}"),
        LocalResult::Ambiguous(a, b) => match occurrence {
            Some(Occurrence::Earlier) => Ok(a.min(b).with_timezone(&Utc)),
            Some(Occurrence::Later) => Ok(a.max(b).with_timezone(&Utc)),
            None => anyhow::bail!("This local time repeats in {tz}; choose Earlier or Later"),
        },
    }
}
/// Civil-day boundaries can fall in a midnight DST gap. Use the first instant
/// of that date, and reject entirely skipped dates (rather than shifting dates).
pub fn day_boundary(date: NaiveDate, tz: Tz) -> Result<DateTime<Utc>> {
    let midnight = date.and_hms_opt(0, 0, 0).unwrap();
    for minute in 0..1440 {
        if let Ok(t) = local_instant(
            midnight + Duration::minutes(minute),
            tz,
            Some(Occurrence::Earlier),
        ) {
            return Ok(t);
        }
    }
    anyhow::bail!("{date} does not exist in {tz}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub creator_id: Uuid,
    /// The poster's name as the board shows it; `someone` once the account
    /// is gone.
    pub creator_name: String,
    pub title: String,
    pub description: String,
    pub timing: EventTiming,
    pub creator_timezone: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    /// How many said they are in. Always zero on a personal event.
    pub going: i64,
    pub revision: i64,
}
impl CalendarEvent {
    pub fn source(&self) -> CalendarSource {
        match self.owner_id {
            None => CalendarSource::Board,
            Some(_) => CalendarSource::Personal,
        }
    }
    pub fn is_board(&self) -> bool {
        self.owner_id.is_none()
    }
    pub fn timed(&self) -> bool {
        matches!(self.timing, EventTiming::Timed { .. })
    }
    /// Live on the house's surfaces: a timed post inside [`STRIP_LEAD`] of
    /// its start or on, an all-day post on its day. Over is never live.
    pub fn live(&self, now: DateTime<Utc>) -> bool {
        let lead = if self.timed() {
            STRIP_LEAD
        } else {
            Duration::zero()
        };
        self.starts_at - lead <= now && now < self.ends_at
    }
    pub fn started(&self, now: DateTime<Utc>) -> bool {
        self.starts_at <= now
    }
    fn from_row(r: Row) -> Self {
        let timing = match r.get::<_, Option<NaiveDate>>("start_date") {
            Some(start) => EventTiming::AllDay {
                start,
                end_exclusive: r.get("end_date"),
            },
            None => EventTiming::Timed {
                start: r.get("start_at"),
                end: r.get("end_at"),
            },
        };
        Self {
            id: r.get("id"),
            owner_id: r.get("owner_id"),
            creator_id: r.get("creator_id"),
            creator_name: r.get("creator_name"),
            title: r.get("title"),
            description: r.get("description"),
            timing,
            creator_timezone: r.get("creator_timezone"),
            starts_at: r.get("starts_at"),
            ends_at: r.get("ends_at"),
            going: r.get("going"),
            revision: r.get("revision"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDraft {
    pub title: String,
    pub description: String,
    pub timing: EventTiming,
}
impl EventDraft {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.title.trim().is_empty(), "Title is required");
        ensure!(
            self.title.chars().count() <= 300,
            "Title is limited to 300 characters"
        );
        ensure!(
            self.description.chars().count() <= 10000,
            "Description is limited to 10000 characters"
        );
        self.timing.validate()
    }
}
impl From<&CalendarEvent> for EventDraft {
    fn from(e: &CalendarEvent) -> Self {
        Self {
            title: e.title.clone(),
            description: e.description.clone(),
            timing: e.timing.clone(),
        }
    }
}

/// What one viewer may do to one event. The three rules of the board: the
/// poster edits and deletes their own post, staff delete any post, anyone
/// says they are in. A personal event answers only to its owner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventAccess {
    pub edit: bool,
    pub delete: bool,
    pub rsvp: bool,
}
pub fn event_access(e: &CalendarEvent, viewer: Uuid, staff: bool) -> EventAccess {
    match e.owner_id {
        Some(owner) => {
            let own = owner == viewer;
            EventAccess {
                edit: own,
                delete: own,
                rsvp: false,
            }
        }
        None => {
            let own = e.creator_id == viewer;
            EventAccess {
                edit: own,
                delete: own || staff,
                rsvp: true,
            }
        }
    }
}

/// Why a write was refused. Every variant is a thing the user did, worded
/// for them; a database failure is not one of these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalendarRefusal {
    /// A moderator took away posting to the board.
    Banned,
    /// [`BOARD_POSTS_PER_DAY`] board posts already today.
    DailyCap,
    /// Not this viewer's to edit or delete.
    ReadOnly,
    /// Somebody saved a newer revision first.
    Revision,
    /// The event is gone, or was never this viewer's to read.
    Gone,
    /// Only a board event takes an "I'm in".
    NoRsvp,
    /// The draft itself: an empty title, an end before its start.
    Invalid(String),
}
impl CalendarRefusal {
    pub fn message(&self) -> String {
        match self {
            Self::Banned => "You cannot post to the board".into(),
            Self::DailyCap => {
                format!("The board takes {BOARD_POSTS_PER_DAY} posts a day from one account")
            }
            Self::ReadOnly => "This event is not yours to change".into(),
            Self::Revision => {
                "Event changed elsewhere; reload before saving (draft retained)".into()
            }
            Self::Gone => "Event unavailable; reload the calendar".into(),
            Self::NoRsvp => "Only board events take an I'm in".into(),
            Self::Invalid(why) => why.clone(),
        }
    }
    pub fn kind(&self) -> CalendarRefusalKind {
        match self {
            Self::Banned => CalendarRefusalKind::Banned,
            Self::DailyCap => CalendarRefusalKind::DailyCap,
            Self::ReadOnly => CalendarRefusalKind::ReadOnly,
            Self::Revision => CalendarRefusalKind::Revision,
            Self::Gone => CalendarRefusalKind::Gone,
            Self::NoRsvp => CalendarRefusalKind::NoRsvp,
            Self::Invalid(_) => CalendarRefusalKind::Invalid,
        }
    }
}
/// The refusal without its words, for a metric label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarRefusalKind {
    Banned,
    DailyCap,
    ReadOnly,
    Revision,
    Gone,
    NoRsvp,
    Invalid,
}

#[derive(Debug)]
pub enum CalendarError {
    Refused(CalendarRefusal),
    Failed(anyhow::Error),
}
impl From<anyhow::Error> for CalendarError {
    fn from(error: anyhow::Error) -> Self {
        Self::Failed(error)
    }
}
impl From<tokio_postgres::Error> for CalendarError {
    fn from(error: tokio_postgres::Error) -> Self {
        Self::Failed(error.into())
    }
}
impl From<deadpool_postgres::PoolError> for CalendarError {
    fn from(error: deadpool_postgres::PoolError) -> Self {
        Self::Failed(error.into())
    }
}
fn refuse<T>(refusal: CalendarRefusal) -> Result<T, CalendarError> {
    Err(CalendarError::Refused(refusal))
}

/// A save, and whether it made a new event: a new board post is a story,
/// an edit is not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    pub event: CalendarEvent,
    pub created: bool,
}

/// A delete, and whether staff did it to someone else's post.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deleted {
    pub by_staff: bool,
}

const EVENT_COLUMNS: &str = "e.*, COALESCE(u.username,'someone') AS creator_name, \
    (SELECT count(*) FROM calendar_rsvps r WHERE r.event_id=e.id) AS going";

#[derive(Clone)]
pub struct CalendarStore {
    db: Db,
}
impl CalendarStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
    /// Whether the viewer is a moderator or admin right now.
    pub async fn staff(&self, viewer: Uuid) -> Result<bool> {
        let c = self.db.get().await?;
        let r = c
            .query_one(
                "SELECT is_admin OR is_moderator FROM users WHERE id=$1",
                &[&viewer],
            )
            .await?;
        Ok(r.get(0))
    }
    /// The actor under a row lock: a save or delete reads the role and the
    /// zone it will act with, and two posts at once serialize on the row so
    /// the daily cap is exact.
    async fn actor<C: GenericClient + Sync>(c: &C, viewer: Uuid) -> Result<Actor> {
        let r = c
            .query_one(
                "SELECT is_admin,is_moderator,settings FROM users WHERE id=$1 FOR UPDATE",
                &[&viewer],
            )
            .await?;
        let settings: serde_json::Value = r.get("settings");
        Ok(Actor {
            staff: r.get::<_, bool>(0) || r.get::<_, bool>(1),
            tz: effective_timezone(settings.get("timezone").and_then(|v| v.as_str())),
        })
    }
    /// The events one viewer sees between two civil dates in `tz`: their own,
    /// plus the board when `board` is set. The owner scope is inside the
    /// statement, never applied after.
    pub async fn visible(
        &self,
        viewer: Uuid,
        board: bool,
        from: NaiveDate,
        to: NaiveDate,
        tz: Tz,
    ) -> Result<Vec<CalendarEvent>> {
        ensure!(to > from, "Invalid visible range");
        let c = self.db.get().await?;
        let begin = day_boundary(from, tz)?;
        let end = day_boundary(to, tz)?;
        let rows = c
            .query(
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id \
                     WHERE (e.owner_id=$1 OR ($2 AND e.owner_id IS NULL)) \
                     AND ((e.start_date IS NOT NULL AND daterange(e.start_date,e.end_date,'[)') && daterange($3,$4,'[)')) \
                       OR (e.start_at IS NOT NULL AND tstzrange(e.starts_at,e.ends_at,'[)') && tstzrange($5,$6,'[)'))) \
                     ORDER BY e.starts_at, e.id"
                ),
                &[&viewer, &board, &from, &to, &begin, &end],
            )
            .await?;
        Ok(rows.into_iter().map(CalendarEvent::from_row).collect())
    }
    /// One event the viewer may read: on the board, or their own.
    pub async fn event(&self, viewer: Uuid, id: Uuid) -> Result<CalendarEvent, CalendarError> {
        let c = self.db.get().await?;
        let r = c
            .query_opt(
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id \
                     WHERE e.id=$1 AND (e.owner_id IS NULL OR e.owner_id=$2)"
                ),
                &[&id, &viewer],
            )
            .await?;
        match r {
            Some(r) => Ok(CalendarEvent::from_row(r)),
            None => refuse(CalendarRefusal::Gone),
        }
    }
    /// The board posts live at `now` (see [`CalendarEvent::live`]), soonest
    /// first. The replica service reads this into the snapshot every
    /// session shares. Personal events are never live anywhere.
    pub async fn live_board(&self, now: DateTime<Utc>) -> Result<Vec<CalendarEvent>> {
        let c = self.db.get().await?;
        let lead = now + STRIP_LEAD;
        Ok(c.query(
            &format!(
                "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id \
                 WHERE e.owner_id IS NULL AND e.ends_at > $1 \
                   AND ((e.start_at IS NOT NULL AND e.starts_at <= $2) OR (e.start_date IS NOT NULL AND e.starts_at <= $1)) \
                 ORDER BY e.starts_at, e.id"
            ),
            &[&now, &lead],
        )
        .await?
        .into_iter()
        .map(CalendarEvent::from_row)
        .collect())
    }
    /// The board events the viewer said they are in.
    pub async fn rsvps(&self, viewer: Uuid) -> Result<Vec<Uuid>> {
        let c = self.db.get().await?;
        Ok(c.query(
            "SELECT event_id FROM calendar_rsvps WHERE user_id=$1",
            &[&viewer],
        )
        .await?
        .into_iter()
        .map(|r| r.get(0))
        .collect())
    }
    /// Create or update. `existing` is the event and the revision the editor
    /// was opened on; a newer revision in the table refuses the save and the
    /// caller keeps the draft.
    pub async fn save(
        &self,
        viewer: Uuid,
        source: CalendarSource,
        existing: Option<(Uuid, i64)>,
        draft: &EventDraft,
    ) -> Result<Saved, CalendarError> {
        if let Err(why) = draft.validate() {
            return refuse(CalendarRefusal::Invalid(why.to_string()));
        }
        let mut c = self.db.get().await?;
        let tx = c.transaction().await?;
        let actor = Self::actor(&tx, viewer).await?;
        let old = match existing {
            Some((id, revision)) => {
                let Some(row) = tx
                    .query_opt(
                        &format!(
                            "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id \
                             WHERE e.id=$1 AND (e.owner_id IS NULL OR e.owner_id=$2) FOR UPDATE OF e"
                        ),
                        &[&id, &viewer],
                    )
                    .await?
                else {
                    return refuse(CalendarRefusal::Gone);
                };
                let e = CalendarEvent::from_row(row);
                if e.source() != source {
                    return refuse(CalendarRefusal::ReadOnly);
                }
                if !event_access(&e, viewer, actor.staff).edit {
                    return refuse(CalendarRefusal::ReadOnly);
                }
                if e.revision != revision {
                    return refuse(CalendarRefusal::Revision);
                }
                Some(e)
            }
            None => {
                if source == CalendarSource::Board {
                    if Self::banned(&tx, viewer).await? {
                        return refuse(CalendarRefusal::Banned);
                    }
                    // The post log, not the events: a deleted post still
                    // used its slot. The day is UTC whatever the session's
                    // TimeZone.
                    let today: i64 = tx
                        .query_one(
                            "SELECT count(*) FROM calendar_posts WHERE user_id=$1 \
                             AND created >= date_trunc('day', CURRENT_TIMESTAMP AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'",
                            &[&viewer],
                        )
                        .await?
                        .get(0);
                    if today >= BOARD_POSTS_PER_DAY {
                        return refuse(CalendarRefusal::DailyCap);
                    }
                    tx.execute("INSERT INTO calendar_posts(user_id) VALUES($1)", &[&viewer])
                        .await?;
                }
                None
            }
        };
        let creator_timezone = old
            .as_ref()
            .map(|e| e.creator_timezone.clone())
            .unwrap_or_else(|| actor.tz.to_string());
        let (starts_at, ends_at) = match draft
            .timing
            .bounds(effective_timezone(Some(&creator_timezone)))
        {
            Ok(bounds) => bounds,
            Err(why) => return refuse(CalendarRefusal::Invalid(why.to_string())),
        };
        let (sd, ed, st, et) = match draft.timing {
            EventTiming::AllDay {
                start,
                end_exclusive,
            } => (Some(start), Some(end_exclusive), None, None),
            EventTiming::Timed { start, end } => (None, None, Some(start), end),
        };
        let id = old.as_ref().map(|e| e.id).unwrap_or_else(Uuid::now_v7);
        let creator = old.as_ref().map(|e| e.creator_id).unwrap_or(viewer);
        let owner = match source {
            CalendarSource::Board => None,
            CalendarSource::Personal => Some(viewer),
        };
        let saved: Uuid = tx
            .query_one(
                "INSERT INTO calendar_events(id,owner_id,creator_id,title,description,start_date,end_date,start_at,end_at,creator_timezone,starts_at,ends_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
                 ON CONFLICT(id) DO UPDATE SET title=EXCLUDED.title,description=EXCLUDED.description,start_date=EXCLUDED.start_date,end_date=EXCLUDED.end_date,start_at=EXCLUDED.start_at,end_at=EXCLUDED.end_at,starts_at=EXCLUDED.starts_at,ends_at=EXCLUDED.ends_at,revision=calendar_events.revision+1 \
                 RETURNING id",
                &[
                    &id,
                    &owner,
                    &creator,
                    &draft.title.trim(),
                    &draft.description,
                    &sd,
                    &ed,
                    &st,
                    &et,
                    &creator_timezone,
                    &starts_at,
                    &ends_at,
                ],
            )
            .await?
            .get(0);
        let row = tx
            .query_one(
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id WHERE e.id=$1"
                ),
                &[&saved],
            )
            .await?;
        tx.commit().await?;
        Ok(Saved {
            event: CalendarEvent::from_row(row),
            created: old.is_none(),
        })
    }
    async fn banned<C: GenericClient + Sync>(c: &C, viewer: Uuid) -> Result<bool> {
        Ok(c.query_opt(
            "SELECT 1 FROM calendar_bans WHERE target_user_id=$1 AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
            &[&viewer],
        )
        .await?
        .is_some())
    }
    pub async fn delete(
        &self,
        viewer: Uuid,
        id: Uuid,
        revision: i64,
    ) -> Result<Deleted, CalendarError> {
        let mut c = self.db.get().await?;
        let tx = c.transaction().await?;
        let actor = Self::actor(&tx, viewer).await?;
        let Some(row) = tx
            .query_opt(
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id \
                     WHERE e.id=$1 AND (e.owner_id IS NULL OR e.owner_id=$2) FOR UPDATE OF e"
                ),
                &[&id, &viewer],
            )
            .await?
        else {
            return refuse(CalendarRefusal::Gone);
        };
        let e = CalendarEvent::from_row(row);
        let access = event_access(&e, viewer, actor.staff);
        if !access.delete {
            return refuse(CalendarRefusal::ReadOnly);
        }
        if e.revision != revision {
            return refuse(CalendarRefusal::Revision);
        }
        tx.execute("DELETE FROM calendar_events WHERE id=$1", &[&id])
            .await?;
        tx.commit().await?;
        Ok(Deleted {
            by_staff: !access.edit,
        })
    }
    /// Say you are in, or take it back. Returns the event with its new count.
    pub async fn set_rsvp(
        &self,
        viewer: Uuid,
        id: Uuid,
        going: bool,
    ) -> Result<CalendarEvent, CalendarError> {
        let mut c = self.db.get().await?;
        let tx = c.transaction().await?;
        let Some(row) = tx
            .query_opt(
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id \
                     WHERE e.id=$1 AND (e.owner_id IS NULL OR e.owner_id=$2) FOR UPDATE OF e"
                ),
                &[&id, &viewer],
            )
            .await?
        else {
            return refuse(CalendarRefusal::Gone);
        };
        let e = CalendarEvent::from_row(row);
        if !e.is_board() {
            return refuse(CalendarRefusal::NoRsvp);
        }
        if going {
            tx.execute(
                "INSERT INTO calendar_rsvps(event_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING",
                &[&id, &viewer],
            )
            .await?;
        } else {
            tx.execute(
                "DELETE FROM calendar_rsvps WHERE event_id=$1 AND user_id=$2",
                &[&id, &viewer],
            )
            .await?;
        }
        let row = tx
            .query_one(
                &format!(
                    "SELECT {EVENT_COLUMNS} FROM calendar_events e LEFT JOIN users u ON u.id=e.creator_id WHERE e.id=$1"
                ),
                &[&id],
            )
            .await?;
        tx.commit().await?;
        Ok(CalendarEvent::from_row(row))
    }
    /// Claim the timed board events that started and were never announced.
    /// The stamp is the claim: of every replica sweeping, one gets each row
    /// back and posts its line. An event already over is never claimed, so
    /// a restart cannot announce yesterday. An all-day post starts at
    /// midnight somewhere, so it is never announced: it owns the topic row
    /// for its day instead.
    pub async fn claim_started(&self, now: DateTime<Utc>) -> Result<Vec<CalendarEvent>> {
        let c = self.db.get().await?;
        Ok(c.query(
            &format!(
                "WITH claimed AS (UPDATE calendar_events SET announced_at=$1 \
                 WHERE owner_id IS NULL AND start_at IS NOT NULL AND announced_at IS NULL AND starts_at <= $1 AND ends_at > $1 RETURNING *) \
                 SELECT {EVENT_COLUMNS} FROM claimed e LEFT JOIN users u ON u.id=e.creator_id ORDER BY e.starts_at, e.id"
            ),
            &[&now],
        )
        .await?
        .into_iter()
        .map(CalendarEvent::from_row)
        .collect())
    }
}

struct Actor {
    staff: bool,
    tz: Tz,
}
