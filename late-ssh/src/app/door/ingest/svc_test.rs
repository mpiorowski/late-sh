use late_core::models::arcade_handle::ArcadeHandle;
use late_core::test_utils::create_test_user;
use uuid::Uuid;

use super::stream::StatsFrame;
use super::svc::DoorIngestService;
use crate::app::activity::publisher::ActivityPublisher;
use crate::app::games::chips::svc::ChipService;
use crate::test_helpers::{new_test_db, wait_until};

const HANDLE: &str = "Wormsong";

fn ingest_service(db: &late_core::db::Db) -> DoorIngestService {
    let (activity_tx, _rx) = crate::app::activity::channel::new(64);
    let activity = ActivityPublisher::new(db.clone(), activity_tx);
    DoorIngestService::new(db.clone(), ChipService::new(db.clone()), activity)
}

async fn claim_handle(db: &late_core::db::Db, user_id: Uuid) {
    let client = db.get().await.expect("db client");
    let outcome = ArcadeHandle::claim(&client, user_id, HANDLE)
        .await
        .expect("claim handle");
    assert_eq!(
        outcome,
        late_core::models::arcade_handle::ClaimOutcome::Claimed
    );
}

fn death_frame(offset: i64) -> StatsFrame {
    StatsFrame {
        file: "logfile".to_string(),
        next_offset: offset,
        line: format!(
            "v=0.34.1:name={HANDLE}:xl=14:place=D::10:absdepth=10:turn=23456:sc=54321:ktyp=mon:killer=an orc warrior:end=20260006221530S:tmsg=slain by an orc warrior"
        ),
    }
}

fn win_frame(offset: i64) -> StatsFrame {
    StatsFrame {
        file: "logfile".to_string(),
        next_offset: offset,
        line: format!(
            "v=0.34.1:name={HANDLE}:xl=27:absdepth=0:urune=3:turn=91234:sc=2345678:ktyp=winning:end=20260006230000S:tmsg=escaped with the Orb and 3 runes!"
        ),
    }
}

fn orb_frame(offset: i64) -> StatsFrame {
    StatsFrame {
        file: "milestones".to_string(),
        next_offset: offset,
        line: format!(
            "v=0.34.1:name={HANDLE}:xl=25:place=Zot::5:absdepth=27:time=20260006215500S:type=orb:milestone=found the Orb of Zot!"
        ),
    }
}

async fn run_count(db: &late_core::db::Db, user_id: Uuid) -> i64 {
    let client = db.get().await.expect("db client");
    client
        .query_one(
            "SELECT COUNT(*)::bigint AS n FROM door_runs WHERE user_id = $1",
            &[&user_id],
        )
        .await
        .expect("count runs")
        .get("n")
}

async fn cursor_for(db: &late_core::db::Db, file: &str) -> Option<i64> {
    cursor_for_game(db, "dcss", file).await
}

async fn cursor_for_game(db: &late_core::db::Db, game: &str, file: &str) -> Option<i64> {
    let client = db.get().await.expect("db client");
    client
        .query_opt(
            "SELECT next_offset FROM door_log_cursors WHERE game = $1 AND file = $2",
            &[&game, &file],
        )
        .await
        .expect("cursor row")
        .map(|row| row.get("next_offset"))
}

async fn award_chip_total(db: &late_core::db::Db, user_id: Uuid) -> i64 {
    award_chip_total_for(db, user_id, "dcss").await
}

/// What a door actually paid this account, read off the ledger rather than the
/// claim rows: a gated payout writes several claims (run identity plus the
/// lockout) and exactly one ledger line, so the ledger is the only honest
/// total.
async fn award_chip_total_for(db: &late_core::db::Db, user_id: Uuid, game: &str) -> i64 {
    let client = db.get().await.expect("db client");
    client
        .query_one(
            "SELECT COALESCE(SUM(l.delta), 0)::bigint AS total
             FROM chip_ledger l
             JOIN game_payout_claims c ON c.id::text = l.source_ref
             WHERE l.user_id = $1 AND c.game = $2",
            &[&user_id, &game],
        )
        .await
        .expect("claim total")
        .get("total")
}

