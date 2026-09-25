//! The realm engine: pure, deterministic, and the owner of the game-state
//! shape persisted in `realm_games.state`.
//!
//! Actions resolve the moment they are taken. There is no hidden queue and no
//! midnight batch: a player spends one of their action points, the roll
//! happens immediately against the state as it stands, and everyone else sees
//! the result on their next snapshot. Two players reaching for the same
//! territory is settled first-come-first-served by the row CAS in `svc.rs` —
//! the loser reloads and finds the world already changed.
//!
//! What a day still means: the action budget. Every player gets N points per
//! *realm day*, which rolls over at the game's own `reset_hour_utc` (picked at
//! creation so a US game and an EU game can refill at civilised local times).
//! Days also drive the inactivity kick.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::map::{TerritoryId, WorldMap};
use super::rulesets::{
    ProbabilityKind, Reach, RealmRulesetSnapshot, RouteMode, action_probability,
};

pub const STATE_VERSION: u8 = 2;

/// The persisted `realm_games.state` JSONB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmGameState {
    pub version: u8,
    /// Monotonic; every persisted write bumps it (CAS guard input).
    pub revision: u64,
    pub ruleset: RealmRulesetSnapshot,
    pub players: Vec<RealmPlayer>,
    /// Territory -> owner. BTreeMap for deterministic iteration and stable
    /// JSON.
    pub ownership: BTreeMap<TerritoryId, Uuid>,
    /// Territory -> how deep its holder has dug in. Level one is simply
    /// holding the place and is never stored, so a realm nobody fortifies
    /// carries nothing extra; taking a place levels its walls.
    #[serde(default)]
    pub forts: BTreeMap<TerritoryId, u8>,
    /// The running log of the current realm day; older days are archived to
    /// `realm_days` when the day rolls over.
    pub last_day: Option<DayResult>,
    /// Player count when the game started; picks the action-point tier and
    /// the payout tier.
    pub start_player_count: u8,
    /// The realm day the game started on — the clock the early-game attack
    /// grace runs off. Zero on games frozen before it existed, which makes
    /// their grace long expired.
    #[serde(default)]
    pub start_day: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealmPlayer {
    pub user_id: Uuid,
    pub username: String,
    pub status: RealmPlayerStatus,
    /// UTC day (days since epoch) the player entered the game.
    pub joined_day: i32,
    /// UTC day the player exited (eliminated/left/kicked); None while alive.
    pub exit_day: Option<i32>,
    /// Territories held just before exit — ranking tie-break.
    pub exit_territories: u16,
    /// The realm day `actions_used` counts for. A different day means the
    /// budget has refilled; nothing has to run at the turn of the day.
    #[serde(default)]
    pub actions_day: i32,
    /// Action points spent on `actions_day`.
    #[serde(default)]
    pub actions_used: u8,
    /// What `actions_day` was worth when it opened: base plus whatever the
    /// days away banked. Frozen at the first action of the day, because the
    /// gap it was computed from closes the moment you act — without this the
    /// bank would vanish on the first spend.
    #[serde(default)]
    pub actions_allowance: u8,
    /// Last realm day this player acted on — the dormancy clock.
    #[serde(default)]
    pub last_action_day: Option<i32>,
    /// Distinct realm days this player has acted on. The bar for winning and
    /// for being paid: a conquest has to be a week of play, not an afternoon
    /// with an obliging friend.
    #[serde(default)]
    pub active_days: u16,
    /// How much of the map was already claimed on the day they joined, in
    /// percent. Frozen at arrival, because it is what their arrival was
    /// like: it decides how long their new-arrival shield lasts, and a
    /// number that kept moving would keep moving their protection with it.
    /// Zero on players who joined before this was recorded, which is the
    /// behaviour they were already playing under.
    #[serde(default)]
    pub joined_claimed: u8,
    /// Which colour on the map is theirs, as an index into the palette.
    /// Picked when they arrive. `None` on games made before it was a
    /// choice — those fall back to roster order, which is what they were
    /// drawn in, so no live game changes colour under its players.
    #[serde(default)]
    pub color: Option<u8>,
}

/// How many colours a realm can be played in. Presentation lives in the UI,
/// but the count is a rule the service enforces when someone picks one —
/// `state::PLAYER_PALETTES` must have exactly this many, which a test holds
/// it to.
pub const PALETTE_SLOTS: u8 = 10;

impl RealmPlayer {
    /// Points already spent on `day` (zero once the day has rolled).
    pub fn used_on(&self, day: i32) -> u8 {
        if self.actions_day == day {
            self.actions_used
        } else {
            0
        }
    }

    /// The day the quiet clock counts from: their last action, else the day
    /// they joined.
    pub fn idle_since(&self) -> i32 {
        self.last_action_day.unwrap_or(self.joined_day)
    }

    /// Days since they last acted (0 while they are playing today).
    pub fn quiet_days(&self, day: i32) -> i32 {
        (day - self.idle_since()).max(0)
    }
}

/// A player's standing on the clock: what they can spend today, how long
/// they have been quiet, and how far their grip has slipped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayStanding {
    /// Base points for the game's size.
    pub base: u8,
    /// Points banked from missed days, after the penalty ladder.
    pub banked: u8,
    /// Everything they may spend today (base + banked).
    pub allowance: u8,
    /// Already spent today.
    pub used: u8,
    /// Days since their last action.
    pub quiet_days: i32,
    /// What a further missed day would bank, as a fraction of base: 1.0
    /// while they are keeping up, 0.0 once they have been away too long.
    pub bank_rate: f64,
    /// Strength multiplier on their holdings: 1.0 while active, falling as
    /// an abandoned empire crumbles.
    pub decay: f64,
}

impl DayStanding {
    pub fn left(self) -> u8 {
        self.allowance.saturating_sub(self.used)
    }

    /// Their empire is visibly slipping — worth saying out loud in the
    /// standings, both to them and to whoever is eyeing their borders.
    pub fn decaying(self) -> bool {
        self.decay < 1.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealmPlayerStatus {
    Alive,
    Eliminated,
    Left,
    Kicked,
}

/// What a player is trying to do to a territory. Kept as an enum (rather than
/// inferred from ownership) so the log records what was intended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RealmAction {
    Claim {
        target: TerritoryId,
    },
    Attack {
        target: TerritoryId,
    },
    /// Dig in on land you already hold.
    Fortify {
        target: TerritoryId,
    },
}

impl RealmAction {
    pub fn target(&self) -> TerritoryId {
        match self {
            RealmAction::Claim { target }
            | RealmAction::Attack { target }
            | RealmAction::Fortify { target } => *target,
        }
    }
}

/// The board as a day closed: who was playing, who held what, and how deep
/// they were dug in.
///
/// Stored per day beside that day's log (`realm_days.board`) so a finished
/// game can be walked through rather than only read. It carries its own
/// roster rather than pointing into the game's: a player who withdraws is
/// *removed* from `RealmGameState::players`, so an index into that list means
/// something different after they go, and a snapshot that pointed into it
/// would quietly re-attribute a month of history.
///
/// Territories are paired with a slot into this snapshot's own roster because
/// a uuid is 36 bytes and there are up to 242 of them a day. Free land is
/// absent rather than recorded as free — it is most of the map early on, and
/// absent is the same answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DayBoard {
    /// The roster as it stood, in the order the slots below index.
    #[serde(default)]
    pub players: Vec<DayPlayer>,
    /// `(territory, player slot)`.
    #[serde(default)]
    pub owned: Vec<(TerritoryId, u8)>,
    /// `(territory, wall level)`, only where there is a wall.
    #[serde(default)]
    pub forts: Vec<(TerritoryId, u8)>,
}

