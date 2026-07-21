//! The persistent Legend of the Green Dragon character and the pure rules that
//! act on it: stat derivation, leveling, shop pricing, healing, banking, and
//! the win/lose outcomes. All authentic LoGD numbers (see [`super::data`]).
//!
//! This module is pure and serde-able: no DB, no RNG except where a fight is
//! resolved through [`super::combat`]. Tests assert the transcribed formulas.

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::combat::{Buff, Combatant, Companion};
use super::data;

/// Starting on-hand gold for a fresh character (`newplayerstartgold`).
pub const START_GOLD: u64 = 50;
/// Forest fights granted per day (`turns`).
pub const TURNS_PER_DAY: u32 = 10;
/// Hitpoints per level — max HP is a flat `HP_PER_LEVEL * level`.
pub const HP_PER_LEVEL: u32 = 10;
/// Fraction of experience kept after a forest death (`1 - forestexploss`).
pub const EXP_KEEP_ON_DEATH: f64 = 0.90;
/// Forest turns you may leave unused and still earn bank interest (LoGD
/// `fightsforinterest`). Leave more than this unused and you didn't work for it.
pub const FIGHTS_FOR_INTEREST: u32 = 4;
/// Bank balance at/above which no interest is paid (LoGD `maxgoldforinterest`).
pub const MAX_GOLD_FOR_INTEREST: i64 = 100_000;
/// Daily bank interest is a random percent in this inclusive range, rolled fresh
/// each new day (LoGD `mininterest`/`maxinterest` defaults).
pub const MIN_INTEREST_PERCENT: u32 = 1;
pub const MAX_INTEREST_PERCENT: u32 = 10;
/// Gold carried into a fresh run after a dragon kill, before the flawless
/// bonus (LoGD `maxrestartgold`): [`START_GOLD`] plus [`START_GOLD`] per kill,
/// capped here. On-hand gold is *not* retained — the run reset wipes it.
pub const DRAGON_RUN_GOLD_CAP: u64 = 300;
/// Gem ceiling carried into a fresh run after a dragon kill (LoGD
/// `maxrestartgems`).
pub const MAX_RESTART_GEMS: u32 = 10;
/// Max HP granted per dragon point spent on `hp` (LoGD `dragonpointspend`).
pub const HP_PER_DRAGON_POINT: u32 = 5;
/// Gold the bank will lend per character level (LoGD `borrowperlevel`). Debt is
/// a negative balance and accrues interest daily.
pub const BORROW_PER_LEVEL: i64 = 20;
/// Bank transfers (`bank.php` op=transfer, stock-on via `allowgoldtransfer`):
/// the window opens at this level, or at any dragon kill (`mintransferlev`).
pub const MIN_TRANSFER_LEVEL: u8 = 3;
/// A recipient may take `level * this` gold in one transfer
/// (`transferperlevel`; upstream's refusal says "per day" but the check is
/// per transfer, kept 1=1).
pub const TRANSFER_PER_LEVEL: u64 = 25;
/// A sender may move `level * this` gold out per day (`maxtransferout`).
pub const MAX_TRANSFER_OUT_PER_LEVEL: u64 = 25;
/// Transfers one account may receive per day (`transferreceive`).
pub const TRANSFERS_RECEIVED_PER_DAY: u32 = 3;
/// One-in-this chance of a gem on a forest victory under level 15 (LoGD
/// `forestgemchance`).
pub const FOREST_GEM_CHANCE: u32 = 25;
/// Charm gained per dragon kill (LoGD `charm += 5`).
pub const CHARM_PER_DRAGON_KILL: u32 = 5;
/// Bonus gold (3x [`START_GOLD`]) and a gem for a flawless, no-damage dragon
/// kill (LoGD's flawless bonus), added on top of the gold cap.
pub const FLAWLESS_GOLD_BONUS: u64 = START_GOLD * 3;
/// Soulpoints awarded for beating a master (LoGD `train.php`).
pub const SOULPOINTS_PER_MASTER: u32 = 5;
/// Forest turns docked by the *paid* resurrection's immediate new day (LoGD
/// `resurrectionturns`, default -6; `newday.php` applies it only when
/// `resurrection=true`). Waiting out the night for free costs nothing extra.
pub const RESURRECTION_TURNS: i32 = -6;
/// Torment fights granted per day in the graveyard (LoGD `gravefightsperday`).
pub const GRAVE_FIGHTS_PER_DAY: u32 = 10;
/// Favor the death overlord charges to resurrect you on the spot
/// (`lib/graveyard/case_resurrection.php`).
pub const RESURRECTION_FAVOR_COST: u32 = 100;
/// Favor at which the overlord starts granting favors (`case_question.php`).
/// The 25-favor haunt itself is PvP and lands in phase 4; the tier messaging
/// exists now.
pub const HAUNT_FAVOR_THRESHOLD: u32 = 25;
/// Extra daily forest fights for the Plainsborn (LoGD `racehuman.php`'s
/// `bonus` setting, default 2). Like upstream's `newday` hook it applies to
/// every new day, including the paid resurrection's.
pub const PLAINSBORN_FOREST_BONUS: u32 = 2;
/// Percent chance of dying under a goldmine cave-in (`raceminedeath`):
/// the default for most races, and the Deepfolk's miner's instinct
/// (`racedwarf.php`'s `minedeathchance`, default 5).
pub const MINE_DEATH_PERCENT: u32 = 90;
pub const DEEPFOLK_MINE_DEATH_PERCENT: u32 = 5;
/// Hard drinks the inn will pour per day (`drinks.php` `hardlimit`).
pub const HARD_DRINKS_PER_DAY: u32 = 3;
/// Drunkenness above which the barkeep refuses service (`maxdrunk`), and the
/// hangover threshold at dawn (upstream hardcodes the same 66 there).
pub const MAX_DRUNKENNESS_SERVED: u32 = 66;
/// Five Sixes plays allowed per day (`game_fivesix` `dailyuses`).
pub const FIVESIX_PLAYS_PER_DAY: u32 = 10;

/// Gems per potion dose on the barkeep's back shelf (`cedrikspotions`: every
/// stock potion's cost setting defaults to 2).
pub const POTION_COST_GEMS: u64 = 2;

/// A bandit-type creature only bothers cutting purses this heavy (an
/// original late.sh mechanic — see `data::BANDIT_CREATURES`).
pub const BANDIT_GOLD_THRESHOLD: u64 = 200;

/// The private outhouse stall's price (`outhouse` `cost` default 5).
pub const OUTHOUSE_COST: u64 = 5;

/// The wash-up's lucky find (`outhouse` `giveback` default 3) — note it's
/// smaller than the stall price, so even a "refund" runs a 2-gold loss.
pub const OUTHOUSE_GIVEBACK: u64 = 3;

/// The mending draught's overheal above max, per dose (`tempgain` 20).
pub const MENDING_OVERHEAL: u32 = 20;

/// Transmutation sickness: rounds of atk/def x0.75 that survive the new day
/// (`transmuteturns` 10, `atkmod`/`defmod` 0.75, `survive` 1).
pub const TRANSMUTE_ROUNDS: u32 = 10;

/// The lover's ward: defense x1.2 for 60 combat rounds (`lovers_getbuff`).
pub const LOVER_BUFF_ROUNDS: u32 = 60;

/// The flirt ladder's first six rungs as `(success threshold, charm cap)`
/// (`lovers_violet/seth.php`): the test is `e_rand(charm, T) >= T` (certain at
/// charm >= T) and a success grants +1 charm only while under the cap. Rung 7
/// is the marriage proposal, gated on [`MARRY_CHARM_REQUIRED`].
pub const FLIRT_LADDER: [(u32, u32); 6] = [(2, 4), (4, 7), (7, 11), (11, 14), (14, 18), (18, 25)];
/// Gold staked per Five Sixes play; each play also grows the shared pot by
/// this much (`game_fivesix` `cost`).
pub const FIVESIX_COST: u64 = 5;
/// The shared Five Sixes pot's ceiling; growth past it is pocketed by the
/// house (`maxjackpot`).
pub const FIVESIX_MAX_POT: u64 = 5000;
/// The Crooked Wheel barman's price for a word on one enemy (`darkhorse.php`'s
/// bartender: a flat 100 gold per name, no bribe needed).
pub const INTEL_COST: u64 = 100;
/// Charm required to propose marriage (rung 7 checks `charm >= 22` directly).
pub const MARRY_CHARM_REQUIRED: u32 = 22;

/// PvP attacks granted per day (`pvpday`), refilled at a normal dawn only —
/// the paid resurrection skips them like it skips grave fights.
pub const PVP_FIGHTS_PER_DAY: u32 = 3;
/// Seconds a freshly-attacked warrior stays off the target lists
/// (`pvptimeout`), stamped onto the *victim* at engage (the dogpile guard).
pub const PVP_TIMEOUT_SECS: i64 = 600;
/// Newbie-immunity thresholds (`pvpimmunity` 5 days, `pvpminexp` 1500): a
/// warrior is immune while ALL hold — run age <= 5 days, no dragon kills,
/// never forfeited by attacking (`pk`), and experience <= 1500 (upstream's
/// warning test is `<=`; the list filter is the same set negated).
pub const PVP_IMMUNITY_DAYS: u32 = 5;
pub const PVP_IMMUNITY_MAX_EXP: u64 = 1500;
/// Percent of the victim's engage-time experience the winning attacker gains
/// (`pvpattgain`).
pub const PVP_ATTACKER_GAIN_PCT: u32 = 10;
/// Percent of their engage-time experience a slain victim loses (`pvpdeflose`).
pub const PVP_DEFENDER_LOSE_PCT: u32 = 5;
/// Percent of the attacker's experience a victorious sleeping defender gains
/// (`pvpdefgain`).
pub const PVP_DEFENDER_GAIN_PCT: u32 = 10;
/// Percent of experience a defeated attacker loses (`pvpattlose`).
pub const PVP_ATTACKER_LOSE_PCT: u32 = 15;

/// Bounty contracts one warrior may place per day (`dag.php` `maxbounties`).
pub const BOUNTIES_PER_DAY: u32 = 5;
/// The bounty floor per target level (`bountymin` 50).
pub const BOUNTY_MIN_PER_LEVEL: u64 = 50;
/// The cap on a target's *total open* bounty, per level (`bountymax` 200) —
/// counted over every open contract, matured or not.
pub const BOUNTY_MAX_PER_LEVEL: u64 = 200;
/// The broker's listing fee (`bountyfee` 10): the setter pays
/// `round(amount · 1.10)`.
pub const BOUNTY_FEE_PCT: u64 = 10;
/// The lowest level worth contracting on (`bountylevel` 3).
pub const BOUNTY_MIN_TARGET_LEVEL: u8 = 3;
/// A fresh bounty matures `e_rand(0, this)` seconds after placement
/// (dag's "random set date up to 4 hours in the future").
pub const BOUNTY_DELAY_MAX_SECS: i64 = 14_400;

/// What the setter pays to place `amount`: the bounty plus the broker's
/// [`BOUNTY_FEE_PCT`]% fee, rounded half-away like PHP `round`.
pub fn bounty_cost(amount: u64) -> u64 {
    (amount as f64 * (100 + BOUNTY_FEE_PCT) as f64 / 100.0).round() as u64
}

/// Founding a clan costs gold and gems (`goldtostartclan`/`gemstostartclan`).
pub const CLAN_START_GOLD: u64 = 10_000;
pub const CLAN_START_GEMS: u64 = 15;
/// The clan rank rungs (`lib/constants.php`). The founder is literally
/// "leader + 1" (`applicant_new.php` sets `CLAN_LEADER+1`).
pub const CLAN_APPLICANT: u8 = 0;
pub const CLAN_MEMBER: u8 = 10;
pub const CLAN_OFFICER: u8 = 20;
pub const CLAN_LEADER: u8 = 30;
pub const CLAN_FOUNDER: u8 = 31;
/// Clan name shape (`applicant_new.php`): 5–50 chars of letters, spaces,
/// apostrophes, and dashes.
pub const CLAN_NAME_MIN: usize = 5;
pub const CLAN_NAME_MAX: usize = 50;
/// Clan tag ("short name") shape: 2–5 letters.
pub const CLAN_TAG_MIN: usize = 2;
pub const CLAN_TAG_MAX: usize = 5;
/// The custom talk verb's cap (`clan_motd.php`'s maxlength).
pub const CLAN_VERB_MAX: usize = 15;

/// The rank a promotion lands on (`lib/clan/func.php` `clan_nextrank`): one
/// rung up the ladder with the founder rung popped off — so nothing promotes
/// to founder, and a leader "promotes" to leader (the UPDATE also clamps at
/// the actor's own rank; see [`clan_promote_rank`]).
pub fn clan_next_rank(current: u8) -> u8 {
    [CLAN_MEMBER, CLAN_OFFICER, CLAN_LEADER]
        .into_iter()
        .find(|r| *r > current)
        .unwrap_or(CLAN_LEADER)
}

