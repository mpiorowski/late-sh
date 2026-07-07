//! Legend of the Green Dragon service: thin persistence + reward plumbing for
//! the single-player door. Unlike Lateania there is no shared world, no tick
//! loop, and no watch-published world snapshot — each session owns the
//! authoritative character in its own `state::State`. This service only loads
//! the character once (off the DB) and saves blobs back, fire-and-forget.
//!
//! Cheap to `Clone`: everything lives behind an `Arc`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::Utc;
use late_core::{
    db::Db,
    models::{
        greendragon_bounty::GreenDragonBounty, greendragon_character::GreenDragonCharacter,
        greendragon_clan::{ClanNameClash, GreenDragonClan},
        greendragon_commentary::GreenDragonCommentary, greendragon_news::GreenDragonNews,
        greendragon_setting::GreenDragonSetting,
        profile_award::{
            GREENDRAGON_DRAGON_AWARD_CATEGORY, award_badge, grant_unique_milestone_award,
        },
        reward::GREENDRAGON_DRAGON_REWARD_KEY,
    },
};
use rand::Rng;
use serde_json::Value;
use tokio::sync::{Mutex as TokioMutex, watch};
use uuid::Uuid;

use crate::app::{
    activity::event::ActivityGame, activity::publisher::ActivityPublisher,
    games::chips::svc::ChipService,
};

use super::commentary::CommentLine;
use super::model::{self, Character};
use super::persist;

/// The async result of loading a session's character.
#[derive(Clone)]
pub enum CharacterLoad {
    /// The DB round-trip is still in flight.
    Loading,
    /// Loaded (or freshly created) and ready to play.
    Ready(Box<Character>),
}

/// The async result of loading one day's news page.
#[derive(Clone)]
pub enum NewsLoad {
    Loading,
    /// The day's lines, newest first. Empty means a quiet day (or a failed
    /// read — the village paper doesn't distinguish).
    Ready(Arc<Vec<String>>),
}

/// The async result of loading (or posting into) a commentary room's page.
#[derive(Clone)]
pub enum CommentaryLoad {
    Loading,
    Ready {
        /// The room's newest lines, newest first. Empty means a quiet room
        /// (or a failed read — the table doesn't distinguish).
        lines: Arc<Vec<CommentLine>>,
        /// A post was dropped as an exact repeat of the section's newest
        /// line by the same speaker (upstream's double-post check).
        double_post: bool,
    },
}

/// The async result of settling a Five Sixes play against the shared pot.
#[derive(Clone)]
pub enum FiveSixLoad {
    Loading,
    /// `(pot the roll was played against, gold left in the pot afterwards)`.
    /// The win is the difference — or the whole pot on five sixes.
    Ready {
        pot: u64,
        left_over: u64,
    },
    /// The DB failed; the caller refunds the stake and shrugs it off.
    Failed,
}

/// Cap for one day's news page. Upstream pages 50 at a time with page links;
/// a single generous cap stands in for the pager.
const NEWS_PAGE_LIMIT: i64 = 200;

/// How recently a character must have been saved to count as online
/// (upstream `LOGINTIMEOUT`, 900 seconds), ANDed with the blob's presence
/// flag exactly as upstream pairs `laston` with `loggedin`.
pub const ONLINE_WINDOW_SECS: i64 = 900;

/// Chip-ledger reason for the once-per-account dragon-kill payout.
const GREENDRAGON_DRAGON_LEDGER_REASON: &str = "greendragon_dragon_slain";

/// One character as the warrior roster / Hall of Fame reads it: the ranked
/// stats decoded out of the saved blob, plus the presence signals. The
/// session's own character appears too, as its last-saved snapshot.
#[derive(Clone, Debug)]
pub struct RosterEntry {
    pub user_id: Uuid,
    /// The titled display name (upstream `accounts.name` carries the DK
    /// title); the name search runs over this, so titles match too.
    pub name: String,
    /// The bare character name (upstream `login`), the list's final sort key.
    pub handle: String,
    pub level: u8,
    pub alive: bool,
    pub race: &'static str,
    pub dragon_kills: u32,
    pub dragon_age: u32,
    pub best_dragon_age: u32,
    pub resurrections: u32,
    pub gems: u64,
    pub charm: u32,
    pub max_hp: u32,
    pub experience: u64,
    /// Purse plus bank, signed (a live loan drags it down) — the richest
    /// ranking's raw total, fuzzed ±5% at render time.
    pub wealth: i64,
    /// In the door right now: the blob's presence flag ANDed with the
    /// 15-minute save-activity window.
    pub online: bool,
    /// Seconds since the last save (the warrior list's "last seen" column).
    pub idle_secs: i64,
    /// Sleeping upstairs at the inn (`boughtroomtoday`): the flag routes them
    /// to the inn's target list instead of the fields (upstream sets their
    /// `location` to the inn and lists by location).
    pub lodged: bool,
    /// Under newbie PvP immunity ([`Character::pvp_immune`]) — off every
    /// target list.
    pub pvp_immune: bool,
    /// Refused by the bounty broker ([`Character::bounty_immune`] — one
    /// notch more lenient than the PvP test, upstream's own quirk).
    pub bounty_immune: bool,
    /// Epoch seconds an attacker last engaged them (`pvpflag`); within
    /// [`model::PVP_TIMEOUT_SECS`] the row shows but can't be attacked.
    pub pvp_engaged_at: i64,
    /// Clan membership, for the warrior list's online-clan-members slice
    /// (`list.php?op=clan`).
    pub clan_id: Option<Uuid>,
}

/// The sleeping defender as the engage transaction snapshotted them
/// (`lib/pvpsupport.php` `setup_target`'s SELECT): the fight stats, plus the
/// gold/experience the settlement formulas read.
#[derive(Clone, Debug)]
pub struct PvpTarget {
    pub user_id: Uuid,
    /// Titled display name (upstream's `creaturename` carries the title).
    pub name: String,
    pub level: u8,
    /// On-hand gold at engage; the victory settlement re-reads and takes the
    /// lesser (upstream's banked-since guard).
    pub gold: u64,
    /// Experience at engage (already rounded upstream; ours is integral).
    pub experience: u64,
    pub attack: u32,
    pub defense: u32,
    /// The sleeper defends at *full* health regardless of their saved wounds
    /// (`maxhitpoints AS creaturehealth`).
    pub max_hp: u32,
    pub weapon: &'static str,
    /// Asleep upstairs at the inn: the fight adds their bodyguard
    /// (`bodyguardlevel = boughtroomtoday`).
    pub lodged: bool,
}

