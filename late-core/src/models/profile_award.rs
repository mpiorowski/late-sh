use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Datelike, NaiveDate, Utc};

use super::artboard_piece::GALLERY_AWARD_MIN_APPLAUSE;
use super::chips::{ChipMove, UserChips};
use super::leaderboard::DailyPuzzle;
use tokio_postgres::Client;
use uuid::Uuid;

pub const PROFILE_AWARD_RANK_LIMIT: i32 = 3;
pub const LATEANIA_ARCHDEMON_AWARD_CATEGORY: &str = "lateania_archdemon";
pub const LATEANIA_FRONTIER_KING_AWARD_CATEGORY: &str = "lateania_frontier_king";
pub const LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY: &str = "lateania_sundering_deep";
pub const LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY: &str = "lateania_kaethyr_ascendant";
pub const NETHACK_AMULET_AWARD_CATEGORY: &str = "nethack_amulet";
pub const NETHACK_ASCENSION_AWARD_CATEGORY: &str = "nethack_ascension";
pub const DCSS_ORB_AWARD_CATEGORY: &str = "dcss_orb";
pub const DCSS_WIN_AWARD_CATEGORY: &str = "dcss_win";
pub const BROGUE_ESCAPE_AWARD_CATEGORY: &str = "brogue_escape";
pub const BROGUE_MASTERY_AWARD_CATEGORY: &str = "brogue_mastery";
pub const GREENDRAGON_DRAGON_AWARD_CATEGORY: &str = "greendragon_dragon";
pub const DARKROOM_ESCAPE_AWARD_CATEGORY: &str = "darkroom_escape";
pub const DARKROOM_BEACON_AWARD_CATEGORY: &str = "darkroom_beacon";
/// Deadchannel's Old Signal put down, the first time. The one door badge
/// that pays no chips: the game's wallets never convert (GAME.md).
pub const DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY: &str = "deadchannel_old_signal";
/// The month's last crown holder. Monthly like the ranked boards (it is
/// earned again every month and shows only for the month after), but
/// rankless like a milestone: the crown has one holder, so a `#1` on the
/// badge would be noise. That split is why it is in
/// [`is_rankless_award`] and not in [`MILESTONE_AWARD_CATEGORIES`].
pub const CROWN_AWARD_CATEGORY: &str = "crown";
/// The Artboard gallery's monthly board: ranked like the arcade boards
/// (`ART1` to `ART3`), scored by the best single piece's applause so ten
/// mediocre frames never beat one good one, and the only ranked award that
/// pays chips ([`gallery_prize_chips`]). A user needs a piece with at least
/// [`GALLERY_AWARD_MIN_APPLAUSE`] to be ranked at all.
pub const GALLERY_AWARD_CATEGORY: &str = "artboard";
/// The Late Time board's monthly winner: whoever was online longest last
/// month. First place only, so rankless like the crown (`LATE`, never
/// `LATE1`). The score is the month's online time in milliseconds.
pub const LATE_TIME_AWARD_CATEGORY: &str = "late_time";

/// The chip prize behind each gallery placement. Paid inside the snapshot
/// transaction, once per award row.
pub fn gallery_prize_chips(rank: i32) -> Option<i64> {
    match rank {
        1 => Some(40_000),
        2 => Some(15_000),
        3 => Some(10_000),
        _ => None,
    }
}

