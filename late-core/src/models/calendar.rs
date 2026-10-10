//! Calendar truth and authorization. Every write locks the current actor and event.
use crate::db::Db;
use anyhow::{Result, bail, ensure};
use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use deadpool_postgres::GenericClient;
use serde::{Deserialize, Serialize};
use tokio_postgres::Row;
use uuid::Uuid;

pub const CALENDAR_CHANGED_CHANNEL: &str = "calendar_changed";
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarView {
    #[default]
    Month,
    Week,
    ThreeDay,
    Day,
    List,
}
impl CalendarView {
    pub const ALL: [Self; 5] = [
        Self::Month,
        Self::Week,
        Self::ThreeDay,
        Self::Day,
        Self::List,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::Week => "week",
            Self::ThreeDay => "three_day",
            Self::Day => "day",
            Self::List => "list",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Month => "Month",
            Self::Week => "Week",
            Self::ThreeDay => "3-day",
            Self::Day => "Day",
            Self::List => "Event List",
        }
    }
    pub fn parse(s: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|v| v.key() == s)
            .unwrap_or_default()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarSource {
    Server,
    Personal(Uuid),
}
impl CalendarSource {
    pub fn owner(self) -> Option<Uuid> {
        match self {
            Self::Server => None,
            Self::Personal(id) => Some(id),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarPreferences {
    pub week_start: u8,
    pub default_view: CalendarView,
    pub server_overlay: bool,
    pub public: bool,
    pub revision: i64,
}
impl Default for CalendarPreferences {
    fn default() -> Self {
        Self {
            week_start: 0,
            default_view: CalendarView::Month,
            server_overlay: true,
            public: false,
            revision: 0,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationTier {
    User,
    Moderator,
    Admin,
}
impl CreationTier {
    pub fn key(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Moderator => "moderator",
            Self::Admin => "admin",
        }
    }
    pub fn from_flags(admin: bool, moderator: bool) -> Self {
        if admin {
            Self::Admin
        } else if moderator {
            Self::Moderator
        } else {
            Self::User
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
        LocalResult::None => bail!("This local time does not exist in {tz}"),
        LocalResult::Ambiguous(a, b) => match occurrence {
            Some(Occurrence::Earlier) => Ok(a.min(b).with_timezone(&Utc)),
            Some(Occurrence::Later) => Ok(a.max(b).with_timezone(&Utc)),
            None => bail!("This local time repeats in {tz}; choose Earlier or Later"),
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
    bail!("{date} does not exist in {tz}")
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarEvent {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub creator_id: Uuid,
    pub creation_tier: CreationTier,
    pub mod_editable: bool,
    pub title: String,
    pub description: String,
    pub timing: EventTiming,
    pub creator_timezone: String,
    pub notice_lead_seconds: Option<i64>,
    pub notice_start: DateTime<Utc>,
    pub notice_end: DateTime<Utc>,
    pub revision: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDraft {
    pub title: String,
    pub description: String,
    pub timing: EventTiming,
    pub notice_lead_seconds: Option<i64>,
    pub mod_editable: bool,
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
        ensure!(
            self.notice_lead_seconds
                .is_none_or(|n| (0..=315360000).contains(&n)),
            "Lead time must be between 0 and 3650 days"
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
            notice_lead_seconds: e.notice_lead_seconds,
            mod_editable: e.mod_editable,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventAccess {
    pub edit: bool,
    pub notifications: bool,
    pub delegate: bool,
}
pub fn event_access(e: &CalendarEvent, viewer: Uuid, role: CreationTier) -> EventAccess {
    if let Some(owner) = e.owner_id {
        let own = owner == viewer;
        return EventAccess {
            edit: own,
            notifications: own,
            delegate: false,
        };
    }
    let admin = role == CreationTier::Admin;
    let moderator = role == CreationTier::Moderator;
    EventAccess {
        edit: admin
            || (moderator && (e.creation_tier == CreationTier::Moderator || e.mod_editable)),
        notifications: admin
            || (moderator && e.creation_tier == CreationTier::Admin && e.mod_editable),
        delegate: admin && e.creation_tier == CreationTier::Admin,
    }
}
impl CalendarEvent {
    pub fn upcoming(&self, now: DateTime<Utc>) -> bool {
        self.notice_lead_seconds.is_some() && self.notice_start <= now && now < self.notice_end
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
        let creation_tier = match r.get::<_, String>("creation_tier").as_str() {
            "admin" => CreationTier::Admin,
            "moderator" => CreationTier::Moderator,
            _ => CreationTier::User,
        };
        Self {
            id: r.get("id"),
            owner_id: r.get("owner_id"),
            creator_id: r.get("creator_id"),
            creation_tier,
            mod_editable: r.get("mod_editable"),
            title: r.get("title"),
            description: r.get("description"),
            timing,
            creator_timezone: r.get("creator_timezone"),
            notice_lead_seconds: r.get("notice_lead_seconds"),
            notice_start: r.get("notice_start"),
            notice_end: r.get("notice_end"),
            revision: r.get("revision"),
        }
    }
    fn for_viewer(mut self, viewer: Uuid) -> Self {
        if self.owner_id.is_some_and(|o| o != viewer) {
            self.notice_start += Duration::seconds(self.notice_lead_seconds.unwrap_or(0));
            self.notice_lead_seconds = None;
        }
        self
    }
}
#[derive(Clone, Debug)]
pub struct PublicCalendar {
    pub owner_id: Uuid,
    pub username: String,
}
#[derive(Clone)]
pub struct CalendarStore {
    db: Db,
}
impl CalendarStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
    pub async fn preferences(&self, viewer: Uuid) -> Result<CalendarPreferences> {
        let c = self.db.get().await?;
        let r = c
            .query_opt(
                "SELECT * FROM calendar_preferences WHERE user_id=$1",
                &[&viewer],
            )
            .await?;
        Ok(r.map(|r| CalendarPreferences {
            week_start: r.get::<_, i16>("week_start") as u8,
            default_view: CalendarView::parse(&r.get::<_, String>("default_view")),
            server_overlay: r.get("server_overlay"),
            public: r.get("public"),
            revision: r.get("revision"),
        })
        .unwrap_or_default())
    }
    pub async fn save_preferences(
        &self,
        viewer: Uuid,
        p: &CalendarPreferences,
    ) -> Result<CalendarPreferences> {
        ensure!(matches!(p.week_start, 0 | 6), "Invalid week start");
        let mut c = self.db.get().await?;
        let tx = c.transaction().await?;
        tx.query_one("SELECT id FROM users WHERE id=$1 FOR SHARE", &[&viewer])
            .await?;
        let r=tx.query_opt("INSERT INTO calendar_preferences(user_id,week_start,default_view,server_overlay,public) SELECT $1,$2,$3,$4,$5 WHERE $6::bigint=0 ON CONFLICT(user_id) DO NOTHING RETURNING revision",&[&viewer,&(p.week_start as i16),&p.default_view.key(),&p.server_overlay,&p.public,&p.revision]).await?;
        let revision = if let Some(r) = r {
            r.get("revision")
        } else {
            tx.query_opt("UPDATE calendar_preferences SET week_start=$2,default_view=$3,server_overlay=$4,public=$5,revision=revision+1 WHERE user_id=$1 AND revision=$6 RETURNING revision",&[&viewer,&(p.week_start as i16),&p.default_view.key(),&p.server_overlay,&p.public,&p.revision]).await?.ok_or_else(||anyhow::anyhow!("Calendar Settings changed elsewhere; reload"))?.get("revision")
        };
        tx.commit().await?;
        Ok(CalendarPreferences {
            revision,
            ..p.clone()
        })
    }
    pub async fn public_calendars(&self, viewer: Uuid) -> Result<Vec<PublicCalendar>> {
        let c = self.db.get().await?;
        Ok(c.query("SELECT p.user_id,u.username FROM calendar_preferences p JOIN users u ON u.id=p.user_id WHERE p.public AND p.user_id<>$1 ORDER BY lower(u.username),p.user_id",&[&viewer]).await?.into_iter().map(|r|PublicCalendar{owner_id:r.get(0),username:r.get(1)}).collect())
    }
    pub async fn role(&self, viewer: Uuid) -> Result<CreationTier> {
        let c = self.db.get().await?;
        Self::actor(&c, viewer).await.map(|a| a.0)
    }
    async fn actor<C: GenericClient + Sync>(c: &C, viewer: Uuid) -> Result<(CreationTier, Tz)> {
        let r = c
            .query_one(
                "SELECT is_admin,is_moderator,settings FROM users WHERE id=$1 FOR SHARE",
                &[&viewer],
            )
            .await?;
        let settings: serde_json::Value = r.get("settings");
        Ok((
            CreationTier::from_flags(r.get(0), r.get(1)),
            effective_timezone(settings.get("timezone").and_then(|v| v.as_str())),
        ))
    }
    #[allow(clippy::too_many_arguments)] // Explicit session scope and date/zone bounds.
    pub async fn visible(
        &self,
        viewer: Uuid,
        source: CalendarSource,
        overlay: bool,
        from: NaiveDate,
        to: NaiveDate,
        tz: Tz,
    ) -> Result<Vec<CalendarEvent>> {
        ensure!(to > from, "Invalid visible range");
        let c = self.db.get().await?;
        if let Some(owner) = source.owner().filter(|o| *o != viewer) {
            ensure!(
                c.query_opt(
                    "SELECT user_id FROM calendar_preferences WHERE user_id=$1 AND public",
                    &[&owner]
                )
                .await?
                .is_some(),
                "This calendar is private or no longer shared"
            );
        }
        let begin = day_boundary(from, tz)?;
        let end = day_boundary(to, tz)?;
        let owner = source.owner();
        // Authorization is also inside this statement so a concurrent unshare
        // cannot grant a subsequent query access using an earlier check.
        let rows=c.query("SELECT e.* FROM calendar_events e WHERE ((e.owner_id IS NOT DISTINCT FROM $1::uuid AND (e.owner_id IS NULL OR e.owner_id=$2 OR EXISTS(SELECT 1 FROM calendar_preferences p WHERE p.user_id=e.owner_id AND p.public))) OR ($3 AND e.owner_id IS NULL)) AND ((e.start_date IS NOT NULL AND daterange(e.start_date,e.end_date,'[)') && daterange($4,$5,'[)')) OR (e.start_at IS NOT NULL AND tstzrange(e.start_at,e.notice_end,'[)') && tstzrange($6,$7,'[)'))) ORDER BY COALESCE(e.start_at,e.notice_start), e.id",&[&owner,&viewer,&(overlay&&owner.is_some()),&from,&to,&begin,&end]).await?;
        Ok(rows
            .into_iter()
            .map(CalendarEvent::from_row)
            .map(|e| e.for_viewer(viewer))
            .collect())
    }
    pub async fn event(&self, viewer: Uuid, id: Uuid) -> Result<CalendarEvent> {
        let c = self.db.get().await?;
        let r=c.query_opt("SELECT e.* FROM calendar_events e WHERE e.id=$1 AND (e.owner_id IS NULL OR e.owner_id=$2 OR EXISTS(SELECT 1 FROM calendar_preferences p WHERE p.user_id=e.owner_id AND p.public))",&[&id,&viewer]).await?;
        Ok(CalendarEvent::from_row(
            r.ok_or_else(|| anyhow::anyhow!("Event unavailable; reload calendar"))?,
        )
        .for_viewer(viewer))
    }
    pub async fn upcoming_personal(
        &self,
        viewer: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>> {
        self.upcoming_scope(Some(viewer), now).await
    }
    /// Only the replica service calls this, to build a shared server snapshot.
    pub async fn upcoming_server(&self, now: DateTime<Utc>) -> Result<Vec<CalendarEvent>> {
        self.upcoming_scope(None, now).await
    }
    async fn upcoming_scope(
        &self,
        owner: Option<Uuid>,
        now: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>> {
        let c = self.db.get().await?;
        Ok(c.query("SELECT * FROM calendar_events WHERE owner_id IS NOT DISTINCT FROM $1::uuid AND notice_lead_seconds IS NOT NULL AND tstzrange(notice_start,notice_end,'[)') @> $2::timestamptz ORDER BY notice_start + notice_lead_seconds * interval '1 second',id",&[&owner,&now]).await?.into_iter().map(CalendarEvent::from_row).collect())
    }
    pub async fn save(
        &self,
        viewer: Uuid,
        source: CalendarSource,
        existing: Option<(Uuid, i64)>,
        draft: &EventDraft,
    ) -> Result<CalendarEvent> {
        draft.validate()?;
        let mut c = self.db.get().await?;
        let tx = c.transaction().await?;
        let (role, tz) = Self::actor(&tx, viewer).await?;
        ensure!(
            source.owner().is_none_or(|o| o == viewer),
            "Shared calendars are read-only"
        );
        let old = if let Some((id, revision)) = existing {
            let e = CalendarEvent::from_row(
                tx.query_opt(
                    "SELECT * FROM calendar_events WHERE id=$1 FOR UPDATE",
                    &[&id],
                )
                .await?
                .ok_or_else(|| anyhow::anyhow!("Event deleted elsewhere; reload"))?,
            );
            ensure!(
                e.owner_id == source.owner(),
                "Event belongs to a different calendar"
            );
            ensure!(
                e.revision == revision,
                "Event changed elsewhere; reload before saving (draft retained)"
            );
            let access = event_access(&e, viewer, role);
            ensure!(access.edit, "You cannot edit this event");
            ensure!(
                access.notifications || e.notice_lead_seconds == draft.notice_lead_seconds,
                "Only an admin can change this event's notifications"
            );
            ensure!(
                access.delegate || e.mod_editable == draft.mod_editable,
                "Only an admin can delegate this event"
            );
            Some(e)
        } else {
            ensure!(
                source.owner().is_some() || role != CreationTier::User,
                "Server events require a moderator or admin"
            );
            ensure!(
                source.owner().is_some()
                    || role == CreationTier::Admin
                    || draft.notice_lead_seconds.is_none(),
                "Only an admin can enable moderator-event notifications"
            );
            ensure!(
                !draft.mod_editable
                    || (source == CalendarSource::Server && role == CreationTier::Admin),
                "Only admin-created server events can be delegated"
            );
            None
        };
        let creator_timezone = old
            .as_ref()
            .map(|e| e.creator_timezone.clone())
            .unwrap_or_else(|| tz.to_string());
        let (begin, end) = draft
            .timing
            .bounds(effective_timezone(Some(&creator_timezone)))?;
        let notice_start = begin
            .checked_sub_signed(Duration::seconds(draft.notice_lead_seconds.unwrap_or(0)))
            .ok_or_else(|| anyhow::anyhow!("Lead time is outside supported dates"))?;
        let (sd, ed, st, et) = match draft.timing {
            EventTiming::AllDay {
                start,
                end_exclusive,
            } => (Some(start), Some(end_exclusive), None, None),
            EventTiming::Timed { start, end } => (None, None, Some(start), end),
        };
        let id = old.as_ref().map(|e| e.id).unwrap_or_else(Uuid::now_v7);
        let creator = old.as_ref().map(|e| e.creator_id).unwrap_or(viewer);
        let tier = old.as_ref().map(|e| e.creation_tier).unwrap_or(role);
        let row=tx.query_one("INSERT INTO calendar_events(id,owner_id,creator_id,creation_tier,mod_editable,title,description,start_date,end_date,start_at,end_at,creator_timezone,notice_lead_seconds,notice_start,notice_end) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15) ON CONFLICT(id) DO UPDATE SET mod_editable=EXCLUDED.mod_editable,title=EXCLUDED.title,description=EXCLUDED.description,start_date=EXCLUDED.start_date,end_date=EXCLUDED.end_date,start_at=EXCLUDED.start_at,end_at=EXCLUDED.end_at,notice_lead_seconds=EXCLUDED.notice_lead_seconds,notice_start=EXCLUDED.notice_start,notice_end=EXCLUDED.notice_end,revision=calendar_events.revision+1 RETURNING *",&[&id,&source.owner(),&creator,&tier.key(),&draft.mod_editable,&draft.title.trim(),&draft.description,&sd,&ed,&st,&et,&creator_timezone,&draft.notice_lead_seconds,&notice_start,&end]).await?;
        tx.commit().await?;
        Ok(CalendarEvent::from_row(row))
    }
    pub async fn delete(&self, viewer: Uuid, id: Uuid, revision: i64) -> Result<()> {
        let mut c = self.db.get().await?;
        let tx = c.transaction().await?;
        let (role, _) = Self::actor(&tx, viewer).await?;
        let e = CalendarEvent::from_row(
            tx.query_opt(
                "SELECT * FROM calendar_events WHERE id=$1 FOR UPDATE",
                &[&id],
            )
            .await?
            .ok_or_else(|| anyhow::anyhow!("Event deleted elsewhere"))?,
        );
        ensure!(
            event_access(&e, viewer, role).edit,
            "You cannot delete this event"
        );
        ensure!(
            e.revision == revision,
            "Event changed elsewhere; reload before deleting"
        );
        tx.execute("DELETE FROM calendar_events WHERE id=$1", &[&id])
            .await?;
        tx.commit().await?;
        Ok(())
    }
}