/// The rank a demotion lands on (`clan_previousrank`): one rung down, founder
/// rung popped (a stepped-down founder is a leader), floor applicant.
pub fn clan_prev_rank(current: u8) -> u8 {
    [CLAN_LEADER, CLAN_OFFICER, CLAN_MEMBER]
        .into_iter()
        .find(|r| *r < current)
        .unwrap_or(CLAN_APPLICANT)
}

/// What a promote by `actor_rank` actually writes (`clan_membership.php`'s
/// `GREATEST(0, LEAST(yours, next))`): the next rung, clamped at the actor's
/// own rank — an officer lifts an applicant to member, a member to officer,
/// and can go no higher.
pub fn clan_promote_rank(actor_rank: u8, target_rank: u8) -> u8 {
    clan_next_rank(target_rank).min(actor_rank)
}

/// Whether `actor` sees the promote row for `target`
/// (`clan_membership.php`): officers+ only, target strictly below them,
/// never onto the founder rung.
pub fn clan_can_promote(actor_rank: u8, target_rank: u8) -> bool {
    actor_rank > CLAN_MEMBER && target_rank < actor_rank && target_rank < CLAN_FOUNDER
}

/// Whether `actor` sees the demote row for `target`: officers+ may demote
/// their equals-or-below (never themselves) one rung — but only while the
/// rung below isn't applicant, so a member can't be demoted (only removed).
/// The founder demoting *themselves* is the special "step down" row instead
/// (see [`clan_can_step_down`]).
pub fn clan_can_demote(actor_rank: u8, target_rank: u8, is_self: bool) -> bool {
    actor_rank > CLAN_MEMBER
        && !is_self
        && target_rank <= actor_rank
        && target_rank > CLAN_APPLICANT
        && clan_prev_rank(target_rank) > CLAN_APPLICANT
}

/// The founder's one self-demotion: "step down as founder" (31 → 30).
pub fn clan_can_step_down(actor_rank: u8, target_rank: u8, is_self: bool) -> bool {
    is_self && actor_rank == CLAN_FOUNDER && target_rank == CLAN_FOUNDER
}

/// Whether `actor` sees the remove row for `target`: officers+ may remove
/// anyone at-or-below their own rank, except themselves (that's the
/// withdraw). Removing an applicant is the application's rejection.
pub fn clan_can_remove(actor_rank: u8, target_rank: u8, is_self: bool) -> bool {
    actor_rank > CLAN_MEMBER && !is_self && target_rank <= actor_rank
}

/// The registrar's name-shape test (`applicant_new.php`'s regex): letters,
/// spaces, apostrophes, and dashes, 5–50 of them.
pub fn clan_name_valid(name: &str) -> bool {
    let n = name.chars().count();
    (CLAN_NAME_MIN..=CLAN_NAME_MAX).contains(&n)
        && name
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == ' ' || c == '\'' || c == '-')
}

/// The tag's shape test: 2–5 letters, nothing else.
pub fn clan_tag_valid(tag: &str) -> bool {
    let n = tag.chars().count();
    (CLAN_TAG_MIN..=CLAN_TAG_MAX).contains(&n) && tag.chars().all(|c| c.is_ascii_alphabetic())
}

/// The rank's display name (upstream's `$ranks` array, ours uncolored).
pub fn clan_rank_name(rank: u8) -> &'static str {
    match rank {
        r if r >= CLAN_FOUNDER => "Founder",
        r if r >= CLAN_LEADER => "Leader",
        r if r >= CLAN_OFFICER => "Officer",
        r if r >= CLAN_MEMBER => "Member",
        _ => "Applicant",
    }
}

/// The forest hunting intensities. LoGD offers easier/harder pickings that
/// shift the creature level relative to the player's own level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForestHunt {
    /// "Go Slumming" — weaker creatures (player level - 2).
    Slumming,
    /// "Look for Something to Kill" — creatures at the player's level.
    Hunt,
    /// "Go Thrillseeking" — tougher creatures (player level + 2).
    Thrillseeking,
}

impl ForestHunt {
    /// The creature level this hunt produces for a given player level. LoGD
    /// shifts the target level by ±1 for slumming/thrillseeking (a small random
    /// jitter is layered on at the call site).
    pub fn creature_level(self, player_level: u8) -> u8 {
        let delta: i16 = match self {
            ForestHunt::Slumming => -1,
            ForestHunt::Hunt => 0,
            ForestHunt::Thrillseeking => 1,
        };
        (player_level as i16 + delta).clamp(1, 16) as u8
    }
}

/// One slain foe's contribution to a forest victory settlement.
#[derive(Clone, Copy, Debug)]
pub struct SlainFoe {
    pub level: u8,
    pub gold: u32,
    pub exp: u32,
}

/// What a settled forest victory paid out, for logging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForestVictory {
    pub gold: u64,
    pub exp: u64,
    pub gem: bool,
    pub flawless: bool,
    pub turn_refunded: bool,
}

/// A persistent Green Dragon character. One per user, stored as a JSON blob.
///
/// Stats that are fully derivable (attack, defense, max HP) are *not* stored —
/// they come from `level` + equipped tiers, matching how LoGD recomputes them.
/// Every field carries a serde default so old saves load without a migration.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Character {
    /// Display name (the player's late.sh username).
    pub name: String,
    pub level: u8,
    pub experience: u64,
    /// Current hitpoints. Max is derived via [`Character::max_hitpoints`].
    pub hitpoints: u32,
    /// Equipped weapon tier, 0 (Fists) ..= 15.
    pub weapon_tier: u8,
    /// Equipped armor tier, 0 (none) ..= 15.
    pub armor_tier: u8,
    pub gold: u64,
    /// Banked gold. **Signed**: a negative balance is a live loan (LoGD lets
    /// you borrow up to `level * BORROW_PER_LEVEL`), and debt accrues interest.
    pub gold_in_bank: i64,
    /// Forest fights remaining today.
    pub turns: u32,
    /// False after a forest death; revived on the next new day.
    pub alive: bool,
    /// Whether the player has sought the dragon this run (resets per run).
    pub seen_dragon: bool,
    /// Whether the master has been challenged today (LoGD `seenmaster`,
    /// `train.php`): set when a challenge starts, cleared by a win
    /// (`multimaster` default 1) and at every dawn — the paid resurrection
    /// included. A loss locks the Proving Yard until tomorrow.
    pub seen_master_today: bool,
    /// Lifetime dragon kills.
    pub dragon_kills: u32,
    /// Days into the current run (upstream `age`, "days since level 1"): +1
    /// at every new day — the paid resurrection's included — and reset to 0
    /// by a dragon kill (`age` is absent from `dragon.php`'s preserve list).
    pub age: u32,
    /// How many days the *last completed* run took: a snapshot of [`age`]
    /// stamped at each dragon kill (upstream `dragonage`, the Hall of Fame's
    /// "Days" column). 0 until the first kill ("Unknown" upstream).
    pub dragon_age: u32,
    /// The fastest run to a kill so far: the minimum nonzero [`dragon_age`]
    /// (upstream `bestdragonage`, the "Dragon Kill Speed" ranking).
    pub best_dragon_age: u32,
    /// Revivals since the last dragon kill: +1 whenever a dead character
    /// greets a new day (dawn or paid), reset to 0 by a dragon kill —
    /// upstream `resurrections` is outside `dragon.php`'s preserve list too.
    pub resurrections: u32,
    /// Permanent max-HP bought with `hp` dragon points (+5 each).
    pub dragon_hp_bonus: u32,
    /// Permanent attack bought with `at` dragon points (+1 each).
    pub dragon_attack_bonus: u32,
    /// Permanent defense bought with `de` dragon points (+1 each).
    pub dragon_defense_bonus: u32,
    /// Permanent extra daily forest fights bought with `ff` dragon points
    /// (+1/day each, LoGD's `dkff`).
    pub dragon_ff_bonus: u32,
    /// Dragon points earned (one per kill) but not yet allocated. While any are
    /// unspent the spend gate blocks play, exactly like LoGD's new-day gate.
    pub dragon_points_unspent: u32,
    /// A full new day is owed as soon as the post-kill gates close: upstream's
    /// dragon-kill reset wipes `lasthit` (`dragon.php`'s field loop), so the
    /// very next page load runs `newday.php` in full once the spend and race
    /// gates pass. Settled by [`Character::dawn`].
    pub dawn_owed: bool,
    /// Gems: the second currency, found in the forest and spent advancing your
    /// specialty (LoGD's gem economy). Distinct from gold.
    pub gems: u64,
    /// Charm: LoGD's social stat, gained on dragon kills (`+5`). Feeds the
    /// not-yet-built flirting/marriage systems; tracked for parity.
    pub charm: u32,
    /// Soulpoints: the dead-realm HP pool (see [`Character::max_soulpoints`]).
    /// Refilled each new day to `50 + 5*level`, `+5` per master beaten; while
    /// dead, torment fights spend it and damage persists between them.
    pub soulpoints: u32,
    /// Favor with the death overlord (LoGD `deathpower`): earned tormenting
    /// souls while dead, spent restoring the soul pool, fleeing torments, and
    /// buying the paid resurrection. Persists across days and revivals, but
    /// not dragon kills (`dragon.php`'s field loop reverts it).
    pub favor: u32,
    /// Torment fights remaining today in the graveyard (LoGD `gravefights`).
    /// Refilled on a normal new day, *not* by the paid resurrection.
    pub grave_fights: u32,
    /// Persistent combat companions (e.g. a Bonecall skeleton). They fight
    /// alongside you across battles until destroyed (LoGD `apply_companion`).
    pub companions: Vec<Companion>,
    /// Chosen ancestry (LoGD's race). `None` arms the forced choice gate on
    /// load; permanent once picked (until phase 3's transmutation potion).
    pub race: Race,
    /// The current dragon-kill title, shown before the name. Assigned from the
    /// title ladder at first load and re-rolled on every dragon kill
    /// (`dragon.php` + `lib/titles.php`). Empty only on a never-titled save.
    pub title: String,
    /// Which title column (and later, phase-3 flavor) this character uses.
    pub style: AddressStyle,
    /// Chosen combat specialty, picked once but switchable at the inn's
    /// barkeep. `None` until the player decides (LoGD sets it on the first
    /// new day).
    pub specialty: Specialty,
    /// Lifetime skill points in the *current* specialty. Advanced by training
    /// (gems) and by certain forest events. Every 3rd point grants a use.
    pub specialty_skill: u32,
    /// Spendable specialty "uses" for today: `floor(skill/3)` refreshed each new
    /// day, +1 bonus for the specialty you actually chose.
    pub specialty_uses: u32,
    /// Benched (skill, uses) per specialty path, indexed by
    /// [`specialty_index`]. Upstream keeps each specialty module's skill/uses
    /// in its own prefs, so switching paths at the barkeep resumes the other
    /// path where it left off ("you'll have to build up some points in this
    /// one") instead of carrying the current skill across.
    pub benched_specialties: [(u32, u32); 3],
    /// Permanent max-HP bought from the inn's vitality tonic (+1 per dose).
    /// Survives dragon kills (`carrydk` default 1) and feeds investment
    /// scaling exactly like boon HP (upstream's extra HP rides
    /// `maxhitpoints`, which `buffbadguy` reads).
    pub vitality_hp: u32,
    /// Buffs that outlive a single encounter (drinks, the lover's ward,
    /// transmutation sickness). Injected into each fight and ticked down by
    /// combat rounds; stripped by death, dragon kills, and (unless flagged)
    /// the new day.
    pub persistent_buffs: Vec<PersistedBuff>,
    /// Stabled mount: 0 = none, else 1-based index into [`data::MOUNTS`].
    pub mount: u8,
    /// Mounted combat rounds left today: while > 0, fight rounds ride the
    /// mount's attack bonus and burn one each. Refreshed to the mount's
    /// allowance each new day.
    pub mount_rounds_left: u32,
    /// Married to the realm's romance NPC (upstream's INT_MAX `marriedto`
    /// sentinel). Which partner is implied by [`Character::style`].
    pub married: bool,
    /// Drunkenness 0..=100. Sobers by 10% per forest search, resets at dawn
    /// (with a hangover turn dock above 66), on death, and on a dragon kill.
    pub drunkenness: u32,
    /// Hard drinks downed today (max [`HARD_DRINKS_PER_DAY`]).
    pub hard_drinks_today: u32,
    /// Paid for an inn room today (`boughtroomtoday`).
    pub lodged_today: bool,
    /// Spent time with the partner today (`seenlover`).
    pub flirted_today: bool,
    /// Heard the bard today (`sethsong` allows one song per day).
    pub heard_bard_today: bool,
    /// Used the forest outhouse today.
    pub used_outhouse_today: bool,
    /// Played the petting zoo's basket game today (upstream `crazyaudrey`'s
    /// `played` pref, reset by its newday hook). Gates only the *village*
    /// game; the forest basket event is ungated, exactly as upstream.
    pub zoo_played_today: bool,
    /// Five Sixes plays today (max [`FIVESIX_PLAYS_PER_DAY`]).
    pub fivesix_plays_today: u32,
    /// Bounty contracts placed today (max [`BOUNTIES_PER_DAY`]; upstream
    /// keeps this as a dag module pref, reset by its newday hook).
    pub bounties_set_today: u32,
    /// The new-post watermark (upstream `recentcomments`): comments from
    /// this UTC day-number on render marked in every talk room. Advanced at
    /// each new day to the *previous* dawn's day — `newday.php` sets
    /// `recentcomments = lasthit` then `lasthit = now`, and `last_day` is
    /// exactly that `lasthit` at the blob's day granularity.
    pub comments_seen_before_day: i64,
    /// Gold sent away through the bank today (`amountouttoday`), capped at
    /// `level * MAX_TRANSFER_OUT_PER_LEVEL`.
    pub amount_out_today: u64,
    /// Bank transfers received today (`transferredtoday`, max
    /// [`TRANSFERS_RECEIVED_PER_DAY`]): checked and bumped by the *sender's*
    /// settlement against this fresh blob, like the PvP writes.
    pub transfers_received_today: u32,
    /// PvP attacks left today (upstream `playerfights`): spent at engage,
    /// refilled to [`PVP_FIGHTS_PER_DAY`] by a normal dawn only (the paid
    /// resurrection skips them, exactly like grave fights).
    pub player_fights: u32,
    /// Forfeited newbie immunity by attacking while immune (upstream `pk`).
    /// Permanent — it never resets, not even across dragon kills.
    pub pk: bool,
    /// Epoch seconds when an attacker last engaged this character (upstream
    /// `pvpflag`): for [`PVP_TIMEOUT_SECS`] after, the target lists hold
    /// everyone else off. 0 = never engaged. Stamped by *other* sessions'
    /// engage transactions, through the DB.
    pub pvp_engaged_at: i64,
    /// Unread reports of what happened while this character slept (upstream's
    /// system mail): PvP settlements append here through the DB; the next
    /// door entry drains them into the log.
    pub pvp_reports: Vec<String>,
    /// The name of the shade riding this character's dreams (upstream
    /// `hauntedby`, the haunter's titled name), written cross-player through
    /// the DB. Non-empty means the next new day — dawn or paid — docks a
    /// turn and clears it. Empty = unhaunted.
    pub haunted_by: String,
    /// Clan membership (upstream `accounts.clanid`): `None` = clanless. Set
    /// by applying or founding; cleared by withdrawing, being removed (a
    /// cross-player write), or the clan's deletion. Survives dragon kills
    /// (`dragon.php`'s preserve list) and death.
    pub clan_id: Option<uuid::Uuid>,
    /// Clan rank rung (`clanrank`): [`CLAN_APPLICANT`] 0 / [`CLAN_MEMBER`]
    /// 10 / [`CLAN_OFFICER`] 20 / [`CLAN_LEADER`] 30 / [`CLAN_FOUNDER`] 31.
    /// Changed by officers+ through cross-player writes.
    pub clan_rank: u8,
    /// Epoch seconds of joining or applying (`clanjoindate`): the
    /// succession tie-break (highest rank, then oldest join, inherits a
    /// leaderless clan).
    pub clan_joined_at: i64,
    /// The clan's tag, denormalized off the clan row at apply/found time
    /// (tags are immutable here — upstream's rename is superuser tooling).
    /// Rendered `<TAG>` before the name in every comment area while
    /// `clan_rank > 0`, upstream's live-join render.
    pub clan_tag: String,
    /// UTC day-number of the last new-day reset, for turn/heal regeneration.
    pub last_day: i64,
    /// Presence flag mirroring upstream's `loggedin`: stamped true when the
    /// door opens (and by every in-play save), cleared by the leave save. A
    /// crashed session leaves it stale — the roster ANDs it with a 15-minute
    /// activity window, exactly as upstream pairs `loggedin` with `laston`.
    pub online: bool,
}