/// Age every claim this account holds for `game` by `days`, so a test can walk
/// past the 7-day lockout without sleeping.
async fn age_claims(db: &late_core::db::Db, user_id: Uuid, game: &str, days: i32) {
    let client = db.get().await.expect("db client");
    client
        .execute(
            "UPDATE game_payout_claims
             SET created = created - make_interval(days => $3)
             WHERE user_id = $1 AND game = $2",
            &[&user_id, &game, &days],
        )
        .await
        .expect("age claims");
}

async fn badge_count(db: &late_core::db::Db, user_id: Uuid, category: &str) -> i64 {
    let client = db.get().await.expect("db client");
    client
        .query_one(
            "SELECT COUNT(*)::bigint AS n FROM profile_awards
             WHERE user_id = $1 AND category = $2",
            &[&user_id, &category],
        )
        .await
        .expect("badge count")
        .get("n")
}

#[tokio::test]
async fn replayed_run_lines_land_one_row() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dcss-replay").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    let frame = death_frame(500);
    svc.handle_dcss_frame(&frame).await.expect("first ingest");
    svc.handle_dcss_frame(&frame).await.expect("replay ingest");

    assert_eq!(run_count(&test_db.db, user.id).await, 1);
    assert_eq!(cursor_for(&test_db.db, "logfile").await, Some(500));

    let client = test_db.db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT result, score, depth, turns, raw->>'killer' AS killer
             FROM door_runs WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("run row");
    assert_eq!(row.get::<_, &str>("result"), "death");
    assert_eq!(row.get::<_, i64>("score"), 54321);
    assert_eq!(row.get::<_, i32>("depth"), 10);
    assert_eq!(row.get::<_, i64>("turns"), 23456);
    assert_eq!(row.get::<_, &str>("killer"), "an orc warrior");
}

#[tokio::test]
async fn reserved_and_unknown_names_advance_the_cursor_without_rows() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dcss-skip").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    // A legacy derived playname (reserved shape) and a name nobody claimed.
    for (offset, name) in [(100, "late_a1b2c3"), (200, "NeverClaimed")] {
        let frame = StatsFrame {
            file: "logfile".to_string(),
            next_offset: offset,
            line: format!("name={name}:ktyp=mon:end=20260006120000S"),
        };
        svc.handle_dcss_frame(&frame).await.expect("ingest");
    }

    let client = test_db.db.get().await.expect("db client");
    let total: i64 = client
        .query_one("SELECT COUNT(*)::bigint AS n FROM door_runs", &[])
        .await
        .expect("count")
        .get("n");
    assert_eq!(total, 0);
    drop(client);
    assert_eq!(cursor_for(&test_db.db, "logfile").await, Some(200));
}

#[tokio::test]
async fn orb_milestone_pays_per_run_behind_the_lockout() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dcss-orb").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    svc.handle_dcss_frame(&orb_frame(300))
        .await
        .expect("orb ingest");

    // The award grant is fire-and-forget; wait for the claim.
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total(&db, user.id).await == 20_000 }
        },
        "orb chips granted",
    )
    .await;

    // A second run inside the week pays nothing, and a replayed line pays
    // nothing whenever it arrives.
    svc.handle_dcss_frame(&orb_frame(600))
        .await
        .expect("second orb");
    svc.handle_dcss_frame(&orb_frame(600))
        .await
        .expect("replay");
    tokio::task::yield_now().await;
    assert_eq!(award_chip_total(&test_db.db, user.id).await, 20_000);

    let client = test_db.db.get().await.expect("db client");
    let rows = client
        .query(
            "SELECT kind FROM door_milestones WHERE user_id = $1 ORDER BY source_offset",
            &[&user.id],
        )
        .await
        .expect("milestone rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<_, &str>("kind"), "orb");

    // The badge insert trails the chip credit inside the grant task, so it
    // gets its own wait.
    drop(client);
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "dcss_orb").await == 1 }
        },
        "orb badge granted",
    )
    .await;
}

