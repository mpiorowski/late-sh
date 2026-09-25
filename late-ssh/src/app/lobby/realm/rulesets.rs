//! The realm ruleset roster. Every game-mechanic knob lives here as data so
//! new rulesets are added by appending to `RULESETS`, never by branching game
//! code. A game freezes a `RealmRulesetSnapshot` into its state JSONB at
//! creation, so later edits to a ruleset never mutate running games.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// How fast a realm runs. Orthogonal to the ruleset: the rules say how a
/// conquest works, the pace says how long it takes. Picked at creation and
/// baked into the game's frozen snapshot, so a running game never changes
/// tempo underneath its players.
///
/// Everything scales together. More points a day makes the map fill faster,
/// so the day-shaped windows — the opening protection, the week of play a
/// win takes, how long an empire takes to crumble — shrink with it, or a
/// blitz would spend most of its life under a truce. The prize scales too:
/// roughly with the time a game asks for, with a small premium for the
/// longer commitments, so nobody can farm chips-per-day by spamming the
/// quickest variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RealmPace {
    pub id: &'static str,
    pub display_name: &'static str,
    pub tagline: &'static str,
    /// Multiplies the daily action points.
    pub action_scale: f64,
    /// Multiplies every window measured in days.
    pub day_scale: f64,
    /// Multiplies the pot. 1.0 is `normal`, the reference tempo.
    pub pot_scale: f64,
}

/// The pace roster, slowest first. The lengths in the taglines are measured,
/// not guessed: `simulation_test.rs::every_pace_resolves_and_the_order_holds`
/// plays five mixed policies through each one and prints what it took —
/// 158 / 68 / 39 / 17 / 12 days at the time of writing. Change a scale here
/// and that test will tell you the new number.
pub const PACES: &[RealmPace] = &[
    RealmPace {
        id: "epic",
        display_name: "Epic",
        tagline: "~5 months · a war you live alongside",
        action_scale: 0.45,
        day_scale: 1.5,
        pot_scale: 3.0,
    },
    RealmPace {
        id: "slow",
        display_name: "Slow",
        tagline: "~10 weeks · a few moves with morning coffee",
        action_scale: 1.0,
        day_scale: 1.0,
        pot_scale: 1.8,
    },
    RealmPace {
        id: "normal",
        display_name: "Normal",
        tagline: "~6 weeks · the standard tempo",
        action_scale: 1.7,
        day_scale: 0.6,
        pot_scale: 1.0,
    },
    RealmPace {
        id: "fast",
        display_name: "Fast",
        tagline: "~2-3 weeks · the map moves under you",
        action_scale: 3.0,
        day_scale: 0.35,
        pot_scale: 0.5,
    },
    RealmPace {
        id: "blitz",
        display_name: "Blitz",
        tagline: "~2 weeks · a sprint for the world",
        action_scale: 5.0,
        day_scale: 0.2,
        pot_scale: 0.25,
    },
];

/// The reference tempo: the pace whose pot is the plain per-player rate.
pub const DEFAULT_PACE: &str = "normal";

pub fn pace_by_id(id: &str) -> Option<&'static RealmPace> {
    PACES.iter().find(|p| p.id == id)
}

/// A rule a creator can switch on or off for their own game.
///
/// Rulesets are whole variants; these are the knobs inside one, and they
/// exist as data so the create screen is a list rather than a growing pile
/// of branches. Games store only what was chosen (`RealmRulesetSnapshot::
/// options`), and anything absent falls back to the default here — so a new
/// option can be added without touching a single running game.
pub struct RealmOption {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Shown under the list while this row is highlighted.
    pub blurb: &'static str,
    pub default_on: bool,
}

/// Strike anywhere, at whatever the odds have collapsed to. With it off, a
/// move has to stay inside `short_reach_cost` — your own borders and a short
/// crossing — *unless* nothing at all is in range, which lifts the cap so
/// nobody is ever boxed into having no move.
pub const OPTION_LONG_RANGE: &str = "long_range_strikes";
/// Whether a player with no coastline can mount a sea attack at all.
pub const OPTION_LANDLOCKED_SEA: &str = "landlocked_sea_attack";
/// Whether a point can be spent digging in instead of expanding.
pub const OPTION_FORTIFY: &str = "fortifications";