/// A buff that persists on the character between fights (a drink's buzz, the
/// lover's ward, transmutation sickness). A serde-able core of the in-fight
/// [`Buff`]: the fields any village-granted buff actually uses. `slot` mirrors
/// upstream's `apply_buff` key — re-applying to an occupied slot replaces it
/// (a new drink replaces the old buzz), except sickness which stacks rounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PersistedBuff {
    /// The `apply_buff` slot key ("drink", "lover", "transmute", ...).
    pub slot: String,
    /// Display name.
    pub name: String,
    /// Combat rounds left before it wears off.
    pub rounds_left: u32,
    pub player_atk_mod: f32,
    pub player_def_mod: f32,
    pub player_dmg_mod: f32,
    /// Reflected fraction of damage taken (the black cask's kickback).
    pub damage_shield: f32,
    /// Survives the daily reset (transmutation sickness does; drinks don't).
    pub survives_new_day: bool,
    /// Flavor logged the round it wears off.
    pub wearoff: String,
}

impl Default for PersistedBuff {
    fn default() -> Self {
        PersistedBuff {
            slot: String::new(),
            name: String::new(),
            rounds_left: 0,
            player_atk_mod: 1.0,
            player_def_mod: 1.0,
            player_dmg_mod: 1.0,
            damage_shield: 0.0,
            survives_new_day: false,
            wearoff: String::new(),
        }
    }
}

impl PersistedBuff {
    /// The in-fight buff this persists. The encounter ticks it; the leftover
    /// rounds are written back when the fight ends.
    pub fn as_buff(&self) -> Buff {
        let mut b = Buff::new(self.name.clone(), self.rounds_left);
        b.player_atk_mod = self.player_atk_mod;
        b.player_def_mod = self.player_def_mod;
        b.player_dmg_mod = self.player_dmg_mod;
        b.damage_shield = self.damage_shield;
        b.wearoff = self.wearoff.clone();
        b
    }
}

/// What a new day's shared module effects did (for log/news wiring).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewDayFx {
    /// The marriage ended at dawn (charm ran out) — makes the paper.
    pub divorced: bool,
    /// Woke up hungover (-1 turn).
    pub hangover: bool,
    /// A shade rode last night's dreams (-1 turn): the haunter's name, taken
    /// off the cleared mark (`newday.php`'s unconditional `hauntedby` block —
    /// it fires on the paid resurrection too).
    pub haunted_by: Option<String>,
}

/// The five potions on the barkeep's back shelf
/// (`modules/cedrikspotions.php`), each [`POTION_COST_GEMS`] a dose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PotionKind {
    /// +1 charm per dose (`charmgain` 1).
    Charm,
    /// Permanent +1 max HP and +1 current per dose (`vitalgain` 1); survives
    /// dragon kills (`carrydk` default 1).
    Vitality,
    /// Heal to full, then +[`MENDING_OVERHEAL`] over max (`tempgain` 20).
    Mending,
    /// Drop the specialty; the village chooser re-arms. Single dose.
    Forgetting,
    /// Drop the race (the gate re-arms at next load) and take transmutation
    /// sickness. Single dose; a repeat dose stacks the sickness rounds.
    Transmutation,
}

/// The back shelf in display order.
pub const POTIONS: [PotionKind; 5] = [
    PotionKind::Charm,
    PotionKind::Vitality,
    PotionKind::Mending,
    PotionKind::Forgetting,
    PotionKind::Transmutation,
];

impl PotionKind {
    /// Display name (ours; the upstream shelf's names are theirs).
    pub fn name(self) -> &'static str {
        match self {
            PotionKind::Charm => "Rosewater Tonic",
            PotionKind::Vitality => "Oakblood Tonic",
            PotionKind::Mending => "Mending Draught",
            PotionKind::Forgetting => "Draught of Forgetting",
            PotionKind::Transmutation => "Transmutation Draught",
        }
    }

    /// One-line shelf description for the menu row.
    pub fn blurb(self) -> &'static str {
        match self {
            PotionKind::Charm => "+1 charm",
            PotionKind::Vitality => "+1 max hitpoint, forever",
            PotionKind::Mending => "heal to full, and then some",
            PotionKind::Forgetting => "unlearn your specialty",
            PotionKind::Transmutation => "shed your ancestry (you will be ill)",
        }
    }
}

/// The transmutation draught's lingering sickness: atk/def x0.75 for
/// [`TRANSMUTE_ROUNDS`] combat rounds, surviving the new day (the one stock
/// debuff with `survivenewday`). Re-dosing stacks rounds via
/// [`Character::apply_persistent_buff`]'s "transmute" slot rule.
pub fn transmute_sickness() -> PersistedBuff {
    PersistedBuff {
        slot: "transmute".into(),
        name: "Transmutation Sickness".into(),
        rounds_left: TRANSMUTE_ROUNDS,
        player_atk_mod: 0.75,
        player_def_mod: 0.75,
        survives_new_day: true,
        wearoff: "The transmutation sickness finally passes.".into(),
        ..PersistedBuff::default()
    }
}

/// A village visit has this chance (percent) of Mad Juna setting up her
/// petting zoo in the square (`crazyaudrey` `villagepercent` 20, rolled
/// `e_rand(1,100) <= 20` on every village pageview).
pub const ZOO_CHANCE_PCT: u32 = 20;
/// The pitch on her sign (`crazyaudrey` `cost` 5): what a petting costs.
pub const ZOO_PET_COST: u64 = 5;
/// Combat rounds the petting's calm lasts (`apply_buff('crazyaudrey')`).
pub const ZOO_BUFF_ROUNDS: u32 = 5;

/// The petting zoo's take-home (`crazyaudrey_run` op=pet): defense x1.05 for
/// [`ZOO_BUFF_ROUNDS`] rounds. Slot-keyed so a second petting replaces the
/// first, exactly upstream's `apply_buff` slot behavior.
pub fn zoo_buff() -> PersistedBuff {
    PersistedBuff {
        slot: "zoo".into(),
        name: "Soft Reassurance".into(),
        rounds_left: ZOO_BUFF_ROUNDS,
        player_def_mod: 1.05,
        wearoff: "The soft reassurance of the petting zoo fades.".into(),
        ..PersistedBuff::default()
    }
}

/// The lover's ward (`lovers_getbuff`): defense x1.2 for 60 rounds, granted
/// by a successful married visit and on the wedding itself.
pub fn lover_buff(partner: &str) -> PersistedBuff {
    PersistedBuff {
        slot: "lover".into(),
        name: "Lover's Ward".into(),
        rounds_left: LOVER_BUFF_ROUNDS,
        player_def_mod: 1.2,
        wearoff: format!("You find yourself missing {partner}."),
        ..PersistedBuff::default()
    }
}

/// Success chance (percent) of bribing the barkeep with `gems` gems (1-3):
/// `gems * 30` (`inn_bartender.php`).
pub fn gem_bribe_chance(gems: u32) -> u32 {
    gems * 30
}

/// Success chance (percent) of a gold bribe of `amount` at `level`:
/// `(amount/level - 10) * (50/90) + 25` — 25% / ~47% / 75% at the three
/// stock amounts (`level*10/50/100`).
pub fn gold_bribe_chance(amount: u64, level: u8) -> f64 {
    (amount as f64 / level.max(1) as f64 - 10.0) * (50.0 / 90.0) + 25.0
}

/// Gold a PvP winner takes: `round(10 * loserLevel * ln(max(1, loserGold)))`
/// (`lib/pvpsupport.php`, both directions — the log keeps huge purses from
/// changing hands). The loser's actual loss is separate: the whole purse for
/// a slain attacker, the winner's cut for a slain sleeper.
pub fn pvp_win_gold(loser_level: u8, loser_gold: u64) -> u64 {
    (10.0 * loser_level as f64 * (loser_gold.max(1) as f64).ln()).round() as u64
}

/// Experience a winning attacker gains off a slain sleeper (`pvpvictory`):
/// the base [`PVP_ATTACKER_GAIN_PCT`]% of the victim's engage-time
/// experience is rounded first, then the level-difference bonus
/// `round(base * (1 + 0.1*(victimLvl - mineLvl)) - base)` — which can be
/// negative ("the simplistic nature of this fight") — is added. Returns
/// `(total, bonus)`; the bonus gets its own log line.
pub fn pvp_attacker_exp(victim_exp: u64, victim_level: u8, my_level: u8) -> (u64, i64) {
    let base = (PVP_ATTACKER_GAIN_PCT as f64 * victim_exp as f64 / 100.0).round();
    let bonus =
        (base * (1.0 + 0.1 * (victim_level as f64 - my_level as f64)) - base).round() as i64;
    (((base as i64 + bonus).max(0)) as u64, bonus)
}

