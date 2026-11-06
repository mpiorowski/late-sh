//! The orders engine (`Resolution::Orders`): orders are written during the
//! day and resolved together at the night, when the day turns over. Pure:
//! no db, no tokio, no clock. `RULES.md` "Orders" is the rulebook this
//! implements; every number in it is a ruleset field, none a literal here.
//!
//! The board and the night price a take with the same function
//! (`projection` over `project`), so the odds a player is shown are the
//! odds rolled, up to what the board could not see: walls others raised
//! tonight, and neighbours others engaged.

use std::collections::{BTreeMap, BTreeSet};

use uuid::Uuid;

use super::map::{Territory, TerritoryId, WorldMap};
use super::resolver::{
    FizzleReason, LogEntry, Order, OrderRejected, PlayerPhase, RealmAction, RealmGameState,
    RealmPlayer, RealmPlayerStatus, ResolveEnd, SplitMix64, TakerRoll, WinKind, final_ranking,
    spend_points,
};
use super::rulesets::{OPTION_FORTIFY, OPTION_LANDLOCKED_SEA, RealmRulesetSnapshot};

/// What a take is, as the board stands: free land or somebody's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeKind {
    Claim,
    Attack,
}

/// Everything about one take's odds, for the board to explain and the
/// night to roll.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Projection {
    pub kind: TakeKind,
    pub force: f64,
    pub defence: f64,
    pub probability: f64,
    /// The wall levels of the taker's land behind the take, a sea launch
    /// counted at `sea_front`.
    pub front: f64,
    /// No land front at all: the take comes off the sea.
    pub landing: bool,
    /// Every land neighbour of the target is the taker's.
    pub encircled: bool,
    /// The taker's strongest front territory, where the take comes from.
    pub from: TerritoryId,
}

/// A placed order's answer: the take's odds as the board now stands (none
/// for a fortify), and what is left to order today.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub projection: Option<Projection>,
    pub points_left: u8,
}

/// What a night did, counted for the service's telemetry. The entries
/// themselves are on the day's log and on `RealmGameState::last_night`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NightReport {
    /// Targets that changed hands.
    pub taken: u32,
    /// Targets every take on failed.
    pub held: u32,
    /// Targets whose strongest successful takes were level.
    pub bounced: u32,
    /// Takes no longer legal when the night came.
    pub fizzled: u32,
    pub end: ResolveEnd,
}

/// What tonight looks like to whoever is asking: walls ordered tonight, at
/// the level they go up to, and the territories a take is aimed at.
struct Tonight {
    walls: BTreeMap<TerritoryId, u8>,
    engaged: BTreeSet<TerritoryId>,
}

impl Tonight {
    /// What a player's own board can know: their own orders, nobody else's.
    fn seen_by(state: &RealmGameState, actor: Uuid) -> Self {
        let mut tonight = Self {
            walls: BTreeMap::new(),
            engaged: BTreeSet::new(),
        };
        let Some(player) = state.player(actor) else {
            return tonight;
        };
        for order in &player.orders {
            match *order {
                Order::Take { target, .. } => {
                    tonight.engaged.insert(target);
                }
                Order::Fortify { target, levels } => {
                    tonight
                        .walls
                        .insert(target, state.fort_level(target) + levels.get());
                }
            }
        }
        tonight
    }
}

/// Who is behind a take on a target: the taker's land that borders it, and
/// their shores within sailing reach of it. Each holding counts once, by
/// land when it borders the target.
struct Front {
    land: Vec<TerritoryId>,
    sea: Vec<TerritoryId>,
}

impl Front {
    fn of(
        state: &RealmGameState,
        map: &WorldMap,
        actor: Uuid,
        target: TerritoryId,
    ) -> Option<Self> {
        let territory = map.territory(target)?;
        // A border helps only on the landmass it is on, and the taker stands
        // on every landmass their holding has ground on, so a held end of a
        // land link is a land front.
        let mut land: Vec<TerritoryId> = territory
            .land_links
            .iter()
            .map(|(neighbour, _)| *neighbour)
            .filter(|neighbour| state.ownership.get(neighbour) == Some(&actor))
            .collect();
        land.sort_unstable();
        land.dedup();
        // A landing needs a shore to land on and, unless the game lets
        // inland players sail, a shore to sail from.
        let any_launch = state.option(OPTION_LANDLOCKED_SEA);
        let sea: Vec<TerritoryId> = if territory.coastal {
            state
                .holdings(actor)
                .into_iter()
                .filter(|from| !land.contains(from))
                .filter(|from| any_launch || map.territory(*from).is_some_and(|t| t.coastal))
                .filter(|from| map.distance_km(*from, target) <= state.ruleset.sea_reach_km)
                .collect()
        } else {
            Vec::new()
        };
        (!land.is_empty() || !sea.is_empty()).then_some(Self { land, sea })
    }
}