pub const OPTIONS: &[RealmOption] = &[
    RealmOption {
        id: OPTION_LONG_RANGE,
        display_name: "Long-range strikes",
        blurb: "reach anywhere on the map at collapsing odds · off keeps war to your own \
                neighbourhood, unless you have no move at all",
        default_on: true,
    },
    RealmOption {
        id: OPTION_FORTIFY,
        display_name: "Fortifications",
        blurb: "spend a point to dig in on land you hold, up to five levels · walls make a \
                place dearer to take and to march past, and are levelled when it falls",
        default_on: true,
    },
    RealmOption {
        id: OPTION_LANDLOCKED_SEA,
        display_name: "Landlocked navies",
        blurb: "a player with no coast can still mount a sea landing · off means you need \
                a coastline to sail from",
        default_on: false,
    },
];

pub fn option_by_id(id: &str) -> Option<&'static RealmOption> {
    OPTIONS.iter().find(|o| o.id == id)
}

/// The defaults, as a game would store them.
pub fn default_options() -> BTreeMap<String, bool> {
    OPTIONS
        .iter()
        .map(|o| (o.id.to_string(), o.default_on))
        .collect()
}

/// A named ruleset: static data, one entry per playable variant.
pub struct RealmRuleset {
    pub id: &'static str,
    pub display_name: &'static str,
    pub tagline: &'static str,
    pub min_players: u8,
    pub max_players: u8,
    /// `(max_player_count_inclusive, action_points)` tiers, ascending by the
    /// first field; the first tier the game's player count fits picks the
    /// daily action points. Fewer players get more points so the pace toward
    /// total conquest stays comparable across game sizes.
    pub actions_per_day_tiers: &'static [(u8, u8)],
    /// Retired: the old auto-removal clock. Nobody is removed from a running
    /// game any more — a quiet player's empire stays theirs to be taken (see
    /// `dormancy_grace_days`), because "go quiet to exit" was the same hole
    /// as a quit button.
    pub inactivity_kick_days: u8,
    /// Distinct days a player must have acted on before they can win. A
    /// conquest is a week of play, not an afternoon with a friend who
    /// surrenders: eliminating everyone sooner leaves the game running until
    /// the bar is met.
    pub min_active_days_to_win: u16,
    /// Active days a player needs before they can be paid at all — the
    /// podium means "played and placed", not "was in the roster".
    pub min_payout_active_days: u16,
    /// Unspent points bank, up to this many days' worth. The cap is what
    /// stops a fortnight away from becoming one flattening blitz.
    pub bank_cap_days: u8,
    /// What a missed day still banks, by how many were missed in a row:
    /// one missed day keeps most of it, a few keep half, and past
    /// `bank_missed_cutoff` a missed day banks nothing at all.
    pub bank_rate_one_missed: f64,
    pub bank_rate_few_missed: f64,
    pub bank_missed_cutoff: u8,
    /// Quiet days before a player's empire starts to crumble: their strength
    /// (what protects their land) decays `decay_per_day` for each further
    /// quiet day, down to `decay_floor`. An abandoned empire gets easier to
    /// carve up instead of freezing the game.
    pub dormancy_grace_days: u8,
    pub decay_per_day: f64,
    pub decay_floor: f64,
    /// Success multiplier charged per point of route cost beyond the first.
    /// A move onto your own border costs 1 and pays nothing; everything
    /// further multiplies by this per unit, so a march past a rival or an
    /// ocean crossing collapses.
    pub distance_decay: f64,
    /// What it costs to cross one territory nobody owns.
    pub free_transit_cost: f64,
    /// What it costs to cross a territory somebody else holds. Far more than
    /// empty land: going *through* a rival is the expensive way round, and
    /// the whole reason a border is worth holding.
    pub enemy_transit_cost: f64,
    /// Kilometres of open water per point of route cost. A strait is nearly
    /// free; an ocean is not.
    pub sea_km_per_cost: f64,
    /// The reach a game allows when long-range strikes are switched off:
    /// your own borders and a short crossing, and no further.
    pub short_reach_cost: f64,
    /// How deep you can dig in. Level one is simply holding the place.
    pub fort_max_level: u8,
    /// What each level past the first multiplies an attacker's odds by.
    ///
    /// Deliberately a losing trade in pure action economy: four points spent
    /// digging in buy fewer than four points of attacking. Digging in is for
    /// the pass you mean to hold, or for a day with nothing better — a realm
    /// is won by taking ground, and a wall should never be a way to win by
    /// sitting still.
    pub fort_defence_per_level: f64,
    /// What each level past the first adds to the cost of marching past.
    pub fort_transit_per_level: f64,
    /// Odds floor for anything past the first hop. Distant strikes stay
    /// possible (never below this) but never cheap.
    pub far_prob_floor: f64,
    pub base_claim: f64,
    pub claim_weight_factor: f64,
    pub base_attack: f64,
    /// How sharply a defender's size protects them. The strength ratio is
    /// raised to this power, so a bigger neighbour is more than
    /// proportionally harder to take from: at 1.0 it is the plain ratio,
    /// above it the gap between a small and a large defender widens.
    pub attack_strength_exponent: f64,
    pub attack_weight_factor: f64,
    /// Days after a player joins during which they are **new**: they may
    /// only take free land, and nobody may touch them. Counted per player
    /// from their own arrival, so someone joining a running realm gets the
    /// same footing the founders had.
    pub attack_grace_days: u8,
    /// Days after joining during which a player is **ramping up**: they may
    /// fight their own neighbours, and only neighbours may fight them. Past
    /// this they are **involved** and anything goes.
    pub distant_attack_grace_days: u8,
    /// New players may join while less than this much of the map is claimed.
    /// Past it the world is carved up and a newcomer would only be food, so
    /// the realm closes its doors.
    pub join_max_claimed: f64,
    /// Extra days of new-arrival protection for somebody who joins a world
    /// that is already carved up, on top of `attack_grace_days`. Nought when
    /// they arrive to an empty map, the full amount at the join door
    /// (`join_max_claimed`), pro rata between. Joining late is meant to be a
    /// handicap, not a spawn point next to an empire that can take you on
    /// your second day.
    pub newcomer_shield_max_days: u8,
    pub prob_floor: f64,
    pub prob_ceil: f64,
    /// Realm days a game must have run before its finish pays chips, and
    /// territories the winner must be holding. A game that ended because the
    /// other players walked away on day one is not a conquest, and paying
    /// for it would make it the cheapest chips on the platform.
    pub min_payout_days: u8,
    pub min_payout_territories: u16,
}