/// Who somebody was on the day this board was taken. Enough to draw and
/// caption them without the live game, which by then may not remember them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayPlayer {
    pub user_id: Uuid,
    pub username: String,
    /// Palette index, so a replayed day is the colour it was played in.
    #[serde(default)]
    pub color: Option<u8>,
}

impl DayBoard {
    /// Snapshot the state as it stands. Taken at the rollover, which is the
    /// moment the history is about: the board just before everyone's points
    /// came back.
    pub fn of(state: &RealmGameState) -> Self {
        let slot_of: BTreeMap<Uuid, u8> = state
            .players
            .iter()
            .enumerate()
            .map(|(slot, p)| (p.user_id, slot as u8))
            .collect();
        Self {
            players: state
                .players
                .iter()
                .enumerate()
                .map(|(slot, p)| DayPlayer {
                    user_id: p.user_id,
                    username: p.username.clone(),
                    // Fall back to seat order, the way the live board does.
                    color: Some(p.color.unwrap_or(slot as u8)),
                })
                .collect(),
            owned: state
                .ownership
                .iter()
                .filter_map(|(territory, owner)| Some((*territory, *slot_of.get(owner)?)))
                .collect(),
            forts: state
                .forts
                .iter()
                .filter(|(_, level)| **level > 0)
                .map(|(territory, level)| (*territory, *level))
                .collect(),
        }
    }

    /// Who held `territory` that day.
    pub fn owner(&self, territory: TerritoryId) -> Option<&DayPlayer> {
        let slot = self
            .owned
            .iter()
            .find(|(id, _)| *id == territory)
            .map(|(_, slot)| *slot)?;
        self.players.get(slot as usize)
    }

    /// How many territories each player held, biggest first.
    pub fn standings(&self) -> Vec<(&DayPlayer, u16)> {
        let mut counts: BTreeMap<u8, u16> = BTreeMap::new();
        for (_, slot) in &self.owned {
            *counts.entry(*slot).or_default() += 1;
        }
        let mut standings: Vec<(&DayPlayer, u16)> = self
            .players
            .iter()
            .enumerate()
            .map(|(slot, player)| (player, counts.get(&(slot as u8)).copied().unwrap_or(0)))
            .collect();
        standings.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.username.cmp(&b.0.username)));
        standings
    }

    pub fn fort_level(&self, territory: TerritoryId) -> u8 {
        self.forts
            .iter()
            .find(|(id, _)| *id == territory)
            .map(|(_, level)| *level)
            .unwrap_or(0)
    }

    /// Nothing held by anybody: a day before the first spawn, or a board that
    /// failed to parse.
    pub fn is_empty(&self) -> bool {
        self.owned.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayResult {
    pub day: i32,
    pub seed: u64,
    pub entries: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum LogEntry {
    Claimed {
        user_id: Uuid,
        target: TerritoryId,
        probability: f64,
        success: bool,
        /// Route cost, rounded — 1 is a move onto your own border. Named
        /// `hops` because that is what the archives call it.
        #[serde(default = "one_hop")]
        hops: u32,
        /// The route went over open water.
        #[serde(default)]
        by_sea: bool,
    },
    Attacked {
        user_id: Uuid,
        defender_id: Uuid,
        target: TerritoryId,
        probability: f64,
        success: bool,
        #[serde(default = "one_hop")]
        hops: u32,
        #[serde(default)]
        by_sea: bool,
    },
    Fortified {
        user_id: Uuid,
        target: TerritoryId,
        level: u8,
    },
    /// Retired: the queue era's "your queued order was no longer legal at
    /// midnight" row. Nothing writes it now — an illegal action is refused on
    /// the spot and costs nothing — but archived days still hold them.
    Fizzled {
        user_id: Uuid,
        action: RealmAction,
        reason: FizzleReason,
    },
    /// Somebody arrived and was put somewhere. Never logged before this,
    /// which is why the map could not be replayed from the log: every game
    /// began with territories nobody was recorded as taking.
    Spawned {
        user_id: Uuid,
        target: TerritoryId,
    },
    /// Somebody withdrew, and this is the ground they let go of.
    Left {
        user_id: Uuid,
        #[serde(default)]
        freed: Vec<TerritoryId>,
    },
    Kicked {
        user_id: Uuid,
        freed_territories: u16,
        /// Which territories went back to nobody. Empty on days archived
        /// before this was recorded — the count above was all there was.
        #[serde(default)]
        freed: Vec<TerritoryId>,
    },
    Eliminated {
        user_id: Uuid,
        by_user_id: Uuid,
    },
    Won {
        user_id: Uuid,
    },
}

fn one_hop() -> u32 {
    1
}

/// Why a queued action was skipped, back when actions were queued. Retained
/// only so archived `realm_days` rows keep parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FizzleReason {
    TargetOwned,
    AlreadyYours,
    TargetFree,
    NotAdjacent,
    PlayerEliminated,
    UnknownTerritory,
}

/// Why an action was refused. Nothing is spent and nothing is logged: the
/// player is told and can pick again, which is the whole point of resolving
/// on the spot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionRejected {
    /// Not an alive player in this game (spectator, eliminated, left).
    NotPlaying,
    /// Out of action points until the next refill.
    NoPointsLeft,
    /// Target id is not on this map.
    UnknownTerritory,
    /// Claiming a territory that someone already owns.
    TargetOwned,
    /// Attacking a territory that has no owner.
    TargetFree,
    /// The target is already yours.
    AlreadyYours,
    /// Digging in is switched off in this realm.
    NoFortifying,
    /// You can only dig in on your own ground.
    NotYours,
    /// Already as deep as this realm allows.
    FullyFortified(u8),
    /// You are still new here: free land only. Carries the days left.
    AttacksLocked(u8),
    /// You are ramping up: your own neighbours only. Carries the days left.
    DistantAttacksLocked(u8),
    /// This game keeps war to your own neighbourhood, and the target is
    /// beyond it — while something nearer is still available.
    OutOfRange,
    /// There is no way to get there: another landmass, with nothing to sail
    /// from or nothing to land on.
    NoRoute,
    /// The target has just joined and cannot be attacked yet. Carries the
    /// days left of their protection.
    TargetIsNew(u8),
    /// The target is still ramping up: only their neighbours may come for
    /// them, and you are not one.
    TargetIsRamping(u8),
}

