//! `RealmService`: the process-global realm domain service, modeled on
//! `daily::svc::DailyService`. The house runs one standing world at a time:
//! the sweeper opens a numbered season whenever no world is open or active,
//! and the next one rises when that one falls. No live actor per game: every
//! mutation loads the row, validates, and persists with a revision CAS, so
//! nothing needs reconciling after a restart, and two players reaching for
//! the same territory are settled first-come-first-served by that CAS.
//!
//! Actions resolve the instant they are taken (see `resolver::apply_action`).
//! The 60s sweeper is not a resolver: it rolls the world's day over at its
//! `reset_hour_utc` (archiving the day's log), dissolves a realm everyone has
//! gone quiet in, settles the pot of a finished game whose chips have not
//! moved yet, and opens the next season when no world stands.
//!
//! Fan-out is derived, not sent. A trigger on `realm_games` notifies every
//! replica of every write; each one re-reads the snapshot and
//! `derive_events` turns the difference into `GameChanged` / `DayRolled` /
//! `GameFinished` / `SeasonOpened`, so a session learns the same thing about
//! a write whichever replica made it. Only the actor's own result and their
//! error are sent by the write, since that session is on this replica. What
//! the house is told once, the #lounge headlines when a world rises and
//! falls and the `news` the live strip reads off the row, goes out from the
//! write that won the CAS.

use std::collections::{BTreeMap, HashSet};
use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration as ChronoDuration, Timelike, Utc};
use late_core::{
    MutexRecover,
    db::Db,
    models::{realm_game::RealmGame, user::User},
};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, watch};
use uuid::Uuid;

use crate::app::activity::lounge::SYSTEM_FINGERPRINT;
use crate::app::activity::publisher::ActivityPublisher;
use crate::app::ai::svc::{AI_MODEL, AiService};
use crate::app::games::chips::svc::ChipService;
use crate::pg_listener::{Channel, Refresh, Signal, read_until_ok};

use super::map::{
    EARTH_MAP_ID, GENERATED_MAP_ID, TerritoryId, WorldMap, map_handle, territory_total,
};
use super::mapgen::{self, GeneratedMapSpec};
use super::resolver::{
    self, ActionRejected, DayBoard, DayResult, PALETTE_SLOTS, PlayerPhase, RealmAction,
    RealmGameState, RealmNews, RealmNewsKind, RealmPlayer, RealmPlayerStatus, ResolveEnd,
    STATE_VERSION, SplitMix64, day_seed,
};
use super::rulesets::{RealmPace, RealmRuleset, RealmRulesetSnapshot, STANDARD, pace_by_id};

/// How long a world stands frozen after it is opened: the gates are open,
/// the map, the spawns and the roster are all there to look at, and nothing
/// can be done to it until the clock runs out. A season is announced to the
/// whole house, so the window is a day: long enough for every time zone to
/// read the headline and take a spot before the first point is spent.
pub const REALM_MUSTER: chrono::Duration = chrono::Duration::hours(24);

/// What is left of a realm's muster, or `None` once it is open.
pub fn muster_left(created: DateTime<Utc>, now: DateTime<Utc>) -> Option<chrono::Duration> {
    let left = (created + REALM_MUSTER) - now;
    (left > chrono::Duration::zero()).then_some(left)
}

/// The first realm day a point can be spent on, in a world opened at
/// `created`: the day its muster ends on.
pub fn first_playable_day(created: DateTime<Utc>, reset_hour_utc: i16) -> i32 {
    realm_day(created + REALM_MUSTER, reset_hour_utc)
}

/// A countdown for the header, the hub card and the refusal that explains
/// why a key did nothing: "23h 59m" while there are hours in it, "1:23" for
/// the last hour.
pub fn countdown_label(left: chrono::Duration) -> String {
    let secs = left.num_seconds().max(0);
    match secs / 3600 {
        0 => format!("{}:{:02}", secs / 60, secs % 60),
        hours => format!("{hours}h {:02}m", (secs % 3600) / 60),
    }
}