/// Index of a chooseable specialty into [`Character::benched_specialties`].
pub fn specialty_index(s: Specialty) -> Option<usize> {
    match s {
        Specialty::None => None,
        Specialty::Mystical => Some(0),
        Specialty::DarkArts => Some(1),
        Specialty::Thief => Some(2),
    }
}

/// The four permanent upgrades a dragon point can buy (LoGD `dragonpointspend`:
/// `hp`/`ff`/`at`/`de`). One point is earned per dragon kill and must be
/// allocated before the next day's play begins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DragonPointKind {
    /// +5 permanent max hitpoints.
    Hp,
    /// +1 permanent daily forest fight.
    ForestFights,
    /// +1 permanent attack.
    Attack,
    /// +1 permanent defense.
    Defense,
}

impl DragonPointKind {
    /// Short display label for the spend menu.
    pub fn label(self) -> &'static str {
        match self {
            DragonPointKind::Hp => "+5 max hitpoints",
            DragonPointKind::ForestFights => "+1 forest fight per day",
            DragonPointKind::Attack => "+1 attack",
            DragonPointKind::Defense => "+1 defense",
        }
    }
}

/// The four ancestries, mirroring LoGD's stock race modules
/// (`racehuman`/`raceelf`/`racedwarf`/`racetroll`). A forced one-time choice at
/// the new-day gate; `None` re-arms it (fresh characters, and phase 3's
/// transmutation potion). **Effect numbers are upstream's exactly; the race
/// names are original to late.sh** (the generic analogs are noted per variant).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Race {
    /// Unchosen; the race gate arms on load.
    #[default]
    None,
    /// The human-analog: tireless plains stock, +2 forest fights per day.
    Plainsborn,
    /// The elf-analog: wary forest folk, `+1 + level/5` defense.
    Wealdkin,
    /// The dwarf-analog: delvers of the under-roads, creature gold x1.2 and
    /// a near-immunity to mine cave-ins.
    Deepfolk,
    /// The troll-analog: crag-born brutes, `+1 + level/5` attack.
    Cragborn,
}

/// The four choosable ancestries, in menu order.
pub const RACES: [Race; 4] = [
    Race::Plainsborn,
    Race::Wealdkin,
    Race::Deepfolk,
    Race::Cragborn,
];

impl Race {
    /// Short display label for the stat rail.
    pub fn name(self) -> &'static str {
        match self {
            Race::None => "Unchosen",
            Race::Plainsborn => "Plainsborn",
            Race::Wealdkin => "Wealdkin",
            Race::Deepfolk => "Deepfolk",
            Race::Cragborn => "Cragborn",
        }
    }

    /// The level-scaled stat bonus the elf/troll analogs share:
    /// `1 + floor(level / 5)` (`raceelf.php`/`racetroll.php` `adjuststats`).
    fn scaling_bonus(level: u8) -> u32 {
        1 + level as u32 / 5
    }

    /// Permanent attack bonus (the Cragborn's brawn). Upstream implements this
    /// as a `racialbenefit` buff recomputed each new day; a flat add into the
    /// attack stat is numerically identical. Buffs don't follow the dead, and
    /// neither does this: [`Character::dead_combatant`] ignores it.
    pub fn attack_bonus(self, level: u8) -> u32 {
        match self {
            Race::Cragborn => Self::scaling_bonus(level),
            _ => 0,
        }
    }

    /// Permanent defense bonus (the Wealdkin's wariness); see
    /// [`Race::attack_bonus`] for the shape.
    pub fn defense_bonus(self, level: u8) -> u32 {
        match self {
            Race::Wealdkin => Self::scaling_bonus(level),
            _ => 0,
        }
    }

    /// Extra daily forest fights (`racehuman.php`'s `newday` hook). Applies to
    /// the paid resurrection's docked day too, exactly like the upstream hook.
    pub fn daily_forest_bonus(self) -> u32 {
        match self {
            Race::Plainsborn => PLAINSBORN_FOREST_BONUS,
            _ => 0,
        }
    }

    /// Scale a forest creature's gold drop (`racedwarf.php`'s
    /// `creatureencounter` hook: `round(gold * 1.2)`). Fires where upstream's
    /// hook does: after `buff_foe`, before the thrillseeking x1.1.
    pub fn creature_gold(self, gold: u32) -> u32 {
        match self {
            Race::Deepfolk => (gold as f64 * 1.2).round() as u32,
            _ => gold,
        }
    }

    /// Percent chance a goldmine cave-in kills (`raceminedeath`): rolled as
    /// `e_rand(1,100) < chance`, so the Deepfolk almost always dig free.
    pub fn mine_death_percent(self) -> u32 {
        match self {
            Race::Deepfolk => DEEPFOLK_MINE_DEATH_PERCENT,
            _ => MINE_DEATH_PERCENT,
        }
    }
}

/// The two address styles. Upstream keys DK titles, the romance partner, and
/// one bard outcome off a binary `sex` field; our adaptation is a flavor
/// choice that picks the title column (and the partner, and bard outcome 15).
/// `Unchosen` arms the one-time chooser gate at load; until it's picked,
/// first-style titles render.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressStyle {
    /// Not yet picked; the style gate arms on load.
    #[default]
    Unchosen,
    First,
    Second,
}

/// The three combat specialties, mirroring LoGD's `MP`/`DA`/`TS`. The in-fight
/// skills each unlocks live in [`super::combat`]; `None` is the undecided state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Specialty {
    /// Undecided — no specialty chosen yet.
    #[default]
    None,
    /// Mystical Powers: regeneration, earth fist, life siphon, lightning aura.
    Mystical,
    /// Dark Arts: skeleton minions, voodoo, the foe-weakening curse, wither.
    DarkArts,
    /// Thief skills: insult, poison blade, hidden attack, backstab.
    Thief,
}

impl Specialty {
    /// Short display label.
    pub fn name(self) -> &'static str {
        match self {
            Specialty::None => "None",
            Specialty::Mystical => "Mystical Powers",
            Specialty::DarkArts => "Dark Arts",
            Specialty::Thief => "Thief Skills",
        }
    }
}

impl Default for Character {
    fn default() -> Self {
        Character {
            name: String::new(),
            level: 1,
            experience: 0,
            hitpoints: HP_PER_LEVEL,
            weapon_tier: 0,
            armor_tier: 0,
            gold: START_GOLD,
            gold_in_bank: 0,
            turns: TURNS_PER_DAY,
            alive: true,
            seen_dragon: false,
            seen_master_today: false,
            dragon_kills: 0,
            age: 0,
            dragon_age: 0,
            best_dragon_age: 0,
            resurrections: 0,
            dragon_hp_bonus: 0,
            dragon_attack_bonus: 0,
            dragon_defense_bonus: 0,
            dragon_ff_bonus: 0,
            dragon_points_unspent: 0,
            dawn_owed: false,
            gems: 0,
            charm: 0,
            // Fresh level-1 soulpoints: 50 + 5*1 (LoGD new-day formula).
            soulpoints: 55,
            favor: 0,
            // Upstream rolls a new day at a fresh account's first login, which
            // fills the daily torment pool; seed it directly.
            grave_fights: GRAVE_FIGHTS_PER_DAY,
            companions: Vec::new(),
            race: Race::None,
            title: String::new(),
            style: AddressStyle::First,
            specialty: Specialty::None,
            specialty_skill: 0,
            specialty_uses: 0,
            benched_specialties: [(0, 0); 3],
            vitality_hp: 0,
            persistent_buffs: Vec::new(),
            mount: 0,
            mount_rounds_left: 0,
            married: false,
            drunkenness: 0,
            hard_drinks_today: 0,
            lodged_today: false,
            flirted_today: false,
            heard_bard_today: false,
            used_outhouse_today: false,
            zoo_played_today: false,
            fivesix_plays_today: 0,
            bounties_set_today: 0,
            comments_seen_before_day: 0,
            amount_out_today: 0,
            transfers_received_today: 0,
            // Seeded like grave fights: the skipped first-login new day would
            // have filled the day's PvP pool.
            player_fights: PVP_FIGHTS_PER_DAY,
            pk: false,
            pvp_engaged_at: 0,
            pvp_reports: Vec::new(),
            haunted_by: String::new(),
            clan_id: None,
            clan_rank: CLAN_APPLICANT,
            clan_joined_at: 0,
            clan_tag: String::new(),
            last_day: 0,
            online: false,
        }
    }
}

impl Character {
    /// A brand-new level-1 character for `name`, stamped with the current day.
    pub fn new(name: impl Into<String>, today: i64) -> Self {
        Character {
            name: name.into(),
            last_day: today,
            // Upstream rolls a fresh account's first new day at first login
            // ("It is day number 1"); we skip that roll (`last_day` is
            // today), so the run age is seeded directly.
            age: 1,
            ..Character::default()
        }
    }

    /// The name as the realm knows it: the dragon-kill title, then the name
    /// (LoGD renders "Farmboy Name" everywhere).
    pub fn titled_name(&self) -> String {
        if self.title.is_empty() {
            self.name.clone()
        } else {
            format!("{} {}", self.title, self.name)
        }
    }

    /// The name as a comment area shows it: `<TAG>` before the bare name for
    /// real clan members (upstream tags every comment area, rank > 0 only —
    /// applicants stay bare-named).
    pub fn commentary_name(&self) -> String {
        if self.clan_rank > CLAN_APPLICANT && !self.clan_tag.is_empty() {
            format!("<{}> {}", self.clan_tag, self.name)
        } else {
            self.name.clone()
        }
    }

    /// Enroll with a clan: as its rank-0 applicant (`applicant_apply.php`)
    /// or its rank-31 founder (`applicant_new.php`).
    pub fn join_clan(&mut self, clan_id: uuid::Uuid, tag: &str, rank: u8, now_secs: i64) {
        self.clan_id = Some(clan_id);
        self.clan_rank = rank;
        self.clan_joined_at = now_secs;
        self.clan_tag = tag.to_string();
    }

    /// Clear membership (withdraw, removal, or the clan's deletion) — the
    /// same reset every upstream path writes: `clanid=0`, rank applicant,
    /// join date zeroed.
    pub fn leave_clan(&mut self) {
        self.clan_id = None;
        self.clan_rank = CLAN_APPLICANT;
        self.clan_joined_at = 0;
        self.clan_tag = String::new();
    }

    /// Maximum hitpoints: `10 * level` plus any retained dragon-kill bonus and
    /// vitality-tonic doses.
    pub fn max_hitpoints(&self) -> u32 {
        HP_PER_LEVEL * self.level as u32 + self.dragon_hp_bonus + self.vitality_hp
    }

    /// Attack stat fed to the combat roll: `level + weapon_tier` plus dragon
    /// boons and any level-scaled ancestry bonus.
    pub fn attack(&self) -> u32 {
        self.level as u32
            + self.weapon_tier as u32
            + self.dragon_attack_bonus
            + self.race.attack_bonus(self.level)
    }

    /// Defense stat fed to the combat roll: `level + armor_tier` plus dragon
    /// boons and any level-scaled ancestry bonus.
    pub fn defense(&self) -> u32 {
        self.level as u32
            + self.armor_tier as u32
            + self.dragon_defense_bonus
            + self.race.defense_bonus(self.level)
    }

    /// The player as a [`Combatant`] for [`super::combat::resolve_round`].
    pub fn combatant(&self) -> Combatant {
        Combatant {
            attack: self.attack(),
            defense: self.defense(),
        }
    }

    /// Maximum soulpoints, the dead-realm HP pool ceiling: `5*level + 50`.
    /// LoGD computes this fresh everywhere and never stores it.
    pub fn max_soulpoints(&self) -> u32 {
        5 * self.level as u32 + 50
    }

    /// The player as a combatant while dead (`graveyard.php`): gear and boons
    /// mean nothing beyond the grave — attack and defense are both
    /// `10 + round((level - 1) * 1.5)`.
    pub fn dead_combatant(&self) -> Combatant {
        let stat = 10 + ((self.level as f64 - 1.0) * 1.5).round() as u32;
        Combatant {
            attack: stat,
            defense: stat,
        }
    }

    /// Favor the mausoleum charges to restore the soul pool to max:
    /// `round(10 * missing / max)` (`lib/graveyard/case_restore.php`), 0..=10
    /// scaling with depletion.
    pub fn soul_restore_cost(&self) -> u32 {
        let max = self.max_soulpoints();
        let missing = max.saturating_sub(self.soulpoints);
        (10.0 * missing as f64 / max as f64).round() as u32
    }