impl ActionRejected {
    pub fn message(self) -> String {
        match self {
            ActionRejected::NotPlaying => "you are not playing in this realm".into(),
            ActionRejected::NoPointsLeft => "no action points left until the next refill".into(),
            ActionRejected::UnknownTerritory => "no such territory on this map".into(),
            ActionRejected::TargetOwned => {
                "someone claimed that territory first — attack it".into()
            }
            ActionRejected::TargetFree => "that territory is unowned — claim it".into(),
            ActionRejected::AlreadyYours => "that territory is already yours".into(),
            ActionRejected::AttacksLocked(days) => {
                format!("you are new here for {days}d — take free land first")
            }
            ActionRejected::DistantAttacksLocked(days) => {
                format!("you can reach past your own border in {days}d — for now, neighbours only")
            }
            ActionRejected::NoFortifying => {
                "this realm has no fortifications — take ground instead".into()
            }
            ActionRejected::NotYours => "you can only dig in on your own land".into(),
            ActionRejected::FullyFortified(level) => {
                format!("already dug in as deep as it goes (level {level})")
            }
            ActionRejected::OutOfRange => {
                "too far for this realm — take something nearer first".into()
            }
            ActionRejected::NoRoute => {
                "no way to get there — you need a coast to sail from, and it needs one to \
                 land on"
                    .into()
            }
            ActionRejected::TargetIsNew(days) => {
                format!("they only just arrived — untouchable for another {days}d")
            }
            ActionRejected::TargetIsRamping(days) => {
                format!(
                    "they are still finding their feet for {days}d — only their neighbours may come for them"
                )
            }
        }
    }
}

/// Where a player stands in their own first days. A realm has no single
/// starting gun any more — people arrive when they arrive — so the opening
/// protection is per player, counted from the day they joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlayerPhase {
    /// Just arrived: may only take free land, and nobody may touch them.
    New,
    /// Finding their feet: may fight neighbours, and only neighbours may
    /// fight them.
    Rampup,
    /// Fully in the war; anything goes, in both directions.
    Involved,
}

impl PlayerPhase {
    pub fn label(self) -> &'static str {
        match self {
            PlayerPhase::New => "new",
            PlayerPhase::Rampup => "ramping up",
            PlayerPhase::Involved => "involved",
        }
    }

    /// One line on what this phase means, for the board.
    pub fn blurb(self) -> &'static str {
        match self {
            PlayerPhase::New => "free land only · nobody can attack you",
            PlayerPhase::Rampup => "neighbours only · only neighbours can attack you",
            PlayerPhase::Involved => "anything goes, both ways",
        }
    }
}

/// How the game stands after an action or a sweep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveEnd {
    Ongoing,
    /// Every rival conquered, or the whole map held.
    Won(Uuid),
    /// Everyone exited; no winner.
    Dissolved,
}

/// One resolved action, ready to be persisted and told to the actor.
#[derive(Debug, Clone)]
pub struct ActionOutcome {
    pub entry: LogEntry,
    pub probability: f64,
    pub success: bool,
    pub reach: Reach,
    /// Points left for this player today, after spending one.
    pub points_left: u8,
    pub end: ResolveEnd,
}

/// Deterministic day seed: `blake3(game_id || day)` truncated to u64. Still
/// the seed the day's log is stamped with.
pub fn day_seed(game_id: Uuid, day: i32) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(game_id.as_bytes());
    hasher.update(&day.to_le_bytes());
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 >= 8 bytes"))
}

/// Seed for a single action: the day seed mixed with the state revision, the
/// actor and the target. The revision makes every attempt its own roll (a CAS
/// loser that retries against the changed world genuinely re-rolls), while
/// keeping a given attempt reproducible from the persisted inputs.
pub fn action_seed(
    game_id: Uuid,
    day: i32,
    revision: u64,
    actor: Uuid,
    target: TerritoryId,
) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(game_id.as_bytes());
    hasher.update(&day.to_le_bytes());
    hasher.update(&revision.to_le_bytes());
    hasher.update(actor.as_bytes());
    hasher.update(&target.to_le_bytes());
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 >= 8 bytes"))
}

/// SplitMix64: tiny, portable, deterministic forever — resolution must never
/// change under a dependency bump, so no external RNG.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1) with 53-bit precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform index in [0, n). n must be > 0.
    pub fn next_index(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// Fisher-Yates.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.next_index(i + 1));
        }
    }
}

impl RealmGameState {
    /// Palette slots already spoken for. A player from before colours were
    /// picked counts as holding the slot their seat was drawn in, so nobody
    /// can take a colour already on the map.
    pub fn taken_colors(&self) -> Vec<u8> {
        self.players
            .iter()
            .enumerate()
            .map(|(seat, p)| p.color.unwrap_or(seat as u8))
            .collect()
    }

    pub fn player(&self, user_id: Uuid) -> Option<&RealmPlayer> {
        self.players.iter().find(|p| p.user_id == user_id)
    }

    fn player_mut(&mut self, user_id: Uuid) -> Option<&mut RealmPlayer> {
        self.players.iter_mut().find(|p| p.user_id == user_id)
    }

    pub fn is_alive(&self, user_id: Uuid) -> bool {
        self.player(user_id).map(|p| p.status) == Some(RealmPlayerStatus::Alive)
    }

    pub fn territory_count(&self, user_id: Uuid) -> u16 {
        self.ownership.values().filter(|o| **o == user_id).count() as u16
    }

    /// Territories held by a player, ascending — the source set for reach.
    pub fn holdings(&self, user_id: Uuid) -> Vec<TerritoryId> {
        self.ownership
            .iter()
            .filter(|(_, owner)| **owner == user_id)
            .map(|(t, _)| *t)
            .collect()
    }

    /// `1 + sum of held territory weights` — the strength term in attack
    /// odds. Raw, before dormancy is taken into account.
    pub fn strength(&self, user_id: Uuid, map: &WorldMap) -> f64 {
        1.0 + self
            .ownership
            .iter()
            .filter(|(_, owner)| **owner == user_id)
            .filter_map(|(t, _)| map.territory(*t))
            .map(|t| t.weight)
            .sum::<f64>()
    }

    /// Strength as it counts on `day`: an empire whose ruler has gone quiet
    /// defends with less than its size suggests.
    pub fn effective_strength(&self, user_id: Uuid, map: &WorldMap, day: i32) -> f64 {
        let raw = self.strength(user_id, map);
        (raw * self.decay_multiplier(user_id, day)).max(1.0)
    }