/// The muster written for a sentence ("a day"), derived rather than spelled
/// out, so the copy cannot drift from the constant.
pub fn muster_label() -> String {
    let secs = REALM_MUSTER.num_seconds();
    match (secs / 3600, (secs % 3600) / 60) {
        (24, 0) => "a day".to_string(),
        (0, 1) => "a minute".to_string(),
        (0, m) => format!("{m} minutes"),
        (1, 0) => "an hour".to_string(),
        (h, 0) => format!("{h} hours"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// How long a finished world stays listed after it ends. Long enough that
/// everyone gets to see how it ended and what they were paid, and the next
/// world has risen and mustered by then. The row itself is never deleted:
/// the history and the log archive stay.
pub const REALM_RESULTS_HOURS: i64 = 24;

/// The house tempo: a season every few weeks, so an arrival at the wrong
/// moment is never far from the next world (`rulesets::PACES`).
pub const HOUSE_PACE: &str = "fast";
/// When the house world's points come back, every day. An evening in Europe
/// and an afternoon in the Americas: the hours the house is fullest.
pub const HOUSE_RESET_HOUR_UTC: i16 = 18;

/// What a season is opened as, and nothing else: the rules and the tempo are
/// entries on the rulesets' own rosters, so an unknown one cannot be asked
/// for, and the map is one of the two kinds the house plays.
#[derive(Clone)]
pub struct WorldSpec {
    pub ruleset: &'static RealmRuleset,
    pub pace: &'static RealmPace,
    /// UTC hour the daily points come back. The column's CHECK holds it to
    /// 0-23.
    pub reset_hour_utc: i16,
    pub map: WorldChoice,
}

/// The world a season is played on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldChoice {
    Earth,
    /// A world drawn from a seed the server picks, in this shape. The seed
    /// and the names are settled at the opening, so what is asked for here
    /// is three numbers and nothing that could fish for a favourable world.
    Drawn {
        continents: u8,
        islands: u8,
        territories: u16,
    },
}

/// What the house opens for `season`: the standard rules at the house pace,
/// on Earth for odd seasons and a drawn world for even ones, so the two kinds
/// of world take turns and nobody plays the same map twice running.
pub fn house_spec(season: i32) -> WorldSpec {
    let shape = GeneratedMapSpec::default();
    WorldSpec {
        ruleset: &STANDARD,
        pace: pace_by_id(HOUSE_PACE).expect("the house pace is on the roster"),
        reset_hour_utc: HOUSE_RESET_HOUR_UTC,
        map: match season % 2 == 0 {
            true => WorldChoice::Drawn {
                continents: shape.continents,
                islands: shape.islands,
                territories: shape.territories,
            },
            false => WorldChoice::Earth,
        },
    }
}

/// A seed for a new generated world. Wall-clock plus a fresh uuid: the map
/// has to be unguessable before it exists (anyone who could predict it could
/// wait for a world where their own corner looked good) and reproducible after,
/// which is what storing it in the snapshot is for.
fn rand_seed() -> u64 {
    let bytes = Uuid::now_v7();
    let bytes = bytes.as_bytes();
    u64::from_le_bytes(bytes[8..16].try_into().unwrap_or([0; 8]))
}

/// How many names to ask the model for at once. Gemini is capped at a few
/// thousand output tokens and spends some of them thinking, so a 400-country
/// world is several requests rather than one that silently comes back short.
const NAME_BATCH: usize = 80;
/// Total budget for naming a world. Creation is already asynchronous, but a
/// realm that takes a minute to appear because a model is slow is a realm
/// that looks broken; past this the pool finishes the job.
const NAMING_BUDGET: Duration = Duration::from_secs(20);

/// The flavours a generated world can be named in. Picked from the seed, so
/// two worlds made a minute apart do not read as the same continent — one
/// prompt for every map would converge on the same hundred names.
const NAME_FLAVOURS: &[&str] = &[
    "windswept and northern, with hard consonants and fjord-and-moor endings",
    "sunlit and coastal, with soft vowels and Romance-sounding endings",
    "arid and old, with desert-and-caravan cadence",
    "mountainous and remote, with high-valley and highland cadence",
    "riverine and humid, with long flowing vowels",
    "volcanic and oceanic, with island cadence and repeated vowels",
    "steppe and horse country, with open plains cadence",
    "forested and cold, with deep-woods cadence",
];

const SWEEP_INTERVAL: Duration = Duration::from_secs(60);
/// Resolution work is bounded per sweep tick so a mass catch-up after long
/// downtime can't stall the loop; the next tick continues where this one
/// stopped.
const MAX_GAMES_PER_SWEEP: usize = 20;
/// CAS-retry attempts for join/leave style read-modify-write mutations.
const CAS_RETRIES: usize = 3;

/// Days since the Unix epoch of `now`, UTC. The unit of realm time for
/// anything not tied to a single game (joins, exits, seeds).
pub fn utc_day(now: DateTime<Utc>) -> i32 {
    now.timestamp().div_euclid(86_400) as i32
}

/// The realm day of `now` for a game that refills at `reset_hour_utc`. A
/// game resetting at 18:00 UTC counts 18:00 -> 18:00 as one day, so its
/// players get their points back in their own evening rather than at some
/// arbitrary midnight.
pub fn realm_day(now: DateTime<Utc>, reset_hour_utc: i16) -> i32 {
    let shift = i64::from(reset_hour_utc.clamp(0, 23)) * 3_600;
    (now.timestamp() - shift).div_euclid(86_400) as i32
}

/// When the next refill lands for a game at `reset_hour_utc`.
pub fn next_reset(now: DateTime<Utc>, reset_hour_utc: i16) -> DateTime<Utc> {
    let hour = u32::from(reset_hour_utc.clamp(0, 23) as u8);
    let today = now
        .with_hour(hour)
        .and_then(|t| t.with_minute(0))
        .and_then(|t| t.with_second(0))
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(now);
    if today > now {
        today
    } else {
        today + ChronoDuration::days(1)
    }
}

/// `HH:00 UTC` plus the same instant in the viewer's zone, for the hour
/// picker and the board header. Without the second half a player has to do
/// the arithmetic themselves, which is exactly the trap this feature exists
/// to avoid.
///
/// The zone conversion is sampled on the next occurrence of the hour, not on
/// a fixed instant: a zone that keeps summer time is an hour off for half the
/// year otherwise, and being an hour off about when your points come back is
/// the whole failure this label prevents. When that instant lands on another
/// date in the viewer's zone the label says so — a refill at 23:00 UTC is
/// tomorrow morning for half the world, and the day it belongs to is what a
/// player is actually planning around.
pub fn reset_hour_label(reset_hour_utc: i16, viewer_tz: Option<chrono_tz::Tz>) -> String {
    reset_hour_label_at(reset_hour_utc, viewer_tz, Utc::now())
}

/// `reset_hour_label` with the clock passed in, so the day-shift and summer
/// time branches are testable without waiting for the calendar.
pub fn reset_hour_label_at(
    reset_hour_utc: i16,
    viewer_tz: Option<chrono_tz::Tz>,
    now: DateTime<Utc>,
) -> String {
    let hour = reset_hour_utc.clamp(0, 23);
    match viewer_tz {
        Some(tz) => {
            let sample = next_reset(now, hour);
            let local = sample.with_timezone(&tz);
            let shift = match local
                .date_naive()
                .signed_duration_since(sample.date_naive())
                .num_days()
            {
                d if d > 0 => " next day",
                d if d < 0 => " prev day",
                _ => "",
            };
            format!(
                "{:02}:00 UTC · {} local{shift}",
                hour,
                local.format("%H:%M")
            )
        }
        None => format!("{hour:02}:00 UTC"),
    }
}

#[derive(Clone)]
pub struct RealmService {
    db: Db,
    chip_svc: ChipService,
    /// Only used to name generated worlds, and only ever as an improvement on
    /// the built-in pool: a realm is fully playable with the AI switched off.
    ai: AiService,
    /// What realm tells the rest of the house: the headline when a world
    /// rises and when it falls, the #lounge line for the conqueror, and the
    /// daily call. All of it goes out from the write that won the CAS, so
    /// once.
    activity: ActivityPublisher,
    snapshot_tx: watch::Sender<Arc<RealmSnapshot>>,
    snapshot_rx: watch::Receiver<Arc<RealmSnapshot>>,
    event_tx: broadcast::Sender<RealmEvent>,
    /// Publishes are serialised so the diff each one derives its events from
    /// is against the snapshot the previous one wrote. Two publishes racing
    /// from the same old snapshot would announce the same change twice.
    publish_lock: Arc<tokio::sync::Mutex<()>>,
    /// Rows this replica has already reported as unreadable, so a corrupt
    /// row is logged and counted once rather than every minute.
    rejected_rows: Arc<std::sync::Mutex<HashSet<Uuid>>>,
}

#[derive(Clone, Debug, Default)]
pub struct RealmSnapshot {
    pub open_games: Vec<RealmGameItem>,
    pub active_games: Vec<RealmGameItem>,
    /// Finished within the last `REALM_RESULTS_HOURS` (window enforced in SQL).
    pub finished_games: Vec<RealmGameItem>,
    /// False only for the empty snapshot a service starts with. The first
    /// read seeds it; events are derived only from a seeded snapshot, so a
    /// resync after a reconnect does not announce every game as new.
    pub seeded: bool,
}

impl RealmSnapshot {
    /// The standing world: the one open or active row. `None` between a
    /// fall and the next rise, which lasts at most one sweeper tick.
    pub fn live(&self) -> Option<&RealmGameItem> {
        self.open_games
            .iter()
            .chain(self.active_games.iter())
            .next()
    }

    /// The last world that ended, while it is still listed.
    pub fn last_finished(&self) -> Option<&RealmGameItem> {
        self.finished_games.first()
    }

    /// The worlds the house still shows: the standing one, and the last one
    /// that ended while it is listed. What the live strip paints and opens.
    pub fn listed(&self) -> impl Iterator<Item = &RealmGameItem> {
        self.live().into_iter().chain(self.last_finished())
    }
}

/// Public per-game info: enough for the hub card, the live strip and the
/// board header, never the whole state (the board loads that itself).
#[derive(Clone, Debug)]
pub struct RealmGameItem {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub creator_id: Uuid,
    /// Which world this is, counted from the first the house opened.
    pub season: i32,
    /// The last moment the live strip shows for this world, stamped by the
    /// write that made it.
    pub news: Option<RealmNews>,
    /// The row's status: what list it is in, and what the diff keys on.
    pub status: String,
    /// The state's revision: any write moves it, which is how a replica
    /// that did not make the write knows the world moved.
    pub revision: u64,
    /// What the pot did once settled; `None` until the sweeper has.
    pub payout: Option<String>,
    /// The world's name: the stored one, else "Season N" (`game_name`).
    pub name: String,
    pub ruleset_id: String,
    pub ruleset_name: String,
    /// The tempo this game runs at (`Blitz`, `Normal`, `Epic`…).
    pub pace_name: String,
    pub map_id: String,
    pub min_players: u8,
    pub max_players: u8,
    pub last_resolved_day: i32,
    /// UTC hour this game's action points refill at.
    pub reset_hour_utc: i16,
    /// Newcomers are still welcome: there is room on the roster and the map
    /// is not yet carved up past `join_max_claimed`.
    pub joinable: bool,
    /// How much of the map is spoken for, 0.0 to 1.0 — what closes the door.
    pub claimed: f64,
    /// The viewer-independent half of "can I withdraw": somebody in this
    /// game is still in their untouchable first day, or is its only player.
    /// The service has the final say per player.
    pub can_withdraw: bool,
    pub winner_user_id: Option<Uuid>,
    /// The world's chat room; `None` on a world from before rooms were born
    /// with it, until the sweeper attaches one. Part of the diff, so a board
    /// open on such a world reloads and finds its room without a reopen.
    pub chat_room_id: Option<Uuid>,
    pub players: Vec<RealmPlayerItem>,
}

#[derive(Clone, Debug)]
pub struct RealmPlayerItem {
    pub user_id: Uuid,
    pub username: String,
    pub status: RealmPlayerStatus,
    pub territories: u16,
    /// The palette slot they play in, which is how the colour picker knows
    /// which colours are still going when someone asks to join.
    pub color: u8,
}

impl RealmGameItem {
    pub fn is_member(&self, user_id: Uuid) -> bool {
        self.players.iter().any(|p| p.user_id == user_id)
    }
}

/// What a session is told. Everything but `ActionResolved` and `Error` is
/// derived from the snapshot diff in `publish` (see `derive_events`), never
/// sent by a mutation, so it reads the same on every replica.
#[derive(Clone, Debug, PartialEq)]
pub enum RealmEvent {
    /// A world rose and is still mustering: an invitation, which every
    /// session raises as a banner while the realm is frozen.
    SeasonOpened {
        game_id: Uuid,
        season: i32,
        name: String,
        opens_at: DateTime<Utc>,
    },
    /// Targeted result of the actor's own action — the banner they see. Other
    /// sessions learn about it through `GameChanged`.
    ActionResolved {
        game_id: Uuid,
        user_id: Uuid,
        target: TerritoryId,
        probability: f64,
        success: bool,
        attack: bool,
        points_left: u8,
    },
    /// The world moved (a write, or the row leaving the listing): anyone with
    /// this board open should reload it.
    GameChanged {
        game_id: Uuid,
    },
    /// A game's day rolled over at its reset hour: budgets are back. Carries
    /// the game's name because the call goes out to everyone on its roster
    /// wherever they are in the app, and "a realm called you" is no use
    /// without saying which.
    DayRolled {
        game_id: Uuid,
        day: i32,
        name: String,
    },
    GameFinished {
        game_id: Uuid,
        winner_user_id: Option<Uuid>,
    },
    Error {
        user_id: Uuid,
        message: String,
    },
}

impl RealmService {
    pub fn new(db: Db, chip_svc: ChipService, activity: ActivityPublisher, ai: AiService) -> Self {
        let (snapshot_tx, snapshot_rx) = watch::channel(Arc::new(RealmSnapshot::default()));
        let (event_tx, _) = broadcast::channel(256);
        Self {
            db,
            chip_svc,
            activity,
            ai,
            snapshot_tx,
            snapshot_rx,
            event_tx,
            publish_lock: Arc::new(tokio::sync::Mutex::new(())),
            rejected_rows: Arc::new(std::sync::Mutex::new(HashSet::new())),
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<Arc<RealmSnapshot>> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<RealmEvent> {
        self.event_tx.subscribe()
    }

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::RealmGameChanged];

    /// Keep every replica's snapshot in step with `realm_game_changed`, which
    /// a trigger fires on every write to `realm_games` on whichever replica
    /// made it. The resync is the seed and the reconnect catch-up, retried
    /// until it lands; a notify is one re-read, and a burst that queued while
    /// one ran collapses into the next. The re-read is also where this
    /// replica's sessions learn what changed (`derive_events`). A failed
    /// re-read after a notify leaves this replica on its last snapshot until
    /// the next notify or the sweeper's minute, so it is logged and the
    /// worker keeps going.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                let mut resync = signal == Signal::Resync;
                while let Ok(queued) = signals.try_recv() {
                    resync |= queued == Signal::Resync;
                }
                if resync {
                    read_until_ok(Refresh::RealmGames, || svc.refresh()).await;
                } else if let Err(error) = svc.refresh().await {
                    tracing::error!(error = ?error, "failed to refresh realm games after a notify");
                }
            }
        })
    }

    /// The one background loop: roll days over at each game's own reset hour
    /// (archiving the finished day, dissolving the abandoned), settle any pot
    /// still owed, then republish the snapshot (which doubles as the
    /// slow-poll backstop).
    pub fn start_sweeper_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(e) = svc.sweep(Utc::now()).await {
                    tracing::error!(error = ?e, "failed to sweep realm games");
                }
                if let Err(e) = svc.refresh().await {
                    tracing::error!(error = ?e, "failed to refresh realm games");
                }
                tokio::time::sleep(SWEEP_INTERVAL).await;
            }
        });
    }

    // ----- mutating tasks (fire-and-forget, errors become targeted events)

    pub fn join_game_task(
        &self,
        user_id: Uuid,
        username: String,
        game_id: Uuid,
        color: Option<u8>,
    ) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.join_game(user_id, &username, game_id, color).await {
                tracing::error!(error = ?e, %user_id, %game_id, "failed to join realm game");
                svc.send_error(user_id, &e);
            }
        });
    }

    pub fn leave_game_task(&self, user_id: Uuid, game_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.leave_game(user_id, game_id).await {
                tracing::error!(error = ?e, %user_id, %game_id, "failed to leave realm game");
                svc.send_error(user_id, &e);
            }
        });
    }

    /// Take one action now. Fire-and-forget like the rest; the result comes
    /// back as a targeted `ActionResolved` (or `Error`) event.
    pub fn act_task(&self, user_id: Uuid, game_id: Uuid, action: RealmAction) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.act(user_id, game_id, action).await {
                tracing::error!(error = ?e, %user_id, %game_id, "failed to resolve realm action");
                svc.send_error(user_id, &e);
            }
        });
    }

    // ----- reads

    pub async fn load_game(&self, game_id: Uuid) -> Result<Option<RealmGame>> {
        let client = self.db.get().await?;
        RealmGame::get(&client, game_id).await
    }

    pub async fn load_day_logs(
        &self,
        game_id: Uuid,
        before_day: Option<i32>,
        limit: i64,
    ) -> Result<Vec<ArchivedDay>> {
        let client = self.db.get().await?;
        let days = RealmGame::load_day_logs(&client, game_id, before_day, limit).await?;
        Ok(days
            .into_iter()
            .filter_map(|day| {
                let result: DayResult = serde_json::from_value(day.log).ok()?;
                let board = day
                    .board
                    .and_then(|board| serde_json::from_value::<DayBoard>(board).ok());
                Some(ArchivedDay { result, board })
            })
            .collect())
    }

    // ----- mutations

    /// Open the next season of the house world, when none is standing: the
    /// house's spec (`house_spec`) for the season number the row will get.
    /// `None` when a world already stands or another replica is opening one.
    pub async fn open_season(&self) -> Result<Option<RealmGame>> {
        self.open_world(house_spec).await
    }

    /// Open a world, `spec` saying what it is for the season number it will
    /// carry. The opening is claimed in the database first, and everything
    /// after the claim (the season number, the names the AI is paid for, the
    /// insert) happens in its one transaction: the loser of a race spends
    /// nothing, and an opening that dies half way leaves no row and no claim
    /// behind. The row is the system user's and lands live: nobody is on the
    /// roster until somebody marches in, and the muster is what holds the
    /// first point back. `None` when a world already stands or another
    /// replica holds the claim.
    pub(crate) async fn open_world(
        &self,
        spec: impl FnOnce(i32) -> WorldSpec,
    ) -> Result<Option<RealmGame>> {
        let mut client = self.db.get().await?;
        // The house's own row. The system user is ensured by the lounge feed
        // at startup; a sweeper tick that runs before it is simply too early
        // and the next one opens the world.
        let house = match User::find_by_fingerprint(&client, SYSTEM_FINGERPRINT).await? {
            Some(user) => user.id,
            None => bail!("system user not ensured yet"),
        };
        let tx = client.transaction().await?;
        if !RealmGame::try_lock_season_opening(&tx).await? {
            return Ok(None);
        }
        if RealmGame::live(&tx).await?.is_some() {
            return Ok(None);
        }
        let season = RealmGame::next_season(&tx).await?;
        let spec = spec(season);
        // The rules, the tempo and the map freeze into the snapshot: a map
        // or a pace retired later never strands a live game.
        let mut snapshot = RealmRulesetSnapshot::at_pace(spec.ruleset, spec.pace);
        match spec.map {
            WorldChoice::Earth => snapshot.map_id = EARTH_MAP_ID.to_string(),
            WorldChoice::Drawn {
                continents,
                islands,
                territories,
            } => {
                // Drawn from a seed the server picks: the seed decides the
                // map, and anyone who could choose it could shop for a world
                // where their own first spawn is a fortress. Names are
                // settled here too, once, because the AI is not a pure
                // function of the seed and everything after this point has
                // to be able to rebuild the same world from what was stored.
                let mut built = GeneratedMapSpec {
                    seed: rand_seed(),
                    continents,
                    islands,
                    territories,
                    names: Vec::new(),
                }
                .normalized();
                built.names = self
                    .invent_names(built.territories as usize, built.seed)
                    .await;
                snapshot.map_id = GENERATED_MAP_ID.to_string();
                snapshot.map_spec = Some(built);
            }
        }
        map_handle(&snapshot.map_id, snapshot.map_spec.as_ref())
            .context("realm map has no board")?;
        let now = Utc::now();
        let today = realm_day(now, spec.reset_hour_utc);
        let mut state = RealmGameState {
            version: STATE_VERSION,
            revision: 1,
            ruleset: snapshot,
            // An empty world: everyone arrives through the gates, and the
            // first to do so is protected by their own first days like
            // everyone after them.
            players: Vec::new(),
            ownership: BTreeMap::new(),
            forts: BTreeMap::new(),
            last_day: None,
            start_player_count: 0,
            start_day: today,
            news: Some(RealmNews {
                at: now,
                kind: RealmNewsKind::Opened,
            }),
        };
        state.roll_day(today, day_seed(Uuid::nil(), today));
        let Some(game) = RealmGame::open_season(
            &tx,
            season,
            house,
            spec.ruleset.id,
            spec.reset_hour_utc,
            today,
            &serde_json::to_value(&state)?,
        )
        .await?
        else {
            return Ok(None);
        };
        // The world's own room is born with it, in the same transaction: a
        // world never stands without somewhere to talk, and a rolled-back
        // opening leaves no room behind either.
        let chat_room_id = RealmGame::attach_chat_room(&tx, game.id)
            .await
            .context("attaching realm chat room")?;
        let game = RealmGame {
            chat_room_id: Some(chat_room_id),
            ..game
        };
        tx.commit().await?;
        // The banner every session gets is derived from the snapshot (so it
        // is raised on every replica); the headline goes out from here, once,
        // so the announcement sits in #lounge history for everyone who was
        // not looking.
        self.activity.realm_season_opened(
            game.id,
            game.season,
            &world_label(&state),
            &muster_label(),
        );
        self.publish(&client).await?;
        Ok(Some(game))
    }

    /// Join a realm that is already being played. There is no waiting room:
    /// you arrive, you spawn, and your own first days protect you (see
    /// `PlayerPhase`). Doors close once the map is mostly carved up —
    /// `join_max_claimed` — because past that a newcomer is only food.
    pub async fn join_game(
        &self,
        user_id: Uuid,
        username: &str,
        game_id: Uuid,
        color: Option<u8>,
    ) -> Result<()> {
        let client = self.db.get().await?;
        for _ in 0..CAS_RETRIES {
            let game = RealmGame::get(&client, game_id)
                .await?
                .context("realm game not found")?;
            if game.status != RealmGame::STATUS_ACTIVE && game.status != RealmGame::STATUS_OPEN {
                bail!("this realm game is over");
            }
            let mut state = parse_state(&game.state)?;
            let map = self.map_for(&state)?;
            if state.players.iter().any(|p| p.user_id == user_id) {
                bail!("you are already in this realm game");
            }
            if state.players.len() >= state.ruleset.max_players as usize {
                bail!("this realm game is full");
            }
            if !state.joinable(&map) {
                bail!(
                    "this realm is {:.0}% conquered — too late to join",
                    state.claimed_fraction(&map) * 100.0
                );
            }
            let expected = state.revision as i64;
            // Clocks start when play does: an arrival during the muster is
            // new on the day the gates open, not on a day spent frozen.
            let joined_day = realm_day(Utc::now(), game.reset_hour_utc)
                .max(first_playable_day(game.created, game.reset_hour_utc));
            let color = pick_color(&state.taken_colors(), color)?;
            let claimed = (state.claimed_fraction(&map) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            state
                .players
                .push(new_player(user_id, username, joined_day, color, claimed));
            state.start_player_count = state.start_player_count.max(state.players.len() as u8);
            let spawn_seed =
                day_seed(game_id, state.revision as i32) ^ u64::from(user_id.as_u128() as u32);
            if !spawn_player(&mut state, &map, game_id, user_id, spawn_seed) {
                bail!("there is no free land left to spawn on");
            }
            state.news = Some(RealmNews {
                at: Utc::now(),
                kind: RealmNewsKind::Joined {
                    user_id,
                    username: username.to_string(),
                    color,
                },
            });
            state.revision += 1;
            let updated = RealmGame::update_state_cas(
                &client,
                game_id,
                &serde_json::to_value(&state)?,
                expected,
            )
            .await?;
            if updated > 0 {
                self.publish(&client).await?;
                return Ok(());
            }
        }
        bail!("the realm moved while you were joining, try again")
    }

    /// Withdraw from a realm — only while you are still **new**, the days
    /// where nobody can touch you and you have taken nothing that matters.
    /// After that you are committed: quitting used to hand the game to
    /// whoever was left. The last player standing may always pack the whole
    /// thing up, since there is nobody to hand it to.
    pub async fn leave_game(&self, user_id: Uuid, game_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        for _ in 0..CAS_RETRIES {
            let game = RealmGame::get(&client, game_id)
                .await?
                .context("realm game not found")?;
            if game.status == RealmGame::STATUS_FINISHED
                || game.status == RealmGame::STATUS_CANCELLED
            {
                bail!("this realm game is over");
            }
            let mut state = parse_state(&game.state)?;
            if !state.players.iter().any(|p| p.user_id == user_id) {
                bail!("you are not in this realm game");
            }
            let today = realm_day(Utc::now(), game.reset_hour_utc);
            let alone = state.players.len() == 1;
            if !alone && state.phase(user_id, today) != PlayerPhase::New {
                bail!("you can only withdraw in your first day here");
            }
            // The whole realm goes with the last player out.
            if alone {
                let cancelled = RealmGame::cancel(&client, game_id).await?;
                if cancelled > 0 {
                    self.publish(&client).await?;
                }
                return Ok(());
            }
            let expected = state.revision as i64;
            let freed = state.release_holdings(user_id);
            state.log(
                today,
                day_seed(game_id, today),
                resolver::LogEntry::Left { user_id, freed },
            );
            state.players.retain(|p| p.user_id != user_id);
            state.revision += 1;
            let updated = RealmGame::update_state_cas(
                &client,
                game_id,
                &serde_json::to_value(&state)?,
                expected,
            )
            .await?;
            if updated > 0 {
                self.publish(&client).await?;
                return Ok(());
            }
        }
        bail!("the realm moved while you were leaving, try again")
    }

    // ----- playing

    /// Take one action, right now. Loads the game, rolls it against the world
    /// as it currently stands, and persists under the row CAS — so when two
    /// players go for the same territory within the same second, the one
    /// whose write lands first has it, and the other is told the ground
    /// moved and keeps their point.
    pub async fn act(&self, user_id: Uuid, game_id: Uuid, action: RealmAction) -> Result<()> {
        self.act_at(user_id, game_id, action, Utc::now()).await
    }

    /// `act` with the clock passed in, the way `sweep` takes one: the muster
    /// is a wall-clock rule, and a test should not have to wait two real
    /// minutes to watch it end.
    pub async fn act_at(
        &self,
        user_id: Uuid,
        game_id: Uuid,
        action: RealmAction,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let client = self.db.get().await?;
        for attempt in 0..CAS_RETRIES {
            let game = RealmGame::get(&client, game_id)
                .await?
                .context("realm game not found")?;
            if game.status != RealmGame::STATUS_ACTIVE {
                bail!("this realm game is not running");
            }
            // Frozen while it musters: everyone who is coming gets to be
            // here before the first point is spent.
            if let Some(left) = muster_left(game.created, now) {
                bail!(
                    "this realm opens in {} — everyone starts together",
                    countdown_label(left)
                );
            }
            let mut state = parse_state(&game.state)?;
            let map = self.map_for(&state)?;
            let expected = state.revision as i64;
            let day = realm_day(now, game.reset_hour_utc);

            // A day that turned over while nobody was looking: archive the
            // finished log before writing into the new one. The board goes
            // with it, taken *before* this action is applied — yesterday's
            // closing position is yesterday's, and this action belongs to
            // today.
            let closing_board = state
                .last_day
                .as_ref()
                .filter(|last| last.day < day)
                .map(|_| DayBoard::of(&state));
            let rolled = state.roll_day(day, day_seed(game_id, day));
            let logged_before = state.last_day.as_ref().map_or(0, |d| d.entries.len());

            let seed =
                resolver::action_seed(game_id, day, state.revision, user_id, action.target());
            let outcome = match resolver::apply_action(&mut state, &map, user_id, action, day, seed)
            {
                Ok(outcome) => outcome,
                Err(rejected) => return Err(reject(rejected)),
            };
            if let Some(news) = news_of_action(&state, &map, logged_before, outcome.end, now) {
                state.news = Some(news);
            }

            let state_value = serde_json::to_value(&state)?;
            let name = display_name(&game, &state);
            let updated = match outcome.end {
                ResolveEnd::Ongoing => {
                    RealmGame::advance_day_cas(&client, game_id, day, &state_value, expected)
                        .await?
                }
                ResolveEnd::Won(winner) => {
                    let updated = RealmGame::finish_cas(
                        &client,
                        game_id,
                        day,
                        Some(winner),
                        &state_value,
                        expected,
                    )
                    .await?;
                    if updated > 0 {
                        self.on_finished(&game, &state, Some(winner));
                    }
                    updated
                }
                ResolveEnd::Dissolved => {
                    let updated =
                        RealmGame::finish_cas(&client, game_id, day, None, &state_value, expected)
                            .await?;
                    if updated > 0 {
                        self.on_finished(&game, &state, None);
                    }
                    updated
                }
            };
            if updated == 0 {
                // Someone else wrote first. Their action changed the world, so
                // this one is rolled again from scratch rather than replayed.
                if attempt + 1 < CAS_RETRIES {
                    continue;
                }
                bail!("the realm moved while you acted, try again");
            }

            if let Some(finished) = rolled {
                self.archive_with_board(&client, game_id, &finished, closing_board.as_ref())
                    .await;
                // The day that opened, which is not `finished.day + 1` when
                // the realm went days without a rollover.
                self.announce_day_rolled(game_id, &name, day, &state);
            }
            if let Some(current) = state.last_day.as_ref() {
                // A game that just ended has no tomorrow to snapshot, so the
                // final board is written onto the day it ended on.
                let final_board =
                    (!matches!(outcome.end, ResolveEnd::Ongoing)).then(|| DayBoard::of(&state));
                self.archive_with_board(&client, game_id, current, final_board.as_ref())
                    .await;
            }
            // The actor's own banner is the one thing the write sends
            // itself: their session is on this replica. Everyone else, here
            // and on every other replica, learns the world moved from the
            // snapshot diff.
            let _ = self.event_tx.send(RealmEvent::ActionResolved {
                game_id,
                user_id,
                target: action.target(),
                probability: outcome.probability,
                success: outcome.success,
                attack: matches!(action, RealmAction::Attack { .. }),
                points_left: outcome.points_left,
            });
            self.publish(&client).await?;
            return Ok(());
        }
        bail!("the realm moved while you acted, try again")
    }

    // ----- sweeping

    /// Per-game housekeeping, run every 60s: roll the day over at the game's
    /// own reset hour (archiving the log it closes), dissolve a realm that
    /// every living player has been quiet in for `REALM_ABANDON_DAYS`, and
    /// settle the pot of any finished game whose chips have not moved.
    /// Nothing here decides a move — moves resolved when they were taken.
    /// Both work lists are bounded per tick and filtered in SQL, so a tick
    /// only ever picks up games with something to do and the next tick
    /// continues where this one stopped.
    pub async fn sweep(&self, now: DateTime<Utc>) -> Result<()> {
        let client = self.db.get().await?;
        match RealmGame::live(&client).await? {
            // No world standing: the next season rises. Every replica's
            // sweeper tries; the unique index lets exactly one in.
            None => {
                let outcome = match self.open_season().await {
                    Ok(Some(game)) => {
                        tracing::info!(game_id = %game.id, season = game.season, "realm season opened");
                        RealmSeasonOpen::Opened
                    }
                    Ok(None) => RealmSeasonOpen::Standing,
                    Err(e) => {
                        tracing::error!(error = ?e, "failed to open the next realm season");
                        RealmSeasonOpen::Failed
                    }
                };
                crate::metrics::record_realm_season_open(outcome);
            }
            // A world opened before rooms were born with it gets one here.
            // Idempotent, so two replicas doing it at once land on one room.
            Some(game) if game.chat_room_id.is_none() => {
                let outcome = match RealmGame::attach_chat_room(&client, game.id).await {
                    Ok(room_id) => {
                        tracing::info!(game_id = %game.id, %room_id, "realm chat room attached");
                        RealmChatRoomAttach::Attached
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, game_id = %game.id, "failed to attach realm chat room");
                        RealmChatRoomAttach::Failed
                    }
                };
                crate::metrics::record_realm_chat_room_attach(outcome);
            }
            Some(_) => {}
        }
        match RealmGame::delete_stale_chat_rooms(&client).await {
            Ok(0) => {}
            Ok(deleted) => {
                tracing::info!(deleted, "reaped stale realm chat rooms");
                crate::metrics::record_realm_chat_rooms_reaped(deleted);
            }
            Err(e) => tracing::error!(error = ?e, "failed to reap stale realm chat rooms"),
        }
        let games =
            RealmGame::list_active_for_sweep(&client, now, MAX_GAMES_PER_SWEEP as i64).await?;
        let mut changed = false;
        for game in games {
            let day = realm_day(now, game.reset_hour_utc);
            match self.sweep_game(&client, &game, day).await {
                Ok(touched) => changed |= touched,
                Err(e) => {
                    tracing::error!(error = ?e, game_id = %game.id, "failed to sweep realm game");
                }
            }
        }
        let owed = RealmGame::list_finished_unpaid(&client, MAX_GAMES_PER_SWEEP as i64).await?;
        for game in owed {
            if let Err(e) = self.settle_payouts(&client, &game).await {
                tracing::error!(error = ?e, game_id = %game.id, "failed to settle realm payout, retrying next sweep");
            }
        }
        if changed {
            self.publish(&client).await?;
        }
        Ok(())
    }

    async fn sweep_game(
        &self,
        client: &tokio_postgres::Client,
        game: &RealmGame,
        day: i32,
    ) -> Result<bool> {
        let mut state = parse_state(&game.state)?;
        let map = self.map_for(&state)?;
        let expected = state.revision as i64;
        // The closing board, taken before the day turns and before the sweep
        // frees anybody's land: what the map looked like as the day ended.
        let closing_board = DayBoard::of(&state);
        let finished_day = state.roll_day(day, day_seed(game.id, day));
        // The kicks, like the rollover itself, reach every board through the
        // snapshot diff that follows this write.
        let (_kicked, end) = resolver::sweep_idle(&mut state, &map, day);
        // `roll_day` alone is a state change worth persisting (the new day's
        // empty log), so this always writes once per game per day.
        state.revision += 1;
        if let Some(news) = ended_news(&state, end, Utc::now()) {
            state.news = Some(news);
        }

        let state_value = serde_json::to_value(&state)?;
        let name = display_name(game, &state);
        let updated = match end {
            ResolveEnd::Ongoing => {
                RealmGame::advance_day_cas(client, game.id, day, &state_value, expected).await?
            }
            ResolveEnd::Won(winner) => {
                let updated = RealmGame::finish_cas(
                    client,
                    game.id,
                    day,
                    Some(winner),
                    &state_value,
                    expected,
                )
                .await?;
                if updated > 0 {
                    self.on_finished(game, &state, Some(winner));
                }
                updated
            }
            ResolveEnd::Dissolved => {
                let updated =
                    RealmGame::finish_cas(client, game.id, day, None, &state_value, expected)
                        .await?;
                if updated > 0 {
                    self.on_finished(game, &state, None);
                }
                updated
            }
        };
        if updated == 0 {
            // A player acted in the same instant; their write carries the
            // rollover too, and the next tick re-checks the kicks.
            return Ok(false);
        }
        if let Some(finished) = finished_day {
            self.archive_with_board(client, game.id, &finished, Some(&closing_board))
                .await;
        }
        if let Some(current) = state.last_day.as_ref() {
            // A sweep that ended the game writes the final board on the day
            // it ended, since there is no rollover left to carry it.
            let final_board = (!matches!(end, ResolveEnd::Ongoing)).then(|| DayBoard::of(&state));
            self.archive_with_board(client, game.id, current, final_board.as_ref())
                .await;
        }
        self.announce_day_rolled(game.id, &name, day, &state);
        Ok(true)
    }

    /// Write a day's log to the archive. Best-effort: the log is history, and
    /// losing a line must never fail the action that produced it.
    /// Write a day's log, and the board as it closed when there is one. Only
    /// the rollover and the finish pass a board: the live upsert while a day
    /// is being played must not overwrite a closing snapshot with a mid-day
    /// one.
    async fn archive_with_board(
        &self,
        client: &tokio_postgres::Client,
        game_id: Uuid,
        day: &DayResult,
        board: Option<&DayBoard>,
    ) {
        let Ok(log) = serde_json::to_value(day) else {
            return;
        };
        let board = board.and_then(|board| serde_json::to_value(board).ok());
        if let Err(e) = RealmGame::archive_day(
            client,
            game_id,
            day.day,
            day.seed as i64,
            &log,
            board.as_ref(),
        )
        .await
        {
            tracing::error!(error = ?e, %game_id, day = day.day, "failed to archive realm day");
        }
    }

    /// The daily summons to the lounge: one ticker line so the rest of the
    /// place can see a war is running — but only for a realm with more than
    /// one person in it, since a realm sat in alone calling for players
    /// every day is noise, not news. The players' own nudge (`DayRolled`)
    /// is derived from the snapshot on every replica, not sent from here.
    fn announce_day_rolled(&self, game_id: Uuid, name: &str, day: i32, state: &RealmGameState) {
        let playing = state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .count();
        if playing > 1 {
            self.activity.realm_calls(game_id, name, day);
        }
    }

    /// What the write that finished a game does, once: the #lounge line for
    /// the conqueror, the headline that the world fell, and the pot. The
    /// finish is durable before this runs, so the chips are settled from the
    /// stored row in a task of their own, and anything that stops them is
    /// picked up by the sweeper (`list_finished_unpaid`) rather than lost.
    fn on_finished(&self, game: &RealmGame, state: &RealmGameState, winner: Option<Uuid>) {
        let game_id = game.id;
        let name = display_name(game, state);
        if let Some(winner) = winner {
            self.activity.game_won_task(
                winner,
                crate::app::activity::event::ActivityGame::Realm,
                Some(format!(
                    "{}: a {}-player war for {}",
                    name,
                    state.start_player_count,
                    world_label(state)
                )),
                None,
            );
        }
        let winner_name = winner
            .and_then(|id| state.player(id))
            .map(|p| p.username.clone());
        self.activity.realm_season_ended(
            game_id,
            game.season,
            winner_name,
            ended_pot(state, game_id, winner),
            state.start_player_count,
        );
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.settle_finished(game_id).await {
                tracing::error!(error = ?error, %game_id, "failed to settle realm payout, the sweeper retries");
            }
        });
    }

    async fn settle_finished(&self, game_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let game = RealmGame::get(&client, game_id)
            .await?
            .context("realm game not found")?;
        self.settle_payouts(&client, &game).await
    }

    /// Settle a finished game's pot and record what it did on the row.
    ///
    /// Ranked chip payouts go only to players who saw the game through
    /// (alive at finish or eliminated fighting) and played enough to place;
    /// leavers and kicked players are skipped and the next eligible player
    /// moves up. Each credit is idempotent per (template, game, user)
    /// through the per_event claim, so running this twice, or on two
    /// replicas at once, pays nothing twice. `payout` is written only once
    /// every place in the plan has been credited; a failed credit leaves it
    /// NULL and the error with the caller, and the sweeper comes back for it.
    pub async fn settle_payouts(
        &self,
        client: &tokio_postgres::Client,
        game: &RealmGame,
    ) -> Result<()> {
        let state = parse_state(&game.state)?;
        let Some(winner) = game.winner_user_id else {
            // A dissolved game pays nobody, however long they played.
            RealmGame::set_payout(client, game.id, RealmGame::PAYOUT_DISSOLVED).await?;
            crate::metrics::record_realm_payout(RealmPayout::Dissolved);
            return Ok(());
        };
        // A game nobody really played pays nothing; the win still stands and
        // still had its line in #lounge.
        let today = realm_day(Utc::now(), game.reset_hour_utc);
        if !payout_earned(&state, today, winner) {
            tracing::info!(
                game_id = %game.id, %winner,
                active_days = state.player(winner).map(|p| p.active_days).unwrap_or(0),
                held = state.territory_count(winner),
                "realm finished too early or too small to pay out"
            );
            RealmGame::set_payout(client, game.id, RealmGame::PAYOUT_UNEARNED).await?;
            crate::metrics::record_realm_payout(RealmPayout::Unearned);
            return Ok(());
        }
        let plan = payout_plan(paying_players(&state), state.ruleset.pot_scale);
        let event_key = game.id.to_string();
        let mut failed: Option<anyhow::Error> = None;
        let mut failures = 0usize;
        for place in final_table(&state, game.id, Some(winner)) {
            let Some(rank) = place.place else {
                continue;
            };
            let Some((reward_key, amount)) = plan.get(rank - 1).copied() else {
                break;
            };
            let credit = self
                .chip_svc
                .credit_per_event_reward_template_amount(
                    place.user_id,
                    reward_key,
                    &event_key,
                    amount,
                    late_core::models::chips::ChipMove::RealmConquest,
                )
                .await;
            match credit {
                Ok(grant) if grant.credited => {
                    crate::metrics::record_realm_payout(RealmPayout::Paid);
                }
                Ok(_) => {
                    crate::metrics::record_realm_payout(RealmPayout::AlreadyPaid);
                }
                Err(error) => {
                    crate::metrics::record_realm_payout(RealmPayout::Failed);
                    failures += 1;
                    if failed.is_none() {
                        failed = Some(error.context(format!(
                            "crediting {reward_key} ({amount} chips) to {}",
                            place.user_id
                        )));
                    }
                }
            }
        }
        if let Some(error) = failed {
            return Err(error.context(format!("{failures} realm payout credit(s) failed")));
        }
        RealmGame::set_payout(client, game.id, RealmGame::PAYOUT_PAID).await?;
        Ok(())
    }

    // ----- snapshot

    async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        self.publish(&client).await
    }

    /// Re-read the listing, hand it to every session, and tell them what
    /// moved. This is the one place realm fan-out comes from: whoever made
    /// the write (this replica, another, the sweeper), every replica reads
    /// the same rows and derives the same events from how they differ from
    /// what it had.
    async fn publish(&self, client: &tokio_postgres::Client) -> Result<()> {
        let _serial = self.publish_lock.lock().await;
        let open = RealmGame::list_open(client).await?;
        let active = RealmGame::list_active(client).await?;
        let finished = RealmGame::list_finished_recent(client, REALM_RESULTS_HOURS).await?;
        let snapshot = RealmSnapshot {
            open_games: open.iter().filter_map(|g| self.admit_row(g)).collect(),
            active_games: active.iter().filter_map(|g| self.admit_row(g)).collect(),
            finished_games: finished.iter().filter_map(|g| self.admit_row(g)).collect(),
            seeded: true,
        };
        let previous = self.snapshot_rx.borrow().clone();
        let events = derive_events(&previous, &snapshot, Utc::now());
        // The truth first, then the news about it: a session that ticks in
        // between sees a newer snapshot and no event yet, never an event
        // about a world it cannot see.
        let _ = self.snapshot_tx.send(Arc::new(snapshot));
        for event in events {
            let _ = self.event_tx.send(event);
        }
        Ok(())
    }

    /// One row into the listing, or out of it with a reason. A row that does
    /// not parse is a corrupt row or a build that cannot read what another
    /// wrote; it is logged and counted the first time this replica drops
    /// it, and forgotten again once it reads.
    fn admit_row(&self, game: &RealmGame) -> Option<RealmGameItem> {
        match game_item(game) {
            Ok(item) => {
                self.rejected_rows.lock_recover().remove(&game.id);
                Some(item)
            }
            Err(error) => {
                let first_time = self.rejected_rows.lock_recover().insert(game.id);
                if first_time {
                    crate::metrics::record_realm_snapshot_row_rejected(&error);
                    tracing::error!(
                        game_id = %game.id,
                        error = %error,
                        "realm game row left out of the realm snapshot"
                    );
                }
                None
            }
        }
    }

    /// Names for a generated world: invented by the model when it is
    /// available, drawn from the built-in pool when it is not. The pool is
    /// not a degraded mode — it is a thousand names written for exactly this,
    /// and it is what tests and a keyless dev box always use. Anything the
    /// model returns that is empty, repeated, or too long for a map label is
    /// dropped on the way in, so a bad batch costs nothing.
    async fn invent_names(&self, count: usize, seed: u64) -> Vec<String> {
        let mut names: Vec<String> = Vec::with_capacity(count);
        if self.ai.is_enabled() {
            let flavour = NAME_FLAVOURS[(seed % NAME_FLAVOURS.len() as u64) as usize];
            let gathered = tokio::time::timeout(
                NAMING_BUDGET,
                self.ask_for_names(count, flavour, &mut names),
            )
            .await;
            if gathered.is_err() {
                tracing::warn!(count, "realm world naming timed out; using the name pool");
            }
        }
        // Whatever is missing — all of it, with no AI — comes from the pool.
        // `resolve_names` in the generator tops up and de-duplicates as well,
        // but doing it here means the stored spec is already complete and the
        // map does not depend on the pool staying the same forever.
        let mut seen: std::collections::HashSet<String> =
            names.iter().map(|n| n.to_lowercase()).collect();
        for name in mapgen::pool_names(seed, mapgen::NAME_POOL.len()) {
            if names.len() >= count {
                break;
            }
            if seen.insert(name.to_lowercase()) {
                names.push(name);
            }
        }
        names.truncate(count);
        names
    }

    /// One batched conversation with the model. Errors are logged and end the
    /// batching rather than failing the opening: a half-named world is finished
    /// from the pool and nobody can tell which country came from where.
    async fn ask_for_names(&self, count: usize, flavour: &str, names: &mut Vec<String>) {
        let schema = serde_json::json!({
            "type": "array",
            "items": { "type": "string" }
        });
        let system = "You invent place names for a fictional world in a strategy game. \
             You return only the JSON array asked for.";
        let mut seen: std::collections::HashSet<String> = Default::default();
        while names.len() < count {
            let want = NAME_BATCH.min(count - names.len());
            let prompt = format!(
                "Invent {want} names for countries on an imaginary world. Style: {flavour}. \
                 Rules: one to two words each, at most 18 characters, ASCII letters and \
                 spaces only, no real country/city/region names, no names from existing \
                 fiction, all different from each other and from these: {}.",
                names
                    .iter()
                    .rev()
                    .take(40)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            let reply = match self
                .ai
                .generate_json(AI_MODEL, system, &prompt, schema.clone())
                .await
            {
                Ok(Some(reply)) => reply,
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!(error = ?e, "realm world naming failed; using the name pool");
                    return;
                }
            };
            let Ok(batch) = serde_json::from_str::<Vec<String>>(&reply) else {
                tracing::warn!("realm world naming returned something that was not a name list");
                return;
            };
            let before = names.len();
            for name in batch {
                let name = name.trim();
                let sane = !name.is_empty()
                    && name.chars().count() <= 18
                    && name.is_ascii()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '-');
                if sane && seen.insert(name.to_lowercase()) {
                    names.push(name.to_string());
                }
            }
            if names.len() == before {
                // A batch that added nothing usable will not get better by
                // being asked again.
                return;
            }
        }
    }

    /// The board a game is played on. For a generated world this is a cache
    /// lookup that may rebuild the map, which is why it hands back an `Arc`
    /// rather than a borrow: nothing here is `'static` any more.
    fn map_for(&self, state: &RealmGameState) -> Result<Arc<WorldMap>> {
        map_handle(&state.ruleset.map_id, state.ruleset.map_spec.as_ref())
            .with_context(|| format!("unknown realm map: {}", state.ruleset.map_id))
    }

    fn send_error(&self, user_id: Uuid, error: &anyhow::Error) {
        let _ = self.event_tx.send(RealmEvent::Error {
            user_id,
            message: error.root_cause().to_string(),
        });
    }
}