#[tokio::test]
async fn a_run_past_the_lockout_pays_again_without_a_second_badge() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dcss-again").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    svc.handle_dcss_frame(&orb_frame(300))
        .await
        .expect("first orb");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total(&db, user.id).await == 20_000 }
        },
        "first orb chips",
    )
    .await;
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "dcss_orb").await == 1 }
        },
        "first orb badge",
    )
    .await;

    // Walk the whole account past the week. A distinct run then pays in full.
    age_claims(&test_db.db, user.id, "dcss", 8).await;
    svc.handle_dcss_frame(&orb_frame(900))
        .await
        .expect("later orb");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total(&db, user.id).await == 40_000 }
        },
        "second orb chips",
    )
    .await;

    // The badge never repeats, and the aged run's own line still pays nothing
    // however late it is replayed.
    svc.handle_dcss_frame(&orb_frame(300))
        .await
        .expect("replay of the aged line");
    tokio::task::yield_now().await;
    assert_eq!(award_chip_total(&test_db.db, user.id).await, 40_000);
    assert_eq!(badge_count(&test_db.db, user.id, "dcss_orb").await, 1);
}

#[tokio::test]
async fn a_lost_badge_heals_on_the_next_sighting() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dcss-heal").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    svc.handle_dcss_frame(&orb_frame(300))
        .await
        .expect("orb ingest");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "dcss_orb").await == 1 }
        },
        "orb badge granted",
    )
    .await;

    // Simulate a crash between the chip claim and the badge insert: the
    // claim is committed, the badge row never landed.
    let client = test_db.db.get().await.expect("db client");
    client
        .execute(
            "DELETE FROM profile_awards WHERE user_id = $1 AND category = 'dcss_orb'",
            &[&user.id],
        )
        .await
        .expect("drop badge row");

    // Replaying the same line pays no further chips but restores the badge.
    svc.handle_dcss_frame(&orb_frame(300))
        .await
        .expect("replay");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "dcss_orb").await == 1 }
        },
        "orb badge healed",
    )
    .await;
    assert_eq!(award_chip_total(&test_db.db, user.id).await, 20_000);
}

/// One line, one milestone: the win line pays the win and never the Orb
/// pickup it implies. The pickup has its own line on the milestone stream;
/// paying it here too would pay it twice once the pickup's week had passed.
#[tokio::test]
async fn a_win_grants_only_its_own_badge() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "dcss-win").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    svc.handle_dcss_frame(&win_frame(900))
        .await
        .expect("win ingest");

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total(&db, user.id).await == 50_000 }
        },
        "win chips granted",
    )
    .await;

    // The badge insert trails the chip credit inside the grant task, so it
    // gets its own wait before the exact-rows assertion.
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "dcss_win").await == 1 }
        },
        "win badge granted",
    )
    .await;
    assert_eq!(badge_count(&test_db.db, user.id, "dcss_orb").await, 0);

    let client = test_db.db.get().await.expect("db client");
    let categories: Vec<String> = client
        .query(
            "SELECT category FROM profile_awards
             WHERE user_id = $1 AND category IN ('dcss_orb', 'dcss_win')
             ORDER BY category",
            &[&user.id],
        )
        .await
        .expect("award rows")
        .into_iter()
        .map(|row| row.get("category"))
        .collect();
    assert_eq!(categories, vec!["dcss_win"]);

    let row = client
        .query_one(
            "SELECT result FROM door_runs WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("run row");
    assert_eq!(row.get::<_, &str>("result"), "win");
}

fn nethack_death_frame(offset: i64) -> StatsFrame {
    StatsFrame {
        file: "xlogfile".to_string(),
        next_offset: offset,
        // Died carrying the Amulet (achieve 0x20): the bit is a fact about
        // the run and never a payout; only the livelog pickup line pays.
        line: format!(
            "version=5.0.0\tpoints=12345\tdeathlev=6\tmaxlvl=8\tname={HANDLE}\tdeath=killed by a soldier ant\tturns=23456\tachieve=0x20\tendtime=1754560000\tflags=0x4"
        ),
    }
}