    /// Pay favor to restore soulpoints to max. Returns the favor spent, or
    /// `None` if already whole or the favor can't cover the price.
    pub fn restore_soul(&mut self) -> Option<u32> {
        if self.soulpoints >= self.max_soulpoints() {
            return None;
        }
        let cost = self.soul_restore_cost();
        if self.favor < cost {
            return None;
        }
        self.favor -= cost;
        self.soulpoints = self.max_soulpoints();
        Some(cost)
    }

    /// The paid resurrection (`case_resurrection.php` + `newday.php` with
    /// `resurrection=true`): [`RESURRECTION_FAVOR_COST`] favor buys an
    /// immediate extra new day — revive, heal to full, and take
    /// `base + ff + `[`RESURRECTION_TURNS`] turns for what's left of today.
    /// Like upstream's flagged new day it settles bank interest, refreshes
    /// specialty uses, and runs the daily module effects (mount, hangover,
    /// marriage upkeep — `modulehook("newday")` fires regardless of the
    /// flag), but does **not** refill soulpoints or grave fights, and leaves
    /// `last_day` alone so the real next dawn still rolls a full day.
    /// Returns `None` (spending nothing) if alive or short on favor.
    pub fn resurrect(&mut self, interest_percent: u32, rng: &mut impl Rng) -> Option<NewDayFx> {
        if self.alive || self.favor < RESURRECTION_FAVOR_COST {
            return None;
        }
        self.favor -= RESURRECTION_FAVOR_COST;
        // The paid resurrection is an extra new day: the run ages a day and
        // the revival is counted, same as the passive dawn (`newday.php`
        // increments both regardless of the `resurrection` flag).
        self.age += 1;
        self.resurrections += 1;
        // The watermark line runs on the resurrection day too (`newday.php`
        // line 254 is unconditional); at day granularity the morning's dawn
        // and the resurrection share `last_day`.
        self.comments_seen_before_day = self.last_day;
        self.apply_new_day_interest(interest_percent);
        // The race's `newday` hook fires on the resurrection day too
        // (`newday.php` runs `modulehook("newday")` regardless of the flag),
        // so a Plainsborn's bonus fights soften the -6 dock.
        let turns = TURNS_PER_DAY as i32
            + self.dragon_ff_bonus as i32
            + self.race.daily_forest_bonus() as i32
            + RESURRECTION_TURNS;
        self.turns = turns.max(0) as u32;
        let fx = self.newday_shared_effects(rng);
        self.refresh_specialty_uses();
        self.alive = true;
        self.seen_dragon = false;
        self.hitpoints = self.max_hitpoints();
        Some(fx)
    }

    /// Re-roll the dragon-kill title off the ladder for the current kill
    /// count (`dragon.php` re-titles on every kill; the load path uses this to
    /// stamp never-titled saves). The address style picks the column.
    pub fn reroll_title(&mut self, rng: &mut impl Rng) {
        let (first, second) = data::dk_title_pair(self.dragon_kills, rng);
        self.title = match self.style {
            AddressStyle::Second => second,
            // The unchosen render first-style until the gate is answered.
            AddressStyle::First | AddressStyle::Unchosen => first,
        }
        .to_string();
    }

    /// Experience required to advance to the next level (with DK scaling).
    pub fn exp_for_next_level(&self) -> u64 {
        data::exp_to_advance(self.level, self.dragon_kills)
    }

    /// Whether the player has banked enough experience to challenge their
    /// master. (Beating the master is what actually advances the level.)
    pub fn can_challenge_master(&self) -> bool {
        self.level < data::MAX_LEVEL
            && self.experience >= self.exp_for_next_level()
            && !self.seen_master_today
    }

    /// Whether the Seek-the-Dragon option is available: level 15, not yet
    /// sought this run.
    pub fn can_seek_dragon(&self) -> bool {
        self.level >= data::MAX_LEVEL && !self.seen_dragon
    }

    /// Advance one level after beating the master: +1 level (so +10 max HP, +1
    /// attack, +1 defense via derivation), +5 soulpoints, then heal to full.
    pub fn advance_level(&mut self) {
        if self.level < data::MAX_LEVEL {
            self.level += 1;
            self.soulpoints = self.soulpoints.saturating_add(SOULPOINTS_PER_MASTER);
            self.hitpoints = self.max_hitpoints();
            // A win unlocks the next master immediately (`train.php` clears
            // `seenmaster` on victory, `multimaster` default 1).
            self.seen_master_today = false;
            // Companions train alongside you (`train.php` `companionslevelup`
            // default 1): each gains its per-level growth and heals to full.
            for comp in &mut self.companions {
                comp.attack += comp.attack_per_level as f64;
                comp.defense += comp.defense_per_level as f64;
                comp.max_hitpoints += comp.hp_per_level;
                comp.hitpoints = comp.max_hitpoints;
            }
        }
    }

    /// Whether the master hunts you down on the next village visit
    /// (`village.php`'s `automaster` default 1): banked experience strictly
    /// past what the *next* level's advancement requires, with the yard
    /// still open today.
    pub fn truant(&self) -> bool {
        self.alive
            && self.level < data::MAX_LEVEL
            && !self.seen_master_today
            && self.experience > data::exp_to_advance(self.level + 1, self.dragon_kills)
    }

    /// The master fought to advance from the current level, as a combatant.
    pub fn current_master(&self) -> Option<(data::Master, Combatant, u32)> {
        if self.level >= data::MAX_LEVEL {
            return None;
        }
        let master = data::MASTERS[(self.level - 1) as usize];
        let (atk, def, hp) = data::master_stats(self.level);
        Some((
            master,
            Combatant {
                attack: atk,
                defense: def,
            },
            hp,
        ))
    }

    // --- endgame investment scaling (LoGD `dragon.php` / `train.php`) --------
    //
    // The dragon and your master grow with how much *permanent* power you've
    // banked, so buying boons makes those fights keep pace instead of trivially
    // out-gearing a fixed foe. Without this, enough Gypsy purchases make you
    // undefeatable; this is LoGD's fix, transcribed.

    /// Banked permanent power the endgame scales against: attack + defense
    /// boons, plus earned HP over the level-15 base (each 5 HP = 1 point).
    /// Vitality-tonic HP counts too — upstream's extra HP rides
    /// `maxhitpoints`, which feeds `buffbadguy`'s `(maxhp - level*10)/5` term.
    fn investment_points(&self) -> u32 {
        self.dragon_attack_bonus
            + self.dragon_defense_bonus
            + (self.dragon_hp_bonus + self.vitality_hp) / 5
    }

    /// Randomly split `points` into (attack, defense, hp) flux: +1 attack or
    /// defense per point, +5 HP per leftover point (the dragon's toughening
    /// roll, `dragon.php`).
    fn partition_flux(points: u32, rng: &mut impl Rng) -> (u32, u32, u32) {
        let atk = rng.gen_range(0..=points);
        let def = rng.gen_range(0..=(points - atk));
        let hp = (points - atk - def) * 5;
        (atk, def, hp)
    }

    /// The Green Dragon's effective stats for this fight (`dragon.php`): base
    /// 45/25/300 plus a random flux over `round(investment * 0.75)` points.
    pub fn scaled_dragon(&self, rng: &mut impl Rng) -> (u32, u32, u32) {
        let points = (self.investment_points() as f64 * 0.75).round() as u32;
        let (a, d, h) = Self::partition_flux(points, rng);
        (
            data::DRAGON_ATTACK + a,
            data::DRAGON_DEFENSE + d,
            data::DRAGON_HP + h,
        )
    }

    /// The current master scaled by investment (`train.php`): base stats plus a
    /// flux over `round(investment * 0.33)` points, attack/defense each capped at
    /// a quarter of that. Returns `None` past the max level (no master).
    pub fn scaled_master(&self, rng: &mut impl Rng) -> Option<(data::Master, Combatant, u32)> {
        let (master, base, hp) = self.current_master()?;
        let points = (self.investment_points() as f64 * 0.33).round() as u32;
        let cap = (points as f64 * 0.25).round() as u32;
        // Upstream rolls over the whole budget and *clamps* each share to the
        // quarter cap (`min(e_rand(0,dk), round(dk*.25))`, `train.php`) — a
        // spike at the cap, not a uniform pick beneath it.
        let a = rng.gen_range(0..=points).min(cap);
        let d = rng.gen_range(0..=(points - a)).min(cap);
        let h = (points - a - d) * 5;
        Some((
            master,
            Combatant {
                attack: base.attack + a,
                defense: base.defense + d,
            },
            hp + h,
        ))
    }

    /// Cost in gold to upgrade to `target_tier`, crediting a 75% trade-in on the
    /// currently equipped item of `current_tier`. Returns `None` if the target
    /// is not a strict upgrade or is out of range.
    fn upgrade_cost(current_tier: u8, target_tier: u8) -> Option<u64> {
        if target_tier == 0 || target_tier as usize > data::COST_LADDER.len() {
            return None;
        }
        if target_tier <= current_tier {
            return None;
        }
        let cost = data::COST_LADDER[(target_tier - 1) as usize] as f64;
        let trade_in = if current_tier == 0 {
            0.0
        } else {
            data::COST_LADDER[(current_tier - 1) as usize] as f64 * data::TRADE_IN_FRACTION as f64
        };
        Some((cost - trade_in).max(0.0).round() as u64)
    }

    /// Cost to upgrade the weapon to `tier`, or `None` if not a valid upgrade.
    pub fn weapon_upgrade_cost(&self, tier: u8) -> Option<u64> {
        Self::upgrade_cost(self.weapon_tier, tier)
    }

    /// Cost to upgrade the armor to `tier`, or `None` if not a valid upgrade.
    pub fn armor_upgrade_cost(&self, tier: u8) -> Option<u64> {
        Self::upgrade_cost(self.armor_tier, tier)
    }

    /// Attempt to buy weapon `tier`, spending on-hand gold. Returns true on
    /// success.
    pub fn buy_weapon(&mut self, tier: u8) -> bool {
        match self.weapon_upgrade_cost(tier) {
            Some(cost) if self.gold >= cost => {
                self.gold -= cost;
                self.weapon_tier = tier;
                true
            }
            _ => false,
        }
    }

    /// Attempt to buy armor `tier`, spending on-hand gold. Returns true on
    /// success.
    pub fn buy_armor(&mut self, tier: u8) -> bool {
        match self.armor_upgrade_cost(tier) {
            Some(cost) if self.gold >= cost => {
                self.gold -= cost;
                self.armor_tier = tier;
                true
            }
            _ => false,
        }
    }

    /// Gold cost to fully heal: `round(ln(level) * (damage_taken + 10))`. Free
    /// at level 1 (`ln(1) == 0`).
    pub fn full_heal_cost(&self) -> u64 {
        let missing = self.max_hitpoints().saturating_sub(self.hitpoints);
        if missing == 0 {
            return 0;
        }
        ((self.level as f64).ln() * (missing as f64 + 10.0))
            .round()
            .max(0.0) as u64
    }

    /// Price of a partial heal of `pct` percent of the damage taken:
    /// `round(cost * pct / 100)` off the rounded full-heal price (`healer.php`
    /// sells 100% down to 10% in steps of ten).
    pub fn heal_cost(&self, pct: u32) -> u64 {
        (self.full_heal_cost() as f64 * pct as f64 / 100.0).round() as u64
    }

    /// Pay for a heal of `pct` percent of the missing HP (`round(missing *
    /// pct / 100)`). Returns the HP restored, or `None` if unaffordable.
    pub fn buy_heal(&mut self, pct: u32) -> Option<u32> {
        let cost = self.heal_cost(pct);
        if self.gold < cost {
            return None;
        }
        self.gold -= cost;
        let missing = self.max_hitpoints().saturating_sub(self.hitpoints);
        let healed = (missing as f64 * pct as f64 / 100.0).round() as u32;
        self.hitpoints += healed;
        Some(healed)
    }

    /// Pay to fully heal if affordable. Returns true on success (including the
    /// free level-1 case).
    pub fn buy_full_heal(&mut self) -> bool {
        self.buy_heal(100).is_some()
    }

    /// The healer's free forced normalize: HP above max (a lapsed overheal) is
    /// clipped back down, no charge (`healer.php`'s over-max branch). Returns
    /// true if anything was clipped.
    pub fn normalize_overheal(&mut self) -> bool {
        if self.hitpoints > self.max_hitpoints() {
            self.hitpoints = self.max_hitpoints();
            return true;
        }
        false
    }

    /// Deposit on-hand gold into the bank (clamped to what's on hand). Paying
    /// down debt is the same move: a deposit into a negative balance.
    pub fn deposit(&mut self, amount: u64) {
        let amount = amount.min(self.gold);
        self.gold -= amount;
        self.gold_in_bank = self.gold_in_bank.saturating_add(amount as i64);
    }