/// Put a player on the map: a free territory drawn with `spawn_seed`.
/// Returns false when the world is full, which is the one thing that can
/// refuse a join outright. The caller owns the seed, drawn from the game and
/// its revision at the join, so two arrivals never land by the same draw.
fn spawn_player(
    state: &mut RealmGameState,
    map: &WorldMap,
    game_id: Uuid,
    user_id: Uuid,
    spawn_seed: u64,
) -> bool {
    let free: Vec<u16> = (0..map.territories.len() as u16)
        .filter(|t| !state.ownership.contains_key(t))
        .collect();
    if free.is_empty() {
        return false;
    }
    // Prefer somewhere with a land border. A lone island is a fine place to
    // *play* from and a hopeless place to *start* from: every move off it is
    // a sea crossing at sea-crossing odds, so an islander spends their first
    // week paying double for their first neighbour. Nobody chose the spawn,
    // so nobody should be handed that. Islands stay in the draw only when
    // there is nothing else left.
    let mainland: Vec<u16> = free
        .iter()
        .copied()
        .filter(|t| map.territory(*t).is_some_and(|t| !t.land_links.is_empty()))
        .collect();
    let free = if mainland.is_empty() { free } else { mainland };
    let mut rng = SplitMix64::new(spawn_seed);
    let territory = free[rng.next_index(free.len())];
    state.ownership.insert(territory, user_id);
    // Logged, because the history replays from the log and a game whose
    // opening positions nobody recorded cannot be replayed at all.
    let day = state
        .last_day
        .as_ref()
        .map(|d| d.day)
        .unwrap_or(state.start_day);
    let seed = day_seed(game_id, day);
    state.log(
        day,
        seed,
        resolver::LogEntry::Spawned {
            user_id,
            target: territory,
        },
    );
    true
}