/// Every rankless milestone award: the one-off badges a game grants outright
/// rather than the monthly ranked boards. They differ from the ranked awards
/// in three ways at once (no `#1` suffix on the badge, shown whatever month
/// they were earned, granted by the game rather than the monthly snapshot), so
/// the set is worth naming once instead of being spelled out at each of those
/// three call sites.
///
/// Adding a game's badge means adding it here, to `award_category_code`, to
/// `award_category_label` and to `award_category_priority` (and, for a new
/// ladder, to `BADGE_LADDERS` and `ladder_label`), and the two badge
/// legends (`app/profile_modal/badges.rs`, `app/help_modal/data.rs`) are
/// tested against this list so a new badge cannot ship undocumented.
pub static MILESTONE_AWARD_CATEGORIES: [&str; 14] = [
    LATEANIA_ARCHDEMON_AWARD_CATEGORY,
    LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
    LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY,
    LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY,
    NETHACK_AMULET_AWARD_CATEGORY,
    NETHACK_ASCENSION_AWARD_CATEGORY,
    DCSS_ORB_AWARD_CATEGORY,
    DCSS_WIN_AWARD_CATEGORY,
    BROGUE_ESCAPE_AWARD_CATEGORY,
    BROGUE_MASTERY_AWARD_CATEGORY,
    GREENDRAGON_DRAGON_AWARD_CATEGORY,
    DARKROOM_ESCAPE_AWARD_CATEGORY,
    DARKROOM_BEACON_AWARD_CATEGORY,
    DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY,
];

/// Whether an award is one of those: granted outright, kept forever, shown
/// in chat labels whatever month it was earned.
pub fn is_milestone_award(category: &str) -> bool {
    MILESTONE_AWARD_CATEGORIES.contains(&category)
}

/// The monthly awards with a single holder: earned again every month and
/// shown only for the month after, like the ranked boards, but a `#1` on the
/// badge would be noise. The badge legends are tested against this list
/// alongside [`MILESTONE_AWARD_CATEGORIES`].
pub static SINGLE_HOLDER_AWARD_CATEGORIES: [&str; 2] =
    [CROWN_AWARD_CATEGORY, LATE_TIME_AWARD_CATEGORY];

/// The monthly ranked boards (`AW1`..`ART3`): the snapshot's `ranked` arms.
pub static RANKED_AWARD_CATEGORIES: [&str; 6] = [
    "arcade_wins",
    "top_chips",
    "tetris",
    "twenty_forty_eight",
    "snake",
    GALLERY_AWARD_CATEGORY,
];

/// One row of the Settings badge picker. A game with a milestone ladder is a
/// single row covering every rung: the chat label only ever shows its top
/// rung (`top_badge_per_game`), so the switch is "this game's badge", on or
/// off, never a choice between rungs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatBadgeRow {
    pub label: &'static str,
    /// The codes the row covers, as they read on a label (`LMG LKN LYS LKA`).
    pub codes: String,
    pub categories: Vec<&'static str>,
}

/// The name a whole ladder goes by in the picker.
fn ladder_label(ladder: &[&str]) -> &'static str {
    match ladder.first().copied() {
        Some(LATEANIA_ARCHDEMON_AWARD_CATEGORY) => "Lateania bosses",
        Some(NETHACK_AMULET_AWARD_CATEGORY) => "NetHack",
        Some(DCSS_ORB_AWARD_CATEGORY) => "DCSS",
        Some(BROGUE_ESCAPE_AWARD_CATEGORY) => "Brogue",
        Some(DARKROOM_ESCAPE_AWARD_CATEGORY) => "A Dark Room",
        other => unreachable!("badge ladder without a picker name: {other:?}"),
    }
}

/// Every badge that can ever show on a chat label, as picker rows in label
/// order: each ladder folded into one row, everything else one row each.
/// If you can earn it, you can hide it.
pub fn chat_badge_rows() -> Vec<ChatBadgeRow> {
    let mut rows = Vec::new();
    for category in all_award_categories() {
        match BADGE_LADDERS
            .iter()
            .find(|ladder| ladder.contains(&category))
        {
            Some(ladder) if ladder[0] != category => {}
            Some(ladder) => rows.push(ChatBadgeRow {
                label: ladder_label(ladder),
                codes: ladder
                    .iter()
                    .map(|rung| award_category_code(rung))
                    .collect::<Vec<_>>()
                    .join(" "),
                categories: ladder.to_vec(),
            }),
            None => rows.push(ChatBadgeRow {
                label: award_category_label(category),
                codes: match is_rankless_award(category) {
                    true => award_category_code(category).to_string(),
                    false => format!("{}1-3", award_category_code(category)),
                },
                categories: vec![category],
            }),
        }
    }
    rows
}