/// The async result of a PvP engage (`setup_target`): the locked-in target,
/// or the reason the attack fell through.
#[derive(Clone)]
pub enum PvpEngage {
    Loading,
    Ready(Box<PvpTarget>),
    /// The engage-time re-check failed (gone, out of range, dogpiled, awake,
    /// dead) or the DB did; the line is shown to the player.
    Refused(String),
}

/// The async result of settling a won PvP fight onto the victim's blob.
#[derive(Clone)]
pub enum PvpSettle {
    Loading,
    Ready {
        /// Gold the attacker won: `round(10 * lvl * ln(max(1, taken)))`.
        win_gold: u64,
        /// What the victim actually lost off purse+bank (the lesser-of rule).
        taken_gold: u64,
        /// The matured bounty gold swept off the victim's head (`dag`'s
        /// `pvpwin` hook) — paid on top of `win_gold` and, unlike it,
        /// exempt from the level-15 zeroing.
        bounty_gold: u64,
        /// The share the broker "keeps": matured bounties the attacker set
        /// on this head themselves. Never paid — and never closed either
        /// (upstream leaves them open for the next hunter).
        forfeited: u64,
        /// The victim's display name, for the bounty news line.
        victim: String,
    },
    /// The DB failed; the attacker gets no spoils (and the victim keeps
    /// their skin — the fight still made the news).
    Failed,
}

/// The async result of reading the bounty broker's ledger.
#[derive(Clone)]
pub enum BountyBoardLoad {
    Loading,
    Ready {
        /// The matured price on the *asking* player's own head — what the
        /// broker admits to on approach.
        on_my_head: u64,
        /// The wanted list: matured open gold aggregated per target,
        /// unordered (the view joins the roster and sorts).
        wanted: Arc<Vec<(Uuid, u64)>>,
    },
}

/// The async result of placing a bounty contract.
#[derive(Clone)]
pub enum BountyPlace {
    Loading,
    /// Inserted; the caller charges the fee'd cost it already quoted.
    Placed,
    /// The target's total open bounty (matured or not) would pass the
    /// `200·level` cap; nothing was placed. Carries the current total.
    OverCap(u64),
    /// The DB failed; nothing was placed or charged.
    Failed,
}

/// The async result of a haunt attempt (`case_haunt3.php`): the 25 favor is
/// the caller's to charge on `Success`/`Fumble` only — a refused target
/// costs nothing, exactly as upstream skips the deduction.
#[derive(Clone)]
pub enum HauntLoad {
    Loading,
    /// The roll won: the mark is on them and a report awaits their return.
    Success { target: String },
    /// The roll lost (publicly — the failure makes the news too).
    Fumble { target: String },
    /// Another shade already rides their dreams; no charge.
    AlreadyHaunted { target: String },
    /// The target vanished between the search and the attempt; no charge.
    Gone,
}

/// The async result of loading the full character roster.
#[derive(Clone)]
pub enum RosterLoad {
    Loading,
    /// Every saved character, unordered; the views sort. Empty also covers a
    /// failed read (the list shrugs, like the news page).
    Ready(Arc<Vec<RosterEntry>>),
}

/// The async result of the barman's paid enemy lookup (`darkhorse.php`'s
/// bartender): the target's character decoded fresh off the DB — upstream
/// SELECTs the row at pay time, so the sheet shows the purse as it stands,
/// not the roster snapshot. `None` means the row is gone (or unreadable):
/// no sheet, no charge.
#[derive(Clone)]
pub enum IntelLoad {
    Loading,
    Ready(Option<Box<Character>>),
}

/// One clan member as the hall's membership views read them, decoded off the
/// character blobs (`clan_membership.php` / `detail.php` read `accounts`).
#[derive(Clone, Debug)]
pub struct ClanMemberRow {
    pub user_id: Uuid,
    /// Titled display name (upstream's `name` column carries the DK title).
    pub name: String,
    pub level: u8,
    pub dragon_kills: u32,
    pub rank: u8,
    /// Epoch seconds of joining/applying (`clanjoindate`).
    pub joined_at: i64,
    pub alive: bool,
    /// In the door right now (the roster's presence test), for the hall's
    /// online-members slice.
    pub online: bool,
    /// Seconds since their last save (the "last on" column).
    pub idle_secs: i64,
}

/// The async result of loading one clan's hall (or public detail) view.
#[derive(Clone)]
pub enum ClanLoad {
    Loading,
    Ready {
        clan: Box<GreenDragonClan>,
        /// Every character enrolled with the clan, applicants included,
        /// unordered (the views sort per upstream's two orderings).
        members: Arc<Vec<ClanMemberRow>>,
        /// A leaderless hall auto-promoted this member to leader during the
        /// load (`clan_default.php`'s no-leader block): `(user, name)`. If
        /// it's the viewing session, the caller applies the rank locally.
        promoted: Option<(Uuid, String)>,
    },
    /// The clan row is gone; the caller heals its own dangling membership
    /// (`common.php` resets clanid/clanrank at page load).
    Gone,
    /// The DB failed — distinct from [`ClanLoad::Gone`] so a transient error
    /// never wipes a real membership.
    Failed,
}

/// One clan on the public list / application list: the row plus its count
/// of real members (rank > applicant, upstream's `clanrank > 0` counts).
#[derive(Clone, Debug)]
pub struct ClanListEntry {
    pub clan: GreenDragonClan,
    pub members: usize,
}

/// The async result of the clan list (both lists order by member count).
#[derive(Clone)]
pub enum ClanListLoad {
    Loading,
    Ready(Arc<Vec<ClanListEntry>>),
}

/// The async result of filing a new clan (`applicant_new.php`'s approval).
#[derive(Clone)]
pub enum ClanFound {
    Loading,
    /// Approved and inserted; the caller enrolls itself as founder and the
    /// fee (already taken) stays paid.
    Founded { clan_id: Uuid },
    /// The registrar's two "already taken" refusals; the fee comes back.
    NameTaken,
    TagTaken,
    /// The DB failed; nothing was filed, the fee comes back.
    Failed,
}