/// A territory's walls as they stand tonight: each level past the first
/// adds `garrison_per_wall`, with the holder's dormancy walking them back
/// towards 1. Free land has walls 1 and no decay.
fn walls_eff(state: &RealmGameState, tonight: &Tonight, territory: TerritoryId, day: i32) -> f64 {
    let level = tonight
        .walls
        .get(&territory)
        .copied()
        .unwrap_or_else(|| state.fort_level(territory));
    let decay = state
        .ownership
        .get(&territory)
        .map(|holder| state.decay_multiplier(*holder, day))
        .unwrap_or(1.0);
    1.0 + f64::from(level - 1) * state.ruleset.garrison_per_wall * decay
}

/// What holds a territory: its walls, and more of them the bigger it is.
pub fn garrison(rs: &RealmRulesetSnapshot, walls_eff: f64, weight: f64) -> f64 {
    walls_eff * (1.0 + rs.garrison_weight * weight)
}

/// What a take hits with: its points, multiplied by the land behind it.
pub fn force(
    rs: &RealmRulesetSnapshot,
    points: u8,
    front: f64,
    landing: bool,
    encircled: bool,
) -> f64 {
    let landing = if landing { rs.landing_factor } else { 1.0 };
    let encircle = if encircled { rs.encircle_factor } else { 1.0 };
    f64::from(points) * (1.0 + rs.front_factor * front) * landing * encircle
}

/// The odds of a take: `force^k / (force^k + defence^k)`, inside the
/// ruleset's floor and ceiling. The one place a fight is decided; the
/// deterministic variant is `force > defence` here.
pub fn odds(rs: &RealmRulesetSnapshot, force: f64, defence: f64) -> f64 {
    let a = force.powf(rs.contest_exponent);
    let d = defence.powf(rs.contest_exponent);
    (a / (a + d)).clamp(rs.prob_floor, rs.prob_ceil)
}

/// What stands in a take's way tonight: the garrison, and for held land
/// half (`support_factor`) of every neighbouring garrison of the holder's
/// that no take is aimed at. Free land defends at `neutral_factor` of its
/// garrison, alone.
fn defence(
    state: &RealmGameState,
    map: &WorldMap,
    tonight: &Tonight,
    target: TerritoryId,
    day: i32,
) -> f64 {
    let rs = &state.ruleset;
    let territory = map
        .territory(target)
        .expect("a priced target is on the map");
    let own = garrison(rs, walls_eff(state, tonight, target, day), territory.weight);
    let Some(holder) = state.ownership.get(&target).copied() else {
        return own * rs.neutral_factor;
    };
    let mut supports: Vec<TerritoryId> = territory
        .land_links
        .iter()
        .map(|(neighbour, _)| *neighbour)
        .filter(|neighbour| state.ownership.get(neighbour) == Some(&holder))
        .filter(|neighbour| !tonight.engaged.contains(neighbour))
        .collect();
    supports.sort_unstable();
    supports.dedup();
    let support: f64 = supports
        .iter()
        .filter_map(|n| map.territory(*n))
        .map(|n| garrison(rs, walls_eff(state, tonight, n.id, day), n.weight))
        .sum();
    own + rs.support_factor * support
}