/// Every award category that can ever show on a chat label, in the order the
/// label stacks them (`award_category_priority`).
pub fn all_award_categories() -> Vec<&'static str> {
    let mut all: Vec<&'static str> = RANKED_AWARD_CATEGORIES
        .iter()
        .chain(SINGLE_HOLDER_AWARD_CATEGORIES.iter())
        .chain(MILESTONE_AWARD_CATEGORIES.iter())
        .copied()
        .collect();
    all.sort_by_key(|category| award_category_priority(category));
    all
}

/// Whether an award's badge is printed without a rank digit: every milestone
/// and every single-holder monthly award. The chat-label SQL in `user.rs`
/// spells the same split: this list is the arm that skips `|| rank::text`.
pub fn is_rankless_award(category: &str) -> bool {
    is_milestone_award(category) || SINGLE_HOLDER_AWARD_CATEGORIES.contains(&category)
}

/// The milestone ladders, one per game, weakest first. Chat author labels show
/// only the highest badge a player holds on each ladder, so a shelf of crowns
/// does not push the message off the line; the profile page still lists every
/// award. A game with a single milestone badge (Green Dragon) needs no entry.
///
/// This is the one place the ordering lives. Adding a badge to a game means
/// adding it here, not writing another pair of comparisons at the call site.
pub static BADGE_LADDERS: [&[&str]; 5] = [
    &[
        LATEANIA_ARCHDEMON_AWARD_CATEGORY,
        LATEANIA_FRONTIER_KING_AWARD_CATEGORY,
        LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY,
        LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY,
    ],
    &[
        NETHACK_AMULET_AWARD_CATEGORY,
        NETHACK_ASCENSION_AWARD_CATEGORY,
    ],
    &[DCSS_ORB_AWARD_CATEGORY, DCSS_WIN_AWARD_CATEGORY],
    &[BROGUE_ESCAPE_AWARD_CATEGORY, BROGUE_MASTERY_AWARD_CATEGORY],
    &[
        DARKROOM_ESCAPE_AWARD_CATEGORY,
        DARKROOM_BEACON_AWARD_CATEGORY,
    ],
];

/// Drop every badge code that a badge higher on the same game's ladder
/// supersedes, keeping the input's order. Codes belonging to no ladder pass
/// through untouched.
pub fn top_badge_per_game<'a>(badges: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let held: Vec<&str> = badges.into_iter().collect();
    let mut superseded: Vec<&'static str> = Vec::new();
    for ladder in BADGE_LADDERS {
        // Everything below the highest rung this player holds is hidden.
        let highest = ladder
            .iter()
            .rposition(|category| held.contains(&award_category_code(category)));
        if let Some(highest) = highest {
            superseded.extend(
                ladder[..highest]
                    .iter()
                    .map(|category| award_category_code(category)),
            );
        }
    }
    held.into_iter()
        .filter(|badge| !superseded.contains(badge))
        .collect()
}

#[derive(Clone, Debug)]
pub struct ProfileAward {
    pub id: Uuid,
    pub user_id: Uuid,
    pub category: String,
    pub period_month: NaiveDate,
    pub rank: i32,
    pub score_value: i64,
    pub awarded_at: DateTime<Utc>,
}

impl ProfileAward {
    pub fn badge(&self) -> String {
        award_badge(&self.category, self.rank)
    }

    pub fn label(&self) -> &'static str {
        award_category_label(&self.category)
    }

    pub fn month_label(&self) -> String {
        month_label(self.period_month)
    }

    pub fn description(&self) -> String {
        format!(
            "{} #{} · {} · {}",
            self.label(),
            self.rank,
            format_score_value(&self.category, self.score_value),
            self.month_label()
        )
    }
}