    /// Where a player stands in their own opening days.
    /// How long this player's new-arrival shield lasts. `attack_grace_days`
    /// for somebody who was here at the start, up to that plus
    /// `newcomer_shield_max_days` for somebody who arrived at the join door
    /// with the world already carved up. The shield covers taking free land
    /// safely, which is the only thing a one-territory newcomer can usefully
    /// do; what it buys them is the chance to have a position before anybody
    /// is allowed to come for it.
    pub fn shield_days(&self, user_id: Uuid) -> i32 {
        let base = i32::from(self.ruleset.attack_grace_days);
        let Some(player) = self.player(user_id) else {
            return base;
        };
        let door = self.ruleset.join_max_claimed.max(0.01);
        let crowding = (f64::from(player.joined_claimed) / 100.0 / door).clamp(0.0, 1.0);
        base + (f64::from(self.ruleset.newcomer_shield_max_days) * crowding).round() as i32
    }

    /// The rampup window keeps its own length; a longer shield moves it back
    /// rather than eating it, so a late arrival gets the same "neighbours
    /// only" run-in that everybody else got.
    fn rampup_until(&self, user_id: Uuid) -> i32 {
        let span = i32::from(self.ruleset.distant_attack_grace_days)
            - i32::from(self.ruleset.attack_grace_days);
        self.shield_days(user_id) + span.max(0)
    }

    pub fn phase(&self, user_id: Uuid, day: i32) -> PlayerPhase {
        let Some(player) = self.player(user_id) else {
            return PlayerPhase::Involved;
        };
        let since = (day - player.joined_day).max(0);
        if since < self.shield_days(user_id) {
            PlayerPhase::New
        } else if since < self.rampup_until(user_id) {
            PlayerPhase::Rampup
        } else {
            PlayerPhase::Involved
        }
    }

    /// Days until this player reaches `phase` (0 once they are there).
    pub fn days_until(&self, user_id: Uuid, day: i32, phase: PlayerPhase) -> u8 {
        let Some(player) = self.player(user_id) else {
            return 0;
        };
        let target = match phase {
            PlayerPhase::New => return 0,
            PlayerPhase::Rampup => self.shield_days(user_id),
            PlayerPhase::Involved => self.rampup_until(user_id),
        };
        let since = (day - player.joined_day).max(0);
        (target - since).clamp(0, i32::from(u8::MAX)) as u8
    }

    /// Points a day is worth right now: the tier for however many players
    /// are properly in the war. Nobody involved yet (a realm in its first
    /// days) falls on the most generous tier.
    pub fn daily_base(&self, day: i32) -> u8 {
        self.ruleset.actions_per_day(self.involved_count(day))
    }

    /// Living players who are past their opening days — the count the daily
    /// action budget is sized against. A realm nobody has settled into yet
    /// pays the most generous tier, which is what the founders deserve while
    /// they are alone on a whole world.
    pub fn involved_count(&self, day: i32) -> u8 {
        self.players
            .iter()
            .filter(|p| p.status == RealmPlayerStatus::Alive)
            .filter(|p| self.phase(p.user_id, day) == PlayerPhase::Involved)
            .count()
            .min(u8::MAX as usize) as u8
    }

    /// Whether an attack from `actor` on `defender` is allowed yet, given
    /// where each of them stands and how far apart they are. Protection runs
    /// both ways: a newcomer cannot swing, and cannot be swung at.
    pub fn attack_allowed(
        &self,
        actor: Uuid,
        defender: Uuid,
        reach: Reach,
        day: i32,
    ) -> Result<(), ActionRejected> {
        match self.phase(actor, day) {
            PlayerPhase::New => {
                return Err(ActionRejected::AttacksLocked(self.days_until(
                    actor,
                    day,
                    PlayerPhase::Rampup,
                )));
            }
            PlayerPhase::Rampup if !reach.is_adjacent() => {
                return Err(ActionRejected::DistantAttacksLocked(self.days_until(
                    actor,
                    day,
                    PlayerPhase::Involved,
                )));
            }
            _ => {}
        }
        match self.phase(defender, day) {
            PlayerPhase::New => Err(ActionRejected::TargetIsNew(self.days_until(
                defender,
                day,
                PlayerPhase::Rampup,
            ))),
            PlayerPhase::Rampup if !reach.is_adjacent() => Err(ActionRejected::TargetIsRamping(
                self.days_until(defender, day, PlayerPhase::Involved),
            )),
            _ => Ok(()),
        }
    }

    /// How much of the map is spoken for, 0.0 to 1.0.
    pub fn claimed_fraction(&self, map: &WorldMap) -> f64 {
        if map.territories.is_empty() {
            return 1.0;
        }
        self.ownership.len() as f64 / map.territories.len() as f64
    }

    /// Can someone still join? Only while there is a world left to join
    /// into: past `join_max_claimed` a newcomer would be food, not a player.
    pub fn joinable(&self, map: &WorldMap) -> bool {
        self.players.len() < self.ruleset.max_players as usize
            && self.claimed_fraction(map) < self.ruleset.join_max_claimed
    }

    /// What a missed day banks, as a fraction of the daily base, given how
    /// many days in a row were missed. Missing one is nearly free; missing a
    /// few costs half; past the cutoff a missed day banks nothing, so being
    /// away cannot be hoarded into a blitz.
    pub fn bank_rate(&self, missed: i32) -> f64 {
        let rs = &self.ruleset;
        match missed {
            ..=0 => 1.0,
            1 => rs.bank_rate_one_missed,
            m if m <= i32::from(rs.bank_missed_cutoff) => rs.bank_rate_few_missed,
            _ => 0.0,
        }
    }

    /// How much an abandoned empire has crumbled: full strength while the
    /// player is keeping up, then falling for every quiet day past the
    /// grace, down to the floor. This is what stops a dormant player from
    /// freezing a game — their land gets easier to take rather than being
    /// handed out free.
    pub fn decay_multiplier(&self, user_id: Uuid, day: i32) -> f64 {
        let rs = &self.ruleset;
        let quiet = self.player(user_id).map(|p| p.quiet_days(day)).unwrap_or(0);
        let over = quiet - i32::from(rs.dormancy_grace_days);
        if over <= 0 {
            return 1.0;
        }
        (1.0 - rs.decay_per_day * over as f64).max(rs.decay_floor)
    }

    /// What a day opens with for this player: base plus the bank. Computed
    /// from the gap since their last action, so it must be read *before*
    /// they act on `day` — which is why the first action freezes it.
    fn opening_allowance(&self, player: &RealmPlayer, day: i32) -> (u8, u8) {
        let base = self.daily_base(day);
        // Days between their last action and today are the missed ones; the
        // ladder decides what each of them is still worth.
        let missed = (player.quiet_days(day) - 1).max(0);
        let rate = self.bank_rate(missed);
        let cap = f64::from(base) * f64::from(self.ruleset.bank_cap_days);
        let banked = (f64::from(base) * rate * missed as f64).min(cap).floor() as u8;
        (base, banked)
    }