/// The async result of a clan operation that runs through the DB (an
/// application's officer notice, a rank change, a removal, a withdrawal).
#[derive(Clone)]
pub enum ClanOp {
    Loading,
    /// Done; the line (possibly empty) is for the log.
    Done(String),
    /// Refused against fresh state; the line explains.
    Refused(String),
}

#[derive(Clone)]
pub struct GreenDragonService {
    inner: Arc<Inner>,
}

struct Inner {
    db: Db,
    /// Monotonic write sequence. Every save/delete is stamped at submit time so
    /// a stale fire-and-forget write can be discarded instead of clobbering
    /// newer state.
    seq: AtomicU64,
    /// Per-user write gate: serializes that user's persistence and holds the
    /// highest sequence committed so far. An older snapshot (lower seq) that
    /// wins the race is skipped, so saves never go backwards.
    gates: StdMutex<HashMap<Uuid, Arc<TokioMutex<u64>>>>,
    activity: ActivityPublisher,
    chips: ChipService,
}

impl Inner {
    /// Allocate the next write sequence (stamped synchronously at submit time).
    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    /// The write gate for `user_id`, created on first use.
    fn gate(&self, user_id: Uuid) -> Arc<TokioMutex<u64>> {
        self.gates
            .lock()
            .unwrap()
            .entry(user_id)
            .or_default()
            .clone()
    }
}

/// Commit a character blob under the user's write gate, dropping the write if a
/// newer one (higher `seq`) already landed. Holding the gate across the DB write
/// serializes that user's persistence.
async fn commit_save(db: Db, gate: Arc<TokioMutex<u64>>, seq: u64, user_id: Uuid, blob: Value) {
    let mut watermark = gate.lock().await;
    if seq <= *watermark {
        return; // a newer snapshot already committed
    }
    match db.get().await {
        Ok(client) => match GreenDragonCharacter::save(&client, user_id, blob).await {
            Ok(_) => *watermark = seq,
            Err(e) => tracing::warn!("greendragon character save failed: {e}"),
        },
        Err(e) => tracing::warn!("greendragon db get failed on save: {e}"),
    }
}

/// Delete a character under the same write gate, ordered against pending saves.
async fn commit_delete(db: Db, gate: Arc<TokioMutex<u64>>, seq: u64, user_id: Uuid) {
    let mut watermark = gate.lock().await;
    if seq <= *watermark {
        return;
    }
    match db.get().await {
        Ok(client) => match GreenDragonCharacter::delete_by_user_id(&client, user_id).await {
            Ok(_) => *watermark = seq,
            Err(e) => tracing::warn!("greendragon character delete failed: {e}"),
        },
        Err(e) => tracing::warn!("greendragon db get failed on delete: {e}"),
    }
}

/// UTC day-number, used to drive once-per-day forest-turn/heal regeneration.
fn today() -> i64 {
    Utc::now().timestamp().div_euclid(86_400)
}

/// The engage transaction (see [`GreenDragonService::pvp_engage`]): lock the
/// target's row, re-check the attack against their fresh blob, stamp the
/// dogpile flag, and snapshot the fight stats. Check order is upstream's
/// (`setup_target`): found, level range, pvp flag, awake, alive.
async fn pvp_engage_tx(db: &Db, attacker_level: u8, target_id: Uuid) -> anyhow::Result<PvpEngage> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    let Some((blob, updated)) = GreenDragonCharacter::load_for_update(&tx, target_id).await? else {
        return Ok(PvpEngage::Refused(
            "They seem to have quit the realm entirely.".into(),
        ));
    };
    let mut c = persist::from_json(&blob);
    let now = Utc::now();
    if (attacker_level as i16 - c.level as i16).abs() > 2 {
        return Ok(PvpEngage::Refused(
            "They are beyond your reach in prowess now.".into(),
        ));
    }
    if now.timestamp() - c.pvp_engaged_at < model::PVP_TIMEOUT_SECS {
        return Ok(PvpEngage::Refused(
            "Someone else is already stalking them; wait your turn.".into(),
        ));
    }
    if c.online && (now - updated).num_seconds() < ONLINE_WINDOW_SECS {
        return Ok(PvpEngage::Refused(
            "They are awake and about, and cannot be caught sleeping.".into(),
        ));
    }
    if !c.alive {
        return Ok(PvpEngage::Refused("The dead cannot be slain twice.".into()));
    }
    let target = PvpTarget {
        user_id: target_id,
        name: c.titled_name(),
        level: c.level,
        gold: c.gold,
        experience: c.experience,
        attack: c.attack(),
        defense: c.defense(),
        max_hp: c.max_hitpoints(),
        weapon: super::data::weapon_name(c.weapon_tier),
        lodged: c.lodged_today,
    };
    c.pvp_engaged_at = now.timestamp();
    GreenDragonCharacter::update_data_keep_updated(&tx, target_id, persist::to_json(&c)).await?;
    tx.commit().await?;
    Ok(PvpEngage::Ready(Box::new(target)))
}

/// The victory settlement transaction (see
/// [`GreenDragonService::pvp_settle_victory`]).
async fn pvp_settle_victory_tx(
    db: &Db,
    victim_id: Uuid,
    engage: &PvpTarget,
    attacker_id: Uuid,
    attacker_name: &str,
) -> anyhow::Result<PvpSettle> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    let Some((blob, _)) = GreenDragonCharacter::load_for_update(&tx, victim_id).await? else {
        // The victim deleted their character mid-fight; nothing to settle.
        return Ok(PvpSettle::Failed);
    };
    let mut c = persist::from_json(&blob);
    // If they banked (or spent) gold since engage, take the lesser — the
    // point is to move only what was on the table (`pvpvictory`'s re-read).
    let taken_gold = engage.gold.min(c.gold);
    let lost_exp =
        (model::PVP_DEFENDER_LOSE_PCT as f64 * engage.experience as f64 / 100.0).round() as u64;
    c.pvp_slain(taken_gold, lost_exp);
    // The bounty sweep (`dag`'s `pvpwin` hook, run inside the settlement):
    // matured contracts on this head close to the attacker — except any the
    // attacker set themselves, which the broker "keeps" and quietly leaves
    // open for the next hunter, exactly as upstream never closes them.
    let bounty_gold = GreenDragonBounty::collect(&tx, victim_id, attacker_id).await?.max(0) as u64;
    let forfeited = GreenDragonBounty::forfeited_total(&tx, victim_id, attacker_id)
        .await?
        .max(0) as u64;
    let where_slept = if engage.lodged {
        "in your room at the inn"
    } else {
        "in the fields"
    };
    let mut report = format!(
        "While you slept {where_slept}, {attacker_name} attacked and bested you: \
         {taken_gold} gold and {lost_exp} experience lost. The graveyard has your bones \
         now; perhaps revenge will warm them.",
    );
    if bounty_gold > 0 {
        report.push_str(&format!(
            " They also collected the {bounty_gold} gold bounty on your head."
        ));
    }
    c.pvp_reports.push(report);
    GreenDragonCharacter::update_data_keep_updated(&tx, victim_id, persist::to_json(&c)).await?;
    tx.commit().await?;
    Ok(PvpSettle::Ready {
        win_gold: model::pvp_win_gold(engage.level, taken_gold),
        taken_gold,
        bounty_gold,
        forfeited,
        victim: engage.name.clone(),
    })
}