/// One day out of the archive: what happened, and — from the day boards were
/// kept — where everybody stood when it ended.
#[derive(Clone, Debug)]
pub struct ArchivedDay {
    pub result: DayResult,
    /// `None` for days archived before `realm_days.board` existed, and for a
    /// day still being played.
    pub board: Option<DayBoard>,
}

impl ArchivedDay {
    pub fn day(&self) -> i32 {
        self.result.day
    }
}

/// The display name of a loaded row.
fn display_name(game: &RealmGame, _state: &RealmGameState) -> String {
    game_name(&game.name, game.season)
}

/// The name to show: the stored one, or the season's number when the house
/// left it blank.
pub fn game_name(stored: &str, season: i32) -> String {
    if stored.trim().is_empty() {
        format!("Season {season}")
    } else {
        stored.to_string()
    }
}

/// The world a state is played on, for a sentence: "Earth", or what a drawn
/// world is ("5 continents · 8 islands · 240 lands").
pub fn world_label(state: &RealmGameState) -> String {
    match state.ruleset.map_spec.as_ref() {
        Some(spec) => spec.summary(),
        None => map_handle(&state.ruleset.map_id, None)
            .map(|m| m.display_name.clone())
            .unwrap_or_else(|| "the world".to_string()),
    }
}

/// The news an action made, if any: the last of their land taken from
/// somebody is the bigger story than the take itself, and the end of the
/// world is bigger than either. `logged_before` is how long the day's log
/// was before the action, so only what this action wrote is read.
fn news_of_action(
    state: &RealmGameState,
    map: &WorldMap,
    logged_before: usize,
    end: ResolveEnd,
    now: DateTime<Utc>,
) -> Option<RealmNews> {
    if let Some(news) = ended_news(state, end, now) {
        return Some(news);
    }
    let entries = state
        .last_day
        .as_ref()
        .map(|d| &d.entries[logged_before.min(d.entries.len())..])
        .unwrap_or(&[]);
    let name_of = |id: Uuid| {
        state
            .player(id)
            .map(|p| p.username.clone())
            .unwrap_or_else(|| "someone".to_string())
    };
    let fell = entries.iter().find_map(|entry| match entry {
        resolver::LogEntry::Eliminated {
            user_id,
            by_user_id,
        } => Some(RealmNewsKind::Fell {
            user_id: *user_id,
            username: name_of(*user_id),
            by_user_id: *by_user_id,
            by_username: name_of(*by_user_id),
        }),
        _ => None,
    });
    let took = entries.iter().find_map(|entry| match entry {
        resolver::LogEntry::Attacked {
            user_id,
            defender_id,
            target,
            success: true,
            ..
        } => Some(RealmNewsKind::Took {
            user_id: *user_id,
            username: name_of(*user_id),
            from_user_id: *defender_id,
            from_username: name_of(*defender_id),
            target: *target,
            target_name: map
                .territory(*target)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "a country".to_string()),
        }),
        _ => None,
    });
    fell.or(took).map(|kind| RealmNews { at: now, kind })
}