/// The awards behind a batch of ledger refs, keyed by id: one primary-key
/// scan. Ids matching nothing are absent.
pub async fn find_profile_awards_by_ids(
    client: &Client,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, ProfileAward>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = client
        .query(
            "SELECT id, user_id, category, period_month, rank, score_value, awarded_at
             FROM profile_awards
             WHERE id = ANY($1)",
            &[&ids],
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let award = ProfileAward {
                id: row.get("id"),
                user_id: row.get("user_id"),
                category: row.get("category"),
                period_month: row.get("period_month"),
                rank: row.get("rank"),
                score_value: row.get("score_value"),
                awarded_at: row.get("awarded_at"),
            };
            (award.id, award)
        })
        .collect())
}

pub async fn list_profile_awards_for_user(
    client: &Client,
    user_id: Uuid,
) -> Result<Vec<ProfileAward>> {
    let rows = client
        .query(
            "SELECT id, user_id, category, period_month, rank, score_value, awarded_at
             FROM profile_awards
             WHERE user_id = $1
               AND rank <= $2
             ORDER BY period_month DESC,
                      rank ASC,
                      CASE category
                        WHEN 'arcade_wins' THEN 0
                        WHEN 'top_chips' THEN 1
                        WHEN 'tetris' THEN 2
                        WHEN 'twenty_forty_eight' THEN 3
                        WHEN 'snake' THEN 4
                        WHEN 'crown' THEN 5
                        WHEN 'artboard' THEN 6
                        WHEN 'late_time' THEN 7
                        ELSE 99
                      END,
                      awarded_at DESC",
            &[&user_id, &PROFILE_AWARD_RANK_LIMIT],
        )
        .await?;

    Ok(rows.into_iter().map(ProfileAward::from).collect())
}

/// What one snapshot pass wrote: the award rows it created and the gallery
/// prizes it paid for them. Once a month is settled a re-run creates
/// nothing and pays nothing, however the applause has moved since.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AwardSnapshotOutcome {
    pub inserted: u64,
    /// `(user_id, rank, chips)` for each gallery prize paid in this pass.
    pub gallery_prizes_paid: Vec<(Uuid, i32, i64)>,
}