#[tokio::test]
async fn replayed_nethack_run_lands_one_row_and_pays_nothing() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nh-replay").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    let frame = nethack_death_frame(500);
    svc.handle_nethack_frame(&frame)
        .await
        .expect("first ingest");
    svc.handle_nethack_frame(&frame).await.expect("replay");

    assert_eq!(run_count(&test_db.db, user.id).await, 1);
    assert_eq!(
        cursor_for_game(&test_db.db, "nethack", "xlogfile").await,
        Some(500)
    );

    // A death carrying the Amulet pays nothing: the xlogfile achieve bit is
    // not a milestone source, whatever it says.
    tokio::task::yield_now().await;
    assert_eq!(
        award_chip_total_for(&test_db.db, user.id, "nethack").await,
        0
    );
    assert_eq!(badge_count(&test_db.db, user.id, "nethack_amulet").await, 0);

    let client = test_db.db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT game, result, score, depth, turns FROM door_runs WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("run row");
    assert_eq!(row.get::<_, &str>("game"), "nethack");
    assert_eq!(row.get::<_, &str>("result"), "death");
    assert_eq!(row.get::<_, i64>("score"), 12345);
    assert_eq!(row.get::<_, i32>("depth"), 8);
    assert_eq!(row.get::<_, i64>("turns"), 23456);
}

#[tokio::test]
async fn cheat_mode_nethack_runs_advance_the_cursor_without_attribution() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nh-cheat").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    // An explore-mode ascension: flagged non-scoring, must land nothing.
    svc.handle_nethack_frame(&StatsFrame {
        file: "xlogfile".to_string(),
        next_offset: 300,
        line: format!(
            "name={HANDLE}\tdeath=ascended\tpoints=999999\tmaxlvl=50\tachieve=0x1ff\tendtime=1754560000\tflags=0x2"
        ),
    })
    .await
    .expect("cheat ingest");

    assert_eq!(run_count(&test_db.db, user.id).await, 0);
    assert_eq!(
        cursor_for_game(&test_db.db, "nethack", "xlogfile").await,
        Some(300)
    );
    tokio::task::yield_now().await;
    assert_eq!(
        award_chip_total_for(&test_db.db, user.id, "nethack").await,
        0
    );
}

/// The xlogfile ascension line pays the ascension only. The Amulet pickup
/// has its own livelog line and its own payout; nothing here touches it.
#[tokio::test]
async fn a_nethack_ascension_grants_only_its_own_badge() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nh-win").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    svc.handle_nethack_frame(&StatsFrame {
        file: "xlogfile".to_string(),
        next_offset: 900,
        line: format!(
            "name={HANDLE}\tdeath=ascended\tpoints=3654321\tmaxlvl=53\tturns=81234\tachieve=0x1ff\tendtime=1754560000\tflags=0x0"
        ),
    })
    .await
    .expect("ascension ingest");

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total_for(&db, user.id, "nethack").await == 50_000 }
        },
        "ascension chips granted",
    )
    .await;

    // The badge insert trails the chip credit inside the grant task, so it
    // gets its own wait before the exact-rows assertion.
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "nethack_ascension").await == 1 }
        },
        "ascension badge granted",
    )
    .await;
    assert_eq!(badge_count(&test_db.db, user.id, "nethack_amulet").await, 0);

    let client = test_db.db.get().await.expect("db client");
    let categories: Vec<String> = client
        .query(
            "SELECT category FROM profile_awards
             WHERE user_id = $1 AND category IN ('nethack_amulet', 'nethack_ascension')
             ORDER BY category",
            &[&user.id],
        )
        .await
        .expect("award rows")
        .into_iter()
        .map(|row| row.get("category"))
        .collect();
    assert_eq!(categories, vec!["nethack_ascension"]);

    let row = client
        .query_one(
            "SELECT result FROM door_runs WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("run row");
    assert_eq!(row.get::<_, &str>("result"), "win");
}