/// The news of a world ending, or `None` while it goes on.
fn ended_news(state: &RealmGameState, end: ResolveEnd, now: DateTime<Utc>) -> Option<RealmNews> {
    match end {
        ResolveEnd::Ongoing => None,
        ResolveEnd::Won(winner) => Some(RealmNews {
            at: now,
            kind: RealmNewsKind::Ended {
                winner_user_id: Some(winner),
                winner_username: state.player(winner).map(|p| p.username.clone()),
            },
        }),
        ResolveEnd::Dissolved => Some(RealmNews {
            at: now,
            kind: RealmNewsKind::Ended {
                winner_user_id: None,
                winner_username: None,
            },
        }),
    }
}

/// Chips in the pot per player who started the game.
///
/// The pot scales with the roster because placing in a big game is the hard
/// part: being one of ten is a different feat from being one of two, and the
/// game also runs longer for it. A duel is worth 6k, a full table 30k —
/// which puts a realm win in the same class as the door-game milestones
/// (Green Dragon 10k, A Dark Room 15-20k) rather than below a daily puzzle,
/// where it sat when it paid a flat 2000.
pub const REALM_POT_PER_PLAYER: i64 = 3_000;

/// The most a single realm can ever pay, however many players and however
/// long the tempo. Ten players at the epic tempo would otherwise compute a
/// pot bigger than anything else here pays for anything — a NetHack
/// ascension, the hardest solo feat on the platform, is 40k — and a prize
/// that size warps every other thing a player could be doing.
pub const REALM_POT_MAX: i64 = 60_000;