/// Write last month's ranked awards, and pay the gallery prizes for the
/// rows written. One transaction: an award row without its prize would be
/// a prize lost forever, since the `ON CONFLICT DO NOTHING` re-run never
/// sees that row again. `RETURNING` on the insert is what keeps a second
/// replica's pass from paying: it inserts nothing, so it pays nothing. The
/// gallery arm is settled once per month (see `gallery_best`), because its
/// input, applause, is the one score that keeps moving after the rollover.
pub async fn snapshot_previous_month_profile_awards(
    client: &mut Client,
) -> Result<AwardSnapshotOutcome> {
    let rank_limit = i64::from(PROFILE_AWARD_RANK_LIMIT);
    let excluded_reasons = ChipMove::excluded_earning_reasons();
    // One arm per daily puzzle, generated from the roster with the same
    // points expression the live Arcade Wins board uses, so the persisted
    // award cannot score a different set of games than the page did.
    let arcade_arms: String = DailyPuzzle::ALL
        .iter()
        .map(|puzzle| {
            format!(
                "SELECT user_id, {points} AS points
                 FROM {table}, bounds
                 WHERE puzzle_date >= bounds.period_month
                   AND puzzle_date < (bounds.period_month + INTERVAL '1 month')::date",
                points = puzzle.points_sql(),
                table = puzzle.wins_table(),
            )
        })
        .collect::<Vec<_>>()
        .join("\nUNION ALL\n");
    let tx = client.transaction().await?;
    let inserted = tx
        .query(
            &format!("INSERT INTO profile_awards (user_id, category, period_month, rank, score_value)
             WITH bounds AS (
                SELECT
                    (date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month')::date AS period_month,
                    ((date_trunc('month', now() AT TIME ZONE 'UTC')::date - INTERVAL '1 month') AT TIME ZONE 'UTC') AS period_start,
                    (date_trunc('month', now() AT TIME ZONE 'UTC')::date AT TIME ZONE 'UTC') AS period_end
             ),
             chip_totals AS (
                SELECT user_id, SUM(delta)::bigint AS value
                FROM chip_ledger, bounds
                WHERE reason <> ALL($2)
                  AND created_at >= bounds.period_start
                  AND created_at < bounds.period_end
                GROUP BY user_id
                HAVING SUM(delta) > 0
             ),
             arcade_wins AS (
                {arcade_arms}
             ),
             arcade_totals AS (
                SELECT user_id, SUM(points)::bigint AS value
                FROM arcade_wins
                GROUP BY user_id
             ),
             score_events AS (
                SELECT user_id, game, score
                FROM game_score_events, bounds
                WHERE game IN ('tetris', '2048', 'snake')
                  AND created_at >= bounds.period_start
                  AND created_at < bounds.period_end
                UNION ALL
                SELECT user_id, 'tetris' AS game, score
                FROM tetris_high_scores, bounds
                WHERE updated >= bounds.period_start
                  AND updated < bounds.period_end
                UNION ALL
                SELECT user_id, '2048' AS game, score
                FROM twenty_forty_eight_high_scores, bounds
                WHERE updated >= bounds.period_start
                  AND updated < bounds.period_end
                UNION ALL
                SELECT user_id, 'snake' AS game, score
                FROM snake_high_scores, bounds
                WHERE updated >= bounds.period_start
                  AND updated < bounds.period_end
             ),
             score_totals AS (
                SELECT user_id,
                       CASE game
                         WHEN 'tetris' THEN 'tetris'
                         WHEN '2048' THEN 'twenty_forty_eight'
                         WHEN 'snake' THEN 'snake'
                       END AS category,
                       MAX(score)::bigint AS value
                FROM score_events
                GROUP BY user_id, game
             ),
             -- The crown's month-end holder: the last reign taken inside
             -- the month, whether or not it is still open. The rollover
             -- itself needs no sweeper (a reign is current only while its
             -- month is), so this reads the row rather than a closed flag.
             crown_holder AS (
                SELECT crown_reigns.holder_user_id AS user_id,
                       crown_reigns.paid_chips AS value
                FROM crown_reigns, bounds
                WHERE crown_reigns.month = bounds.period_month
                ORDER BY crown_reigns.taken_at DESC, crown_reigns.id DESC
                LIMIT 1
             ),
             -- The gallery: each hanger's best piece of the month by
             -- applause (earliest hang breaks a tie between their own),
             -- and only hangers whose best piece cleared the floor.
             --
             -- This arm pays chips, so it must run to completion exactly
             -- once a month. `ArtboardPiece::toggle_applause` closes the
             -- month at the rollover, but a mod removal (`removed_at`)
             -- still moves the ranking afterwards, and `ON CONFLICT DO
             -- NOTHING` alone would let a hanger who climbed into the top
             -- 3 on a later pass (the 24h fallback, a restart, another
             -- replica) get a fresh row and a fresh prize. The `NOT
             -- EXISTS` settles it: once any `artboard` row exists for the
             -- month every later pass ranks nobody, inserts nothing and
             -- pays nothing.
             gallery_best AS (
                SELECT DISTINCT ON (p.user_id)
                       p.user_id,
                       applause.count::bigint AS value,
                       p.created AS hung_at
                FROM artboard_pieces p
                JOIN (
                    SELECT piece_id, count(*) AS count
                    FROM artboard_piece_votes
                    GROUP BY piece_id
                ) applause ON applause.piece_id = p.id
                CROSS JOIN bounds
                WHERE p.period_month = bounds.period_month
                  AND p.removed_at IS NULL
                  AND applause.count >= $3
                  AND NOT EXISTS (
                    SELECT 1 FROM profile_awards
                    WHERE category = 'artboard'
                      AND period_month = bounds.period_month
                  )
                ORDER BY p.user_id, applause.count DESC, p.created ASC
             ),
             -- Late Time's first place. A segment is attributed to the
             -- month it began, so last month's totals can still grow for
             -- one checkpoint (five minutes) past the rollover. Settled
             -- once like the gallery: without the `NOT EXISTS` a later
             -- pass (the 24h fallback, a restart) could crown a second
             -- user who overtook in that spill, and `ON CONFLICT` would
             -- not stop it because the conflict key includes the user.
             -- RANK, so an exact-millisecond tie shares first place.
             late_time_leader AS (
                SELECT user_id, value, rank
                FROM (
                    SELECT online.user_id,
                           online.total_milliseconds AS value,
                           RANK() OVER (ORDER BY online.total_milliseconds DESC) AS rank
                    FROM user_online_time_monthly online, bounds
                    WHERE online.month_start = bounds.period_month
                      AND online.total_milliseconds > 0
                      AND NOT EXISTS (
                        SELECT 1 FROM profile_awards
                        WHERE category = 'late_time'
                          AND period_month = bounds.period_month
                      )
                ) standings
                WHERE rank = 1
             ),
             ranked AS (
                SELECT user_id,
                       'top_chips'::text AS category,
                       value,
                       RANK() OVER (ORDER BY value DESC) AS rank
                FROM chip_totals
                UNION ALL
                SELECT user_id,
                       'arcade_wins'::text AS category,
                       value,
                       RANK() OVER (ORDER BY value DESC) AS rank
                FROM arcade_totals
                UNION ALL
                SELECT user_id,
                       category,
                       value,
                       RANK() OVER (PARTITION BY category ORDER BY value DESC) AS rank
                FROM score_totals
                UNION ALL
                -- One holder, so the rank is a constant rather than a
                -- window; `award_badge` prints this category without the
                -- digit (`is_rankless_award`).
                SELECT user_id,
                       'crown'::text AS category,
                       value,
                       1::bigint AS rank
                FROM crown_holder
                UNION ALL
                -- First place only; `award_badge` prints it bare.
                SELECT user_id,
                       'late_time'::text AS category,
                       value,
                       rank
                FROM late_time_leader
                UNION ALL
                -- ROW_NUMBER, not RANK: this is the one arm that mints
                -- chips, and RANK would hand every hanger tied at the top
                -- the full first prize. Ties break toward the earlier
                -- hang, the same order the hall of fame uses. At most
                -- three rows, three prizes, a month.
                SELECT user_id,
                       'artboard'::text AS category,
                       value,
                       ROW_NUMBER() OVER (ORDER BY value DESC, hung_at ASC) AS rank
                FROM gallery_best
             )
             SELECT ranked.user_id, ranked.category, bounds.period_month, ranked.rank::int, ranked.value
             FROM ranked
             CROSS JOIN bounds
             WHERE ranked.rank <= $1
             ON CONFLICT (user_id, category, period_month)
             DO NOTHING
             RETURNING id, user_id, category, rank"),
            &[&rank_limit, &excluded_reasons, &GALLERY_AWARD_MIN_APPLAUSE],
        )
        .await?;

    let mut gallery_prizes_paid = Vec::new();
    for row in &inserted {
        let category: String = row.get("category");
        if category != GALLERY_AWARD_CATEGORY {
            continue;
        }
        let rank: i32 = row.get("rank");
        let Some(chips) = gallery_prize_chips(rank) else {
            continue;
        };
        let award_id: Uuid = row.get("id");
        let user_id: Uuid = row.get("user_id");
        UserChips::apply(
            &tx,
            user_id,
            ChipMove::ArtboardPrize,
            chips,
            &award_id.to_string(),
        )
        .await?;
        gallery_prizes_paid.push((user_id, rank, chips));
    }
    tx.commit().await?;

    Ok(AwardSnapshotOutcome {
        inserted: inserted.len() as u64,
        gallery_prizes_paid,
    })
}

/// Grant a one-time, rankless milestone award (Lateania bosses, NetHack
/// milestones) to a user. Idempotent per (user, category): the `NOT EXISTS`
/// guard means a re-run after the award already exists is a no-op, so this is
/// safe to call from a fire-and-forget task that may run more than once.
pub async fn grant_unique_milestone_award(
    client: &Client,
    user_id: Uuid,
    category: &str,
    score_value: i64,
) -> Result<bool> {
    let today = Utc::now().date_naive();
    let period_month = today
        .with_day(1)
        .expect("every valid date has a first day of its month");
    let inserted = client
        .execute(
            "INSERT INTO profile_awards (user_id, category, period_month, rank, score_value)
             SELECT $1, $2, $3, 1, $4
             WHERE NOT EXISTS (
                SELECT 1
                FROM profile_awards
                WHERE user_id = $1
                  AND category = $2
             )",
            &[&user_id, &category, &period_month, &score_value],
        )
        .await?;
    Ok(inserted > 0)
}

pub fn award_badge(category: &str, rank: i32) -> String {
    if is_rankless_award(category) {
        return award_category_code(category).to_string();
    }
    let prefix = award_category_code(category);
    format!("{prefix}{rank}")
}

pub fn award_category_code(category: &str) -> &'static str {
    match category {
        "top_chips" => "CHIP",
        "arcade_wins" => "AW",
        "tetris" => "LA",
        "twenty_forty_eight" => "24#",
        "snake" => "SN",
        // Boss badges are coded after the boss, not the place: Mal'Gareth,
        // the King who was promised Nothing, YSsgar, KAethyr Ascendant.
        LATEANIA_ARCHDEMON_AWARD_CATEGORY => "LMG",
        LATEANIA_FRONTIER_KING_AWARD_CATEGORY => "LKN",
        LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY => "LYS",
        LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY => "LKA",
        NETHACK_AMULET_AWARD_CATEGORY => "NHA",
        NETHACK_ASCENSION_AWARD_CATEGORY => "NHY",
        DCSS_ORB_AWARD_CATEGORY => "DCO",
        DCSS_WIN_AWARD_CATEGORY => "DCW",
        BROGUE_ESCAPE_AWARD_CATEGORY => "BRE",
        BROGUE_MASTERY_AWARD_CATEGORY => "BRM",
        GREENDRAGON_DRAGON_AWARD_CATEGORY => "GDS",
        DARKROOM_ESCAPE_AWARD_CATEGORY => "ADE",
        DARKROOM_BEACON_AWARD_CATEGORY => "ADB",
        DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY => "SIG",
        CROWN_AWARD_CATEGORY => "CRWN",
        GALLERY_AWARD_CATEGORY => "ART",
        LATE_TIME_AWARD_CATEGORY => "LATE",
        _ => "LB",
    }
}

pub fn award_category_label(category: &str) -> &'static str {
    match category {
        "top_chips" => "Top Chips",
        "arcade_wins" => "Arcade Wins",
        "tetris" => "Lateris",
        "twenty_forty_eight" => "2048",
        "snake" => "Snake",
        LATEANIA_ARCHDEMON_AWARD_CATEGORY => "Lateania Archdemon",
        LATEANIA_FRONTIER_KING_AWARD_CATEGORY => "Lateania Frontier King",
        LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY => "Lateania Sundering Deep",
        LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY => "Lateania Kaethyr Ascendant",
        NETHACK_AMULET_AWARD_CATEGORY => "NetHack Amulet",
        NETHACK_ASCENSION_AWARD_CATEGORY => "NetHack Ascension",
        DCSS_ORB_AWARD_CATEGORY => "DCSS Orb of Zot",
        DCSS_WIN_AWARD_CATEGORY => "DCSS Escape",
        BROGUE_ESCAPE_AWARD_CATEGORY => "Brogue Escape",
        BROGUE_MASTERY_AWARD_CATEGORY => "Brogue Mastery",
        GREENDRAGON_DRAGON_AWARD_CATEGORY => "Green Dragon Slayer",
        DARKROOM_ESCAPE_AWARD_CATEGORY => "A Dark Room Escape",
        DARKROOM_BEACON_AWARD_CATEGORY => "A Dark Room Homefleet",
        DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY => "Old Signal",
        CROWN_AWARD_CATEGORY => "The Crown",
        GALLERY_AWARD_CATEGORY => "Artboard Gallery",
        LATE_TIME_AWARD_CATEGORY => "Late Time",
        _ => "Leaderboard",
    }
}