/// The defeat settlement transaction (see
/// [`GreenDragonService::pvp_settle_defeat`]): the sleeping winner collects,
/// unless they leveled down since engage (upstream's guard — the reward
/// would be "way too rich" for a fresh run).
async fn pvp_settle_defeat_tx(
    db: &Db,
    victim_id: Uuid,
    engage_level: u8,
    win_gold: u64,
    won_exp: u64,
    attacker_name: &str,
) -> anyhow::Result<()> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    let Some((blob, _)) = GreenDragonCharacter::load_for_update(&tx, victim_id).await? else {
        return Ok(());
    };
    let mut c = persist::from_json(&blob);
    if c.level < engage_level {
        c.pvp_reports.push(format!(
            "{attacker_name} crept up on you in your sleep and lost the fight — but the \
             {win_gold} gold and {won_exp} experience you'd have claimed went up in \
             dragonfire with the rest of your old life.",
        ));
    } else {
        c.gold = c.gold.saturating_add(win_gold);
        c.experience = c.experience.saturating_add(won_exp);
        c.pvp_reports.push(format!(
            "{attacker_name} crept up on you in your sleep, but your sleeping arm bested \
             them: {win_gold} gold and {won_exp} experience claimed off their corpse.",
        ));
    }
    GreenDragonCharacter::update_data_keep_updated(&tx, victim_id, persist::to_json(&c)).await?;
    tx.commit().await?;
    Ok(())
}

/// Grace before the lazy empty-clan sweep may reap a clan (upstream deletes
/// synchronously at list render, but its member writes are synchronous too —
/// ours are fire-and-forget, so a brand-new clan gets an hour for its
/// founder's save to land before it can look empty).
const CLAN_SWEEP_GRACE_SECS: i64 = 3600;

/// Append a sleep report to a character's blob, row-locked, without touching
/// their presence (`updated`). The clan flows use this for the officer
/// notices upstream sends as system mail. The caller holds the write gate.
async fn append_report_tx(db: &Db, user_id: Uuid, line: &str) -> anyhow::Result<()> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    let Some((blob, _)) = GreenDragonCharacter::load_for_update(&tx, user_id).await? else {
        return Ok(());
    };
    let mut c = persist::from_json(&blob);
    c.pvp_reports.push(line.to_string());
    GreenDragonCharacter::update_data_keep_updated(&tx, user_id, persist::to_json(&c)).await?;
    tx.commit().await?;
    Ok(())
}

/// Decode every character enrolled with `clan_id` (applicants included):
/// `(user, character, last save)`. Corrupt blobs are skipped.
async fn decode_clan_members(
    client: &tokio_postgres::Client,
    clan_id: Uuid,
) -> anyhow::Result<Vec<(Uuid, Character, chrono::DateTime<Utc>)>> {
    let rows = GreenDragonCharacter::load_all(client).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(user_id, blob, updated)| {
            let c = persist::from_json(&blob);
            (c.clan_id == Some(clan_id) && !c.name.trim().is_empty())
                .then_some((user_id, c, updated))
        })
        .collect())
}

/// A decoded member as the views read them.
fn clan_member_row(
    user_id: Uuid,
    c: &Character,
    updated: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
) -> ClanMemberRow {
    let idle_secs = (now - updated).num_seconds().max(0);
    ClanMemberRow {
        user_id,
        name: c.titled_name(),
        level: c.level,
        dragon_kills: c.dragon_kills,
        rank: c.clan_rank,
        joined_at: c.clan_joined_at,
        alive: c.alive,
        online: c.online && idle_secs < ONLINE_WINDOW_SECS,
        idle_secs,
    }
}

/// The succession pick (`clan_default.php` / `clan_withdraw.php`, identical
/// queries): the highest-ranked, oldest-joined real member (rank > 0).
fn succession_candidate(members: &[(Uuid, Character, chrono::DateTime<Utc>)]) -> Option<Uuid> {
    members
        .iter()
        .filter(|(_, c, _)| c.clan_rank > model::CLAN_APPLICANT)
        .max_by(|a, b| {
            a.1.clan_rank
                .cmp(&b.1.clan_rank)
                .then(b.1.clan_joined_at.cmp(&a.1.clan_joined_at))
        })
        .map(|(id, _, _)| *id)
}

/// Row-locked promotion of `target` straight to leader (both leaderless
/// paths), re-verified against their fresh blob. Returns the display name
/// on success. The caller holds the write gate.
async fn promote_to_leader_tx(
    db: &Db,
    clan_id: Uuid,
    target: Uuid,
) -> anyhow::Result<Option<String>> {
    let mut client = db.get().await?;
    let tx = client.transaction().await?;
    let Some((blob, _)) = GreenDragonCharacter::load_for_update(&tx, target).await? else {
        return Ok(None);
    };
    let mut c = persist::from_json(&blob);
    if c.clan_id != Some(clan_id) || c.clan_rank <= model::CLAN_APPLICANT {
        return Ok(None);
    }
    c.clan_rank = model::CLAN_LEADER;
    GreenDragonCharacter::update_data_keep_updated(&tx, target, persist::to_json(&c)).await?;
    tx.commit().await?;
    Ok(Some(c.titled_name()))
}