/// A take priced on a given night. `None` when the target is the taker's
/// own, off the map, or out of reach.
fn project(
    state: &RealmGameState,
    map: &WorldMap,
    tonight: &Tonight,
    actor: Uuid,
    target: TerritoryId,
    points: u8,
    day: i32,
) -> Option<Projection> {
    let rs = &state.ruleset;
    let territory = map.territory(target)?;
    let kind = match state.ownership.get(&target) {
        None => TakeKind::Claim,
        Some(holder) if *holder == actor => return None,
        Some(_) => TakeKind::Attack,
    };
    let front = Front::of(state, map, actor, target)?;
    let strongest = |ids: &[TerritoryId]| {
        ids.iter()
            .copied()
            .map(|id| (id, walls_eff(state, tonight, id, day)))
            .fold(
                None,
                |best: Option<(TerritoryId, f64)>, (id, walls)| match best {
                    Some((_, top)) if top >= walls => best,
                    _ => Some((id, walls)),
                },
            )
            .map(|(id, _)| id)
    };
    let land: f64 = front
        .land
        .iter()
        .map(|id| walls_eff(state, tonight, *id, day))
        .sum();
    let sea: f64 = front
        .sea
        .iter()
        .map(|id| walls_eff(state, tonight, *id, day))
        .sum();
    let total = land + rs.sea_front * sea;
    let landing = front.land.is_empty();
    let mut neighbours: Vec<TerritoryId> = territory.land_links.iter().map(|(n, _)| *n).collect();
    neighbours.sort_unstable();
    neighbours.dedup();
    let encircled = !neighbours.is_empty()
        && neighbours
            .iter()
            .all(|n| state.ownership.get(n) == Some(&actor));
    let force = force(rs, points, total, landing, encircled);
    let defence = defence(state, map, tonight, target, day);
    let from = if landing {
        strongest(&front.sea)
    } else {
        strongest(&front.land)
    }
    .expect("a front has a territory in it");
    Some(Projection {
        kind,
        force,
        defence,
        probability: odds(rs, force, defence),
        front: total,
        landing,
        encircled,
        from,
    })
}

/// The odds the board shows for `points` on `target`: tonight as `actor`
/// can know it, their own orders counted in (walls they ordered go up first,
/// targets they picked give no support). The night prices with the same
/// function over everyone's orders. `None` when the target is theirs, off
/// the map, or out of reach.
pub fn projection(
    state: &RealmGameState,
    map: &WorldMap,
    actor: Uuid,
    target: TerritoryId,
    points: u8,
    day: i32,
) -> Option<Projection> {
    project(
        state,
        map,
        &Tonight::seen_by(state, actor),
        actor,
        target,
        points,
        day,
    )
}

/// Is this take allowed: not on your own land, a front behind it, and
/// protection in neither direction. Checked when it is placed and again at
/// the night, on the board as the night found it; the board's target table
/// marks with it what cannot be taken today.
pub fn check_take(
    state: &RealmGameState,
    map: &WorldMap,
    actor: Uuid,
    target: TerritoryId,
    day: i32,
) -> Result<(), OrderRejected> {
    let holder = state.ownership.get(&target).copied();
    if holder == Some(actor) {
        return Err(OrderRejected::AlreadyYours);
    }
    let Some(front) = Front::of(state, map, actor, target) else {
        return Err(OrderRejected::NoFront);
    };
    let Some(holder) = holder else {
        // Free land is open to anyone with a front, a newcomer included:
        // taking it is what their first days are for.
        return Ok(());
    };
    let by_land = !front.land.is_empty();
    match state.phase(actor, day) {
        PlayerPhase::New => {
            return Err(OrderRejected::AttacksLocked(state.days_until(
                actor,
                day,
                PlayerPhase::Rampup,
            )));
        }
        PlayerPhase::Rampup if !by_land => {
            return Err(OrderRejected::LandingsLocked(state.days_until(
                actor,
                day,
                PlayerPhase::Involved,
            )));
        }
        PlayerPhase::Rampup | PlayerPhase::Involved => {}
    }
    match state.phase(holder, day) {
        PlayerPhase::New => Err(OrderRejected::TargetIsNew(state.days_until(
            holder,
            day,
            PlayerPhase::Rampup,
        ))),
        PlayerPhase::Rampup if !by_land => Err(OrderRejected::TargetIsRamping(state.days_until(
            holder,
            day,
            PlayerPhase::Involved,
        ))),
        PlayerPhase::Rampup | PlayerPhase::Involved => Ok(()),
    }
}