pub const STANDARD: RealmRuleset = RealmRuleset {
    id: "standard",
    display_name: "Standard",
    tagline: "conquer the whole earth, one country a day",
    min_players: 2,
    max_players: 10,
    actions_per_day_tiers: &[(4, 7), (7, 5), (10, 4)],
    inactivity_kick_days: 5,
    min_active_days_to_win: 7,
    min_payout_active_days: 3,
    bank_cap_days: 3,
    bank_rate_one_missed: 0.75,
    bank_rate_few_missed: 0.50,
    bank_missed_cutoff: 3,
    dormancy_grace_days: 3,
    decay_per_day: 0.10,
    decay_floor: 0.40,
    distance_decay: 0.50,
    free_transit_cost: 1.0,
    enemy_transit_cost: 3.0,
    sea_km_per_cost: 2_000.0,
    short_reach_cost: 2.0,
    fort_max_level: 5,
    fort_defence_per_level: 0.88,
    fort_transit_per_level: 0.4,
    far_prob_floor: 0.01,
    base_claim: 0.90,
    claim_weight_factor: 0.25,
    base_attack: 0.45,
    attack_strength_exponent: 1.6,
    attack_weight_factor: 0.15,
    attack_grace_days: 1,
    distant_attack_grace_days: 3,
    join_max_claimed: 0.50,
    newcomer_shield_max_days: 4,
    prob_floor: 0.05,
    prob_ceil: 0.95,
    min_payout_days: 3,
    min_payout_territories: 8,
};

pub const RULESETS: &[RealmRuleset] = &[STANDARD];

pub fn ruleset_by_id(id: &str) -> Option<&'static RealmRuleset> {
    RULESETS.iter().find(|rs| rs.id == id)
}