#[tokio::test]
async fn amulet_livelog_milestone_pays_per_run_behind_the_lockout() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "nh-amulet").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    let amulet_frame = |offset: i64| StatsFrame {
        file: "livelog".to_string(),
        next_offset: offset,
        line: format!(
            "lltype=2\tname={HANDLE}\tturns=65432\tcurtime=1754540000\tmessage=acquired The Amulet of Yendor"
        ),
    };
    svc.handle_nethack_frame(&amulet_frame(300))
        .await
        .expect("amulet ingest");

    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total_for(&db, user.id, "nethack").await == 20_000 }
        },
        "amulet chips granted",
    )
    .await;

    // A second run inside the week pays nothing more; an untracked
    // achievement lands no milestone row at all.
    svc.handle_nethack_frame(&amulet_frame(600))
        .await
        .expect("second amulet");
    svc.handle_nethack_frame(&StatsFrame {
        file: "livelog".to_string(),
        next_offset: 700,
        line: format!("lltype=2\tname={HANDLE}\tcurtime=1754541000\tmessage=entered Gehennom"),
    })
    .await
    .expect("untracked achievement");
    tokio::task::yield_now().await;
    assert_eq!(
        award_chip_total_for(&test_db.db, user.id, "nethack").await,
        20_000
    );
    assert_eq!(
        cursor_for_game(&test_db.db, "nethack", "livelog").await,
        Some(700)
    );

    let client = test_db.db.get().await.expect("db client");
    let rows = client
        .query(
            "SELECT kind FROM door_milestones WHERE user_id = $1 ORDER BY source_offset",
            &[&user.id],
        )
        .await
        .expect("milestone rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<_, &str>("kind"), "amulet");

    // The badge insert trails the chip credit inside the grant task, so it
    // gets its own wait.
    drop(client);
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { badge_count(&db, user.id, "nethack_amulet").await == 1 }
        },
        "amulet badge granted",
    )
    .await;
}

fn brogue_file() -> String {
    format!("players/{HANDLE}/BrogueRunHistory.txt")
}

fn brogue_death_frame(offset: i64) -> StatsFrame {
    StatsFrame {
        file: brogue_file(),
        next_offset: offset,
        line: "8697033734589\t1754560000\tDied\tpink jelly\t1520\t1020\t0\t8\t2341".to_string(),
    }
}

#[tokio::test]
async fn replayed_brogue_run_lands_one_row() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "br-replay").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    let frame = brogue_death_frame(500);
    svc.handle_brogue_frame(&frame).await.expect("first ingest");
    svc.handle_brogue_frame(&frame).await.expect("replay");

    assert_eq!(run_count(&test_db.db, user.id).await, 1);
    assert_eq!(
        cursor_for_game(&test_db.db, "brogue", &brogue_file()).await,
        Some(500)
    );

    let client = test_db.db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT game, result, score, depth, turns, raw->>'killed_by' AS killed_by
             FROM door_runs WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("run row");
    assert_eq!(row.get::<_, &str>("game"), "brogue");
    assert_eq!(row.get::<_, &str>("result"), "death");
    assert_eq!(row.get::<_, i64>("score"), 1520);
    assert_eq!(row.get::<_, i32>("depth"), 8);
    assert_eq!(row.get::<_, i64>("turns"), 2341);
    assert_eq!(row.get::<_, &str>("killed_by"), "pink jelly");
}