/// Place an order for tonight, or replace this player's order on the same
/// target. Refused for free when it could not be carried out; a placed
/// order spends its points now, and the first order of the day freezes the
/// day's allowance the way the first action does.
pub fn place_order(
    state: &mut RealmGameState,
    map: &WorldMap,
    actor: Uuid,
    order: Order,
    day: i32,
) -> Result<Placed, OrderRejected> {
    let Some(player) = state
        .player(actor)
        .filter(|p| p.status == RealmPlayerStatus::Alive)
    else {
        return Err(OrderRejected::NotPlaying);
    };
    let target = order.target();
    let replaced = replaced_cost(player, target);
    if map.territory(target).is_none() {
        return Err(OrderRejected::UnknownTerritory);
    }
    match order {
        Order::Take { .. } => check_take(state, map, actor, target, day)?,
        Order::Fortify { levels, .. } => {
            if !state.option(OPTION_FORTIFY) {
                return Err(OrderRejected::NoFortifying);
            }
            if state.ownership.get(&target) != Some(&actor) {
                return Err(OrderRejected::NotYours);
            }
            if levels.get() > wall_room(state, target) {
                return Err(OrderRejected::FullyFortified {
                    level: state.fort_level(target),
                    max: state.ruleset.fort_max_level,
                });
            }
        }
    }
    let left = state.points_left(actor, day).saturating_add(replaced);
    if order.cost() > left {
        return Err(OrderRejected::NotEnoughPoints { left });
    }

    refund_points(state, actor, replaced);
    spend_points(state, actor, day, order.cost());
    let player = state.player_mut(actor).expect("checked alive above");
    player.orders.retain(|o| o.target() != target);
    player.orders.push(order);
    state.revision += 1;
    let projection = match order {
        Order::Take { points, .. } => projection(state, map, actor, target, points.get(), day),
        Order::Fortify { .. } => None,
    };
    Ok(Placed {
        projection,
        points_left: state.points_left(actor, day),
    })
}

/// What an order can be sized to and what placing it would leave, counted
/// exactly as `place_order` counts it: today's points with those of the
/// order it replaces (the one already standing on its target) counted back
/// in, and for walls no further than the ceiling. The prompt and the orders
/// rail read this, so the sizes offered are the sizes the night accepts.
/// It does not say whether the order stands at all; `place_order` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderQuote {
    /// The most the order can carry. Zero when nothing is left.
    pub max: u8,
    /// What placing the order as it is sized would leave for tonight.
    pub left_after: u8,
}

pub fn order_quote(state: &RealmGameState, actor: Uuid, order: Order, day: i32) -> OrderQuote {
    let target = order.target();
    let replaced = state
        .player(actor)
        .map(|player| replaced_cost(player, target))
        .unwrap_or(0);
    let left = state.points_left(actor, day).saturating_add(replaced);
    let max = match order {
        Order::Take { .. } => left,
        Order::Fortify { .. } => left.min(wall_room(state, target)),
    };
    OrderQuote {
        max,
        left_after: left.saturating_sub(order.cost()),
    }
}

/// What the player's standing order on `target` cost, counted back in when
/// another replaces it. Zero without one.
fn replaced_cost(player: &RealmPlayer, target: TerritoryId) -> u8 {
    player
        .orders
        .iter()
        .find(|o| o.target() == target)
        .map(Order::cost)
        .unwrap_or(0)
}

/// How many levels the walls on `target` can still go up.
fn wall_room(state: &RealmGameState, target: TerritoryId) -> u8 {
    state
        .ruleset
        .fort_max_level
        .saturating_sub(state.fort_level(target))
}

/// Take an order back, and its points with it. The day stays played: it
/// was the placing that counted.
pub fn withdraw_order(
    state: &mut RealmGameState,
    actor: Uuid,
    target: TerritoryId,
    day: i32,
) -> Result<u8, OrderRejected> {
    let Some(player) = state
        .player(actor)
        .filter(|p| p.status == RealmPlayerStatus::Alive)
    else {
        return Err(OrderRejected::NotPlaying);
    };
    let Some(order) = player.orders.iter().find(|o| o.target() == target).copied() else {
        return Err(OrderRejected::NoSuchOrder);
    };
    refund_points(state, actor, order.cost());
    state
        .player_mut(actor)
        .expect("checked alive above")
        .orders
        .retain(|o| o.target() != target);
    state.revision += 1;
    Ok(state.points_left(actor, day))
}