    /// Everything about a player's day: allowance, banking, decay.
    pub fn standing(&self, user_id: Uuid, day: i32) -> DayStanding {
        let base = self.daily_base(day);
        let Some(player) = self.player(user_id) else {
            return DayStanding {
                base,
                banked: 0,
                allowance: 0,
                used: 0,
                quiet_days: 0,
                bank_rate: 0.0,
                decay: 1.0,
            };
        };
        let (allowance, banked) = if player.actions_day == day && player.actions_allowance > 0 {
            // Already playing today: the day's worth was settled when it
            // opened and does not shift underneath them.
            let allowance = player.actions_allowance;
            (allowance, allowance.saturating_sub(base))
        } else {
            let (base, banked) = self.opening_allowance(player, day);
            (base.saturating_add(banked), banked)
        };
        let missed = (player.quiet_days(day) - 1).max(0);
        DayStanding {
            base,
            banked,
            allowance,
            used: player.used_on(day),
            quiet_days: player.quiet_days(day),
            // What another missed day would be worth from here.
            bank_rate: self.bank_rate(missed + 1),
            decay: self.decay_multiplier(user_id, day),
        }
    }

    /// How deep the holder of this territory has dug in. One means they
    /// simply hold it, which is where every territory starts and where every
    /// captured one returns to.
    pub fn fort_level(&self, territory: TerritoryId) -> u8 {
        self.forts.get(&territory).copied().unwrap_or(1).max(1)
    }

    /// What this territory's walls do to an attacker's odds, with the
    /// holder's dormancy folded in — an empire nobody is ruling does not man
    /// its walls, so an abandoned fortress is not a fortress for long.
    pub fn fortification(&self, territory: TerritoryId, day: i32) -> f64 {
        let level = self.fort_level(territory);
        if level <= 1 {
            return 1.0;
        }
        let raw = self
            .ruleset
            .fort_defence_per_level
            .powi(i32::from(level - 1));
        let decay = self
            .ownership
            .get(&territory)
            .map(|owner| self.decay_multiplier(*owner, day))
            .unwrap_or(1.0);
        // Decay walks the bonus back towards nothing rather than scaling it,
        // so a fully dormant holder keeps only a fraction of their walls.
        1.0 - (1.0 - raw) * decay
    }

    /// What a territory's walls add to the cost of marching past it.
    pub fn fort_transit(&self, territory: TerritoryId, day: i32) -> f64 {
        let level = self.fort_level(territory);
        if level <= 1 {
            return 0.0;
        }
        let decay = self
            .ownership
            .get(&territory)
            .map(|owner| self.decay_multiplier(*owner, day))
            .unwrap_or(1.0);
        self.ruleset.fort_transit_per_level * f64::from(level - 1) * decay
    }

    /// Is this option on for this game? Anything the creator never saw
    /// falls back to the roster default, so adding one does not disturb a
    /// running realm.
    pub fn option(&self, id: &str) -> bool {
        self.ruleset
            .options
            .get(id)
            .copied()
            .or_else(|| super::rulesets::option_by_id(id).map(|o| o.default_on))
            .unwrap_or(false)
    }

    /// The furthest a move may reach in this game. `None` when long-range
    /// strikes are allowed, which is the default.
    pub fn reach_cap(&self) -> Option<f64> {
        (!self.option(super::rulesets::OPTION_LONG_RANGE)).then_some(self.ruleset.short_reach_cost)
    }

    /// Has this player put in the days a conquest is supposed to take?
    pub fn can_win(&self, user_id: Uuid) -> bool {
        // Enough rivals to have been a war at all: a realm created and sat
        // in alone for a week is not a conquest.
        self.players.len() >= self.ruleset.min_players as usize
            && self
                .player(user_id)
                .is_some_and(|p| p.active_days >= self.ruleset.min_active_days_to_win)
    }

    /// Days of play still owed before this player could win.
    pub fn days_to_win(&self, user_id: Uuid) -> u16 {
        let played = self.player(user_id).map(|p| p.active_days).unwrap_or(0);
        self.ruleset.min_active_days_to_win.saturating_sub(played)
    }

    /// Action points this player has left on `day`.
    pub fn points_left(&self, user_id: Uuid, day: i32) -> u8 {
        self.standing(user_id, day).left()
    }

    /// Every free or enemy territory bordering the player's holdings. Empty
    /// means they are boxed in and every move is a crossing.
    pub fn reachable_targets(&self, user_id: Uuid, map: &WorldMap) -> Vec<TerritoryId> {
        let mut out: Vec<TerritoryId> = self
            .ownership
            .iter()
            .filter(|(_, owner)| **owner == user_id)
            .filter_map(|(t, _)| map.territory(*t))
            .flat_map(|t| t.neighbors.iter().copied())
            .filter(|n| self.ownership.get(n) != Some(&user_id))
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Free a player's territories and record their exit.
    /// Free a player's territories and record their exit. Hands back the
    /// territories themselves rather than a count: the log is what the
    /// history replays from, and "three territories went somewhere" cannot be
    /// replayed.
    fn exit_player(
        &mut self,
        user_id: Uuid,
        status: RealmPlayerStatus,
        day: i32,
    ) -> Vec<TerritoryId> {
        let freed: Vec<TerritoryId> = self
            .ownership
            .iter()
            .filter(|(_, owner)| **owner == user_id)
            .map(|(territory, _)| *territory)
            .collect();
        self.ownership.retain(|_, owner| *owner != user_id);
        if let Some(p) = self.player_mut(user_id) {
            p.status = status;
            p.exit_day = Some(day);
            p.exit_territories = freed.len() as u16;
        }
        freed
    }

    /// Start (or keep) the log for `day`, handing back the previous day's log
    /// so the caller can archive it.
    pub fn roll_day(&mut self, day: i32, seed: u64) -> Option<DayResult> {
        match &self.last_day {
            Some(existing) if existing.day == day => None,
            _ => self.last_day.replace(DayResult {
                day,
                seed,
                entries: Vec::new(),
            }),
        }
    }

    /// Append to the day's log, opening the day if this is its first line.
    ///
    /// `pub(super)` rather than private: the service writes the two entries
    /// the engine never sees — a spawn and a withdrawal — and those are
    /// exactly the two the map's history cannot be replayed without.
    pub(super) fn log(&mut self, day: i32, seed: u64, entry: LogEntry) {
        self.roll_day(day, seed);
        if let Some(current) = self.last_day.as_mut() {
            current.entries.push(entry);
        }
    }
}

/// Everything reach depends on, computed once for a player: what every
/// target costs to get to, by land and by sea.
///
/// Built once and reused, because the alternative — deriving it per target —
/// is a full pass over the world for every row of the target table, on every
/// frame.
pub struct ReachIndex {
    /// What it costs to be standing on each territory *on each landmass*,
    /// and how many rivals the cheapest way there crossed. Keyed by landmass
    /// as well as territory because a country with an overseas part is not a
    /// bridge between continents: arriving in French Guiana is not arriving
    /// in France's European border, and the router has to be able to say
    /// which of the two you are standing on.
    stand: BTreeMap<(TerritoryId, u32), (f64, u8)>,
    /// Territories this player could sail from, given the game's options.
    launch_points: Vec<TerritoryId>,
    /// Their territories, for the "am I boxed in" question.
    holdings: Vec<TerritoryId>,
}

impl ReachIndex {
    pub fn build(state: &RealmGameState, map: &WorldMap, actor: Uuid) -> Self {
        Self::build_on(state, map, actor, state.start_day)
    }