#[tokio::test]
async fn brogue_endings_grant_only_their_own_badge() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "br-win").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    // An escape pays the 20k tier and nothing else (no back-grant: Brogue's
    // endings are alternatives, not stages).
    svc.handle_brogue_frame(&StatsFrame {
        file: brogue_file(),
        next_offset: 300,
        line: "1234\t1754560000\tEscaped\t-\t4870\t4870\t0\t26\t18023".to_string(),
    })
    .await
    .expect("escape ingest");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total_for(&db, user.id, "brogue").await == 20_000 }
        },
        "escape chips granted",
    )
    .await;

    // A later mastery pays its own 50k tier; replays pay nothing more.
    let mastery = StatsFrame {
        file: brogue_file(),
        next_offset: 600,
        line: "5678\t1754560000\tMastered\t-\t18420\t9420\t3\t40\t31007".to_string(),
    };
    svc.handle_brogue_frame(&mastery).await.expect("mastery");
    svc.handle_brogue_frame(&mastery).await.expect("replay");
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move { award_chip_total_for(&db, user.id, "brogue").await == 70_000 }
        },
        "mastery chips granted",
    )
    .await;

    // The badge inserts trail the chip credits inside the grant tasks, so
    // they get their own wait before the exact-rows assertion.
    let db = test_db.db.clone();
    wait_until(
        || {
            let db = db.clone();
            async move {
                badge_count(&db, user.id, "brogue_escape").await == 1
                    && badge_count(&db, user.id, "brogue_mastery").await == 1
            }
        },
        "both badges granted",
    )
    .await;

    let client = test_db.db.get().await.expect("db client");
    let categories: Vec<String> = client
        .query(
            "SELECT category FROM profile_awards
             WHERE user_id = $1 AND category IN ('brogue_escape', 'brogue_mastery')
             ORDER BY category",
            &[&user.id],
        )
        .await
        .expect("award rows")
        .into_iter()
        .map(|row| row.get("category"))
        .collect();
    assert_eq!(categories, vec!["brogue_escape", "brogue_mastery"]);

    let results: Vec<String> = client
        .query(
            "SELECT result FROM door_runs WHERE user_id = $1 ORDER BY source_offset",
            &[&user.id],
        )
        .await
        .expect("run rows")
        .into_iter()
        .map(|row| row.get("result"))
        .collect();
    assert_eq!(results, vec!["win", "mastery"]);
}

#[tokio::test]
async fn brogue_reset_markers_and_foreign_files_only_advance_the_cursor() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "br-reset").await;
    claim_handle(&test_db.db, user.id).await;
    let svc = ingest_service(&test_db.db);

    // The stats-reset marker: expected shape, nothing persisted.
    svc.handle_brogue_frame(&StatsFrame {
        file: brogue_file(),
        next_offset: 100,
        line: "0\t1754560000\tReset\t-\t0\t0\t0\t0\t0".to_string(),
    })
    .await
    .expect("reset ingest");
    // A file id outside the contract (the host never streams these).
    svc.handle_brogue_frame(&StatsFrame {
        file: "players/nobody/RapidBrogueRunHistory.txt".to_string(),
        next_offset: 40,
        line: "1\t1754560000\tDied\tjackal\t1\t1\t0\t2\t3".to_string(),
    })
    .await
    .expect("foreign file ingest");

    assert_eq!(run_count(&test_db.db, user.id).await, 0);
    assert_eq!(
        cursor_for_game(&test_db.db, "brogue", &brogue_file()).await,
        Some(100)
    );
    assert_eq!(
        cursor_for_game(
            &test_db.db,
            "brogue",
            "players/nobody/RapidBrogueRunHistory.txt"
        )
        .await,
        Some(40)
    );
}

#[tokio::test]
async fn unparseable_and_untracked_lines_only_advance_the_cursor() {
    let test_db = new_test_db().await;
    let _user = create_test_user(&test_db.db, "dcss-junk").await;
    let svc = ingest_service(&test_db.db);

    // Truncated logfile line.
    svc.handle_dcss_frame(&StatsFrame {
        file: "logfile".to_string(),
        next_offset: 50,
        line: "v=0.34.1:name=Wormsong:sc=1".to_string(),
    })
    .await
    .expect("junk line ingest");
    // Untracked milestone type.
    svc.handle_dcss_frame(&StatsFrame {
        file: "milestones".to_string(),
        next_offset: 70,
        line: format!("name={HANDLE}:time=20260006120000S:type=god.worship:milestone=prayed."),
    })
    .await
    .expect("untracked milestone ingest");

    assert_eq!(cursor_for(&test_db.db, "logfile").await, Some(50));
    assert_eq!(cursor_for(&test_db.db, "milestones").await, Some(70));
    let client = test_db.db.get().await.expect("db client");
    let milestones: i64 = client
        .query_one("SELECT COUNT(*)::bigint AS n FROM door_milestones", &[])
        .await
        .expect("count")
        .get("n");
    assert_eq!(milestones, 0);
}