/// The serde-owned copy of a ruleset frozen into a game's state at creation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RealmRulesetSnapshot {
    pub id: String,
    pub display_name: String,
    /// The pace this game was created at, already applied to the numbers
    /// below. Kept for display and for the pot.
    #[serde(default = "default_pace_id")]
    pub pace_id: String,
    #[serde(default = "default_pace_name")]
    pub pace_name: String,
    /// The pot multiplier that came with the pace.
    #[serde(default = "default_pot_scale")]
    pub pot_scale: f64,
    pub min_players: u8,
    pub max_players: u8,
    pub actions_per_day_tiers: Vec<(u8, u8)>,
    pub inactivity_kick_days: u8,
    #[serde(default = "default_min_active_days_to_win")]
    pub min_active_days_to_win: u16,
    #[serde(default = "default_min_payout_active_days")]
    pub min_payout_active_days: u16,
    #[serde(default = "default_bank_cap_days")]
    pub bank_cap_days: u8,
    #[serde(default = "default_bank_rate_one_missed")]
    pub bank_rate_one_missed: f64,
    #[serde(default = "default_bank_rate_few_missed")]
    pub bank_rate_few_missed: f64,
    #[serde(default = "default_bank_missed_cutoff")]
    pub bank_missed_cutoff: u8,
    #[serde(default = "default_dormancy_grace_days")]
    pub dormancy_grace_days: u8,
    #[serde(default = "default_decay_per_day")]
    pub decay_per_day: f64,
    #[serde(default = "default_decay_floor")]
    pub decay_floor: f64,
    #[serde(default = "default_distance_decay")]
    pub distance_decay: f64,
    #[serde(default = "default_free_transit_cost")]
    pub free_transit_cost: f64,
    #[serde(default = "default_enemy_transit_cost")]
    pub enemy_transit_cost: f64,
    #[serde(default = "default_sea_km_per_cost")]
    pub sea_km_per_cost: f64,
    #[serde(default = "default_short_reach_cost")]
    pub short_reach_cost: f64,
    #[serde(default = "default_fort_max_level")]
    pub fort_max_level: u8,
    #[serde(default = "default_fort_defence_per_level")]
    pub fort_defence_per_level: f64,
    #[serde(default = "default_fort_transit_per_level")]
    pub fort_transit_per_level: f64,
    /// What the creator switched on. Absent entries take the roster default,
    /// so a game frozen before an option existed simply gets it.
    #[serde(default)]
    pub options: BTreeMap<String, bool>,
    #[serde(default = "default_far_prob_floor")]
    pub far_prob_floor: f64,
    pub base_claim: f64,
    pub claim_weight_factor: f64,
    pub base_attack: f64,
    #[serde(default = "default_attack_strength_exponent")]
    pub attack_strength_exponent: f64,
    pub attack_weight_factor: f64,
    #[serde(default = "default_attack_grace_days")]
    pub attack_grace_days: u8,
    #[serde(default = "default_distant_attack_grace_days")]
    pub distant_attack_grace_days: u8,
    #[serde(default = "default_join_max_claimed")]
    pub join_max_claimed: f64,
    /// Zero on games frozen before this existed, which is exactly the old
    /// behaviour for them.
    #[serde(default)]
    pub newcomer_shield_max_days: u8,
    pub prob_floor: f64,
    pub prob_ceil: f64,
    #[serde(default = "default_min_payout_days")]
    pub min_payout_days: u8,
    #[serde(default = "default_min_payout_territories")]
    pub min_payout_territories: u16,
    /// The world this game is played on, chosen at creation. Frozen here
    /// like every other rule, so a map being retired never strands a game
    /// halfway through.
    #[serde(default = "default_map_id")]
    pub map_id: String,
    /// For a generated world, everything needed to rebuild it: the seed, the
    /// shape the creator asked for, and the names, which are the one part
    /// that cannot be recomputed. `None` for the fixed maps. Frozen with the
    /// rest, so a generated realm is as re-derivable in a year as it is now.
    #[serde(default)]
    pub map_spec: Option<super::mapgen::GeneratedMapSpec>,
}