    /// As `build`, for a given day — walls weaken as their holder goes
    /// quiet, so the routes around them change with the calendar.
    pub fn build_on(state: &RealmGameState, map: &WorldMap, actor: Uuid, day: i32) -> Self {
        let rs = &state.ruleset;
        let holdings = state.holdings(actor);

        // Dijkstra over "what does it cost to be standing here", charging
        // for entering a territory by who holds it. Your own land is free to
        // move through, empty land is cheap, and a rival's land is dear —
        // which is what makes a border worth holding and a march past one
        // expensive.
        // A holding is a standing place on every landmass it has ground on —
        // hold France and you are in Europe *and* in South America, because
        // you hold both parts. What you cannot do is walk between them.
        let mut stand: BTreeMap<(TerritoryId, u32), (f64, u8)> = holdings
            .iter()
            .filter_map(|id| map.territory(*id))
            .flat_map(|t| {
                t.landmasses
                    .iter()
                    .map(move |mass| ((t.id, *mass), (0.0, 0u8)))
            })
            .collect();
        let mut frontier: Vec<(TerritoryId, u32)> = stand.keys().copied().collect();
        while !frontier.is_empty() {
            // A few hundred territories: a linear scan for the cheapest beats
            // a heap, and ties break on id so the result is deterministic.
            let (index, current) = frontier
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let ca = stand.get(*a).map(|(c, _)| *c).unwrap_or(f64::INFINITY);
                    let cb = stand.get(*b).map(|(c, _)| *c).unwrap_or(f64::INFINITY);
                    ca.partial_cmp(&cb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.cmp(b))
                })
                .map(|(i, id)| (i, *id))
                .expect("frontier is not empty");
            frontier.swap_remove(index);
            let (cost, rivals) = stand[&current];
            let (here, mass) = current;
            let Some(territory) = map.territory(here) else {
                continue;
            };
            // Only borders on the landmass being stood on: a border you can
            // see across an ocean is not a border you can walk over.
            for (neighbour, link_mass) in territory.land_links.iter().filter(|(_, m)| *m == mass) {
                let (transit, rival) = match state.ownership.get(neighbour).copied() {
                    Some(owner) if owner == actor => (0.0, 0),
                    // A rival's ground is dear, and their walls dearer: a
                    // fortified pass is expensive to go through and worth
                    // going around.
                    Some(_) => (
                        rs.enemy_transit_cost + state.fort_transit(*neighbour, day),
                        1,
                    ),
                    None => (rs.free_transit_cost, 0),
                };
                let next = (cost + transit, rivals.saturating_add(rival));
                let key = (*neighbour, *link_mass);
                if stand
                    .get(&key)
                    .is_none_or(|(known, _)| next.0 + f64::EPSILON < *known)
                {
                    stand.insert(key, next);
                    frontier.push(key);
                }
            }
        }

        let launch_points = if state.option(super::rulesets::OPTION_LANDLOCKED_SEA) {
            holdings.clone()
        } else {
            holdings
                .iter()
                .copied()
                .filter(|id| map.territory(*id).is_some_and(|t| t.coastal))
                .collect()
        };
        Self {
            stand,
            launch_points,
            holdings,
        }
    }

    /// What this target costs, and how the route got there: the cheaper of
    /// marching through whoever is in the way, or sailing to it.
    pub fn reach(
        &self,
        map: &WorldMap,
        ruleset: &RealmRulesetSnapshot,
        target: TerritoryId,
    ) -> Reach {
        // Arriving somewhere means standing next to it first, so the land
        // cost is the cheapest neighbour to have reached. The target itself
        // is never charged — you are taking it, not passing through it.
        let overland = map.territory(target).and_then(|t| {
            t.land_links
                .iter()
                .filter_map(|(n, mass)| self.stand.get(&(*n, *mass)))
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(cost, rivals)| Reach {
                    cost: 1.0 + cost,
                    mode: RouteMode::Land {
                        through_rivals: *rivals,
                    },
                })
        });
        // A landing needs a shore: an inland country cannot be taken off a
        // boat, and (unless the game allows it) an inland player has no
        // fleet to take it with.
        let by_sea = map
            .territory(target)
            .filter(|t| t.coastal)
            .and_then(|_| {
                let km = self
                    .launch_points
                    .iter()
                    .map(|from| map.distance_km(*from, target))
                    .fold(f64::INFINITY, f64::min);
                km.is_finite().then_some(km)
            })
            .map(|km| Reach {
                cost: 1.0 + km / ruleset.sea_km_per_cost.max(1.0),
                mode: RouteMode::Sea { km },
            });

        match (overland, by_sea) {
            (Some(land), Some(sea)) if sea.cost < land.cost => sea,
            (Some(land), _) => land,
            (None, Some(sea)) => sea,
            // Nothing held anywhere: a player with no land is not boxed in,
            // they have not started. One plain step, so a first claim is
            // possible; this is the only case the old fallback was for.
            (None, None) if self.holdings.is_empty() => Reach::ADJACENT,
            // No land route and no crossing: another landmass, and either it
            // has no shore to land on or this player has no shore to sail
            // from. Not a cheap move — not a move.
            (None, None) => Reach::UNREACHABLE,
        }
    }

    /// Does this player hold anything at all? A fresh spawn always does.
    pub fn has_holdings(&self) -> bool {
        !self.holdings.is_empty()
    }
}

/// Take one point off this player's day, opening the day if this is their
/// first action on it. Every action pays the same, whether it takes ground
/// or digs into it.
fn spend_point(state: &mut RealmGameState, actor: Uuid, day: i32) {
    // Today's allowance is the base plus whatever missed days banked, read
    // before acting closes the gap it was computed from.
    let allowance = state.standing(actor, day).allowance;
    if let Some(p) = state.player_mut(actor) {
        if p.actions_day != day {
            p.actions_day = day;
            p.actions_used = 0;
            p.actions_allowance = allowance;
        }
        p.actions_used = p.actions_used.saturating_add(1).min(allowance);
        // A day you acted on is a day you played — the clock that gates
        // winning and being paid.
        if p.last_action_day != Some(day) {
            p.active_days = p.active_days.saturating_add(1);
            p.last_action_day = Some(day);
        }
    }
}

