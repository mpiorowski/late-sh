//! `RealmService`: the process-global realm domain service, modeled on
//! `daily::svc::DailyService`. No live actor per game — every mutation loads
//! the row, validates, and persists with a revision CAS, so nothing needs
//! reconciling after a restart, and two players reaching for the same
//! territory are settled first-come-first-served by that CAS.
//!
//! Actions resolve the instant they are taken (see `resolver::apply_action`).
//! The 60s sweeper is no longer a resolver: it rolls each game's day over at
//! that game's own `reset_hour_utc` (archiving the day's log) and kicks
//! players who have gone quiet.

use std::collections::BTreeMap;
use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Duration as ChronoDuration, Timelike, Utc};
use late_core::{db::Db, models::realm_game::RealmGame};
use serde_json::Value;
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::activity::publisher::ActivityPublisher;
use crate::app::ai::svc::{AI_MODEL, AiService};
use crate::app::games::chips::svc::ChipService;

use super::map::{GENERATED_MAP_ID, TerritoryId, WorldMap, map_handle};
use super::mapgen::{self, GeneratedMapSpec};
use super::resolver::{
    self, ActionRejected, DayBoard, DayResult, PALETTE_SLOTS, PlayerPhase, RealmAction,
    RealmGameState, RealmPlayer, RealmPlayerStatus, ResolveEnd, STATE_VERSION, SplitMix64,
    day_seed,
};
use super::rulesets::{RealmRulesetSnapshot, pace_by_id, ruleset_by_id};

/// How long a realm stands frozen after it is made, so the people who see
/// the announcement can be in it before the first point is spent.
///
/// The whole realm is visible in this window — map, spawn, roster — and
/// nothing can be done to it. Two minutes is short on purpose: it levels the
/// people who are looking right now, which is who a freshly announced realm
/// is for, without turning creation into an appointment.
pub const REALM_MUSTER: chrono::Duration = chrono::Duration::minutes(5);

/// What is left of a realm's muster, or `None` once it is open.
pub fn muster_left(created: DateTime<Utc>, now: DateTime<Utc>) -> Option<chrono::Duration> {
    let left = (created + REALM_MUSTER) - now;
    (left > chrono::Duration::zero()).then_some(left)
}

/// A countdown as "1:23", for the header, the Lobby row and the refusal that
/// explains why a key did nothing.
pub fn countdown_label(left: chrono::Duration) -> String {
    let secs = left.num_seconds().max(0);
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// The muster written for a sentence ("2 minutes"), derived rather than
/// spelled out, so the copy cannot drift from the constant.
pub fn muster_label() -> String {
    let secs = REALM_MUSTER.num_seconds();
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s} seconds"),
        (1, 0) => "a minute".to_string(),
        (m, 0) => format!("{m} minutes"),
        (m, s) => format!("{m}m {s}s"),
    }
}

/// How long a finished realm stays listed in the Lobby. Long enough that
/// everyone gets to see how it ended and what they were paid, short enough
/// that the list is about games being played rather than a trophy shelf.
/// The row itself is never deleted — the history and the log archive stay.
pub const REALM_RESULTS_HOURS: i64 = 24;

/// A seed for a new generated world. Wall-clock plus a fresh uuid: the map
/// has to be unguessable before it exists (a creator who could predict it
/// could re-roll until their own corner looked good) and reproducible after,
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

/// Open games created plus open/active games played, per user. The cap is
/// there so one person cannot fill the Lobby by themselves, not to ration
/// play: a realm costs a few actions a day, so carrying a dozen is a normal
/// way to use this, and a game everyone stopped touching now dissolves on
/// its own (`resolver::REALM_ABANDON_DAYS`) rather than holding a slot.
pub const REALM_MAX_ACTIVE_GAMES: i64 = 10;
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
/// creator is actually picking.
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
    /// The one thing realm publishes to activity: a single #lounge line when
    /// a game is conquered (never on create/join/start/resolve).
    activity: ActivityPublisher,
    snapshot_tx: watch::Sender<Arc<RealmSnapshot>>,
    snapshot_rx: watch::Receiver<Arc<RealmSnapshot>>,
    event_tx: broadcast::Sender<RealmEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct RealmSnapshot {
    pub open_games: Vec<RealmGameItem>,
    pub active_games: Vec<RealmGameItem>,
    /// Finished within the last 30 days (window enforced in SQL).
    pub finished_games: Vec<RealmGameItem>,
}