impl From<&RealmRuleset> for RealmRulesetSnapshot {
    /// The ruleset at its own tempo — the `slow` pace, which is the rules as
    /// written. `RealmRulesetSnapshot::at_pace` is what creation uses.
    fn from(rs: &RealmRuleset) -> Self {
        Self {
            id: rs.id.to_string(),
            display_name: rs.display_name.to_string(),
            pace_id: "slow".to_string(),
            pace_name: "Slow".to_string(),
            pot_scale: PACES
                .iter()
                .find(|p| p.id == "slow")
                .map(|p| p.pot_scale)
                .unwrap_or(1.0),
            min_players: rs.min_players,
            max_players: rs.max_players,
            actions_per_day_tiers: rs.actions_per_day_tiers.to_vec(),
            inactivity_kick_days: rs.inactivity_kick_days,
            min_active_days_to_win: rs.min_active_days_to_win,
            min_payout_active_days: rs.min_payout_active_days,
            bank_cap_days: rs.bank_cap_days,
            bank_rate_one_missed: rs.bank_rate_one_missed,
            bank_rate_few_missed: rs.bank_rate_few_missed,
            bank_missed_cutoff: rs.bank_missed_cutoff,
            dormancy_grace_days: rs.dormancy_grace_days,
            decay_per_day: rs.decay_per_day,
            decay_floor: rs.decay_floor,
            distance_decay: rs.distance_decay,
            free_transit_cost: rs.free_transit_cost,
            enemy_transit_cost: rs.enemy_transit_cost,
            sea_km_per_cost: rs.sea_km_per_cost,
            short_reach_cost: rs.short_reach_cost,
            fort_max_level: rs.fort_max_level,
            fort_defence_per_level: rs.fort_defence_per_level,
            fort_transit_per_level: rs.fort_transit_per_level,
            options: default_options(),
            far_prob_floor: rs.far_prob_floor,
            base_claim: rs.base_claim,
            claim_weight_factor: rs.claim_weight_factor,
            base_attack: rs.base_attack,
            attack_strength_exponent: rs.attack_strength_exponent,
            attack_weight_factor: rs.attack_weight_factor,
            attack_grace_days: rs.attack_grace_days,
            distant_attack_grace_days: rs.distant_attack_grace_days,
            join_max_claimed: rs.join_max_claimed,
            newcomer_shield_max_days: rs.newcomer_shield_max_days,
            prob_floor: rs.prob_floor,
            prob_ceil: rs.prob_ceil,
            min_payout_days: rs.min_payout_days,
            min_payout_territories: rs.min_payout_territories,
            map_id: default_map_id(),
            map_spec: None,
        }
    }
}

/// Serde defaults so a game frozen before distance mattered still parses;
/// they are the standard ruleset's values.
fn default_distance_decay() -> f64 {
    STANDARD.distance_decay
}

fn default_free_transit_cost() -> f64 {
    STANDARD.free_transit_cost
}

fn default_enemy_transit_cost() -> f64 {
    STANDARD.enemy_transit_cost
}

fn default_sea_km_per_cost() -> f64 {
    STANDARD.sea_km_per_cost
}

fn default_short_reach_cost() -> f64 {
    STANDARD.short_reach_cost
}

fn default_fort_max_level() -> u8 {
    STANDARD.fort_max_level
}

fn default_fort_defence_per_level() -> f64 {
    STANDARD.fort_defence_per_level
}

fn default_fort_transit_per_level() -> f64 {
    STANDARD.fort_transit_per_level
}

fn default_far_prob_floor() -> f64 {
    STANDARD.far_prob_floor
}

fn default_attack_strength_exponent() -> f64 {
    STANDARD.attack_strength_exponent
}

fn default_attack_grace_days() -> u8 {
    STANDARD.attack_grace_days
}

fn default_distant_attack_grace_days() -> u8 {
    STANDARD.distant_attack_grace_days
}

fn default_pace_id() -> String {
    "slow".to_string()
}

fn default_pace_name() -> String {
    "Slow".to_string()
}

/// Games frozen before paces existed ran at the rules' own tempo, which is
/// what `slow` is; their pot was the plain rate, so they keep it.
fn default_pot_scale() -> f64 {
    1.0
}

fn default_join_max_claimed() -> f64 {
    STANDARD.join_max_claimed
}

fn default_min_active_days_to_win() -> u16 {
    STANDARD.min_active_days_to_win
}

fn default_min_payout_active_days() -> u16 {
    STANDARD.min_payout_active_days
}

fn default_bank_cap_days() -> u8 {
    STANDARD.bank_cap_days
}

fn default_bank_rate_one_missed() -> f64 {
    STANDARD.bank_rate_one_missed
}

fn default_bank_rate_few_missed() -> f64 {
    STANDARD.bank_rate_few_missed
}

fn default_bank_missed_cutoff() -> u8 {
    STANDARD.bank_missed_cutoff
}

fn default_dormancy_grace_days() -> u8 {
    STANDARD.dormancy_grace_days
}

fn default_decay_per_day() -> f64 {
    STANDARD.decay_per_day
}

fn default_decay_floor() -> f64 {
    STANDARD.decay_floor
}

/// Games frozen before the map was a choice were all Earth, and Earth is
/// what a ruleset alone means now.
fn default_map_id() -> String {
    super::map::MAPS[0].id.to_string()
}

fn default_min_payout_days() -> u8 {
    STANDARD.min_payout_days
}

fn default_min_payout_territories() -> u16 {
    STANDARD.min_payout_territories
}