/// Is anything at all inside `cap` for this player? What makes a
/// short-reach game playable rather than a trap: when the answer is no, the
/// cap stops applying.
pub fn anything_within(
    state: &RealmGameState,
    map: &WorldMap,
    index: &ReachIndex,
    actor: Uuid,
    cap: f64,
) -> bool {
    map.territories
        .iter()
        .filter(|t| state.ownership.get(&t.id) != Some(&actor))
        .any(|t| index.reach(map, &state.ruleset, t.id).cost <= cap)
}

/// Reach of a target for a player, building the index for one lookup.
/// Callers pricing many targets should build a [`ReachIndex`] instead.
pub fn reach_for(
    state: &RealmGameState,
    map: &WorldMap,
    actor: Uuid,
    target: TerritoryId,
) -> Reach {
    ReachIndex::build(state, map, actor).reach(map, &state.ruleset, target)
}

/// The odds a player is shown, and the odds the roll uses — one function, so
/// the projection on screen is the real thing (it can still go stale the
/// instant someone else acts).
pub fn projected_probability(
    state: &RealmGameState,
    map: &WorldMap,
    actor: Uuid,
    target: TerritoryId,
    reach: Reach,
    day: i32,
) -> Option<f64> {
    let territory = map.territory(target)?;
    let owner = state.ownership.get(&target).copied();
    if owner == Some(actor) {
        return None;
    }
    // No route means no move, and a percentage next to a move that cannot be
    // made is worse than nothing: it was this that made another continent
    // look like a good bet.
    if reach.is_unreachable() {
        return None;
    }
    let kind = match owner {
        None => ProbabilityKind::Claim,
        Some(_) => ProbabilityKind::Attack,
    };
    let attacker = state.strength(actor, map);
    let defender = owner
        .map(|o| state.effective_strength(o, map, day))
        .unwrap_or(0.0);
    Some(action_probability(
        &state.ruleset,
        kind,
        territory.weight,
        attacker,
        defender,
        reach,
        state.fortification(target, day),
    ))
}

/// Take one action, right now. Spends a point, rolls, and applies the result.
///
/// `day` is the current realm day (see `svc::realm_day`), `seed` the action
/// seed. Returns `Err` when the action is refused — nothing is spent and the
/// state is untouched, so the caller just tells the player why.
pub fn apply_action(
    state: &mut RealmGameState,
    map: &WorldMap,
    actor: Uuid,
    action: RealmAction,
    day: i32,
    seed: u64,
) -> Result<ActionOutcome, ActionRejected> {
    if !state.is_alive(actor) {
        return Err(ActionRejected::NotPlaying);
    }
    if state.points_left(actor, day) == 0 {
        return Err(ActionRejected::NoPointsLeft);
    }
    let target = action.target();
    let Some(territory) = map.territory(target) else {
        return Err(ActionRejected::UnknownTerritory);
    };
    let owner = state.ownership.get(&target).copied();

    // Digging in is its own thing: no route to price, no roll to make. It
    // costs the same point as an attack, which is the whole balance of it —
    // a day spent on walls is a day not spent taking ground.
    if let RealmAction::Fortify { .. } = action {
        if !state.option(super::rulesets::OPTION_FORTIFY) {
            return Err(ActionRejected::NoFortifying);
        }
        if owner != Some(actor) {
            return Err(ActionRejected::NotYours);
        }
        let level = state.fort_level(target);
        if level >= state.ruleset.fort_max_level {
            return Err(ActionRejected::FullyFortified(level));
        }
        let raised = level + 1;
        spend_point(state, actor, day);
        state.forts.insert(target, raised);
        let points_left = state.points_left(actor, day);
        let day_seed = day_seed_of(state, day);
        let entry = LogEntry::Fortified {
            user_id: actor,
            target,
            level: raised,
        };
        state.log(day, day_seed, entry.clone());
        state.revision += 1;
        return Ok(ActionOutcome {
            entry,
            probability: 1.0,
            success: true,
            reach: Reach::ADJACENT,
            points_left,
            end: ResolveEnd::Ongoing,
        });
    }

    let kind = match action {
        RealmAction::Claim { .. } => match owner {
            None => ProbabilityKind::Claim,
            Some(o) if o == actor => return Err(ActionRejected::AlreadyYours),
            Some(_) => return Err(ActionRejected::TargetOwned),
        },
        RealmAction::Attack { .. } => match owner {
            None => return Err(ActionRejected::TargetFree),
            Some(o) if o == actor => return Err(ActionRejected::AlreadyYours),
            Some(_) => ProbabilityKind::Attack,
        },
        RealmAction::Fortify { .. } => unreachable!("handled above"),
    };

    let index = ReachIndex::build(state, map, actor);
    let reach = index.reach(map, &state.ruleset, target);
    if reach.is_unreachable() {
        return Err(ActionRejected::NoRoute);
    }
    // A game that keeps war to its own neighbourhood still has to leave
    // everyone a move: the cap lifts for a player who has nothing at all in
    // range, so nobody is boxed into passing forever.
    if let Some(cap) = state.reach_cap()
        && reach.cost > cap
        && anything_within(state, map, &index, actor, cap)
    {
        return Err(ActionRejected::OutOfRange);
    }
    if kind == ProbabilityKind::Attack {
        let defender = owner.expect("an attack has a defender");
        state.attack_allowed(actor, defender, reach, day)?;
    }
    let probability = action_probability(
        &state.ruleset,
        kind,
        territory.weight,
        state.strength(actor, map),
        owner
            .map(|o| state.effective_strength(o, map, day))
            .unwrap_or(0.0),
        reach,
        state.fortification(target, day),
    );
    let mut rng = SplitMix64::new(seed);
    let success = rng.next_f64() < probability;

    // Spend the point first: a failed roll costs one all the same.
    spend_point(state, actor, day);
    let points_left = state.points_left(actor, day);
    let day_seed = day_seed_of(state, day);

    let entry = match kind {
        ProbabilityKind::Claim => {
            if success {
                state.ownership.insert(target, actor);
                // Whatever stood here is rubble now.
                state.forts.remove(&target);
            }
            LogEntry::Claimed {
                user_id: actor,
                target,
                probability,
                success,
                hops: reach.cost.round() as u32,
                by_sea: reach.by_sea(),
            }
        }
        ProbabilityKind::Attack => {
            let defender = owner.expect("attack has a defender");
            if success {
                state.ownership.insert(target, actor);
                state.forts.remove(&target);
            }
            LogEntry::Attacked {
                user_id: actor,
                defender_id: defender,
                target,
                probability,
                success,
                hops: reach.cost.round() as u32,
                by_sea: reach.by_sea(),
            }
        }
    };
    state.log(day, day_seed, entry.clone());

    // An attack that took someone's last territory knocks them out.
    if let LogEntry::Attacked {
        defender_id,
        success: true,
        ..
    } = entry
        && state.territory_count(defender_id) == 0
        && state.is_alive(defender_id)
    {
        state.exit_player(defender_id, RealmPlayerStatus::Eliminated, day);
        if let Some(p) = state.player_mut(defender_id) {
            p.exit_territories = 1;
        }
        state.log(
            day,
            day_seed,
            LogEntry::Eliminated {
                user_id: defender_id,
                by_user_id: actor,
            },
        );
    }

    let end = check_end(state, map, day, day_seed);
    state.revision += 1;
    Ok(ActionOutcome {
        entry,
        probability,
        success,
        reach,
        points_left,
        end,
    })
}