    /// Withdraw banked gold to hand (clamped to the positive balance). Going
    /// below zero is a loan — see [`Character::borrow`].
    pub fn withdraw(&mut self, amount: u64) {
        let amount = (amount as i64).min(self.gold_in_bank).max(0);
        self.gold_in_bank -= amount;
        self.gold = self.gold.saturating_add(amount as u64);
    }

    /// The bank's lending ceiling: debt may reach `-level * BORROW_PER_LEVEL`
    /// (`bank.php` `borrowperlevel`).
    pub fn max_borrow(&self) -> i64 {
        self.level as i64 * BORROW_PER_LEVEL
    }

    /// Gold still borrowable before the balance hits the lending floor.
    pub fn borrow_available(&self) -> u64 {
        (self.gold_in_bank + self.max_borrow()).max(0) as u64
    }

    /// Take a loan of `amount` gold (clamped to [`Character::borrow_available`]):
    /// the balance goes negative and the gold lands on hand. Returns the amount
    /// actually borrowed.
    pub fn borrow(&mut self, amount: u64) -> u64 {
        let amount = amount.min(self.borrow_available());
        self.gold_in_bank -= amount as i64;
        self.gold = self.gold.saturating_add(amount);
        amount
    }

    /// Whether the bank's transfer window opens at all (`bank.php`'s nav
    /// gate: `mintransferlev` or any dragon kill). Debt is refused at the
    /// window, not here, so the teller gets her line.
    pub fn can_transfer(&self) -> bool {
        self.level >= MIN_TRANSFER_LEVEL || self.dragon_kills > 0
    }

    /// Draw `amount` gold for a bank transfer, hand first and the shortfall
    /// from the bank (`bank.php`'s settle: `gold -= amt`, negative overflow
    /// taken out of `goldinbank`). Returns the bank's share, so a refused
    /// settlement can refund each part where it came from. The caller has
    /// checked `gold + gold_in_bank >= amount` and that the balance isn't
    /// negative.
    pub fn draw_for_transfer(&mut self, amount: u64) -> u64 {
        let from_hand = amount.min(self.gold);
        let from_bank = amount - from_hand;
        self.gold -= from_hand;
        self.gold_in_bank -= from_bank as i64;
        from_bank
    }

    /// Perturb a creature's stat block by the player's banked investment —
    /// LoGD's `buffbadguy` (`lib/forestoutcomes.php`), applied to every forest
    /// creature at spawn:
    /// - scaling pool `dk = round(investment * (0.25 + 0.05 * kills / 100))`
    ///   (creatures harden as dragon kills accumulate),
    /// - experience flux of `±round(exp / 10)`,
    /// - the pool split randomly into +attack / +defense / +5 HP per point,
    /// - gold/exp compensated by `1 + .03*(atk+def) + .001*hp`.
    pub fn buff_foe(&self, base: data::CreatureTier, rng: &mut impl Rng) -> data::CreatureTier {
        let add = (self.dragon_kills as f64 / 100.0) * 0.05;
        let dk = (self.investment_points() as f64 * (0.25 + add)).round() as u32;

        let mut foe = base;
        let expflux = (foe.exp as f64 / 10.0).round() as i32;
        let exp = foe.exp as i64 + rng.gen_range(-expflux..=expflux) as i64;
        foe.exp = exp.max(0) as u32;

        let atkflux = rng.gen_range(0..=dk);
        let defflux = rng.gen_range(0..=(dk - atkflux));
        let hpflux = (dk - atkflux - defflux) * 5;
        foe.attack += atkflux;
        foe.defense += defflux;
        foe.hp += hpflux;

        let bonus = 1.0 + 0.03 * (atkflux + defflux) as f64 + 0.001 * hpflux as f64;
        foe.gold = (foe.gold as f64 * bonus).round() as u32;
        foe.exp = (foe.exp as f64 * bonus).round() as u32;
        foe
    }

    /// Settle a won forest fight — LoGD's `forestvictory`
    /// (`lib/forestoutcomes.php`), covering single kills and multi-fights:
    /// - each foe's gold is rolled `e_rand(0, gold)`, then the total re-rolled
    ///   `e_rand(avg, avg * round((n+1) * 1.2^(n-1)))` (a single kill pays
    ///   `e_rand(g, 2g)` of the first roll; packs multiply),
    /// - experience is the per-foe average plus a level-difference bonus of
    ///   `round(exp * (1 + .25*(foe_level - level)) - exp)` per foe (plus
    ///   `kills * level` on multi-fights), floored at `-exp+1`, a positive
    ///   bonus scaled by `1.05^(n-1)`,
    /// - under level 15, a 1-in-[`FOREST_GEM_CHANCE`] gem,
    /// - a flawless fight refunds the turn if `level <= max_foe_level +
    ///   0.5*(n-1)`,
    /// - a player at 0 HP on a victory is saved at 1 (the mushroom clamp).
    pub fn forest_victory(
        &mut self,
        foes: &[SlainFoe],
        flawless: bool,
        rng: &mut impl Rng,
    ) -> ForestVictory {
        let n = foes.len().max(1) as u32;
        let mut gold_sum: u64 = 0;
        let mut exp_sum: u64 = 0;
        let mut exp_bonus: i64 = 0;
        let mut max_foe_level: u8 = 0;
        for foe in foes {
            gold_sum += rng.gen_range(0..=foe.gold) as u64;
            exp_sum += foe.exp as u64;
            let scaled = foe.exp as f64 * (1.0 + 0.25 * (foe.level as f64 - self.level as f64));
            exp_bonus += (scaled - foe.exp as f64).round() as i64;
            max_foe_level = max_foe_level.max(foe.level);
        }
        if n > 1 {
            exp_bonus += (self.dragon_kills as u64 * self.level as u64) as i64;
        }

        let exp = (exp_sum as f64 / n as f64).round() as i64;
        let avg_gold = (gold_sum as f64 / n as f64).round() as u64;
        let gold_hi = avg_gold * ((n as f64 + 1.0) * 1.2f64.powi(n as i32 - 1)).round() as u64;
        let gold = rng.gen_range(avg_gold..=gold_hi.max(avg_gold));

        let mut exp_bonus = (exp_bonus as f64 / n as f64).round() as i64;
        if exp + exp_bonus < 0 {
            exp_bonus = -exp + 1;
        }
        if exp_bonus > 0 {
            exp_bonus = (exp_bonus as f64 * 1.05f64.powi(n as i32 - 1)).round() as i64;
        }
        let exp_won = (exp + exp_bonus).max(0) as u64;

        self.gold = self.gold.saturating_add(gold);
        self.experience = self.experience.saturating_add(exp_won);

        let gem = self.level < data::MAX_LEVEL && rng.gen_range(1..=FOREST_GEM_CHANCE) == 1;
        if gem {
            self.gems += 1;
        }

        // Flawless fights refund the turn, but only when the foes were a real
        // match; packs count for half a level each past the first.
        let effective_level = max_foe_level as f64 + 0.5 * (n as f64 - 1.0);
        let turn_refunded = flawless && self.level as f64 <= effective_level;
        if turn_refunded {
            self.turns += 1;
        }

        // The mushroom save: a victory never leaves you dead on the ground.
        if self.hitpoints == 0 {
            self.hitpoints = 1;
        }

        ForestVictory {
            gold,
            exp: exp_won,
            gem,
            flawless,
            turn_refunded,
        }
    }

    /// Still under newbie PvP immunity (`lib/pvpwarning.php` and the
    /// `pvplist.php` filter, which is this set negated): immune while the run
    /// is young, dragonless, unforfeited, and under the experience bar.
    pub fn pvp_immune(&self) -> bool {
        self.age <= PVP_IMMUNITY_DAYS
            && self.dragon_kills == 0
            && !self.pk
            && self.experience <= PVP_IMMUNITY_MAX_EXP
    }

    /// Whether the bounty broker refuses contracts on this character
    /// (`dag.php`'s finalize check). Deliberately one notch more lenient
    /// than [`Character::pvp_immune`]: strict `<` on both age and experience
    /// where the PvP list uses `<=`, so a warrior at exactly age 5 or
    /// exactly 1500 experience is still safe from attack yet already
    /// bountyable. Ported 1=1, quirk and all. The level floor
    /// ([`BOUNTY_MIN_TARGET_LEVEL`]) is the caller's separate check.
    pub fn bounty_immune(&self) -> bool {
        self.age < PVP_IMMUNITY_DAYS
            && self.dragon_kills == 0
            && !self.pk
            && self.experience < PVP_IMMUNITY_MAX_EXP
    }

    /// Resolve losing a PvP fight you started (`lib/pvpsupport.php`
    /// `pvpdefeat`): all on-hand gold lost, [`PVP_ATTACKER_LOSE_PCT`]% of
    /// experience lost, dead to the graveyard. Same death hygiene as
    /// [`Character::die`] — companions and buffs don't follow past the grave
    /// (ours die with every death; upstream's PvP path leaves them, a
    /// documented adaptation).
    pub fn pvp_die(&mut self) {
        self.gold = 0;
        self.experience =
            (self.experience as f64 * (100 - PVP_ATTACKER_LOSE_PCT) as f64 / 100.0).round() as u64;
        self.alive = false;
        self.hitpoints = 0;
        self.companions.clear();
        self.persistent_buffs.clear();
        self.drunkenness = 0;
    }

    /// Settle being slain in your sleep (`pvpvictory`'s victim UPDATE):
    /// `taken_gold` comes off the purse with the bank absorbing any shortfall
    /// (upstream's race guard — the caller reads the purse fresh, so normally
    /// none), `lost_exp` (5% of the engage-time snapshot) comes off the
    /// experience, and death applies with the usual hygiene.
    pub fn pvp_slain(&mut self, taken_gold: u64, lost_exp: u64) {
        if self.gold < taken_gold {
            self.gold_in_bank -= (taken_gold - self.gold) as i64;
            self.gold = 0;
        } else {
            self.gold -= taken_gold;
        }
        self.experience = self.experience.saturating_sub(lost_exp);
        self.alive = false;
        self.hitpoints = 0;
        self.companions.clear();
        self.persistent_buffs.clear();
        self.drunkenness = 0;
    }

    /// Resolve a forest/PvE death: all on-hand gold lost, 10% experience lost,
    /// sent to the graveyard (revived on the next new day).
    pub fn die(&mut self) {
        self.gold = 0;
        self.experience = (self.experience as f64 * EXP_KEEP_ON_DEATH).round() as u64;
        self.alive = false;
        self.hitpoints = 0;
        // Your companions don't follow you past the grave.
        self.companions.clear();
        // Neither do your buffs (upstream strips them at the graveyard), and
        // death sobers you right up (the `header-graveyard` drinks hook).
        self.persistent_buffs.clear();
        self.drunkenness = 0;
    }

    /// Extra daily forest fights bought with `ff` dragon points (LoGD `dkff`).
    pub fn dk_forest_bonus(&self) -> u32 {
        self.dragon_ff_bonus
    }

    /// Spend one unspent dragon point on `kind`. Returns false (spending
    /// nothing) if none are unspent. An `ff` point also grows *today's* pool by
    /// one, since LoGD spends points before the new day's turns are assembled;
    /// an `hp` point raises current HP alongside the max for the same reason.
    pub fn spend_dragon_point(&mut self, kind: DragonPointKind) -> bool {
        if self.dragon_points_unspent == 0 {
            return false;
        }
        self.dragon_points_unspent -= 1;
        match kind {
            DragonPointKind::Hp => {
                self.dragon_hp_bonus += HP_PER_DRAGON_POINT;
                self.hitpoints += HP_PER_DRAGON_POINT;
            }
            DragonPointKind::ForestFights => {
                self.dragon_ff_bonus += 1;
                self.turns += 1;
            }
            DragonPointKind::Attack => self.dragon_attack_bonus += 1,
            DragonPointKind::Defense => self.dragon_defense_bonus += 1,
        }
        true
    }