impl RealmRulesetSnapshot {
    /// Freeze a ruleset at a pace: action points scale up, every window
    /// measured in days scales down (or the other way for the slow end), and
    /// the pot multiplier rides along. Floors keep a blitz playable — an
    /// opening protection of zero days, or a win you can reach in one, would
    /// be a different game rather than a faster one.
    pub fn at_pace(rs: &RealmRuleset, pace: &RealmPace) -> Self {
        let scale_days_u8 = |days: u8, floor: u8| -> u8 {
            ((f64::from(days) * pace.day_scale).round() as u8).max(floor)
        };
        let scale_days_u16 = |days: u16, floor: u16| -> u16 {
            ((f64::from(days) * pace.day_scale).round() as u16).max(floor)
        };
        Self {
            pace_id: pace.id.to_string(),
            pace_name: pace.display_name.to_string(),
            pot_scale: pace.pot_scale,
            actions_per_day_tiers: rs
                .actions_per_day_tiers
                .iter()
                .map(|(max_players, points)| {
                    (
                        *max_players,
                        ((f64::from(*points) * pace.action_scale).round() as u8).max(1),
                    )
                })
                .collect(),
            // A conquest is still a commitment, just a shorter one: two days
            // is the floor, because a one-day win is an afternoon.
            min_active_days_to_win: scale_days_u16(rs.min_active_days_to_win, 2),
            min_payout_active_days: scale_days_u16(rs.min_payout_active_days, 1),
            attack_grace_days: scale_days_u8(rs.attack_grace_days, 1),
            distant_attack_grace_days: scale_days_u8(rs.distant_attack_grace_days, 2),
            dormancy_grace_days: scale_days_u8(rs.dormancy_grace_days, 1),
            inactivity_kick_days: scale_days_u8(rs.inactivity_kick_days, 2),
            newcomer_shield_max_days: scale_days_u8(rs.newcomer_shield_max_days, 1),
            bank_cap_days: scale_days_u8(rs.bank_cap_days, 1),
            bank_missed_cutoff: scale_days_u8(rs.bank_missed_cutoff, 1),
            ..Self::from(rs)
        }
    }

    /// Daily action points for a game of `player_count` players (count at
    /// game start). Falls back to the last tier when the count exceeds every
    /// tier bound (defensive; max_players should prevent it).
    pub fn actions_per_day(&self, player_count: u8) -> u8 {
        self.actions_per_day_tiers
            .iter()
            .find(|(max, _)| player_count <= *max)
            .or(self.actions_per_day_tiers.last())
            .map(|(_, points)| *points)
            .unwrap_or(1)
    }
}

/// What a queued action is trying to do, for probability purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbabilityKind {
    /// Take an unowned territory.
    Claim,
    /// Take another player's territory.
    Attack,
}

/// How a route reached its target, and what it cost.
///
/// Cost is continuous, not a count of steps: crossing your own land is free,
/// empty land is cheap, a rival's land is expensive, and open water is
/// priced by the kilometre. One is the cost of a move onto your own border —
/// the ordinary act of the game — and everything above that is paid for in
/// odds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reach {
    pub cost: f64,
    pub mode: RouteMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RouteMode {
    /// Overland, counting how many territories somebody else holds along
    /// the way — the thing that makes a route expensive.
    Land { through_rivals: u8 },
    /// By sea, over this much open water.
    Sea { km: f64 },
    /// No route at all: the target is on another landmass and either it has
    /// no shore to land on or the actor has no shore to sail from. Its own
    /// variant rather than a very large cost, because "expensive" and
    /// "impossible" are different answers and the second one has to be
    /// refused rather than priced.
    None,
}

impl Reach {
    /// A plain frontier move: the target borders the actor's land.
    pub const ADJACENT: Self = Self {
        cost: 1.0,
        mode: RouteMode::Land { through_rivals: 0 },
    };

    /// No way to get there at all.
    pub const UNREACHABLE: Self = Self {
        cost: f64::INFINITY,
        mode: RouteMode::None,
    };

    /// Is this the ordinary move — straight onto a bordering territory?
    pub fn is_adjacent(self) -> bool {
        matches!(self.mode, RouteMode::Land { .. }) && self.cost <= 1.0001
    }

    pub fn is_unreachable(self) -> bool {
        matches!(self.mode, RouteMode::None)
    }

    pub fn by_sea(self) -> bool {
        matches!(self.mode, RouteMode::Sea { .. })
    }