/// Days of total silence — nobody alive having spent a single action — after
/// which a game dissolves itself. Deliberately flat rather than scaled off
/// the ruleset or the pace: what this measures is whether anybody still
/// comes back, and a week away from a terminal is a week away whether the
/// realm was a blitz or a slow burn. Five days is long enough to cover a
/// holiday nobody announced and short enough that a dead realm is not still
/// in the Lobby when the people who started it have forgotten it.
pub const REALM_ABANDON_DAYS: i32 = 5;

/// Per-day housekeeping. Nobody is removed from a running game any more: a
/// quiet player keeps their land, it simply gets easier to take as their
/// empire decays (`decay_multiplier`). What this still does is notice a game
/// that everyone has abandoned, so dead rows do not sit in the Lobby forever
/// — and that path pays nothing, so it cannot be farmed.
pub fn sweep_idle(state: &mut RealmGameState, map: &WorldMap, day: i32) -> (Vec<Uuid>, ResolveEnd) {
    let day_seed = day_seed_of(state, day);
    // Long-abandoned: every living player quiet for `REALM_ABANDON_DAYS`.
    // A player who never acted at all counts from the day they joined, so a
    // realm nobody ever played clears itself the same way.
    let abandon_days = REALM_ABANDON_DAYS;
    let living: Vec<&RealmPlayer> = state
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
        .collect();
    let abandoned = !living.is_empty() && living.iter().all(|p| p.quiet_days(day) >= abandon_days);
    if abandoned {
        let ids: Vec<Uuid> = living.iter().map(|p| p.user_id).collect();
        for user_id in &ids {
            let freed = state.exit_player(*user_id, RealmPlayerStatus::Kicked, day);
            state.log(
                day,
                day_seed,
                LogEntry::Kicked {
                    user_id: *user_id,
                    freed_territories: freed.len() as u16,
                    freed,
                },
            );
        }
        state.revision += 1;
        return (ids, ResolveEnd::Dissolved);
    }
    (Vec::new(), check_end(state, map, day, day_seed))
}

/// Has anyone won? Two ways, and they are the same idea from either end:
/// nobody is left to fight, or there is nothing left to take.
///
/// **Every rival conquered** ends it. Unclaimed land does not keep a realm
/// open — mopping up empty ground nobody is contesting is bookkeeping, not
/// conquest, and a war that is decided should not need another fortnight of
/// clicking to say so.
///
/// **Holding every territory** ends it too, which is the same finish reached
/// from the other side: if one player owns the map then everyone else owns
/// nothing, and they were eliminated on the way.
///
/// Either way the winner must have played `min_active_days_to_win` distinct
/// days. Eliminating everyone in an afternoon does not end the game; it
/// leaves you holding the field until the week is in.
fn check_end(state: &mut RealmGameState, map: &WorldMap, day: i32, seed: u64) -> ResolveEnd {
    let alive: Vec<Uuid> = state
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
        .map(|p| p.user_id)
        .collect();
    if alive.is_empty() {
        return ResolveEnd::Dissolved;
    }
    let holds_everything = alive
        .iter()
        .copied()
        .find(|u| state.territory_count(*u) as usize == map.territories.len());
    let winner = match (alive.len(), holds_everything) {
        (_, Some(u)) => u,
        // The last one standing: every rival is off the board, so the war is
        // over whatever is left lying about unclaimed.
        (1, None) => alive[0],
        _ => return ResolveEnd::Ongoing,
    };
    if !state.can_win(winner) {
        // Won on the field, but not enough days played. A realm swept in an
        // afternoon with an obliging friend is not a conquest, so it stays
        // open and theirs to hold until the days are in.
        return ResolveEnd::Ongoing;
    }
    // Don't log the same win twice if a sweep re-checks a finished game.
    let already = state
        .last_day
        .as_ref()
        .is_some_and(|d| d.entries.iter().any(|e| matches!(e, LogEntry::Won { .. })));
    if !already {
        state.log(day, seed, LogEntry::Won { user_id: winner });
    }
    ResolveEnd::Won(winner)
}

/// The seed stamped on the current day's log (kept stable across a day).
fn day_seed_of(state: &RealmGameState, day: i32) -> u64 {
    state
        .last_day
        .as_ref()
        .filter(|d| d.day == day)
        .map(|d| d.seed)
        .unwrap_or(0)
}

/// Final standings for payouts: winner first, then by exit day descending
/// (later exit = better), then exit territory count, then seeded rng as the
/// last tie-break. Players still alive at finish (e.g. game dissolved) rank
/// by current holdings ahead of exited players.
pub fn final_ranking(state: &RealmGameState, winner: Option<Uuid>, seed: u64) -> Vec<Uuid> {
    let mut rng = SplitMix64::new(seed ^ 0x52414E4B5F53414C); // "RANK_SAL"
    let mut rest: Vec<&RealmPlayer> = state
        .players
        .iter()
        .filter(|p| Some(p.user_id) != winner)
        .collect();
    let mut jitter: BTreeMap<Uuid, u64> = BTreeMap::new();
    for p in &rest {
        jitter.insert(p.user_id, rng.next_u64());
    }
    rest.sort_by(|a, b| {
        let a_alive = a.status == RealmPlayerStatus::Alive;
        let b_alive = b.status == RealmPlayerStatus::Alive;
        b_alive
            .cmp(&a_alive)
            .then_with(|| {
                let a_key = if a_alive {
                    (i32::MAX, state.territory_count(a.user_id))
                } else {
                    (a.exit_day.unwrap_or(i32::MIN), a.exit_territories)
                };
                let b_key = if b_alive {
                    (i32::MAX, state.territory_count(b.user_id))
                } else {
                    (b.exit_day.unwrap_or(i32::MIN), b.exit_territories)
                };
                b_key.cmp(&a_key)
            })
            .then_with(|| jitter[&b.user_id].cmp(&jitter[&a.user_id]))
    });
    winner
        .into_iter()
        .chain(rest.into_iter().map(|p| p.user_id))
        .collect()
}

#[cfg(test)]
#[path = "resolver_test.rs"]
mod resolver_test;

#[cfg(test)]
#[path = "simulation_test.rs"]
mod simulation_test;