/// Hand points back to today's budget. Orders are drained by the night that
/// resolves them, so any order still standing was paid for today.
fn refund_points(state: &mut RealmGameState, actor: Uuid, points: u8) {
    if let Some(player) = state.player_mut(actor) {
        player.actions_used = player.actions_used.saturating_sub(points);
    }
}

/// A player's share of the world: the land they hold over the whole map,
/// free land included, each territory counting `1 + weight`, so a giant is
/// worth twice a speck and no more. What the standings sort by, what
/// domination is measured in, and what the clock decides on. Raw population
/// or area would hand the world to whoever claimed the four biggest
/// countries first; the log weight is the same size the garrison is built
/// from, so a territory is worth about what it costs.
pub fn share(state: &RealmGameState, map: &WorldMap, user_id: Uuid) -> f64 {
    let mass = |t: &Territory| 1.0 + t.weight;
    let world: f64 = map.territories.iter().map(mass).sum();
    let held: f64 = state
        .ownership
        .iter()
        .filter(|(_, owner)| **owner == user_id)
        .filter_map(|(id, _)| map.territory(*id))
        .map(mass)
        .sum();
    match world {
        0.0 => 0.0,
        world => held / world,
    }
}

/// The last day of the season, whose night ends the game: the muster's day
/// is `start_day`, so day `start_day + 1` is the first one played and the
/// clock counts `season_days` of them. `None` for a ruleset with no clock.
pub fn season_last_day(state: &RealmGameState) -> Option<i32> {
    match state.ruleset.season_days {
        0 => None,
        days => Some(state.start_day + i32::from(days)),
    }
}

/// Where `day` falls on the clock, counted from 1: the "12" of "day 12 of
/// 28".
pub fn season_day(state: &RealmGameState, day: i32) -> i32 {
    (day - state.start_day).max(1)
}

/// One take on its way to the night: who, where, with what, and what their
/// own board showed for it.
struct Take {
    actor: Uuid,
    target: TerritoryId,
    points: u8,
    shown: f64,
}

/// A per-target roll stream: `blake3(day_seed ‖ target)`, so one more fight
/// elsewhere never changes this fight's rolls.
fn target_seed(seed: u64, target: TerritoryId) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&seed.to_le_bytes());
    hasher.update(&target.to_le_bytes());
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 >= 8 bytes"))
}