    /// Reward a Green Dragon kill (`dragon.php`), then reset to a fresh,
    /// fully-healed run. `flawless` is true if no damage was taken in the fight.
    ///
    /// Faithful to upstream: the run's gold is wiped and restarted at
    /// [`START_GOLD`] plus [`START_GOLD`] per kill (capped at
    /// [`DRAGON_RUN_GOLD_CAP`]); gems accrue `max(0, kills-7)` (capped); charm
    /// `+5`; companions are wiped; and the kill banks **one dragon point** to
    /// spend at the gate ([`Character::spend_dragon_point`]). A flawless kill
    /// adds [`FLAWLESS_GOLD_BONUS`] gold (over the cap) and a gem. The
    /// specialty skill/uses restart at zero.
    pub fn slay_dragon(&mut self, flawless: bool) {
        self.dragon_kills = self.dragon_kills.saturating_add(1);
        // The run's day count is stamped for the Hall of Fame (`dragon.php`:
        // `dragonage = age`, `bestdragonage` keeps the fastest), then the
        // counters outside the preserve list reset with the run.
        self.dragon_age = self.age;
        if self.dragon_age < self.best_dragon_age || self.best_dragon_age == 0 {
            self.best_dragon_age = self.dragon_age;
        }
        self.age = 0;
        self.resurrections = 0;
        self.dragon_points_unspent = self.dragon_points_unspent.saturating_add(1);
        self.charm = self.charm.saturating_add(CHARM_PER_DRAGON_KILL);
        let restart_gems = self.dragon_kills.saturating_sub(7).min(MAX_RESTART_GEMS);
        self.gems = self.gems.saturating_add(restart_gems as u64);
        // The reset wipes on-hand gold: you restart with 50 + 50/kill, capped.
        self.gold = (START_GOLD + START_GOLD * self.dragon_kills as u64).min(DRAGON_RUN_GOLD_CAP);
        if flawless {
            // The flawless bonus lands on top of the cap.
            self.gold = self.gold.saturating_add(FLAWLESS_GOLD_BONUS);
            self.gems = self.gems.saturating_add(1);
        }
        // Reset the run.
        self.level = 1;
        self.experience = 0;
        self.weapon_tier = 0;
        self.armor_tier = 0;
        self.seen_dragon = false;
        self.alive = true;
        // The account reset (`dragon.php`'s field loop reverts every column
        // outside its preserve list) reaches further than the pocket: the
        // bank balance (debt included), the death overlord's favor, and any
        // haunt mark all revert to their column defaults, and race and
        // specialty are re-chosen every run — their gates re-arm.
        self.gold_in_bank = 0;
        self.favor = 0;
        self.haunted_by.clear();
        self.race = Race::None;
        self.specialty = Specialty::None;
        // The loop also wipes `recentcomments`, so every comment reads as
        // new on the post-kill day (the next dawn re-stamps it normally).
        self.comments_seen_before_day = 0;
        // Upstream also wipes `lasthit`, so the next page load runs a full
        // new day once the spend/race gates close ([`Character::dawn`]).
        self.dawn_owed = true;
        // The specialty path is kept, but its skill/uses restart (LoGD's
        // per-module dragonkill hook fires for every specialty, benched ones
        // included).
        self.specialty_skill = 0;
        self.specialty_uses = 0;
        self.benched_specialties = [(0, 0); 3];
        self.companions.clear();
        // The drinks module sobers a dragon-slayer up; buffs don't outlive
        // the old run either. The mount does (upstream keeps `hashorse`).
        self.drunkenness = 0;
        self.persistent_buffs.clear();
        self.hitpoints = self.max_hitpoints();
    }

    /// Run the daily reset if `today` is past the stored day: pay bank interest,
    /// refill forest turns and grave fights, fully heal, revive, refresh
    /// soulpoints, and run the daily module effects (mount, hangover, marriage
    /// upkeep, the once-a-day flags). `interest_percent` is the day's rolled
    /// rate and `spirits` is the day's `e_rand(-1,1)+e_rand(-1,1)` (-2..+2)
    /// turn jitter. Returns what happened, or `None` if the day already rolled.
    ///
    /// A dawn revives the dead at no extra cost: upstream's `-6` turn dock and
    /// skipped soulpoint/grave-fight refills apply only to the *paid*
    /// resurrection ([`Character::resurrect`]), never this passive path.
    pub fn roll_new_day(
        &mut self,
        today: i64,
        interest_percent: u32,
        spirits: i32,
        rng: &mut impl Rng,
    ) -> Option<NewDayFx> {
        if today <= self.last_day {
            return None;
        }
        // The new-post watermark rolls forward to the previous dawn's day
        // (`newday.php`: `recentcomments = lasthit`, `lasthit = now`) — unless
        // a dragon kill wiped `lasthit` and no dawn has run since, in which
        // case everything reads as new, exactly as upstream's epoch stamp.
        self.comments_seen_before_day = if self.dawn_owed { 0 } else { self.last_day };
        self.last_day = today;
        // A dead character greeting the dawn counts a revival (`newday.php`:
        // `resurrections++` while not alive).
        if !self.alive {
            self.resurrections += 1;
        }
        // Interest is settled before turns refill, so it can read how many of
        // yesterday's turns went unused (LoGD's "work for it" gate).
        self.apply_new_day_interest(interest_percent);
        let fx = self.dawn(spirits, rng);
        self.alive = true;
        Some(fx)
    }

    /// The dawn proper — everything a non-resurrection `newday.php` run does
    /// once its gates pass: the run ages a day (`age++` unconditionally),
    /// turns are assembled (base + ff dragon points + race bonus + the day's
    /// spirits), the daily module effects fire, specialty uses refresh, the
    /// once-per-day dragon seek re-arms, the soul pool / grave fights / PvP
    /// pool refill (`newday.php` skips those three only when `resurrection`),
    /// and you wake fully healed. Runs inside [`Character::roll_new_day`],
    /// and solo as the forced day a dragon kill owes ([`Character::slay_dragon`]
    /// sets `dawn_owed`; upstream wipes `lasthit` so the next page load is a
    /// full `newday.php` run) — the freshly wiped bank makes that day's
    /// interest a no-op, so the forced path skips the interest roll.
    pub fn dawn(&mut self, spirits: i32, rng: &mut impl Rng) -> NewDayFx {
        self.dawn_owed = false;
        self.age += 1;
        let turns = TURNS_PER_DAY as i32
            + self.dragon_ff_bonus as i32
            + self.race.daily_forest_bonus() as i32
            + spirits;
        self.turns = turns.max(0) as u32;
        let fx = self.newday_shared_effects(rng);
        self.refresh_specialty_uses();
        // The dragon may be sought once per day (`newday.php` clears
        // `seendragon` daily): a fled attempt doesn't lock out the run.
        self.seen_dragon = false;
        self.soulpoints = self.max_soulpoints();
        self.grave_fights = GRAVE_FIGHTS_PER_DAY;
        self.player_fights = PVP_FIGHTS_PER_DAY;
        self.hitpoints = self.max_hitpoints();
        fx
    }

    /// The daily effects shared by every kind of new day (upstream's newday
    /// module hooks plus its unconditional user-field resets — they run on
    /// the paid resurrection too): the mount's bonus fights and refreshed
    /// buff rounds, the hangover dock and the drink slate, the once-a-day
    /// flags, buffs fading at dawn, and marriage upkeep. Runs after the day's
    /// turns are assembled (the hangover docks them) and before
    /// [`Character::refresh_specialty_uses`].
    fn newday_shared_effects(&mut self, rng: &mut impl Rng) -> NewDayFx {
        // The stable refreshes the mount (`newday.php`: bonus forest fights
        // + the buff's daily round allowance).
        if let Some(mount) = self.mount_data() {
            self.turns += mount.forest_fights;
            self.mount_rounds_left = mount.buff_rounds;
        }
        // Drinks: the hangover (drunkenness > 66 at dawn costs a turn), then
        // the day's slate is wiped either way.
        let hangover = self.drunkenness > MAX_DRUNKENNESS_SERVED;
        if hangover {
            self.turns = self.turns.saturating_sub(1);
        }
        self.drunkenness = 0;
        self.hard_drinks_today = 0;
        // The once-a-day flags re-arm (`seenmaster` clears unconditionally in
        // `newday.php`, resurrection days included).
        self.seen_master_today = false;
        self.lodged_today = false;
        self.flirted_today = false;
        self.heard_bard_today = false;
        self.used_outhouse_today = false;
        self.zoo_played_today = false;
        self.fivesix_plays_today = 0;
        self.bounties_set_today = 0;
        // The bank's transfer counters (`newday.php` zeroes both
        // unconditionally, resurrection days included).
        self.amount_out_today = 0;
        self.transfers_received_today = 0;
        // A haunt collects (`newday.php`'s `hauntedby` block, unconditional —
        // it fires on the paid resurrection too): one turn lost to the
        // night's fright, and the mark clears. Upstream's `turns--` has no
        // floor; ours saturates (unsigned field, documented deviation).
        let haunted_by = if self.haunted_by.is_empty() {
            None
        } else {
            self.turns = self.turns.saturating_sub(1);
            Some(std::mem::take(&mut self.haunted_by))
        };
        // Buffs fade at dawn unless flagged (transmutation sickness).
        self.persistent_buffs.retain(|b| b.survives_new_day);
        // Marriage upkeep (`lovers.php`): the partner's patience erodes by
        // `e_rand(1, max(1, round(0.85 * sqrt(dragon_kills))))` charm; at
        // zero the marriage ends.
        let mut divorced = false;
        if self.married {
            let cap = ((0.85 * (self.dragon_kills as f64).sqrt()).round() as u32).max(1);
            let loss = rng.gen_range(1..=cap);
            self.charm = self.charm.saturating_sub(loss);
            if self.charm == 0 {
                self.married = false;
                divorced = true;
            }
        }
        NewDayFx {
            divorced,
            hangover,
            haunted_by,
        }
    }

    /// Refill the day's specialty uses: `floor(skill/3)`, plus 1 for having
    /// chosen a specialty at all (LoGD's `specialtybonus` goes to the active
    /// path only). Benched paths refresh their own pools too, exactly like
    /// each upstream module's own newday hook. No-op while undecided.
    pub fn refresh_specialty_uses(&mut self) {
        for (skill, uses) in self.benched_specialties.iter_mut() {
            *uses = *skill / 3;
        }
        if self.specialty == Specialty::None {
            self.specialty_uses = 0;
            return;
        }
        self.specialty_uses = self.specialty_skill / 3 + 1;
    }

    /// Pick a specialty (LoGD chooses on the first new day; here it's the
    /// village chooser). A fresh path seeds the first day's uses immediately;
    /// a path benched earlier (the forgetting potion) resumes where it left
    /// off, like upstream's per-module prefs.
    pub fn choose_specialty(&mut self, specialty: Specialty) {
        self.specialty = specialty;
        if let Some(idx) = specialty_index(specialty) {
            let (skill, uses) = self.benched_specialties[idx];
            if skill > 0 || uses > 0 {
                self.benched_specialties[idx] = (0, 0);
                self.specialty_skill = skill;
                self.specialty_uses = uses;
                return;
            }
        }
        self.specialty_skill = 0;
        self.refresh_specialty_uses();
    }

    /// Switch the active specialty at the barkeep (`inn_bartender.php`):
    /// upstream only rewrites the `specialty` field — each path's skill and
    /// uses live in its own prefs — so the current pair is benched and the
    /// target path resumes its own ("you'll have to build up some points in
    /// this one"). Returns false for a no-op or invalid target.
    pub fn switch_specialty(&mut self, to: Specialty) -> bool {
        if to == self.specialty {
            return false;
        }
        let Some(to_idx) = specialty_index(to) else {
            return false;
        };
        if let Some(cur) = specialty_index(self.specialty) {
            self.benched_specialties[cur] = (self.specialty_skill, self.specialty_uses);
        }
        let (skill, uses) = self.benched_specialties[to_idx];
        self.benched_specialties[to_idx] = (0, 0);
        self.specialty = to;
        self.specialty_skill = skill;
        self.specialty_uses = uses;
        true
    }

    /// The forgetting potion: drop the specialty entirely (the village
    /// chooser re-arms). The path's progress is benched, not lost, exactly
    /// like upstream clearing only the `specialty` field.
    pub fn forget_specialty(&mut self) {
        if let Some(idx) = specialty_index(self.specialty) {
            self.benched_specialties[idx] = (self.specialty_skill, self.specialty_uses);
        }
        self.specialty = Specialty::None;
        self.specialty_skill = 0;
        self.specialty_uses = 0;
    }

    /// Apply (or refresh) a persistent buff. Occupied slots are replaced —
    /// upstream's `apply_buff` keys by slot, so a new drink replaces the old
    /// buzz — except transmutation sickness, which stacks its rounds.
    pub fn apply_persistent_buff(&mut self, buff: PersistedBuff) {
        if let Some(existing) = self
            .persistent_buffs
            .iter_mut()
            .find(|b| b.slot == buff.slot)
        {
            if buff.slot == "transmute" {
                existing.rounds_left += buff.rounds_left;
            } else {
                *existing = buff;
            }
        } else {
            self.persistent_buffs.push(buff);
        }
    }

    /// Reduce drunkenness by 10% (the `soberup` hook: each forest search and
    /// an outhouse wash both pass `soberval = 0.9`).
    pub fn sober_up(&mut self) {
        self.drunkenness = (self.drunkenness as f64 * 0.9).round() as u32;
    }