pub fn award_category_priority(category: &str) -> i32 {
    match category {
        "arcade_wins" => 0,
        "top_chips" => 1,
        CROWN_AWARD_CATEGORY => 5,
        GALLERY_AWARD_CATEGORY => 6,
        LATE_TIME_AWARD_CATEGORY => 7,
        "tetris" => 2,
        "twenty_forty_eight" => 3,
        "snake" => 4,
        LATEANIA_ARCHDEMON_AWARD_CATEGORY => 10,
        LATEANIA_FRONTIER_KING_AWARD_CATEGORY => 11,
        LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY => 12,
        LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY => 13,
        NETHACK_AMULET_AWARD_CATEGORY => 14,
        NETHACK_ASCENSION_AWARD_CATEGORY => 15,
        GREENDRAGON_DRAGON_AWARD_CATEGORY => 16,
        DCSS_ORB_AWARD_CATEGORY => 17,
        DCSS_WIN_AWARD_CATEGORY => 18,
        BROGUE_ESCAPE_AWARD_CATEGORY => 19,
        BROGUE_MASTERY_AWARD_CATEGORY => 20,
        DARKROOM_ESCAPE_AWARD_CATEGORY => 21,
        DARKROOM_BEACON_AWARD_CATEGORY => 22,
        DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY => 23,
        _ => 99,
    }
}