    /// How the route reads on a board: what it crossed, not a bare number.
    pub fn describe(self) -> String {
        match self.mode {
            RouteMode::Land { .. } if self.is_adjacent() => "on your border".to_string(),
            RouteMode::Land { through_rivals: 0 } => "overland".to_string(),
            RouteMode::Land { through_rivals: 1 } => "overland, past 1 rival".to_string(),
            RouteMode::Land { through_rivals } => {
                format!("overland, past {through_rivals} rivals")
            }
            RouteMode::Sea { km } if km < 1.0 => "by sea".to_string(),
            RouteMode::Sea { km } => format!("by sea, {:.0}km", km),
            RouteMode::None => "no route".to_string(),
        }
    }
}

/// Success probability for one action, the one pure formula both the engine
/// (real roll) and the UI (projected %) use.
///
/// `target_weight` is the territory's normalized size weight in [0, 1]
/// (see `map::Territory::weight`); `attacker_strength`/`defender_strength`
/// are `1 + sum of held weights` (defender ignored for claims). `reach` is
/// how far the target sits from the actor's nearest holding.
pub fn action_probability(
    rs: &RealmRulesetSnapshot,
    kind: ProbabilityKind,
    target_weight: f64,
    attacker_strength: f64,
    defender_strength: f64,
    reach: Reach,
    // The defender's fortification multiplier, 1.0 for unwalled ground.
    fortification: f64,
) -> f64 {
    let mut p = match kind {
        ProbabilityKind::Claim => rs.base_claim - rs.claim_weight_factor * target_weight,
        ProbabilityKind::Attack => {
            // Strength is everything a player holds (see
            // `RealmGameState::strength`), so a sprawling empire is a hard
            // target and a cornered one is soft. The exponent makes that
            // lopsided rather than linear.
            let ratio = 2.0 * attacker_strength / (attacker_strength + defender_strength);
            (rs.base_attack * ratio.powf(rs.attack_strength_exponent)
                - rs.attack_weight_factor * target_weight)
                * fortification
        }
    };
    // Everything the route cost beyond the first point is paid in odds. A
    // move onto your own border costs exactly one and pays nothing.
    let over = (reach.cost - 1.0).max(0.0);
    if over > 0.0 {
        p *= rs.distance_decay.powf(over);
    }
    let floor = if reach.is_adjacent() {
        rs.prob_floor
    } else {
        rs.far_prob_floor
    };
    p.clamp(floor, rs.prob_ceil)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> RealmRulesetSnapshot {
        (&STANDARD).into()
    }

    #[test]
    fn roster_lookup_round_trips() {
        for rs in RULESETS {
            assert_eq!(ruleset_by_id(rs.id).unwrap().id, rs.id);
        }
        assert!(ruleset_by_id("nope").is_none());
    }

    #[test]
    fn actions_per_day_tiers_scale_down() {
        let rs = snap();
        assert_eq!(rs.actions_per_day(2), 7);
        assert_eq!(rs.actions_per_day(4), 7);
        assert_eq!(rs.actions_per_day(5), 5);
        assert_eq!(rs.actions_per_day(7), 5);
        assert_eq!(rs.actions_per_day(8), 4);
        assert_eq!(rs.actions_per_day(10), 4);
        assert_eq!(rs.actions_per_day(12), 4);
    }

    fn land(cost: f64) -> Reach {
        Reach {
            cost,
            mode: RouteMode::Land { through_rivals: 0 },
        }
    }

    #[test]
    fn probability_clamped_and_monotone() {
        let rs = snap();
        let near = Reach::ADJACENT;
        // Heavier territories are never easier to take.
        let light = action_probability(&rs, ProbabilityKind::Claim, 0.1, 1.0, 0.0, near, 1.0);
        let heavy = action_probability(&rs, ProbabilityKind::Claim, 0.9, 1.0, 0.0, near, 1.0);
        assert!(light > heavy);
        // Stronger attackers do better against the same defender.
        let weak = action_probability(&rs, ProbabilityKind::Attack, 0.5, 1.0, 5.0, near, 1.0);
        let strong = action_probability(&rs, ProbabilityKind::Attack, 0.5, 5.0, 5.0, near, 1.0);
        assert!(strong > weak);
        // Clamps hold at the extremes.
        let floor = action_probability(&rs, ProbabilityKind::Attack, 1.0, 0.01, 100.0, near, 1.0);
        let ceil = action_probability(&rs, ProbabilityKind::Attack, 0.0, 10_000.0, 1.0, near, 1.0);
        assert_eq!(floor, rs.prob_floor);
        assert_eq!(ceil, rs.prob_ceil);
    }

    #[test]
    fn route_cost_collapses_the_odds_but_never_past_one_percent() {
        let rs = snap();
        let at = |cost: f64| {
            action_probability(&rs, ProbabilityKind::Claim, 0.3, 2.0, 0.0, land(cost), 1.0)
        };
        // A border move pays nothing; everything beyond it pays per point.
        assert!(at(1.0) > at(2.0));
        assert!(at(2.0) > at(3.0));
        assert!(at(3.0) > at(4.0));
        assert!(
            at(2.0) < at(1.0) * 0.6,
            "one territory further already costs most of the odds"
        );
        // A fraction of a point costs a fraction of the odds — the sea is
        // priced by the kilometre, so this has to be smooth.
        assert!(at(1.2) < at(1.0) && at(1.2) > at(2.0));
        // Far strikes stay possible, but never drop under the far floor.
        assert_eq!(at(40.0), rs.far_prob_floor);
    }

    #[test]
    fn crossing_a_rival_is_the_expensive_way_round() {
        let rs = snap();
        // A rival's land is priced well above empty land, which is what
        // makes going around it — or by sea — the sensible move.
        assert!(rs.enemy_transit_cost > rs.free_transit_cost * 2.0);
        let empty = action_probability(
            &rs,
            ProbabilityKind::Claim,
            0.3,
            2.0,
            0.0,
            land(1.0 + rs.free_transit_cost),
            1.0,
        );
        let past_rival = action_probability(
            &rs,
            ProbabilityKind::Claim,
            0.3,
            2.0,
            0.0,
            Reach {
                cost: 1.0 + rs.enemy_transit_cost,
                mode: RouteMode::Land { through_rivals: 1 },
            },
            1.0,
        );
        assert!(past_rival < empty * 0.5);
    }

    #[test]
    fn a_short_crossing_costs_less_than_a_march() {
        let rs = snap();
        let strait = Reach {
            cost: 1.0 + 300.0 / rs.sea_km_per_cost,
            mode: RouteMode::Sea { km: 300.0 },
        };
        let by_sea = action_probability(&rs, ProbabilityKind::Claim, 0.3, 2.0, 0.0, strait, 1.0);
        let by_land = action_probability(
            &rs,
            ProbabilityKind::Claim,
            0.3,
            2.0,
            0.0,
            land(1.0 + rs.free_transit_cost),
            1.0,
        );
        assert!(
            by_sea > by_land,
            "a strait should beat marching through a country"
        );
        assert!(strait.by_sea() && !strait.is_adjacent());
        assert_eq!(strait.describe(), "by sea, 300km");
    }

    #[test]
    fn the_options_are_data_and_old_games_keep_their_defaults() {
        let rs = snap();
        for option in OPTIONS {
            assert_eq!(rs.options.get(option.id), Some(&option.default_on));
            assert!(option_by_id(option.id).is_some());
        }
        assert!(option_by_id("nope").is_none());
        // A game frozen before an option existed stores nothing for it, and
        // the reader supplies the default rather than inventing a rule.
        let mut older = rs.clone();
        older.options.clear();
        let json = serde_json::to_string(&older).unwrap();
        let back: RealmRulesetSnapshot = serde_json::from_str(&json).unwrap();
        assert!(back.options.is_empty());
    }

    #[test]
    fn a_bigger_defender_is_more_than_proportionally_harder() {
        let rs = snap();
        let near = Reach::ADJACENT;
        let vs = |defender: f64| {
            action_probability(&rs, ProbabilityKind::Attack, 0.3, 2.0, defender, near, 1.0)
        };
        let even = vs(2.0);
        let double = vs(4.0);
        let quadruple = vs(8.0);
        assert!(even > double && double > quadruple);
        // The exponent is what makes size protect a player: halving the odds
        // ratio costs more than half the chance.
        let linear_guess = even * (2.0 * 2.0 / 6.0) / (2.0 * 2.0 / 4.0);
        assert!(
            double < linear_guess,
            "a big defender should beat the linear expectation"
        );
        // And attacking at all is harder than claiming empty land.
        let claim = action_probability(&rs, ProbabilityKind::Claim, 0.3, 2.0, 0.0, near, 1.0);
        assert!(even < claim);
    }

    #[test]
    fn snapshot_serde_round_trips() {
        let rs = snap();
        let json = serde_json::to_string(&rs).unwrap();
        let back: RealmRulesetSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rs);
    }
}