/// Resolve the night that closes the open day (`last_day`), with that day's
/// seed: walls go up, every take is re-checked, priced and rolled on the
/// board as the night found it, the fights land, the fallen fall, and the
/// end is checked once. Everything it writes goes on the day's log and, on
/// its own, onto `last_night`. Every player's orders are drained.
pub fn resolve_night(state: &mut RealmGameState, map: &WorldMap) -> NightReport {
    let open = state
        .last_day
        .as_ref()
        .expect("an orders game always has an open day");
    let (day, seed, logged_before) = (open.day, open.seed, open.entries.len());

    // 1. Every alive player's orders, in roster order. A take is re-checked
    // on the opening board and, if it still stands, priced the way its own
    // board priced it, for the dispatch to set beside the real odds.
    let mut takes: Vec<Take> = Vec::new();
    let mut fortifies: Vec<(Uuid, TerritoryId, u8)> = Vec::new();
    let mut fizzles: Vec<LogEntry> = Vec::new();
    for player in state
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
    {
        for order in &player.orders {
            match *order {
                Order::Fortify { target, levels } => {
                    fortifies.push((player.user_id, target, levels.get()));
                }
                Order::Take { target, points } => {
                    match check_take(state, map, player.user_id, target, day) {
                        Ok(()) => takes.push(Take {
                            actor: player.user_id,
                            target,
                            points: points.get(),
                            shown: projection(
                                state,
                                map,
                                player.user_id,
                                target,
                                points.get(),
                                day,
                            )
                            .expect("a legal take is priced")
                            .probability,
                        }),
                        Err(rejected) => fizzles.push(LogEntry::Fizzled {
                            user_id: player.user_id,
                            action: match state.ownership.get(&target) {
                                None => RealmAction::Claim { target },
                                Some(_) => RealmAction::Attack { target },
                            },
                            reason: fizzle_reason(rejected),
                        }),
                    }
                }
            }
        }
    }
    for player in state.players.iter_mut() {
        player.orders.clear();
    }

    // 2. Walls go up first, so a wall ordered today holds tonight.
    for (user_id, target, levels) in fortifies {
        let level = state.fort_level(target) + levels;
        state.forts.insert(target, level);
        state.log(
            day,
            seed,
            LogEntry::Fortified {
                user_id,
                target,
                level,
            },
        );
    }
    let fizzled = fizzles.len() as u32;
    for entry in fizzles {
        state.log(day, seed, entry);
    }

    // 3. Engagement: a territory some take is aimed at lends no support.
    let tonight = Tonight {
        walls: BTreeMap::new(),
        engaged: takes.iter().map(|t| t.target).collect(),
    };

    // 4. Every fight is priced and rolled before any lands, so nothing
    // decided tonight moves another fight's numbers. Targets go in a seeded
    // order, which only decides who is named for a fall.
    let mut by_target: BTreeMap<TerritoryId, Vec<&Take>> = BTreeMap::new();
    for take in &takes {
        by_target.entry(take.target).or_default().push(take);
    }
    let mut targets: Vec<TerritoryId> = by_target.keys().copied().collect();
    SplitMix64::new(seed).shuffle(&mut targets);
    let mut fights: Vec<LogEntry> = Vec::with_capacity(targets.len());
    let (mut taken, mut held, mut bounced) = (0u32, 0u32, 0u32);
    for target in targets {
        let mut rng = SplitMix64::new(target_seed(seed, target));
        let rolls: Vec<TakerRoll> = by_target[&target]
            .iter()
            .map(|take| {
                let priced = project(state, map, &tonight, take.actor, target, take.points, day)
                    .expect("a legal take is priced");
                TakerRoll {
                    user_id: take.actor,
                    points: take.points,
                    from: priced.from,
                    force: priced.force,
                    probability: priced.probability,
                    shown: take.shown,
                    success: rng.next_f64() < priced.probability,
                }
            })
            .collect();
        let best = rolls.iter().filter(|r| r.success).map(|r| r.force).fold(
            None,
            |top: Option<f64>, force| match top {
                Some(top) if top >= force => Some(top),
                _ => Some(force),
            },
        );
        let strongest: Vec<Uuid> = match best {
            None => Vec::new(),
            Some(best) => rolls
                .iter()
                .filter(|r| r.success && r.force == best)
                .map(|r| r.user_id)
                .collect(),
        };
        let (taken_by, was_bounced) = match strongest.as_slice() {
            [] => (None, false),
            [winner] => (Some(*winner), false),
            _ => (None, true),
        };
        match (taken_by, was_bounced) {
            (Some(_), _) => taken += 1,
            (None, true) => bounced += 1,
            (None, false) => held += 1,
        }
        fights.push(LogEntry::Fought {
            target,
            holder: state.ownership.get(&target).copied(),
            taken_by,
            defence: defence(state, map, &tonight, target, day),
            takers: rolls,
            bounced: was_bounced,
        });
    }

    // 5. The fights land, in order. A taken territory's walls are rubble.
    let mut last_loss: BTreeMap<Uuid, (usize, Uuid)> = BTreeMap::new();
    for (index, entry) in fights.into_iter().enumerate() {
        if let LogEntry::Fought {
            target,
            holder,
            taken_by: Some(taker),
            ..
        } = &entry
        {
            state.ownership.insert(*target, *taker);
            state.forts.remove(target);
            if let Some(holder) = holder {
                last_loss.insert(*holder, (index, *taker));
            }
        }
        state.log(day, seed, entry);
    }

    // 6. Falls: whoever took a player's last territory, in the order the
    // fights landed, is the one named.
    let mut falls: Vec<(usize, Uuid, Uuid)> = last_loss
        .into_iter()
        .map(|(holder, (index, taker))| (index, holder, taker))
        .collect();
    falls.sort_unstable();
    for (_, holder, taker) in falls {
        if state.is_alive(holder) && state.territory_count(holder) == 0 {
            state.exit_player(holder, RealmPlayerStatus::Eliminated, day);
            if let Some(p) = state.player_mut(holder) {
                p.exit_territories = 1;
            }
            state.log(
                day,
                seed,
                LogEntry::Eliminated {
                    user_id: holder,
                    by_user_id: taker,
                },
            );
        }
    }

    // 7. The end, once, after everything above.
    let end = check_end_orders(state, map, day, seed);

    let mut night = state.last_day.clone().expect("the open day is still open");
    night.entries.drain(..logged_before);
    state.last_night = Some(night);
    state.revision += 1;
    NightReport {
        taken,
        held,
        bounced,
        fizzled,
        end,
    }
}