    /// The inn's room price: `round(level * (10 + ln(level)))`
    /// (`inn_room.php`).
    pub fn inn_room_cost(&self) -> u64 {
        (self.level as f64 * (10.0 + (self.level as f64).ln())).round() as u64
    }

    /// The gypsy seer's fee for a seance with the dead (`gypsy.php`):
    /// `level * 20` gold, paid per visit.
    pub fn gypsy_cost(&self) -> u64 {
        self.level as u64 * 20
    }

    /// The room price when charged to the bank: the base plus the inn's 5%
    /// convenience fee (`innfee` default "5%").
    pub fn inn_room_bank_cost(&self) -> u64 {
        let cost = self.inn_room_cost();
        cost + (cost as f64 * 5.0 / 100.0).round() as u64
    }

    /// Take the inn's room for the night (`inn_room.php`): gold at the base
    /// price, or the bank at the price plus its 5% fee. Once per day. Returns
    /// the price paid.
    pub fn lodge(&mut self, from_bank: bool) -> Option<u64> {
        if self.lodged_today {
            return None;
        }
        let cost = if from_bank {
            let cost = self.inn_room_bank_cost();
            if self.gold_in_bank < cost as i64 {
                return None;
            }
            self.gold_in_bank -= cost as i64;
            cost
        } else {
            let cost = self.inn_room_cost();
            if self.gold < cost {
                return None;
            }
            self.gold -= cost;
            cost
        };
        self.lodged_today = true;
        Some(cost)
    }

    /// The barkeep's three gold bribe amounts (`inn_bartender.php`):
    /// `level*10`, `level*50`, `level*100`.
    pub fn bribe_gold_amounts(&self) -> [u64; 3] {
        let l = self.level as u64;
        [l * 10, l * 50, l * 100]
    }

    /// Whether a potion is buyable right now: the gems cover a dose, and the
    /// reset potions have something to reset.
    pub fn can_buy_potion(&self, kind: PotionKind) -> bool {
        if self.gems < POTION_COST_GEMS {
            return false;
        }
        match kind {
            PotionKind::Forgetting => self.specialty != Specialty::None,
            PotionKind::Transmutation => self.race != Race::None,
            _ => true,
        }
    }

    /// Buy and drink one dose off the back shelf (`cedrikspotions.php`).
    /// Returns false (spending nothing) if [`Character::can_buy_potion`]
    /// says no.
    pub fn buy_potion(&mut self, kind: PotionKind) -> bool {
        if !self.can_buy_potion(kind) {
            return false;
        }
        self.gems -= POTION_COST_GEMS;
        match kind {
            PotionKind::Charm => self.charm += 1,
            PotionKind::Vitality => {
                self.vitality_hp += 1;
                self.hitpoints += 1;
            }
            // Heal to full first, then the overheal on top (the upstream
            // order: an existing overheal is kept, not clipped).
            PotionKind::Mending => {
                self.hitpoints = self.hitpoints.max(self.max_hitpoints()) + MENDING_OVERHEAL;
            }
            PotionKind::Forgetting => self.forget_specialty(),
            PotionKind::Transmutation => {
                self.race = Race::None;
                self.apply_persistent_buff(transmute_sickness());
            }
        }
        true
    }

    /// Whether the barkeep will pour this drink: service stops entirely above
    /// [`MAX_DRUNKENNESS_SERVED`], and hard liquor is capped per day.
    pub fn can_be_served(&self, d: &data::Drink) -> bool {
        self.drunkenness <= MAX_DRUNKENNESS_SERVED
            && (!d.hard || self.hard_drinks_today < HARD_DRINKS_PER_DAY)
    }

    /// Down one of the inn's drinks (`modules/drinks.php`): pay, take the
    /// drunkenness, roll the HP/turn effects (HP floors at 1 and can ride
    /// over max; turns floor at 0), and apply the drink's buzz — slot
    /// "buzz", so a new drink replaces the old one's leftovers. The caller
    /// checks [`Character::can_be_served`] and affordability. Returns the
    /// lines to log.
    pub fn drink(&mut self, d: &data::Drink, rng: &mut impl Rng) -> Vec<String> {
        let cost = self.level as u64 * d.cost_per_level;
        self.gold -= cost;
        self.drunkenness = (self.drunkenness + d.drunkenness).min(100);
        if d.hard {
            self.hard_drinks_today += 1;
        }
        let mut lines = vec![format!("You pay {cost} gold and down a {}.", d.name)];
        let (mut do_hp, mut do_turn) = (d.always_both, d.always_both);
        if !d.always_both && d.hp_chance + d.turn_chance > 0 {
            if rng.gen_range(1..=d.hp_chance + d.turn_chance) <= d.hp_chance {
                do_hp = true;
            } else {
                do_turn = true;
            }
        }
        if do_hp {
            let delta = if d.hp_percent > 0 {
                (self.max_hitpoints() as f64 * d.hp_percent as f64 / 100.0).round() as i32
            } else {
                rng.gen_range(d.hp_range.0..=d.hp_range.1)
            };
            self.hitpoints = (self.hitpoints as i64 + delta as i64).max(1) as u32;
            match delta.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    lines.push(format!("It goes down warm: +{delta} hitpoints."))
                }
                std::cmp::Ordering::Less => {
                    lines.push(format!("It goes down like a lit coal: {delta} hitpoints."))
                }
                std::cmp::Ordering::Equal => {}
            }
        }
        if do_turn {
            let delta = rng.gen_range(d.turn_range.0..=d.turn_range.1);
            self.turns = (self.turns as i64 + delta as i64).max(0) as u32;
            match delta.cmp(&0) {
                std::cmp::Ordering::Greater => lines.push(format!(
                    "Your blood is up: +{delta} forest fight{}.",
                    if delta == 1 { "" } else { "s" }
                )),
                std::cmp::Ordering::Less => {
                    lines.push("The room swims; you lose a forest fight.".to_string())
                }
                std::cmp::Ordering::Equal => {}
            }
        }
        self.apply_persistent_buff(PersistedBuff {
            slot: "buzz".into(),
            name: d.buff_name.into(),
            rounds_left: d.buff_rounds,
            player_atk_mod: d.atk_mod,
            player_def_mod: d.def_mod,
            player_dmg_mod: d.dmg_mod,
            damage_shield: d.damage_shield,
            wearoff: d.wearoff.into(),
            survives_new_day: false,
        });
        lines.push(format!(
            "{} settles into your limbs ({} rounds).",
            d.buff_name, d.buff_rounds
        ));
        lines
    }

    /// The stabled mount's data row, if any.
    pub fn mount_data(&self) -> Option<&'static data::Mount> {
        if self.mount == 0 {
            return None;
        }
        data::MOUNTS.get(self.mount as usize - 1)
    }

    /// The gem refund for parting with the current mount:
    /// `round(cost * 2/3)` (`stables.php`).
    pub fn mount_refund(&self) -> u64 {
        self.mount_data()
            .map(|m| (m.cost_gems as f64 * 2.0 / 3.0).round() as u64)
            .unwrap_or(0)
    }

    /// Buy the mount at 1-based `index`, trading in any current mount at the
    /// ⅔ refund (affordability counts the refund, like upstream). The new
    /// mount is saddled at once: its bonus fights and buff rounds join today.
    pub fn buy_mount(&mut self, index: u8) -> bool {
        let Some(mount) = data::MOUNTS.get(index as usize - 1) else {
            return false;
        };
        if self.mount == index {
            return false;
        }
        let refund = self.mount_refund();
        if self.gems + refund < mount.cost_gems {
            return false;
        }
        self.gems = self.gems + refund - mount.cost_gems;
        self.mount = index;
        self.mount_rounds_left = mount.buff_rounds;
        self.turns += mount.forest_fights;
        true
    }

    /// Sell the current mount for the ⅔ refund. Returns the gems paid, or
    /// `None` without a mount.
    pub fn sell_mount(&mut self) -> Option<u64> {
        if self.mount == 0 {
            return None;
        }
        let refund = self.mount_refund();
        self.gems += refund;
        self.mount = 0;
        self.mount_rounds_left = 0;
        Some(refund)
    }

    /// Hired companions on the payroll (summons carry `ignore_limit` and
    /// don't count — LoGD's `companionsallowed`, default 1).
    pub fn hired_companions(&self) -> usize {
        self.companions.iter().filter(|c| !c.ignore_limit).count()
    }

    /// Whether this mercenary can be hired right now: the one-hire cap has
    /// room, no companion already answers to the name, and the purse covers
    /// both currencies.
    pub fn can_hire(&self, merc: &data::Mercenary) -> bool {
        self.hired_companions() == 0
            && !self.companions.iter().any(|c| c.name == merc.name)
            && self.gold >= merc.cost_gold
            && self.gems >= merc.cost_gems
    }

    /// Hire a mercenary (`mercenarycamp.php`): stats are baked from the
    /// buyer's level at purchase (`base + per_level * level`) and never
    /// recalculated. Returns false if [`Character::can_hire`] says no.
    pub fn hire_mercenary(&mut self, merc: &data::Mercenary) -> bool {
        if !self.can_hire(merc) {
            return false;
        }
        self.gold -= merc.cost_gold;
        self.gems -= merc.cost_gems;
        let level = self.level as u32;
        let hp = merc.hp.0 + merc.hp.1 * level;
        self.companions.push(Companion {
            name: merc.name.to_string(),
            hitpoints: hp,
            max_hitpoints: hp,
            attack: (merc.attack.0 + merc.attack.1 * level) as f64,
            defense: (merc.defense.0 + merc.defense.1 * level) as f64,
            attack_per_level: merc.attack.1,
            defense_per_level: merc.defense.1,
            hp_per_level: merc.hp.1,
            dying_text: merc.dying_text.to_string(),
            ability: merc.ability,
            ignore_limit: false,
        });
        true
    }

    /// Gold the camp's sawbones charges to patch companion `idx` to full:
    /// `round(ln(level + 1) * (missing + 10) * 1.33)` (`mercenarycamp.php`).
    pub fn companion_heal_cost(&self, idx: usize) -> Option<u64> {
        let comp = self.companions.get(idx)?;
        let missing = comp.max_hitpoints.saturating_sub(comp.hitpoints);
        if missing == 0 {
            return None;
        }
        Some((((self.level as f64) + 1.0).ln() * (missing as f64 + 10.0) * 1.33).round() as u64)
    }

    /// Pay to heal companion `idx` to full. Returns the gold spent, or `None`
    /// if whole or unaffordable.
    pub fn heal_companion(&mut self, idx: usize) -> Option<u64> {
        let cost = self.companion_heal_cost(idx)?;
        if self.gold < cost {
            return None;
        }
        self.gold -= cost;
        let comp = &mut self.companions[idx];
        comp.hitpoints = comp.max_hitpoints;
        Some(cost)
    }

    /// Advance the chosen specialty by one skill point. Every third point also
    /// grants an immediate use (mirrors `incrementspecialty`). Returns the new
    /// skill level, or `None` if the player has no specialty to advance.
    pub fn increment_specialty(&mut self) -> Option<u32> {
        if self.specialty == Specialty::None {
            return None;
        }
        self.specialty_skill += 1;
        if self.specialty_skill.is_multiple_of(3) {
            self.specialty_uses += 1;
        }
        Some(self.specialty_skill)
    }

    /// Spend `cost` specialty uses to fire an in-fight skill. Returns false (and
    /// spends nothing) if the pool can't cover it.
    pub fn spend_specialty_uses(&mut self, cost: u32) -> bool {
        if self.specialty_uses < cost {
            return false;
        }
        self.specialty_uses -= cost;
        true
    }

    /// Pay the day's bank interest, gated exactly like LoGD: a *positive*
    /// balance earns nothing if more than [`FIGHTS_FOR_INTEREST`] of
    /// yesterday's turns went unused, or if it is at/above
    /// [`MAX_GOLD_FOR_INTEREST`]. **Debt always compounds** — the "work for
    /// it" gate only skips positive balances (`newday.php`). Must be called
    /// before turns are refilled so `self.turns` still holds yesterday's
    /// leftover.
    fn apply_new_day_interest(&mut self, interest_percent: u32) {
        if self.turns > FIGHTS_FOR_INTEREST && self.gold_in_bank >= 0 {
            return;
        }
        if self.gold_in_bank >= MAX_GOLD_FOR_INTEREST {
            return;
        }
        self.apply_bank_interest(interest_percent);
    }

    /// Apply a daily bank interest multiplier (percent, e.g. 7 for 7%) to the
    /// signed balance — growth when positive, compounding debt when negative.
    pub fn apply_bank_interest(&mut self, percent: u32) {
        let factor = 1.0 + percent as f64 / 100.0;
        self.gold_in_bank = (self.gold_in_bank as f64 * factor).round() as i64;
    }
}

#[cfg(test)]
#[path = "model_test.rs"]
mod model_test;