/// How the pot splits, by player count at start. More players, more places
/// paid: a ten-player war pays a podium, a duel pays the survivor.
fn payout_shares(paying_count: u8) -> &'static [(&'static str, f64)] {
    match paying_count {
        0..=3 => &[("realm_win_small", 1.0)],
        4..=6 => &[("realm_win_mid", 0.70), ("realm_runnerup_mid", 0.30)],
        _ => &[
            ("realm_win_large", 0.60),
            ("realm_runnerup_large", 0.25),
            ("realm_third_large", 0.15),
        ],
    }
}

/// One line of the final standings.
#[derive(Clone, Debug)]
pub struct FinalPlace {
    pub user_id: Uuid,
    pub username: String,
    /// 1-based, and only for places that were paid; `None` for everyone who
    /// finished outside the money or was not eligible for it.
    pub place: Option<usize>,
    pub chips: i64,
    pub status: RealmPlayerStatus,
    /// Territories held at the end, or at the moment they were knocked out.
    pub territories: u16,
    pub active_days: u16,
    /// Did the pot pass them by, and why it reads as it does.
    pub note: &'static str,
}

/// The final standings of a finished game: who placed where, and what each
/// place was worth.
///
/// The payout walks this same list, so what a player is shown after the
/// battle is by construction what their chips were — a results screen that
/// recomputed the ranking separately would eventually disagree with the
/// ledger, and the ledger would be right.
pub fn final_table(state: &RealmGameState, game_id: Uuid, winner: Option<Uuid>) -> Vec<FinalPlace> {
    let ranking = resolver::final_ranking(state, winner, day_seed(game_id, 0));
    let plan = payout_plan(paying_players(state), state.ruleset.pot_scale);
    let mut paid = 0usize;
    ranking
        .into_iter()
        .filter_map(|user_id| {
            let player = state.player(user_id)?;
            let eligible = payout_earned_by(state, user_id, winner);
            let (place, chips) = if eligible {
                let slot = paid;
                paid += 1;
                match plan.get(slot) {
                    Some((_, chips)) => (Some(slot + 1), *chips),
                    None => (None, 0),
                }
            } else {
                (None, 0)
            };
            let note = match (player.status, eligible, chips) {
                (_, _, c) if c > 0 => "",
                (RealmPlayerStatus::Left, ..) => "withdrew",
                (RealmPlayerStatus::Kicked, ..) => "went quiet",
                (_, false, _) => "too few days played",
                _ => "outside the places",
            };
            Some(FinalPlace {
                user_id,
                username: player.username.clone(),
                place,
                chips,
                status: player.status,
                territories: if player.status == RealmPlayerStatus::Alive {
                    state.territory_count(user_id)
                } else {
                    player.exit_territories
                },
                active_days: player.active_days,
                note,
            })
        })
        .collect()
}

/// What the ending headline says the conquest paid in all, 0 when it paid
/// nothing. The places `final_table` pays, the list the payout walks, so the
/// headline cannot promise a place nobody is left to take.
fn ended_pot(state: &RealmGameState, game_id: Uuid, winner: Option<Uuid>) -> i64 {
    match winner {
        Some(winner) if payout_earned(state, 0, winner) => {
            final_table(state, game_id, Some(winner))
                .iter()
                .map(|place| place.chips)
                .sum()
        }
        Some(_) | None => 0,
    }
}

/// Whether this player can take a place in the money at all.
fn payout_earned_by(state: &RealmGameState, user_id: Uuid, winner: Option<Uuid>) -> bool {
    // A dissolved game pays nobody, however long they played.
    winner.is_some() && payout_eligible(state, user_id)
}

/// How many players the pot is sized on: those who played as long as a win
/// takes, not everyone on the roster.
///
/// With arrivals open all game, counting the roster would let a colluder
/// walk alts in to swell the prize without ever taking a turn, and the
/// podium bar (`min_payout_active_days`) is a few clicks on a few days, so
/// counting players who cleared that let one person with nine alts bank a
/// whole 60/25/15 split. The win bar makes each padded body cost as many
/// days of play as the conquest itself. It is a mitigation: the pot still
/// scales with headcount, which is the thing alts fake. The cost is that an
/// honest player knocked out before the win bar does not grow the pot
/// either. Floored at two, because a realm is never worth less than a duel.
pub fn paying_players(state: &RealmGameState) -> u8 {
    let played = state
        .players
        .iter()
        .filter(|p| p.active_days >= state.ruleset.min_active_days_to_win)
        .count();
    played.clamp(2, u8::MAX as usize) as u8
}

/// Rank -> (reward-template key, chips), for a game whose pot is sized on
/// `paying_count` players at `pot_scale` (the pace's multiplier: a blitz
/// pays a quarter of the reference tempo, an epic three times it). The template carries the policy and the
/// floor of its tier; the amount is the event's own.
pub fn payout_plan(paying_count: u8, pot_scale: f64) -> Vec<(&'static str, i64)> {
    let pot = (((REALM_POT_PER_PLAYER * i64::from(paying_count.max(2))) as f64)
        * pot_scale.max(0.05))
    .min(REALM_POT_MAX as f64) as i64;
    payout_shares(paying_count)
        .iter()
        .map(|(key, share)| {
            // Round to whole chips, to the nearest 50 so the figures read
            // like prizes rather than arithmetic.
            let raw = (pot as f64 * share / 50.0).round() as i64 * 50;
            (*key, raw.max(50))
        })
        .collect()
}

/// Whether a finished game is worth paying out at all.
///
/// Winning already takes `min_active_days_to_win` days of real play, so a
/// game that reaches a winner has been played. This is the backstop for the
/// shapes that bypass conquest — a game that dissolved, or one whose winner
/// somehow holds almost nothing — and it keeps the old territory floor.
pub fn payout_earned(state: &RealmGameState, _end_day: i32, winner: Uuid) -> bool {
    state.can_win(winner) && state.territory_count(winner) >= state.ruleset.min_payout_territories
}

/// Whether a placed player has played enough to be paid. The podium means
/// "played and placed", so someone who joined, sat still and was conquered
/// on day two does not collect a runner-up prize.
pub fn payout_eligible(state: &RealmGameState, user_id: Uuid) -> bool {
    state.player(user_id).is_some_and(|p| {
        matches!(
            p.status,
            RealmPlayerStatus::Alive | RealmPlayerStatus::Eliminated
        ) && p.active_days >= state.ruleset.min_payout_active_days
    })
}

pub fn parse_state(value: &Value) -> Result<RealmGameState> {
    let mut state: RealmGameState =
        serde_json::from_value(value.clone()).context("corrupt realm game state")?;
    anyhow::ensure!(
        state.version <= STATE_VERSION,
        "unsupported realm state version: {}",
        state.version
    );
    state.version = STATE_VERSION;
    Ok(state)
}

/// Settle which colour an arriving player gets. A pick that somebody took
/// in the meantime is refused rather than quietly swapped: arriving as a
/// colour you did not choose, on a map where colour is identity, is worse
/// than being told to pick again.
fn pick_color(taken: &[u8], wanted: Option<u8>) -> Result<u8> {
    let free = |c: u8| c < PALETTE_SLOTS && !taken.contains(&c);
    match wanted {
        Some(c) if free(c) => Ok(c),
        Some(c) if c >= PALETTE_SLOTS => bail!("there is no such colour"),
        Some(_) => bail!("somebody took that colour first — pick another"),
        None => (0..PALETTE_SLOTS)
            .find(|c| free(*c))
            .context("every colour is taken"),
    }
}

/// `joined_claimed` is how much of the world was already taken when they
/// arrived, in percent — measured here, once, because it is a fact about
/// their arrival rather than about the game.
fn new_player(
    user_id: Uuid,
    username: &str,
    joined_day: i32,
    color: u8,
    joined_claimed: u8,
) -> RealmPlayer {
    RealmPlayer {
        color: Some(color),
        joined_claimed,
        user_id,
        username: username.to_string(),
        status: RealmPlayerStatus::Alive,
        joined_day,
        exit_day: None,
        exit_territories: 0,
        actions_day: joined_day,
        actions_used: 0,
        actions_allowance: 0,
        last_action_day: None,
        active_days: 0,
    }
}

/// A refused action is the player's problem to see, not an error to log as a
/// failure: it means the world changed under them, or they are out of points.
fn reject(rejected: ActionRejected) -> anyhow::Error {
    anyhow::anyhow!(rejected.message())
}

/// What a finished game's chips did, as the dashboard counts it: one per
/// place settled, one per game with nothing to pay, one per credit that
/// failed and was left for the sweeper.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmPayout {
    Paid,
    /// A retry found the claim already banked: nothing moved, nothing owed.
    AlreadyPaid,
    Unearned,
    Dissolved,
    Failed,
}

/// What a sweeper tick that found no world standing did about it, as the
/// dashboard counts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmSeasonOpen {
    Opened,
    /// Another replica's tick opened it first: the table refused the second.
    Standing,
    Failed,
}

/// What the sweeper did about a standing world without a chat room, as the
/// dashboard counts it. A world opened since rooms were born with it never
/// gets here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmChatRoomAttach {
    Attached,
    Failed,
}

/// Why a `realm_games` row was left out of the snapshot.
#[derive(Debug)]
pub enum RealmSnapshotRowError {
    /// The state JSONB did not parse into `RealmGameState`.
    UnreadableState(String),
}

impl std::fmt::Display for RealmSnapshotRowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnreadableState(error) => write!(f, "unreadable state: {error}"),
        }
    }
}

/// The events a re-read implies, from the listing this replica had to the
/// one it just read. Pure, so every replica derives the same thing from the
/// same rows, and so it can be tested without a database.
///
/// Nothing is derived from an unseeded snapshot: the first read after a
/// start or a reconnect is a seed, not news. Ordered by game id, so what a
/// session drains is the same whatever the lists came back in.
fn derive_events(
    previous: &RealmSnapshot,
    next: &RealmSnapshot,
    now: DateTime<Utc>,
) -> Vec<RealmEvent> {
    if !previous.seeded {
        return Vec::new();
    }
    let index = |snapshot: &RealmSnapshot| -> BTreeMap<Uuid, RealmGameItem> {
        snapshot
            .open_games
            .iter()
            .chain(snapshot.active_games.iter())
            .chain(snapshot.finished_games.iter())
            .map(|game| (game.id, game.clone()))
            .collect()
    };
    let before = index(previous);
    let after = index(next);
    let mut events = Vec::new();
    for (game_id, game) in &after {
        let game_id = *game_id;
        match before.get(&game_id) {
            None => {
                // New to this replica. Still mustering, it is the invitation
                // every session raises; either way anything open on it (a
                // board that outlived a reconnect) should re-read.
                if game.status == RealmGame::STATUS_ACTIVE
                    && muster_left(game.created, now).is_some()
                {
                    events.push(RealmEvent::SeasonOpened {
                        game_id,
                        season: game.season,
                        name: game.name.clone(),
                        opens_at: game.created + REALM_MUSTER,
                    });
                }
                events.push(RealmEvent::GameChanged { game_id });
            }
            Some(old) => {
                let moved = old.revision != game.revision
                    || old.status != game.status
                    || old.last_resolved_day != game.last_resolved_day
                    || old.payout != game.payout
                    || old.chat_room_id != game.chat_room_id;
                if moved {
                    events.push(RealmEvent::GameChanged { game_id });
                }
                if game.status == RealmGame::STATUS_ACTIVE
                    && game.last_resolved_day > old.last_resolved_day
                {
                    events.push(RealmEvent::DayRolled {
                        game_id,
                        day: game.last_resolved_day,
                        name: game.name.clone(),
                    });
                }
                if old.status != RealmGame::STATUS_FINISHED
                    && game.status == RealmGame::STATUS_FINISHED
                {
                    events.push(RealmEvent::GameFinished {
                        game_id,
                        winner_user_id: game.winner_user_id,
                    });
                }
            }
        }
    }
    // Gone from the listing: cancelled, or a finished game past its window.
    // A board still open on it reloads and reads the row as it is.
    for game_id in before.keys() {
        if !after.contains_key(game_id) {
            events.push(RealmEvent::GameChanged { game_id: *game_id });
        }
    }
    events
}