/// Fetch a section's newest `limit` rows, stamping each with whether it was
/// posted on the current UTC day (which feeds the daily post allowance).
async fn read_commentary(
    client: &tokio_postgres::Client,
    section: &str,
    limit: usize,
) -> Vec<CommentLine> {
    let today = today();
    match GreenDragonCommentary::latest(client, section, limit as i64).await {
        Ok(rows) => rows
            .into_iter()
            .map(|r| CommentLine {
                user_id: r.user_id,
                name: r.name,
                body: r.body,
                today: r.day == today,
            })
            .collect(),
        Err(e) => {
            tracing::warn!("greendragon commentary read failed: {e}");
            Vec::new()
        }
    }
}

impl GreenDragonService {
    pub fn new(activity: ActivityPublisher, chips: ChipService, db: Db) -> Self {
        Self {
            inner: Arc::new(Inner {
                db,
                seq: AtomicU64::new(0),
                gates: StdMutex::new(HashMap::new()),
                activity,
                chips,
            }),
        }
    }

    /// Begin loading `user_id`'s character. Returns a watch receiver that flips
    /// from [`CharacterLoad::Loading`] to [`CharacterLoad::Ready`] once the DB
    /// round-trip completes. A missing save yields a fresh level-1 character
    /// named `name`. The new-day reset is applied before the character is
    /// handed to the session.
    pub fn load_character(&self, user_id: Uuid, name: String) -> watch::Receiver<CharacterLoad> {
        let (tx, rx) = watch::channel(CharacterLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let db = inner.db.clone();
            let day = today();
            let mut character = match db.get().await {
                Ok(client) => match GreenDragonCharacter::load(&client, user_id).await {
                    Ok(Some(blob)) => persist::from_json(&blob),
                    Ok(None) => Character::new(name.clone(), day),
                    Err(e) => {
                        tracing::warn!("greendragon character load failed: {e}");
                        Character::new(name.clone(), day)
                    }
                },
                Err(e) => {
                    tracing::warn!("greendragon db get failed on load: {e}");
                    Character::new(name.clone(), day)
                }
            };
            // A corrupt/incompatible blob deserializes to a nameless default;
            // stamp the logged-in name so the player never loads as "".
            if character.name.trim().is_empty() {
                character.name = name;
            }
            // Refill forest turns / heal / revive if a new day has rolled over
            // since the last save. Spent ff dragon points add extra daily turns;
            // the bank pays a freshly-rolled interest rate; the day's "spirits"
            // (e_rand(-1,1) twice, -2..+2) jitter the forest fights, LoGD-style.
            // The RNG stays inside a sync block (thread_rng isn't Send).
            let rolled = {
                let mut rng = rand::thread_rng();
                let interest =
                    rng.gen_range(model::MIN_INTEREST_PERCENT..=model::MAX_INTEREST_PERCENT);
                let spirits = rng.gen_range(-1..=1) + rng.gen_range(-1..=1);
                character.roll_new_day(day, interest, spirits, &mut rng)
            };
            // A haunt collected at this dawn (`newday.php`'s `hauntedby`
            // block): the message rides the report drain, which the session
            // empties into the log right after this load lands.
            if let Some(haunter) = rolled.as_ref().and_then(|fx| fx.haunted_by.as_ref()) {
                character.pvp_reports.push(format!(
                    "{haunter} haunted your dreams in the night; the fright costs you a forest fight today."
                ));
            }
            // Entering the door marks the character present (upstream's
            // `loggedin`); every in-play save re-stamps it and the leave save
            // clears it, so the roster's 15-minute window reads true presence.
            character.online = true;
            // Persist immediately: the presence stamp should land even if the
            // player just looks around, and a rolled new day must not be lost
            // to an instant disconnect (a reconnect could otherwise re-roll a
            // favorable interest rate or dodge the resurrection cost).
            let seq = inner.next_seq();
            let gate = inner.gate(user_id);
            let blob = persist::to_json(&character);
            tokio::spawn(commit_save(inner.db.clone(), gate, seq, user_id, blob));
            if let Some(fx) = rolled {
                // A dawn divorce makes the paper (`lovers.php`'s addnews).
                if fx.divorced {
                    let body = format!(
                        "{} has left {} to pursue other interests.",
                        crate::app::door::greendragon::data::partner(character.style),
                        character.titled_name(),
                    );
                    if let Ok(client) = inner.db.get().await
                        && let Err(e) =
                            GreenDragonNews::add(&client, day, Some(user_id), &body).await
                    {
                        tracing::warn!("greendragon divorce news write failed: {e}");
                    }
                }
            }
            let _ = tx.send(CharacterLoad::Ready(Box::new(character)));
        });
        rx
    }

    /// Persist a character blob, fire-and-forget but **ordered**: stale writes
    /// are dropped against newer ones for the same user (see [`commit_save`]).
    pub fn save_character(&self, user_id: Uuid, character: &Character) {
        let seq = self.inner.next_seq();
        let gate = self.inner.gate(user_id);
        let db = self.inner.db.clone();
        let blob = persist::to_json(character);
        tokio::spawn(commit_save(db, gate, seq, user_id, blob));
    }

    /// Delete a user's saved character, fire-and-forget (the "start over"
    /// action), ordered against any pending save through the same gate. Any
    /// open bounties on the departed head close to the house (`dag`'s
    /// `delete_character` hook); the lazy stray sweep catches races.
    pub fn delete_character(&self, user_id: Uuid) {
        let seq = self.inner.next_seq();
        let gate = self.inner.gate(user_id);
        let db = self.inner.db.clone();
        tokio::spawn(commit_delete(db, gate, seq, user_id));
        self.close_bounties_on(user_id);
    }

    /// Append a line to the village's daily news, fire-and-forget (LoGD
    /// `addnews`). `user_id` is the item's subject; `None` marks a system line.
    pub fn publish_news(&self, user_id: Option<Uuid>, body: String) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonNews::add(&client, today(), user_id, &body).await {
                        tracing::warn!("greendragon news write failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("greendragon db get failed on news write: {e}"),
            }
        });
    }

    /// Load the news page for `days_back` days ago (0 = today). Expired items
    /// are reaped first — upstream prunes at view time too (`news.php`).
    pub fn load_news(&self, days_back: i64) -> watch::Receiver<NewsLoad> {
        let (tx, rx) = watch::channel(NewsLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let day = today() - days_back;
            let lines = match inner.db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonNews::prune(&client, today()).await {
                        tracing::warn!("greendragon news prune failed: {e}");
                    }
                    match GreenDragonNews::list_for_day(&client, day, NEWS_PAGE_LIMIT).await {
                        Ok(lines) => lines,
                        Err(e) => {
                            tracing::warn!("greendragon news read failed: {e}");
                            Vec::new()
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("greendragon db get failed on news read: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send(NewsLoad::Ready(Arc::new(lines)));
        });
        rx
    }

    /// Load a commentary room's display window: the newest `limit` lines,
    /// newest first (upstream `viewcommentary`).
    pub fn load_commentary(
        &self,
        section: String,
        limit: usize,
    ) -> watch::Receiver<CommentaryLoad> {
        let (tx, rx) = watch::channel(CommentaryLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let lines = match inner.db.get().await {
                Ok(client) => read_commentary(&client, &section, limit).await,
                Err(e) => {
                    tracing::warn!("greendragon db get failed on commentary read: {e}");
                    Vec::new()
                }
            };
            let _ = tx.send(CommentaryLoad::Ready {
                lines: Arc::new(lines),
                double_post: false,
            });
        });
        rx
    }

    /// Post a prepared line into a room and return its refreshed window. The
    /// double-post check runs here against the section's actual newest row
    /// (upstream `injectcommentary`), not the possibly stale page the player
    /// was reading. Old comments are pruned opportunistically on write.
    pub fn post_commentary(
        &self,
        section: String,
        limit: usize,
        user_id: Uuid,
        name: String,
        body: String,
    ) -> watch::Receiver<CommentaryLoad> {
        let (tx, rx) = watch::channel(CommentaryLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let (lines, double_post) = match inner.db.get().await {
                Ok(client) => {
                    let newest = GreenDragonCommentary::latest(&client, &section, 1)
                        .await
                        .unwrap_or_default();
                    let double_post = newest
                        .first()
                        .is_some_and(|r| r.user_id == Some(user_id) && r.body == body);
                    if !double_post {
                        if let Err(e) = GreenDragonCommentary::add(
                            &client,
                            &section,
                            Some(user_id),
                            &name,
                            &body,
                        )
                        .await
                        {
                            tracing::warn!("greendragon commentary write failed: {e}");
                        }
                        if let Err(e) = GreenDragonCommentary::prune(&client).await {
                            tracing::warn!("greendragon commentary prune failed: {e}");
                        }
                    }
                    (read_commentary(&client, &section, limit).await, double_post)
                }
                Err(e) => {
                    tracing::warn!("greendragon db get failed on commentary write: {e}");
                    (Vec::new(), false)
                }
            };
            let _ = tx.send(CommentaryLoad::Ready {
                lines: Arc::new(lines),
                double_post,
            });
        });
        rx
    }

    /// Load every saved character for the warrior list and Hall of Fame
    /// (`list.php` / `hof.php` read the whole accounts table; ours decodes
    /// the blobs and lets the views sort). Corrupt/empty blobs are skipped.
    pub fn load_roster(&self) -> watch::Receiver<RosterLoad> {
        let (tx, rx) = watch::channel(RosterLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let rows = match inner.db.get().await {
                Ok(client) => match GreenDragonCharacter::load_all(&client).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        tracing::warn!("greendragon roster read failed: {e}");
                        Vec::new()
                    }
                },
                Err(e) => {
                    tracing::warn!("greendragon db get failed on roster read: {e}");
                    Vec::new()
                }
            };
            let now = Utc::now();
            let entries: Vec<RosterEntry> = rows
                .into_iter()
                .filter_map(|(user_id, blob, updated)| {
                    let c = persist::from_json(&blob);
                    if c.name.trim().is_empty() {
                        return None; // corrupt blob: nothing worth listing
                    }
                    let idle_secs = (now - updated).num_seconds().max(0);
                    Some(RosterEntry {
                        user_id,
                        name: c.titled_name(),
                        handle: c.name.clone(),
                        level: c.level,
                        alive: c.alive,
                        race: c.race.name(),
                        dragon_kills: c.dragon_kills,
                        dragon_age: c.dragon_age,
                        best_dragon_age: c.best_dragon_age,
                        resurrections: c.resurrections,
                        gems: c.gems,
                        charm: c.charm,
                        max_hp: c.max_hitpoints(),
                        experience: c.experience,
                        wealth: c.gold as i64 + c.gold_in_bank,
                        online: c.online && idle_secs < ONLINE_WINDOW_SECS,
                        idle_secs,
                        lodged: c.lodged_today,
                        pvp_immune: c.pvp_immune(),
                        bounty_immune: c.bounty_immune(),
                        pvp_engaged_at: c.pvp_engaged_at,
                        clan_id: c.clan_id,
                    })
                })
                .collect();
            let _ = tx.send(RosterLoad::Ready(Arc::new(entries)));
        });
        rx
    }

    /// Read one character fresh for the barman's paid intel (`darkhorse.php`
    /// SELECTs the accounts row at pay time). A plain read — no lock, no
    /// gate — since nothing is written.
    pub fn load_enemy_intel(&self, target_id: Uuid) -> watch::Receiver<IntelLoad> {
        let (tx, rx) = watch::channel(IntelLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let target = match inner.db.get().await {
                Ok(client) => match GreenDragonCharacter::load(&client, target_id).await {
                    Ok(Some(blob)) => {
                        let c = persist::from_json(&blob);
                        // A corrupt blob decodes nameless: nothing to sell.
                        (!c.name.trim().is_empty()).then(|| Box::new(c))
                    }
                    Ok(None) => None,
                    Err(e) => {
                        tracing::warn!("greendragon intel read failed: {e}");
                        None
                    }
                },
                Err(e) => {
                    tracing::warn!("greendragon db get failed on intel read: {e}");
                    None
                }
            };
            let _ = tx.send(IntelLoad::Ready(target));
        });
        rx
    }

    /// Engage a sleeping warrior (`lib/pvpsupport.php` `setup_target`): a
    /// row-locked transaction re-checks everything against the target's
    /// *fresh* blob — still there, within two levels either way (wider than
    /// the list's `[-1, +2]` band, exactly upstream), not engaged by someone
    /// else inside the 10-minute window, not awake in the door, still alive —
    /// then stamps `pvp_engaged_at` (the dogpile guard) and snapshots the
    /// fight stats. The victim's `updated` is deliberately preserved: being
    /// attacked isn't presence. The per-user write gate is held across the
    /// transaction so in-process saves can't interleave.
    pub fn pvp_engage(&self, attacker_level: u8, target_id: Uuid) -> watch::Receiver<PvpEngage> {
        let (tx, rx) = watch::channel(PvpEngage::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let gate = inner.gate(target_id);
            let _held = gate.lock().await;
            let result = pvp_engage_tx(&inner.db, attacker_level, target_id).await;
            let msg = match result {
                Ok(engage) => engage,
                Err(e) => {
                    tracing::warn!("greendragon pvp engage failed: {e}");
                    PvpEngage::Refused("The dark swallows your approach; try again.".into())
                }
            };
            let _ = tx.send(msg);
        });
        rx
    }

    /// Settle a won PvP fight onto the sleeping victim (`pvpvictory`'s victim
    /// half): re-read their purse under the row lock, take the lesser of the
    /// engage-time and current gold (the bank absorbs any shortfall), dock
    /// [`model::PVP_DEFENDER_LOSE_PCT`]% of their engage-time experience,
    /// kill them, and leave a report for their next visit. Returns what the
    /// attacker won; the level-15 "no prowess" zeroing of the *attacker's*
    /// spoils is the caller's (the victim's losses stand either way).
    pub fn pvp_settle_victory(
        &self,
        victim_id: Uuid,
        engage: PvpTarget,
        attacker_id: Uuid,
        attacker_name: String,
    ) -> watch::Receiver<PvpSettle> {
        let (tx, rx) = watch::channel(PvpSettle::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let gate = inner.gate(victim_id);
            let _held = gate.lock().await;
            let msg = match pvp_settle_victory_tx(
                &inner.db,
                victim_id,
                &engage,
                attacker_id,
                &attacker_name,
            )
            .await
            {
                Ok(settle) => settle,
                Err(e) => {
                    tracing::warn!("greendragon pvp victory settle failed: {e}");
                    PvpSettle::Failed
                }
            };
            let _ = tx.send(msg);
        });
        rx
    }

    /// Read the bounty broker's ledger: the matured price on `me`'s own head
    /// plus the wanted list, sweeping stray (deleted-target) contracts and
    /// pruning old closed rows on the way — upstream does both lazily at
    /// list render.
    pub fn load_bounty_board(&self, me: Uuid) -> watch::Receiver<BountyBoardLoad> {
        let (tx, rx) = watch::channel(BountyBoardLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let (on_my_head, wanted) = match inner.db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonBounty::sweep_stray(&client).await {
                        tracing::warn!("greendragon bounty stray sweep failed: {e}");
                    }
                    if let Err(e) = GreenDragonBounty::prune_closed(&client).await {
                        tracing::warn!("greendragon bounty prune failed: {e}");
                    }
                    let on_my_head = GreenDragonBounty::matured_total_on(&client, me)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!("greendragon bounty head read failed: {e}");
                            0
                        })
                        .max(0) as u64;
                    let wanted = GreenDragonBounty::wanted_list(&client)
                        .await
                        .unwrap_or_else(|e| {
                            tracing::warn!("greendragon bounty list read failed: {e}");
                            Vec::new()
                        })
                        .into_iter()
                        .map(|(target, gold)| (target, gold.max(0) as u64))
                        .collect();
                    (on_my_head, wanted)
                }
                Err(e) => {
                    tracing::warn!("greendragon db get failed on bounty read: {e}");
                    (0, Vec::new())
                }
            };
            let _ = tx.send(BountyBoardLoad::Ready {
                on_my_head,
                wanted: Arc::new(wanted),
            });
        });
        rx
    }

    /// Place a bounty on `target`. The caller has already run the local
    /// checks (self, level, immunity, the minimum, the fee'd cost against
    /// gold on hand — upstream's order); this transaction runs the last one,
    /// the per-target open-total cap (which counts immature contracts too),
    /// and inserts with the `e_rand(0, 4h)` activation delay.
    pub fn place_bounty(
        &self,
        setter: Uuid,
        target: Uuid,
        amount: u64,
        cap: u64,
    ) -> watch::Receiver<BountyPlace> {
        let (tx, rx) = watch::channel(BountyPlace::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let placed = async {
                let mut client = inner.db.get().await?;
                let db_tx = client.transaction().await?;
                let current = GreenDragonBounty::open_total_on(&db_tx, target).await?.max(0) as u64;
                if amount + current > cap {
                    return anyhow::Ok(BountyPlace::OverCap(current));
                }
                let delay = rand::thread_rng().gen_range(0..=model::BOUNTY_DELAY_MAX_SECS);
                GreenDragonBounty::place(&db_tx, target, Some(setter), amount as i64, delay)
                    .await?;
                db_tx.commit().await?;
                Ok(BountyPlace::Placed)
            }
            .await;
            let msg = placed.unwrap_or_else(|e| {
                tracing::warn!("greendragon bounty place failed: {e}");
                BountyPlace::Failed
            });
            let _ = tx.send(msg);
        });
        rx
    }

    /// Close every open bounty on `user_id` to the house, fire-and-forget —
    /// the dragon-kill and character-deletion hooks (`dag`'s `dragonkill` /
    /// `delete_character`).
    pub fn close_bounties_on(&self, user_id: Uuid) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(e) = GreenDragonBounty::close_all_on(&client, user_id).await {
                        tracing::warn!("greendragon bounty close failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("greendragon db get failed on bounty close: {e}"),
            }
        });
    }

    /// The dragon-kill reward, fire-and-forget (the Lateania/NetHack milestone
    /// shape): a feed line for every kill, and — first kill only, deduped by
    /// the lifetime reward template and the `NOT EXISTS` award insert — a
    /// once-per-account chip payout plus the rankless GDS profile badge.
    pub fn reward_dragon_kill(&self, user_id: Uuid, kills: u32) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            // Every kill makes the dashboard feed; the paper in the village
            // square is the in-door counterpart.
            inner.activity.game_won_task(
                user_id,
                ActivityGame::GreenDragon,
                Some(format!("dragon kill #{kills}")),
                None,
            );

            let grant = match inner
                .chips
                .credit_lifetime_reward_template(
                    user_id,
                    GREENDRAGON_DRAGON_REWARD_KEY,
                    GREENDRAGON_DRAGON_LEDGER_REASON,
                )
                .await
            {
                Ok(grant) => grant,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        "failed to credit greendragon dragon-kill chips"
                    );
                    return;
                }
            };
            // Already claimed on an earlier kill — nothing more to do.
            if !grant.credited {
                return;
            }

            let badge = award_badge(GREENDRAGON_DRAGON_AWARD_CATEGORY, 1);
            match inner.db.get().await {
                Ok(client) => {
                    if let Err(error) = grant_unique_milestone_award(
                        &client,
                        user_id,
                        GREENDRAGON_DRAGON_AWARD_CATEGORY,
                        grant.amount,
                    )
                    .await
                    {
                        tracing::error!(
                            ?error,
                            user_id = %user_id,
                            badge = %badge,
                            "failed to grant greendragon profile award badge"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        badge = %badge,
                        "no db client for greendragon profile award badge"
                    );
                }
            }
        });
    }

    /// Attempt a haunt (`case_haunt3.php`) as a row-locked cross-player
    /// transaction: re-check "no active haunt" against the target's *fresh*
    /// blob, roll `e_rand(0, yourLevel) > e_rand(0, targetLevel)` (strict —
    /// ties fail), and on success write the mark plus a report in the same
    /// write. The 25 favor is the caller's to charge on a rolled attempt;
    /// refusals cost nothing. The target's `updated` stays untouched (being
    /// haunted isn't presence), and their gate is held across the
    /// transaction like every cross-player write.
    pub fn haunt(
        &self,
        my_level: u8,
        my_name: String,
        target_id: Uuid,
    ) -> watch::Receiver<HauntLoad> {
        let (tx, rx) = watch::channel(HauntLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let gate = inner.gate(target_id);
            let _held = gate.lock().await;
            let outcome = async {
                let mut client = inner.db.get().await?;
                let db_tx = client.transaction().await?;
                let Some((blob, _)) =
                    GreenDragonCharacter::load_for_update(&db_tx, target_id).await?
                else {
                    return anyhow::Ok(HauntLoad::Gone);
                };
                let mut c = persist::from_json(&blob);
                let target = c.titled_name();
                if !c.haunted_by.is_empty() {
                    return Ok(HauntLoad::AlreadyHaunted { target });
                }
                // Strict: ties fail (`$roll2 > $roll1`).
                let success = {
                    let mut rng = rand::thread_rng();
                    let theirs: u32 = rng.gen_range(0..=c.level as u32);
                    let mine: u32 = rng.gen_range(0..=my_level as u32);
                    mine > theirs
                };
                if !success {
                    return Ok(HauntLoad::Fumble { target });
                }
                c.haunted_by = my_name.clone();
                c.pvp_reports.push(format!(
                    "{my_name}'s shade crept through your dreams in the night. \
                     You will wake all the wearier for it."
                ));
                GreenDragonCharacter::update_data_keep_updated(
                    &db_tx,
                    target_id,
                    persist::to_json(&c),
                )
                .await?;
                db_tx.commit().await?;
                Ok(HauntLoad::Success { target })
            }
            .await;
            let msg = outcome.unwrap_or_else(|e| {
                tracing::warn!("greendragon haunt failed: {e}");
                HauntLoad::Gone
            });
            let _ = tx.send(msg);
        });
        rx
    }

    /// Settle a *lost* PvP fight onto the sleeping winner (`pvpdefeat`'s
    /// victim half), fire-and-forget — the attacker's own ruin is applied
    /// in-session. The sleeper gains the gold and experience the attacker
    /// computed off their own corpse, unless they somehow leveled down since
    /// engage (upstream's mid-fight dragon-kill guard), and gets a report.
    pub fn pvp_settle_defeat(
        &self,
        victim_id: Uuid,
        engage_level: u8,
        win_gold: u64,
        won_exp: u64,
        attacker_name: String,
    ) {
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let gate = inner.gate(victim_id);
            let _held = gate.lock().await;
            if let Err(e) = pvp_settle_defeat_tx(
                &inner.db,
                victim_id,
                engage_level,
                win_gold,
                won_exp,
                &attacker_name,
            )
            .await
            {
                tracing::warn!("greendragon pvp defeat settle failed: {e}");
            }
        });
    }

    /// Read the current Five Sixes jackpot (for the tavern's signboard).
    pub fn load_fivesix_pot(&self) -> watch::Receiver<Option<u64>> {
        let (tx, rx) = watch::channel(None);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            if let Ok(client) = inner.db.get().await
                && let Ok(Some(pot)) = GreenDragonSetting::get(&client, "fivesix_jackpot").await
            {
                let _ = tx.send(Some(pot.max(0) as u64));
            }
        });
        rx
    }

    /// Settle a Five Sixes play (`cost` staked, `sixes` rolled) against the
    /// one shared jackpot, atomically. The caller has already taken the stake
    /// off the character; the receiver reports what the pot paid.
    pub fn settle_fivesix(
        &self,
        cost: u64,
        max_pot: u64,
        sixes: u32,
    ) -> watch::Receiver<FiveSixLoad> {
        let (tx, rx) = watch::channel(FiveSixLoad::Loading);
        let inner = self.inner.clone();
        tokio::spawn(async move {
            let settled = match inner.db.get().await {
                Ok(client) => {
                    GreenDragonSetting::settle_fivesix(&client, cost as i64, max_pot as i64, sixes)
                        .await
                }
                Err(e) => Err(e),
            };
            let msg = match settled {
                Ok((pot, left_over)) => FiveSixLoad::Ready {
                    pot: pot.max(0) as u64,
                    left_over: left_over.max(0) as u64,
                },
                Err(e) => {
                    tracing::warn!("greendragon fivesix settle failed: {e}");
                    FiveSixLoad::Failed
                }
            };
            let _ = tx.send(msg);
        });
        rx
    }
}