/// Public per-game info: enough for the Lobby row and the board header,
/// never the whole state (the board loads that itself).
#[derive(Clone, Debug)]
pub struct RealmGameItem {
    pub id: Uuid,
    pub created: DateTime<Utc>,
    pub creator_id: Uuid,
    /// What the creator called it, already filled in when they left it blank.
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
    pub players: Vec<RealmPlayerItem>,
}

#[derive(Clone, Debug)]
pub struct RealmPlayerItem {
    pub user_id: Uuid,
    pub username: String,
    pub status: RealmPlayerStatus,
    pub territories: u16,
    /// The palette slot they play in — what the Lobby needs to know which
    /// colours are still going when someone asks to join.
    pub color: u8,
}

impl RealmGameItem {
    pub fn is_member(&self, user_id: Uuid) -> bool {
        self.players.iter().any(|p| p.user_id == user_id)
    }
}

#[derive(Clone, Debug)]
pub enum RealmEvent {
    GameCreated {
        game_id: Uuid,
        creator_id: Uuid,
        /// Named and timed, because this event is an invitation now: every
        /// session raises it as a banner while the realm is still frozen.
        name: String,
        opens_at: DateTime<Utc>,
    },
    PlayerJoined {
        game_id: Uuid,
        user_id: Uuid,
    },
    PlayerLeft {
        game_id: Uuid,
        user_id: Uuid,
    },
    GameStarted {
        game_id: Uuid,
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
    /// The world moved: anyone with this board open should reload it.
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
        }
    }

    pub fn subscribe_snapshot(&self) -> watch::Receiver<Arc<RealmSnapshot>> {
        self.snapshot_rx.clone()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<RealmEvent> {
        self.event_tx.subscribe()
    }

    pub fn refresh_task(&self) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.refresh().await {
                tracing::error!(error = ?e, "failed to refresh realm games");
            }
        });
    }

    /// The one background loop: roll days over at each game's own reset hour
    /// (archiving the finished day, kicking the quiet), then republish the
    /// snapshot (which doubles as the slow-poll backstop).
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

    pub fn create_game_task(&self, user_id: Uuid, username: String, spec: NewRealm) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(e) = svc.create_game(user_id, &username, &spec).await {
                tracing::error!(error = ?e, %user_id, "failed to create realm game");
                svc.send_error(user_id, &e);
            }
        });
    }

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

    /// Open a game others can join. `reset_hour_utc` is when this game's
    /// action points come back every day — the creator picks it so the game
    /// can run on its players' clock instead of a global midnight.
    pub async fn create_game(
        &self,
        user_id: Uuid,
        username: &str,
        spec: &NewRealm,
    ) -> Result<RealmGame> {
        if !(0..=23).contains(&spec.reset_hour_utc) {
            bail!("the daily reset must be a whole hour, 00-23 UTC");
        }
        let ruleset = ruleset_by_id(&spec.ruleset_id)
            .with_context(|| format!("unknown realm ruleset: {}", spec.ruleset_id))?;
        let pace = pace_by_id(&spec.pace_id)
            .with_context(|| format!("unknown realm pace: {}", spec.pace_id))?;
        let mut snapshot = RealmRulesetSnapshot::at_pace(ruleset, pace);
        // Only options this build knows are stored; anything else would be a
        // rule nothing enforces.
        snapshot.options = super::rulesets::OPTIONS
            .iter()
            .map(|o| {
                let on = spec.options.get(o.id).copied().unwrap_or(o.default_on);
                (o.id.to_string(), on)
            })
            .collect();
        // The map is the creator's pick, and freezes into the snapshot with
        // everything else: a map retired later never strands a live game.
        super::map::map_info_by_id(&spec.map_id)
            .with_context(|| format!("unknown realm map: {}", spec.map_id))?;
        snapshot.map_id = spec.map_id.clone();
        if spec.map_id == GENERATED_MAP_ID {
            // A generated world is drawn from a seed the server picks, not one
            // the client sends: the seed decides the map, and a client that
            // chose it could shop for a world where its own first spawn is a
            // fortress. Names are settled here too, once, because the AI is
            // not a pure function of the seed and everything after this point
            // has to be able to rebuild the same world from what was stored.
            let shape = spec.map_spec.clone().unwrap_or_default();
            let mut built = GeneratedMapSpec {
                seed: rand_seed(),
                names: Vec::new(),
                ..shape
            }
            .normalized();
            built.names = self
                .invent_names(built.territories as usize, built.seed)
                .await;
            snapshot.map_spec = Some(built);
        }
        let client = self.db.get().await?;
        self.ensure_game_capacity(&client, user_id).await?;
        let map = map_handle(&snapshot.map_id, snapshot.map_spec.as_ref())
            .context("realm map has no board")?;
        let today = realm_day(Utc::now(), spec.reset_hour_utc);
        let mut state = RealmGameState {
            version: STATE_VERSION,
            revision: 1,
            ruleset: snapshot,
            // The creator arrives to an empty world by definition.
            players: vec![new_player(
                user_id,
                username,
                today,
                pick_color(&[], spec.color)?,
                0,
            )],
            ownership: BTreeMap::new(),
            forts: BTreeMap::new(),
            last_day: None,
            start_player_count: 1,
            start_day: today,
        };
        // A realm is live the moment it exists: there is no starting gun to
        // wait for, so the creator spawns and can play at once. Everyone who
        // arrives later does the same, protected by their own first days.
        state.roll_day(today, day_seed(Uuid::nil(), today));
        if !spawn_player(&mut state, &map, Uuid::nil(), user_id) {
            bail!("that map has no room to start a realm");
        }
        let game = RealmGame::create_game(
            &client,
            user_id,
            &sanitize_name(&spec.name),
            &spec.ruleset_id,
            spec.reset_hour_utc,
            today,
            &serde_json::to_value(&state)?,
        )
        .await?;
        // Live from creation: the row goes straight to active.
        RealmGame::start_cas(
            &client,
            game.id,
            user_id,
            today,
            &serde_json::to_value(&state)?,
            state.revision as i64,
        )
        .await?;
        let game = RealmGame::get(&client, game.id)
            .await?
            .context("realm game vanished after creation")?;
        let name = display_name(&game, &state);
        let _ = self.event_tx.send(RealmEvent::GameCreated {
            game_id: game.id,
            creator_id: user_id,
            name: name.clone(),
            opens_at: game.created + REALM_MUSTER,
        });
        // And the lounge, so the announcement outlives the moment for
        // anyone reading the ticker.
        self.activity.realm_forming_task(user_id, game.id, &name);
        self.publish(&client).await?;
        Ok(game)
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
        self.ensure_game_capacity(&client, user_id).await?;
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
            let today = realm_day(Utc::now(), game.reset_hour_utc);
            let color = pick_color(&state.taken_colors(), color)?;
            let claimed = (state.claimed_fraction(&map) * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8;
            state
                .players
                .push(new_player(user_id, username, today, color, claimed));
            state.start_player_count = state.start_player_count.max(state.players.len() as u8);
            if !spawn_player(&mut state, &map, game_id, user_id) {
                bail!("there is no free land left to spawn on");
            }
            state.revision += 1;
            let updated = RealmGame::update_state_cas(
                &client,
                game_id,
                &serde_json::to_value(&state)?,
                expected,
            )
            .await?;
            if updated > 0 {
                let _ = self
                    .event_tx
                    .send(RealmEvent::PlayerJoined { game_id, user_id });
                let _ = self.event_tx.send(RealmEvent::GameChanged { game_id });
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
                    let _ = self
                        .event_tx
                        .send(RealmEvent::PlayerLeft { game_id, user_id });
                    self.publish(&client).await?;
                }
                return Ok(());
            }
            let expected = state.revision as i64;
            let freed: Vec<TerritoryId> = state
                .ownership
                .iter()
                .filter(|(_, owner)| **owner == user_id)
                .map(|(territory, _)| *territory)
                .collect();
            state.log(
                today,
                day_seed(game_id, today),
                resolver::LogEntry::Left {
                    user_id,
                    freed: freed.clone(),
                },
            );
            state.players.retain(|p| p.user_id != user_id);
            state.ownership.retain(|_, owner| *owner != user_id);
            state.revision += 1;
            let updated = RealmGame::update_state_cas(
                &client,
                game_id,
                &serde_json::to_value(&state)?,
                expected,
            )
            .await?;
            if updated > 0 {
                let _ = self
                    .event_tx
                    .send(RealmEvent::PlayerLeft { game_id, user_id });
                let _ = self.event_tx.send(RealmEvent::GameChanged { game_id });
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

            let seed =
                resolver::action_seed(game_id, day, state.revision, user_id, action.target());
            let outcome = match resolver::apply_action(&mut state, &map, user_id, action, day, seed)
            {
                Ok(outcome) => outcome,
                Err(rejected) => return Err(reject(rejected)),
            };

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
                        self.finish_events(
                            &state,
                            game_id,
                            &name,
                            game.reset_hour_utc,
                            Some(winner),
                        );
                    }
                    updated
                }
                ResolveEnd::Dissolved => {
                    let updated =
                        RealmGame::finish_cas(&client, game_id, day, None, &state_value, expected)
                            .await?;
                    if updated > 0 {
                        self.finish_events(&state, game_id, &name, game.reset_hour_utc, None);
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
                self.day_rolled_events(game_id, &name, finished.day + 1, &state);
            }
            if let Some(current) = state.last_day.as_ref() {
                // A game that just ended has no tomorrow to snapshot, so the
                // final board is written onto the day it ended on.
                let final_board =
                    (!matches!(outcome.end, ResolveEnd::Ongoing)).then(|| DayBoard::of(&state));
                self.archive_with_board(&client, game_id, current, final_board.as_ref())
                    .await;
            }
            let _ = self.event_tx.send(RealmEvent::ActionResolved {
                game_id,
                user_id,
                target: action.target(),
                probability: outcome.probability,
                success: outcome.success,
                attack: matches!(action, RealmAction::Attack { .. }),
                points_left: outcome.points_left,
            });
            let _ = self.event_tx.send(RealmEvent::GameChanged { game_id });
            self.publish(&client).await?;
            return Ok(());
        }
        bail!("the realm moved while you acted, try again")
    }

    // ----- sweeping

    /// Per-game housekeeping, run every 60s: roll the day over at the game's
    /// own reset hour (archiving the log it closes) and dissolve a realm that
    /// every living player has been quiet in for `REALM_ABANDON_DAYS`.
    /// Nothing here decides a move — moves resolved when they were taken.
    pub async fn sweep(&self, now: DateTime<Utc>) -> Result<()> {
        let client = self.db.get().await?;
        let games = RealmGame::list_active_for_sweep(&client, MAX_GAMES_PER_SWEEP as i64).await?;
        let mut changed = false;
        for game in games {
            let day = realm_day(now, game.reset_hour_utc);
            if game.last_resolved_day >= day {
                continue;
            }
            match self.sweep_game(&client, &game, day).await {
                Ok(touched) => changed |= touched,
                Err(e) => {
                    tracing::error!(error = ?e, game_id = %game.id, "failed to sweep realm game");
                }
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
        let (kicked, end) = resolver::sweep_idle(&mut state, &map, day);
        // `roll_day` alone is a state change worth persisting (the new day's
        // empty log), so this always writes once per game per day.
        state.revision += 1;

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
                    self.finish_events(&state, game.id, &name, game.reset_hour_utc, Some(winner));
                }
                updated
            }
            ResolveEnd::Dissolved => {
                let updated =
                    RealmGame::finish_cas(client, game.id, day, None, &state_value, expected)
                        .await?;
                if updated > 0 {
                    self.finish_events(&state, game.id, &name, game.reset_hour_utc, None);
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
        self.day_rolled_events(game.id, &name, day, &state);
        if !kicked.is_empty() {
            let _ = self
                .event_tx
                .send(RealmEvent::GameChanged { game_id: game.id });
        }
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

    /// Everything a finished game emits: the event plus ranked chip payouts.
    /// Payouts go only to players who saw the game through (alive at finish or
    /// eliminated fighting); leavers and kicked players are skipped and the
    /// next eligible player moves up. Idempotent per (template, game, user)
    /// via the per_event claim policy.
    /// The daily summons. Every player gets a nudge wherever they are (the
    /// session turns it into a banner), and the lounge gets one line so the
    /// rest of the place can see a war is running — but only for a realm
    /// with more than one person in it, since a realm sat in alone calling
    /// for players every day is noise, not news.
    fn day_rolled_events(&self, game_id: Uuid, name: &str, day: i32, state: &RealmGameState) {
        let _ = self.event_tx.send(RealmEvent::DayRolled {
            game_id,
            day,
            name: name.to_string(),
        });
        let playing = state
            .players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .count();
        if playing > 1 {
            self.activity.realm_calls(game_id, name, day);
        }
    }

    fn finish_events(
        &self,
        state: &RealmGameState,
        game_id: Uuid,
        name: &str,
        reset_hour_utc: i16,
        winner: Option<Uuid>,
    ) {
        let _ = self.event_tx.send(RealmEvent::GameFinished {
            game_id,
            winner_user_id: winner,
        });
        let Some(winner) = winner else {
            return;
        };
        self.activity.game_won_task(
            winner,
            crate::app::activity::event::ActivityGame::Realm,
            Some(format!(
                "{}: a {}-player war for {}",
                name,
                state.start_player_count,
                map_handle(&state.ruleset.map_id, state.ruleset.map_spec.as_ref())
                    .map(|m| m.display_name.clone())
                    .unwrap_or_else(|| "the world".to_string())
            )),
            None,
        );
        // A game nobody really played pays nothing; the win still stands and
        // still gets its line in #lounge.
        let today = realm_day(Utc::now(), reset_hour_utc);
        if !payout_earned(state, today, winner) {
            tracing::info!(
                %game_id, %winner,
                active_days = state.player(winner).map(|p| p.active_days).unwrap_or(0),
                held = state.territory_count(winner),
                "realm finished too early or too small to pay out"
            );
            return;
        }
        let plan = payout_plan(paying_players(state), state.ruleset.pot_scale);
        for place in final_table(state, game_id, Some(winner)) {
            let Some(rank) = place.place else {
                continue;
            };
            let Some((reward_key, amount)) = plan.get(rank - 1).copied() else {
                break;
            };
            let user_id = place.user_id;
            let chip_svc = self.chip_svc.clone();
            tokio::spawn(async move {
                match chip_svc
                    .credit_per_event_reward_template_amount(
                        user_id,
                        reward_key,
                        &game_id.to_string(),
                        amount,
                        late_core::models::chips::ChipMove::RealmConquest,
                    )
                    .await
                {
                    Ok(payout) => {
                        if !payout.credited {
                            tracing::info!(%user_id, %game_id, reward_key, "realm payout already granted");
                        }
                    }
                    Err(error) => {
                        tracing::error!(?error, %user_id, %game_id, reward_key, "failed to credit realm payout");
                    }
                }
            });
        }
    }

    // ----- snapshot

    async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        self.publish(&client).await
    }

    async fn publish(&self, client: &tokio_postgres::Client) -> Result<()> {
        let open = RealmGame::list_open(client).await?;
        let active = RealmGame::list_active(client).await?;
        let finished = RealmGame::list_finished_recent(client, REALM_RESULTS_HOURS).await?;
        let snapshot = RealmSnapshot {
            open_games: open.iter().filter_map(game_item).collect(),
            active_games: active.iter().filter_map(game_item).collect(),
            finished_games: finished.iter().filter_map(game_item).collect(),
        };
        let _ = self.snapshot_tx.send(Arc::new(snapshot));
        Ok(())
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
    /// batching rather than failing creation: a half-named world is finished
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

    async fn ensure_game_capacity(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
    ) -> Result<()> {
        let count = RealmGame::count_active_memberships(client, user_id).await?;
        if count >= REALM_MAX_ACTIVE_GAMES {
            bail!("realm games limit reached: max {REALM_MAX_ACTIVE_GAMES} games at once");
        }
        Ok(())
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

/// Put a player on the map: a random free territory, picked with a seeded
/// RNG so the same arrival always lands the same way. Returns None when the
/// world is full, which is the one thing that can refuse a join outright.
fn spawn_player(state: &mut RealmGameState, map: &WorldMap, game_id: Uuid, user_id: Uuid) -> bool {
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
    let mut rng = SplitMix64::new(
        day_seed(game_id, state.revision as i32) ^ u64::from(user_id.as_u128() as u32),
    );
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

/// Everything a creator chose, in one piece. Creation grew a dimension at a
/// time — rules, tempo, refill hour, name, options — and a five-argument
/// call was becoming a place to swap two strings by accident.
#[derive(Clone, Debug)]
pub struct NewRealm {
    pub name: String,
    pub ruleset_id: String,
    pub pace_id: String,
    /// UTC hour the daily points come back.
    pub reset_hour_utc: i16,
    /// Rule toggles, by option id. Anything missing takes its default.
    pub options: BTreeMap<String, bool>,
    /// The world to play on, from `map::MAPS`.
    pub map_id: String,
    /// For `map::GENERATED_MAP_ID`, the shape the creator asked for. The seed
    /// and the names are filled in by the service, so what arrives here is
    /// three numbers and nothing that could be used to fish for a favourable
    /// world.
    pub map_spec: Option<GeneratedMapSpec>,
    /// The creator's colour, as a palette index. None takes the first free
    /// one, which on an empty roster is the first.
    pub color: Option<u8>,
}

/// How long a game's name may be. Long enough for "The Second Punic War",
/// short enough to sit in a Lobby row next to the ruleset and the roster.
pub const REALM_NAME_MAX: usize = 32;

/// Tidy a creator-typed name: no control characters, no runaway whitespace,
/// capped at [`REALM_NAME_MAX`] characters (characters, not bytes — a name in
/// any script gets the same room).
pub fn sanitize_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(REALM_NAME_MAX)
        .collect()
}

/// The display name of a loaded row, filling the blank-name default from the
/// creator in its own state.
fn display_name(game: &RealmGame, state: &RealmGameState) -> String {
    let creator = state
        .players
        .iter()
        .find(|p| p.user_id == game.creator_id)
        .map(|p| p.username.as_str())
        .unwrap_or("someone");
    game_name(&game.name, creator)
}

/// The name to show: what the creator typed, or their own name on it when
/// they left it blank.
pub fn game_name(stored: &str, creator: &str) -> String {
    if stored.trim().is_empty() {
        let fallback = format!("{creator}'s realm");
        fallback.chars().take(REALM_NAME_MAX).collect()
    } else {
        stored.to_string()
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

/// Whether this player can take a place in the money at all.
fn payout_earned_by(state: &RealmGameState, user_id: Uuid, winner: Option<Uuid>) -> bool {
    // A dissolved game pays nobody, however long they played.
    winner.is_some() && payout_eligible(state, user_id)
}

/// How many players the pot is sized on: those who have actually played,
/// not everyone on the roster.
///
/// With arrivals open all game, counting the roster would let a colluder
/// walk alts in to swell the prize without ever taking a turn. Counting
/// players who cleared `min_payout_active_days` means an honest latecomer
/// grows the pot and a body thrown in to pad it does not. Floored at two,
/// because a realm is never worth less than a duel.
pub fn paying_players(state: &RealmGameState) -> u8 {
    let played = state
        .players
        .iter()
        .filter(|p| p.active_days >= state.ruleset.min_payout_active_days)
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

fn game_item(game: &RealmGame) -> Option<RealmGameItem> {
    let state = parse_state(&game.state).ok()?;
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
    let creator = state
        .players
        .iter()
        .find(|p| p.user_id == game.creator_id)
        .map(|p| p.username.as_str())
        .unwrap_or("someone");
    let map = map_handle(&state.ruleset.map_id, state.ruleset.map_spec.as_ref());
    let today = realm_day(Utc::now(), game.reset_hour_utc);
    let joinable = map.as_ref().is_some_and(|m| state.joinable(m)) && game.winner_user_id.is_none();
    let claimed = map
        .as_ref()
        .map(|m| state.claimed_fraction(m))
        .unwrap_or(1.0);
    let can_withdraw = state.players.len() == 1
        || state
            .players
            .iter()
            .any(|p| state.phase(p.user_id, today) == PlayerPhase::New);
    Some(RealmGameItem {
        id: game.id,
        created: game.created,
        creator_id: game.creator_id,
        name: game_name(&game.name, creator),
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
        players,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let bar = state.ruleset.min_payout_active_days;
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