fn game_item(game: &RealmGame) -> Result<RealmGameItem, RealmSnapshotRowError> {
    let state = match parse_state(&game.state) {
        Ok(state) => state,
        Err(error) => return Err(RealmSnapshotRowError::UnreadableState(error.to_string())),
    };
    let players = state
        .players
        .iter()
        .enumerate()
        .map(|(seat, p)| RealmPlayerItem {
            user_id: p.user_id,
            username: p.username.clone(),
            status: p.status,
            territories: state.territory_count(p.user_id),
            color: p.color.unwrap_or(seat as u8),
        })
        .collect();
    // The world's size, not the world: a listing redrawn after every action
    // must never be what builds a generated map.
    let total = territory_total(&state.ruleset.map_id, state.ruleset.map_spec.as_ref());
    let today = realm_day(Utc::now(), game.reset_hour_utc);
    let joinable = total.is_some_and(|t| state.joinable_of(t)) && game.winner_user_id.is_none();
    let claimed = total.map(|t| state.claimed_fraction_of(t)).unwrap_or(1.0);
    let can_withdraw = state.players.len() == 1
        || state
            .players
            .iter()
            .any(|p| state.phase(p.user_id, today) == PlayerPhase::New);
    Ok(RealmGameItem {
        id: game.id,
        created: game.created,
        creator_id: game.creator_id,
        season: game.season,
        news: state.news.clone(),
        status: game.status.clone(),
        revision: state.revision,
        payout: game.payout.clone(),
        name: game_name(&game.name, game.season),
        ruleset_id: state.ruleset.id.clone(),
        ruleset_name: state.ruleset.display_name.clone(),
        pace_name: state.ruleset.pace_name.clone(),
        map_id: state.ruleset.map_id.clone(),
        min_players: state.ruleset.min_players,
        max_players: state.ruleset.max_players,
        last_resolved_day: game.last_resolved_day,
        reset_hour_utc: game.reset_hour_utc,
        joinable,
        claimed,
        can_withdraw,
        winner_user_id: game.winner_user_id,
        chat_room_id: game.chat_room_id,
        players,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed(
        id: u128,
        status: &str,
        revision: u64,
        last_resolved_day: i32,
        created: DateTime<Utc>,
        winner: Option<Uuid>,
    ) -> RealmGameItem {
        RealmGameItem {
            id: Uuid::from_u128(id),
            created,
            creator_id: Uuid::from_u128(900 + id),
            season: id as i32,
            news: None,
            status: status.to_string(),
            revision,
            payout: None,
            name: format!("war {id}"),
            ruleset_id: "standard".to_string(),
            ruleset_name: "Standard".to_string(),
            pace_name: "Normal".to_string(),
            map_id: "earth".to_string(),
            min_players: 2,
            max_players: 10,
            last_resolved_day,
            reset_hour_utc: 0,
            joinable: true,
            claimed: 0.0,
            can_withdraw: false,
            winner_user_id: winner,
            chat_room_id: None,
            players: Vec::new(),
        }
    }

    /// Every replica tells its sessions the same thing about a write, because
    /// what it tells them is read off the rows rather than sent by the writer.
    /// One re-read, every kind of change at once, the whole list of events.
    #[test]
    fn a_re_read_derives_the_same_events_on_every_replica() {
        let now = DateTime::<Utc>::from_timestamp(86_400 * 20_000, 0).unwrap();
        let long_ago = now - ChronoDuration::days(2);
        let just_now = now - ChronoDuration::seconds(30);
        let winner = Uuid::from_u128(77);

        let previous = RealmSnapshot {
            open_games: Vec::new(),
            active_games: vec![
                listed(1, "active", 4, 20_000, long_ago, None), // untouched
                listed(2, "active", 4, 20_000, long_ago, None), // somebody acted
                listed(3, "active", 9, 20_000, long_ago, None), // its day rolled
                listed(4, "active", 2, 20_000, long_ago, None), // conquered
                listed(5, "active", 2, 20_000, long_ago, None), // cancelled: gone
                listed(9, "active", 1, 20_000, long_ago, None), // the sweeper gave it a room
            ],
            finished_games: Vec::new(),
            seeded: true,
        };
        let mut given_a_room = listed(9, "active", 1, 20_000, long_ago, None);
        given_a_room.chat_room_id = Some(Uuid::from_u128(999));
        let mut paid_out = listed(8, "finished", 6, 19_990, long_ago, Some(winner));
        paid_out.payout = Some("paid".to_string());
        let next = RealmSnapshot {
            open_games: Vec::new(),
            active_games: vec![
                listed(1, "active", 4, 20_000, long_ago, None),
                listed(2, "active", 5, 20_000, long_ago, None),
                listed(3, "active", 10, 20_001, long_ago, None),
                listed(6, "active", 1, 20_000, just_now, None), // raised, mustering
                listed(7, "active", 3, 20_000, long_ago, None), // first seen underway
                given_a_room,
            ],
            finished_games: vec![
                listed(4, "finished", 3, 20_000, long_ago, Some(winner)),
                paid_out,
            ],
            seeded: true,
        };
        let mut previous = previous;
        previous
            .finished_games
            .push(listed(8, "finished", 6, 19_990, long_ago, Some(winner)));

        let id = Uuid::from_u128;
        assert_eq!(
            derive_events(&previous, &next, now),
            vec![
                RealmEvent::GameChanged { game_id: id(2) },
                RealmEvent::GameChanged { game_id: id(3) },
                RealmEvent::DayRolled {
                    game_id: id(3),
                    day: 20_001,
                    name: "war 3".to_string(),
                },
                RealmEvent::GameChanged { game_id: id(4) },
                RealmEvent::GameFinished {
                    game_id: id(4),
                    winner_user_id: Some(winner),
                },
                RealmEvent::SeasonOpened {
                    game_id: id(6),
                    season: 6,
                    name: "war 6".to_string(),
                    opens_at: just_now + REALM_MUSTER,
                },
                RealmEvent::GameChanged { game_id: id(6) },
                RealmEvent::GameChanged { game_id: id(7) },
                RealmEvent::GameChanged { game_id: id(8) },
                RealmEvent::GameChanged { game_id: id(9) },
                RealmEvent::GameChanged { game_id: id(5) },
            ]
        );

        // The first read after a start or a reconnect seeds; it is not news.
        let unseeded = RealmSnapshot::default();
        assert!(derive_events(&unseeded, &next, now).is_empty());
    }

    /// What the strip is told about an action: the last of someone's land
    /// outranks the take that did it, a claim on empty land is not news at
    /// all, and the end of the world outranks everything.
    #[test]
    fn the_fall_of_a_player_is_bigger_news_than_the_take() {
        use super::super::map::map_by_id;
        use super::super::rulesets::STANDARD;

        let map = map_by_id("testmap").expect("testmap");
        let alice = Uuid::from_u128(1);
        let bob = Uuid::from_u128(2);
        let pace = pace_by_id("normal").expect("normal");
        let mut state = RealmGameState {
            version: STATE_VERSION,
            revision: 1,
            ruleset: RealmRulesetSnapshot::at_pace(&STANDARD, pace),
            players: vec![
                new_player(alice, "alice", 10, 0, 0),
                new_player(bob, "bob", 10, 1, 0),
            ],
            ownership: BTreeMap::new(),
            forts: BTreeMap::new(),
            last_day: None,
            start_player_count: 2,
            start_day: 10,
            news: None,
        };
        state.roll_day(10, 7);
        let now = Utc::now();
        let before = 0;
        state.log(
            10,
            7,
            resolver::LogEntry::Attacked {
                user_id: alice,
                defender_id: bob,
                target: 3,
                probability: 0.5,
                success: true,
                hops: 1,
                by_sea: false,
            },
        );
        let took = news_of_action(&state, &map, before, ResolveEnd::Ongoing, now).expect("news");
        assert_eq!(
            took.kind,
            RealmNewsKind::Took {
                user_id: alice,
                username: "alice".to_string(),
                from_user_id: bob,
                from_username: "bob".to_string(),
                target: 3,
                target_name: map.territory(3).expect("territory").name.clone(),
            }
        );

        state.log(
            10,
            7,
            resolver::LogEntry::Eliminated {
                user_id: bob,
                by_user_id: alice,
            },
        );
        let fell = news_of_action(&state, &map, before, ResolveEnd::Ongoing, now).expect("news");
        assert_eq!(
            fell.kind,
            RealmNewsKind::Fell {
                user_id: bob,
                username: "bob".to_string(),
                by_user_id: alice,
                by_username: "alice".to_string(),
            }
        );

        let logged = state.last_day.as_ref().expect("day").entries.len();
        state.log(
            10,
            7,
            resolver::LogEntry::Claimed {
                user_id: alice,
                target: 4,
                probability: 0.9,
                success: true,
                hops: 1,
                by_sea: false,
            },
        );
        assert!(news_of_action(&state, &map, logged, ResolveEnd::Ongoing, now).is_none());
        assert_eq!(
            news_of_action(&state, &map, logged, ResolveEnd::Won(alice), now).map(|n| n.kind),
            Some(RealmNewsKind::Ended {
                winner_user_id: Some(alice),
                winner_username: Some("alice".to_string()),
            })
        );
    }

    #[test]
    fn utc_day_is_epoch_days() {
        let epoch = DateTime::<Utc>::from_timestamp(0, 0).unwrap();
        assert_eq!(utc_day(epoch), 0);
        let later = DateTime::<Utc>::from_timestamp(86_400 * 20_000 + 3600, 0).unwrap();
        assert_eq!(utc_day(later), 20_000);
    }

    #[test]
    fn the_pot_grows_with_the_roster() {
        let total = |n: u8| {
            payout_plan(n, 1.0)
                .iter()
                .map(|(_, chips)| chips)
                .sum::<i64>()
        };
        // Every extra player adds to the pot, never subtracts.
        for n in 2..10u8 {
            assert!(
                total(n + 1) >= total(n),
                "pot shrank going from {n} to {} players",
                n + 1
            );
        }
        assert!(total(10) > total(2), "ten players must beat two");
        // And it is roughly the per-player rate times the roster, until the
        // ceiling takes over.
        for n in 2..=10u8 {
            let expected = (REALM_POT_PER_PLAYER * i64::from(n)).min(REALM_POT_MAX);
            let slack = (expected as f64 * 0.02) as i64 + 50;
            assert!(
                (total(n) - expected).abs() <= slack,
                "{n} players: pot {} strays from {expected}",
                total(n)
            );
        }
    }

    #[test]
    fn payout_tiers_pay_more_places_as_the_game_grows() {
        // A duel pays the survivor, a mid game pays two, a full table three.
        assert_eq!(payout_plan(2, 1.0).len(), 1);
        assert_eq!(payout_plan(3, 1.0).len(), 1);
        assert_eq!(payout_plan(5, 1.0).len(), 2);
        assert_eq!(payout_plan(9, 1.0).len(), 3);
        // Ranks are ordered, and the keys are the seeded templates.
        for n in [2u8, 5, 9] {
            let plan = payout_plan(n, 1.0);
            assert!(
                plan.windows(2).all(|w| w[0].1 > w[1].1),
                "ranks out of order"
            );
            assert!(plan.iter().all(|(key, _)| key.starts_with("realm_")));
        }
        // Winning a crowded game beats winning a duel.
        assert!(payout_plan(10, 1.0)[0].1 > payout_plan(2, 1.0)[0].1);
        // A realm win is worth more than a day's Asterion escape (4000),
        // which is the whole reason these numbers moved.
        assert!(payout_plan(2, 1.0)[0].1 > 4_000);
    }

    /// The headline that a world fell names what the conquest paid, and the
    /// payout walks `final_table`. Players who cleared the win bar size the
    /// pot whatever became of them, but only the ones still standing take a
    /// place, so a plan with more places than takers must not be announced
    /// in full.
    #[test]
    fn the_ending_headline_names_only_the_places_that_pay() {
        let mut state: RealmGameState = serde_json::from_value(serde_json::json!({
            "version": STATE_VERSION,
            "revision": 2,
            "ruleset": serde_json::to_value(RealmRulesetSnapshot::from(
                &super::super::rulesets::STANDARD,
            ))
            .unwrap(),
            "players": [],
            "ownership": {},
            "last_day": null,
            "start_player_count": 0,
            "start_day": 20_000
        }))
        .expect("state");
        let bar = state.ruleset.min_active_days_to_win;
        for (n, status) in [
            (1u128, "alive"),
            (2, "kicked"),
            (3, "kicked"),
            (4, "kicked"),
        ] {
            state.players.push(
                serde_json::from_value(serde_json::json!({
                    "user_id": Uuid::from_u128(n),
                    "username": format!("p{n}"),
                    "status": status,
                    "joined_day": 20_000,
                    "exit_day": null,
                    "exit_territories": 0,
                    "active_days": bar
                }))
                .expect("player"),
            );
        }
        let winner = Uuid::from_u128(1);
        for territory in 0..state.ruleset.min_payout_territories {
            state.ownership.insert(territory, winner);
        }
        let game_id = Uuid::from_u128(77);

        let paid: i64 = final_table(&state, game_id, Some(winner))
            .iter()
            .map(|place| place.chips)
            .sum();
        assert!(paid > 0, "the winner is paid");
        assert_eq!(ended_pot(&state, game_id, Some(winner)), paid);
        assert_eq!(
            ended_pot(&state, game_id, None),
            0,
            "a dissolve pays nothing"
        );
    }

    #[test]
    fn the_pot_counts_players_who_played_not_bodies_on_the_roster() {
        let mut state: RealmGameState = serde_json::from_value(serde_json::json!({
            "version": STATE_VERSION,
            "revision": 2,
            "ruleset": serde_json::to_value(RealmRulesetSnapshot::from(
                &super::super::rulesets::STANDARD,
            ))
            .unwrap(),
            "players": [],
            "ownership": {},
            "last_day": null,
            "start_player_count": 0,
            "start_day": 20_000
        }))
        .expect("state");
        let bar = state.ruleset.min_active_days_to_win;
        let mut add = |n: u128, active_days: u16| {
            state.players.push(
                serde_json::from_value(serde_json::json!({
                    "user_id": Uuid::from_u128(n),
                    "username": format!("p{n}"),
                    "status": "alive",
                    "joined_day": 20_000,
                    "exit_day": null,
                    "exit_territories": 0,
                    "active_days": active_days
                }))
                .expect("player"),
            );
        };

        // Two real players and six alts walked in to swell the prize.
        add(1, bar + 4);
        add(2, bar + 2);
        for n in 3..9u128 {
            add(n, 0);
        }
        assert_eq!(state.players.len(), 8);
        assert_eq!(
            paying_players(&state),
            2,
            "padding the roster must not pay anyone"
        );
        let padded: i64 = payout_plan(paying_players(&state), 1.0)
            .iter()
            .map(|(_, chips)| chips)
            .sum();
        assert_eq!(
            padded,
            payout_plan(2, 1.0).iter().map(|(_, c)| c).sum::<i64>()
        );

        // The same eight, having actually played, are worth eight players.
        for player in &mut state.players {
            player.active_days = bar;
        }
        assert_eq!(paying_players(&state), 8);
        assert!(
            payout_plan(paying_players(&state), 1.0)
                .iter()
                .map(|(_, c)| c)
                .sum::<i64>()
                > padded
        );

        // And the floor holds: a duel is the least a realm can be worth.
        state.players.truncate(1);
        assert_eq!(paying_players(&state), 2);
    }

    /// Clicking through the podium bar on a few days is cheap, and an alt
    /// that does it must not grow the prize or open a place for itself:
    /// otherwise one person with nine alts banks the whole 60/25/15 split.
    /// Only somebody who played as long as a win takes adds to the pot.
    #[test]
    fn alts_who_click_through_the_podium_bar_do_not_swell_the_pot() {
        let mut state: RealmGameState = serde_json::from_value(serde_json::json!({
            "version": STATE_VERSION,
            "revision": 2,
            "ruleset": serde_json::to_value(RealmRulesetSnapshot::from(
                &super::super::rulesets::STANDARD,
            ))
            .unwrap(),
            "players": [],
            "ownership": {},
            "last_day": null,
            "start_player_count": 0,
            "start_day": 20_000
        }))
        .expect("state");
        let podium = state.ruleset.min_payout_active_days;
        let win = state.ruleset.min_active_days_to_win;
        assert!(podium < win, "the podium bar is the cheaper one");
        for n in 1..=10u128 {
            state.players.push(
                serde_json::from_value(serde_json::json!({
                    "user_id": Uuid::from_u128(n),
                    "username": format!("p{n}"),
                    "status": if n == 1 { "alive" } else { "eliminated" },
                    "joined_day": 20_000,
                    "exit_day": if n == 1 { None } else { Some(20_002) },
                    "exit_territories": 1,
                    "active_days": if n == 1 { win } else { podium }
                }))
                .expect("player"),
            );
        }
        assert_eq!(paying_players(&state), 2, "nine alts are not nine players");
        assert_eq!(
            payout_plan(paying_players(&state), 1.0).len(),
            1,
            "and they open no places"
        );
    }

    #[test]
    fn the_pace_scales_the_pot_and_the_windows_together() {
        use super::super::rulesets::{PACES, STANDARD, pace_by_id};
        let pot_of = |pace_id: &str| -> i64 {
            let pace = pace_by_id(pace_id).expect("pace");
            payout_plan(5, pace.pot_scale)
                .iter()
                .map(|(_, chips)| chips)
                .sum::<i64>()
        };
        // Faster tempo, smaller prize: the roster is ordered slowest first,
        // and the pot follows it down.
        let pots: Vec<i64> = PACES.iter().map(|p| pot_of(p.id)).collect();
        assert!(
            pots.windows(2).all(|w| w[0] > w[1]),
            "pots should fall as the tempo rises: {pots:?}"
        );
        // Normal is the reference: what the game paid before paces existed.
        assert_eq!(
            pot_of("normal"),
            payout_plan(5, 1.0).iter().map(|(_, c)| c).sum::<i64>()
        );

        // Nobody can farm chips-per-day by spamming the quickest variant: a
        // blitz pays less per day of play than an epic does.
        for pace in PACES {
            let snapshot = RealmRulesetSnapshot::at_pace(&STANDARD, pace);
            let per_day = pot_of(pace.id) as f64 / f64::from(snapshot.min_active_days_to_win);
            let epic = pace_by_id("epic").expect("epic");
            let epic_snapshot = RealmRulesetSnapshot::at_pace(&STANDARD, epic);
            let epic_per_day =
                pot_of("epic") as f64 / f64::from(epic_snapshot.min_active_days_to_win);
            assert!(
                per_day <= epic_per_day * 1.35,
                "{} pays {per_day:.0}/day against epic's {epic_per_day:.0}",
                pace.id
            );
        }

        // Windows scale with the tempo, and never collapse to nothing.
        for pace in PACES {
            let snapshot = RealmRulesetSnapshot::at_pace(&STANDARD, pace);
            assert!(snapshot.min_active_days_to_win >= 2, "{}", pace.id);
            assert!(snapshot.attack_grace_days >= 1, "{}", pace.id);
            assert!(
                snapshot.distant_attack_grace_days >= snapshot.attack_grace_days,
                "{}: ramping up must outlast being new",
                pace.id
            );
            assert!(snapshot.bank_cap_days >= 1, "{}", pace.id);
            assert!(
                snapshot.actions_per_day_tiers.iter().all(|(_, p)| *p >= 1),
                "{}",
                pace.id
            );
        }

        // A blitz really is faster than the rules' own tempo.
        let blitz = RealmRulesetSnapshot::at_pace(&STANDARD, pace_by_id("blitz").expect("blitz"));
        let slow = RealmRulesetSnapshot::at_pace(&STANDARD, pace_by_id("slow").expect("slow"));
        assert!(blitz.actions_per_day(5) > slow.actions_per_day(5) * 3);
        assert!(blitz.min_active_days_to_win < slow.min_active_days_to_win);
        // And slow is the rules as written.
        assert_eq!(slow.actions_per_day(5), STANDARD.actions_per_day_tiers[1].1);
    }

    #[test]
    fn a_game_that_was_never_played_pays_nothing() {
        let mut state: RealmGameState = serde_json::from_value(serde_json::json!({
            "version": STATE_VERSION,
            "revision": 2,
            "ruleset": serde_json::to_value(RealmRulesetSnapshot::from(
                &super::super::rulesets::STANDARD,
            ))
            .unwrap(),
            "players": [
                {
                    "user_id": Uuid::from_u128(1),
                    "username": "a",
                    "status": "alive",
                    "joined_day": 20_000,
                    "exit_day": null,
                    "exit_territories": 0,
                    "active_days": 1
                },
                {
                    "user_id": Uuid::from_u128(2),
                    "username": "b",
                    "status": "eliminated",
                    "joined_day": 20_000,
                    "exit_day": 20_002,
                    "exit_territories": 1,
                    "active_days": 4
                }
            ],
            "ownership": {},
            "last_day": null,
            "start_player_count": 2,
            "start_day": 20_000
        }))
        .expect("state");
        let winner = Uuid::from_u128(1);
        let bar = state.ruleset.min_active_days_to_win;

        // One day played, one territory: the partner simply walked away.
        state.ownership.insert(0, winner);
        assert!(!payout_earned(&state, 20_000, winner));
        // Ground held, but the week is not in.
        for t in 1..12u16 {
            state.ownership.insert(t, winner);
        }
        assert!(!payout_earned(&state, 20_010, winner));
        // A week of real play, and it pays.
        state.players[0].active_days = bar;
        assert!(payout_earned(&state, 20_010, winner));
        assert!(payout_eligible(&state, winner));

        // Being on the roster is not being on the podium: a player who was
        // conquered on day two does not collect a place.
        state.players[0].status = RealmPlayerStatus::Eliminated;
        state.players[0].active_days = 1;
        assert!(!payout_eligible(&state, winner));
    }

    #[test]
    fn a_queue_era_state_still_loads() {
        // A v1 game: no budgets, no action days, the fields instant
        // resolution added are simply absent.
        let value = serde_json::json!({
            "version": 1,
            "revision": 7,
            "ruleset": {
                "id": "standard",
                "display_name": "Standard",
                "min_players": 2,
                "max_players": 10,
                "actions_per_day_tiers": [[4, 7], [7, 5], [10, 4]],
                "inactivity_kick_days": 5,
                "jump_mult": 0.8,
                "base_claim": 0.9,
                "claim_weight_factor": 0.25,
                "base_attack": 0.5,
                "attack_weight_factor": 0.15,
                "prob_floor": 0.05,
                "prob_ceil": 0.95,
                "map_id": "earth"
            },
            "players": [{
                "user_id": Uuid::from_u128(1),
                "username": "a",
                "status": "alive",
                "idle_days": 2,
                "joined_day": 20_000,
                "exit_day": null,
                "exit_territories": 0
            }],
            "ownership": {},
            "last_day": null,
            "start_player_count": 2
        });
        let state = parse_state(&value).expect("a v1 state upgrades");
        assert_eq!(state.version, STATE_VERSION);
        assert_eq!(state.revision, 7);
        // Defaults filled in: a full budget, and an idle clock that runs from
        // the day they joined.
        assert_eq!(state.points_left(Uuid::from_u128(1), 20_001), 7);
        assert_eq!(
            state.player(Uuid::from_u128(1)).unwrap().idle_since(),
            20_000
        );
        // The distance knobs the old ruleset never had come from the roster.
        assert_eq!(
            state.ruleset.distance_decay,
            super::super::rulesets::STANDARD.distance_decay
        );
    }
}