/// Why a take that was legal when it was placed no longer is at the night.
fn fizzle_reason(rejected: OrderRejected) -> FizzleReason {
    match rejected {
        OrderRejected::AlreadyYours => FizzleReason::AlreadyYours,
        OrderRejected::NoFront => FizzleReason::NoFront,
        OrderRejected::AttacksLocked(_)
        | OrderRejected::LandingsLocked(_)
        | OrderRejected::TargetIsNew(_)
        | OrderRejected::TargetIsRamping(_) => FizzleReason::TargetProtected,
        // The rest are placement rules on the order itself, not on the
        // board, and a take that passed them once passes them again.
        OrderRejected::NotPlaying
        | OrderRejected::UnknownTerritory
        | OrderRejected::NotYours
        | OrderRejected::NoFortifying
        | OrderRejected::FullyFortified { .. }
        | OrderRejected::NotEnoughPoints { .. }
        | OrderRejected::NoSuchOrder => {
            unreachable!("check_take only refuses on the board: {rejected:?}")
        }
    }
}

/// How an `Orders` game stands at the end of a night. Moves every alive
/// player's run of dominating nights, then: the last one standing, a
/// domination held long enough, or the season clock run out, each only for
/// a player who `can_win`. Ties among several go to the bigger share, then
/// the territory count and the seeded jitter `final_ranking` uses.
pub fn check_end_orders(
    state: &mut RealmGameState,
    map: &WorldMap,
    day: i32,
    seed: u64,
) -> ResolveEnd {
    if state.players.is_empty() {
        return ResolveEnd::Ongoing;
    }
    let alive: Vec<Uuid> = state
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
        .map(|p| p.user_id)
        .collect();
    if alive.is_empty() {
        return ResolveEnd::Dissolved;
    }
    let threshold = state.ruleset.domination_share(state.war_roster(day));
    let shares: BTreeMap<Uuid, f64> = alive.iter().map(|u| (*u, share(state, map, *u))).collect();
    for player in state
        .players
        .iter_mut()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
    {
        player.dominating_nights = if shares[&player.user_id] >= threshold {
            player.dominating_nights.saturating_add(1)
        } else {
            0
        };
    }
    let ranking = final_ranking(state, None, seed);
    let best = |candidates: Vec<Uuid>| -> Option<Uuid> {
        candidates.into_iter().min_by(|a, b| {
            shares[b].total_cmp(&shares[a]).then_with(|| {
                let at = |u: &Uuid| ranking.iter().position(|r| r == u);
                at(a).cmp(&at(b))
            })
        })
    };
    let eligible: Vec<Uuid> = alive
        .iter()
        .copied()
        .filter(|u| state.can_win(*u))
        .collect();
    let dominating: Vec<Uuid> = eligible
        .iter()
        .copied()
        .filter(|u| {
            state
                .player(*u)
                .is_some_and(|p| p.dominating_nights >= state.ruleset.domination_nights)
        })
        .collect();
    let clock_out = season_last_day(state).is_some_and(|last| day >= last);
    let won = match (alive.as_slice(), best(dominating)) {
        // Every rival off the board, the days played.
        ([last], _) if state.can_win(*last) => Some((*last, WinKind::LastStanding)),
        (_, Some(winner)) => Some((winner, WinKind::Dominated)),
        _ if clock_out => match best(eligible) {
            Some(winner) => Some((winner, WinKind::SeasonOver)),
            // The clock ran out on a world nobody had played long enough
            // to be given.
            None => return ResolveEnd::Dissolved,
        },
        _ => None,
    };
    match won {
        Some((winner, kind)) => {
            state.log(
                day,
                seed,
                LogEntry::Won {
                    user_id: winner,
                    kind,
                },
            );
            ResolveEnd::Won { winner, kind }
        }
        None => ResolveEnd::Ongoing,
    }
}

#[cfg(test)]
#[path = "night_test.rs"]
mod night_test;

#[cfg(test)]
#[path = "night_sim_test.rs"]
mod night_sim_test;