pub fn month_label(month: NaiveDate) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month_name = MONTHS
        .get(month.month0() as usize)
        .copied()
        .unwrap_or("???");
    format!("{month_name}'{:02}", month.year().rem_euclid(100))
}

pub fn format_score_value(category: &str, value: i64) -> String {
    match category {
        "top_chips" => format!("{value} chips"),
        "arcade_wins" => format!("{value} pts"),
        LATEANIA_ARCHDEMON_AWARD_CATEGORY
        | LATEANIA_FRONTIER_KING_AWARD_CATEGORY
        | LATEANIA_SUNDERING_DEEP_AWARD_CATEGORY
        | LATEANIA_KAETHYR_ASCENDANT_AWARD_CATEGORY
        | NETHACK_AMULET_AWARD_CATEGORY
        | NETHACK_ASCENSION_AWARD_CATEGORY
        | DCSS_ORB_AWARD_CATEGORY
        | DCSS_WIN_AWARD_CATEGORY
        | BROGUE_ESCAPE_AWARD_CATEGORY
        | BROGUE_MASTERY_AWARD_CATEGORY
        | GREENDRAGON_DRAGON_AWARD_CATEGORY
        | DARKROOM_ESCAPE_AWARD_CATEGORY
        | DARKROOM_BEACON_AWARD_CATEGORY => {
            format!("{value} chips")
        }
        // No chips: the score is the mark the badge was granted on.
        DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY => format!("mark {value}"),
        // The crown's score is what the final holder burned to take it.
        CROWN_AWARD_CATEGORY => format!("{value} chips"),
        GALLERY_AWARD_CATEGORY => format!("{value} applause"),
        LATE_TIME_AWARD_CATEGORY => {
            let minutes = value / 60_000;
            format!("{}h {}m online", minutes / 60, minutes % 60)
        }
        _ => format!("{value} score"),
    }
}

impl From<tokio_postgres::Row> for ProfileAward {
    fn from(row: tokio_postgres::Row) -> Self {
        Self {
            id: row.get("id"),
            user_id: row.get("user_id"),
            category: row.get("category"),
            period_month: row.get("period_month"),
            rank: row.get("rank"),
            score_value: row.get("score_value"),
            awarded_at: row.get("awarded_at"),
        }
    }
}
