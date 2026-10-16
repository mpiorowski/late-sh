use chrono::{Duration, Utc};
use late_core::{
    models::{
        realm_game::RealmGame,
        user::{User, UserParams},
    },
    test_utils::{TestDb, create_test_user},
};
use uuid::Uuid;

use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::app::activity::lounge::{SYSTEM_FINGERPRINT, SYSTEM_USERNAME};
use crate::app::activity::publisher::ActivityPublisher;
use crate::app::door::realm::map::TerritoryId;
use crate::app::door::realm::resolver::{
    RealmAction, RealmGameState, RealmNewsKind, RealmPlayerStatus,
};
use crate::app::door::realm::svc::{
    NewRealm, RealmService, next_reset, parse_state, realm_day, reset_hour_label,
    reset_hour_label_at,
};
use crate::app::games::chips::svc::ChipService;
use crate::test_helpers::new_test_db;
use tokio::sync::broadcast;

/// The hour every test game refills at; 0 keeps realm days aligned with UTC
/// days so the arithmetic in the assertions stays obvious.
const RESET_HOUR: i16 = 0;

/// The usual game: standard rules at the slow tempo, default options.
fn spec(name: &str, hour: i16) -> NewRealm {
    NewRealm {
        name: name.to_string(),
        ruleset_id: "standard".to_string(),
        pace_id: "slow".to_string(),
        reset_hour_utc: hour,
        options: Default::default(),
        map_id: "earth".to_string(),
        map_spec: None,
    }
}

/// The house's own account, which the lounge feed ensures at startup and a
/// test has to make itself: the world's row is the system user's.
async fn house(test_db: &TestDb) -> User {
    let client = test_db.db.get().await.expect("db");
    match User::find_by_fingerprint(&client, SYSTEM_FINGERPRINT)
        .await
        .expect("find system user")
    {
        Some(user) => user,
        None => User::create(
            &client,
            UserParams {
                fingerprint: SYSTEM_FINGERPRINT.to_string(),
                username: SYSTEM_USERNAME.to_string(),
                settings: serde_json::json!({ "bot": true, "system": true }),
            },
        )
        .await
        .expect("create system user"),
    }
}

/// Open a world from `spec`, the way the house does: nobody on it yet.
async fn open(svc: &RealmService, test_db: &TestDb, spec: &NewRealm) -> RealmGame {
    house(test_db).await;
    let client = test_db.db.get().await.expect("db");
    svc.open_world(&client, spec)
        .await
        .expect("open world")
        .expect("no world was standing")
}

/// Open a world and march `user` straight in, which is what "creating a
/// game" amounts to now: the row is the house's, and the first player is a
/// joiner like every player after them.
async fn create_game(
    svc: &RealmService,
    test_db: &TestDb,
    user: &User,
    spec: &NewRealm,
) -> RealmGame {
    let game = open(svc, test_db, spec).await;
    svc.join_game(user.id, &user.username, game.id, None)
        .await
        .expect("join the new world");
    svc.load_game(game.id)
        .await
        .expect("load")
        .expect("the world is there")
}

/// The next activity event `matches`, or a panic after a few seconds.
async fn activity_where(
    activity: &mut broadcast::Receiver<ActivityEvent>,
    matches: impl Fn(&ActivityEvent) -> bool,
) -> ActivityEvent {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let event = activity.recv().await.expect("activity");
            if matches(&event) {
                return event;
            }
        }
    })
    .await
    .expect("the event was published")
}

fn realm_service(test_db: &TestDb) -> RealmService {
    realm_service_watching(test_db).0
}

/// The service plus the activity feed it publishes to, for the tests that
/// care what the rest of the place is told.
fn realm_service_watching(
    test_db: &TestDb,
) -> (
    RealmService,
    broadcast::Receiver<crate::app::activity::event::ActivityEvent>,
) {
    let (activity_tx, activity_rx) = broadcast::channel(64);
    let publisher = ActivityPublisher::new(test_db.db.clone(), activity_tx);
    (
        RealmService::new(
            test_db.db.clone(),
            ChipService::new(test_db.db.clone()),
            publisher,
            crate::app::ai::svc::AiService::new(false, None),
        ),
        activity_rx,
    )
}

/// Open a standard world and march the given players in; returns the game id.
async fn started_game(svc: &RealmService, test_db: &TestDb, names: &[&str]) -> (Uuid, Vec<Uuid>) {
    let mut ids = Vec::new();
    for name in names {
        ids.push(create_test_user(&test_db.db, name).await);
    }
    let game = open(svc, test_db, &spec("The Long War", RESET_HOUR)).await;
    for user in &ids {
        svc.join_game(user.id, &user.username, game.id, None)
            .await
            .expect("join game");
    }
    open_the_doors(test_db, game.id).await;
    (game.id, ids.iter().map(|u| u.id).collect())
}

/// Wind a world's opening muster into the past, so a test that is not about
/// the muster can act right away. The freeze is a wall-clock day long and
/// nobody should wait it out to assert something else.
async fn open_the_doors(test_db: &TestDb, game_id: Uuid) {
    let client = test_db.db.get().await.expect("db");
    client
        .execute(
            "UPDATE realm_games SET created = created - INTERVAL '2 days' WHERE id = $1",
            &[&game_id],
        )
        .await
        .expect("backdate the muster");
}

/// Claim a territory, however many rolls it takes. A claim is a dice roll, so
/// a test that needs the ground taken has to keep asking — asserting on one
/// attempt is asserting on the weather.
async fn claim_until_taken(svc: &RealmService, game_id: Uuid, user_id: Uuid, target: u16) {
    for _ in 0..40 {
        let state = load_state(svc, game_id).await;
        if state.ownership.get(&target) == Some(&user_id) {
            return;
        }
        if svc
            .act(user_id, game_id, RealmAction::Claim { target })
            .await
            .is_err()
        {
            // Out of points for the day, or the game ended under us; either
            // way the next loop's ownership check is the answer.
            let state = load_state(svc, game_id).await;
            if state.ownership.get(&target) == Some(&user_id) {
                return;
            }
            panic!("could not take {target}");
        }
    }
    panic!("{target} never fell");
}

/// The moves in a day's log, without the arrivals. Spawns are logged too —
/// the map's history cannot be replayed otherwise — but a test about acting
/// is not a test about who turned up.
fn moves(entries: &[crate::app::door::realm::resolver::LogEntry]) -> usize {
    use crate::app::door::realm::resolver::LogEntry;
    entries
        .iter()
        .filter(|entry| !matches!(entry, LogEntry::Spawned { .. } | LogEntry::Left { .. }))
        .count()
}

async fn load_state(svc: &RealmService, game_id: Uuid) -> RealmGameState {
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    parse_state(&row.state).expect("state")
}

/// The nearest free territory to this player — their ordinary next move. A
/// spawn can land on an island with no land border at all, so this falls back
/// to the closest free land by hops rather than assuming a neighbour.
fn nearest_free(state: &RealmGameState, user_id: Uuid) -> TerritoryId {
    let map = crate::app::door::realm::map::map_by_id(&state.ruleset.map_id).expect("map");
    let index = crate::app::door::realm::resolver::ReachIndex::build(
        state,
        &map,
        user_id,
        realm_day(Utc::now(), RESET_HOUR),
    );
    map.territories
        .iter()
        .filter(|t| !state.ownership.contains_key(&t.id))
        .min_by(|a, b| {
            let ca = index.reach(&map, &state.ruleset, a.id).cost;
            let cb = index.reach(&map, &state.ruleset, b.id).cost;
            ca.partial_cmp(&cb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.id.cmp(&b.id))
        })
        .map(|t| t.id)
        .expect("a fresh board has free land")
}

/// Chips credited to a user with the realm reason, polled briefly because the
/// payout is a spawned task.
async fn wait_for_payout(test_db: &TestDb, user_id: Uuid) -> Option<i64> {
    let client = test_db.db.get().await.expect("client");
    for _ in 0..60 {
        let rows = client
            .query(
                "SELECT delta FROM chip_ledger WHERE user_id = $1 AND reason = 'realm_conquest'",
                &[&user_id],
            )
            .await
            .expect("ledger");
        if let Some(row) = rows.first() {
            return Some(row.get::<_, i64>("delta"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    None
}

#[tokio::test]
async fn a_world_is_live_from_its_opening_and_takes_comers() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "realm_alice").await;
    let bob = create_test_user(&test_db.db, "realm_bob").await;
    let client = test_db.db.get().await.expect("client");

    // Unknown ruleset and an impossible hour are refused before anything
    // is written.
    house(&test_db).await;
    assert!(
        svc.open_world(
            &client,
            &NewRealm {
                ruleset_id: "nope".into(),
                ..spec("x", RESET_HOUR)
            }
        )
        .await
        .is_err()
    );
    assert!(svc.open_world(&client, &spec("x", 24)).await.is_err());

    let game = open(&svc, &test_db, &spec("  Spring   Offensive  ", 18)).await;
    // No waiting room and no starting gun: it is being played already, by
    // nobody yet. The row is the house's and the season is numbered.
    assert_eq!(game.status, RealmGame::STATUS_ACTIVE);
    assert_eq!(game.season, 1, "the first world the house opened");
    assert_eq!(game.reset_hour_utc, 18, "the chosen hour is persisted");
    assert_eq!(
        game.name, "Spring Offensive",
        "the name is tidied, not taken raw"
    );
    let state = parse_state(&game.state).expect("state");
    assert!(state.players.is_empty(), "nobody has marched in");
    assert_eq!(
        state.news.as_ref().map(|n| &n.kind),
        Some(&RealmNewsKind::Opened),
        "the opening is the first thing the strip can read"
    );

    // The first to arrive is a joiner like everyone after them.
    svc.join_game(alice.id, &alice.username, game.id, None)
        .await
        .expect("alice joins");
    let state = load_state(&svc, game.id).await;
    assert_eq!(state.territory_count(alice.id), 1, "she spawns on arrival");
    assert_eq!(
        state.points_left(alice.id, realm_day(Utc::now(), 18)),
        7,
        "alone on a whole world, on the most generous tier"
    );

    // Somebody turns up later and simply joins the war in progress.
    svc.join_game(bob.id, &bob.username, game.id, None)
        .await
        .expect("bob joins");
    assert!(
        svc.join_game(bob.id, &bob.username, game.id, None)
            .await
            .is_err()
    );
    let state = load_state(&svc, game.id).await;
    assert_eq!(state.players.len(), 2);
    assert_eq!(state.territory_count(bob.id), 1, "and spawns on arrival");
    assert_eq!(
        state.phase(bob.id, realm_day(Utc::now(), 18)),
        crate::app::door::realm::resolver::PlayerPhase::New,
        "with his own first days to find his feet"
    );
    // His arrival is the world's latest moment, colour and all, for the
    // strip to read off the row.
    assert_eq!(
        state.news.as_ref().map(|n| n.kind.clone()),
        Some(RealmNewsKind::Joined {
            user_id: bob.id,
            username: bob.username.clone(),
            color: state.player(bob.id).expect("bob").color.expect("a colour"),
        })
    );
}

/// The house keeps one world standing. A sweeper tick that finds none opens
/// the next season, numbered on from the last, on the standard rules at the
/// house pace, Earth and a drawn world taking turns; a tick that finds one
/// standing opens nothing; and the house hears about every opening.
#[tokio::test]
async fn the_house_raises_the_next_world_when_none_stands() {
    use crate::app::door::realm::map::GENERATED_MAP_ID;
    use crate::app::door::realm::svc::{HOUSE_RESET_HOUR_UTC, REALM_MUSTER};

    let test_db = new_test_db().await;
    let (svc, mut activity) = realm_service_watching(&test_db);
    house(&test_db).await;
    let client = test_db.db.get().await.expect("client");
    assert!(RealmGame::live(&client).await.expect("live").is_none());

    svc.sweep(Utc::now()).await.expect("sweep");
    let first = RealmGame::live(&client)
        .await
        .expect("live")
        .expect("the first world stands");
    assert_eq!(first.season, 1);
    assert_eq!(first.status, RealmGame::STATUS_ACTIVE);
    assert_eq!(first.reset_hour_utc, HOUSE_RESET_HOUR_UTC);
    let state = parse_state(&first.state).expect("state");
    assert_eq!(
        state.ruleset.map_id, "earth",
        "odd seasons are played on Earth"
    );
    assert_eq!(state.ruleset.pace_name, "Fast");
    assert!(state.players.is_empty());
    assert!(
        svc.act_at(
            Uuid::nil(),
            first.id,
            RealmAction::Claim { target: 0 },
            Utc::now()
        )
        .await
        .is_err(),
        "frozen while it musters"
    );
    assert!(
        crate::app::door::realm::svc::muster_left(first.created, Utc::now() + REALM_MUSTER / 2)
            .is_some(),
        "the muster is a day long"
    );

    // Standing worlds are left alone.
    svc.sweep(Utc::now()).await.expect("sweep");
    let standing: i64 = client
        .query_one(
            "SELECT COUNT(*)::bigint AS n FROM realm_games WHERE status IN ('open', 'active')",
            &[],
        )
        .await
        .expect("count")
        .get("n");
    assert_eq!(standing, 1, "one world at a time");

    // It falls; the next tick raises the next one, on the other kind of world.
    RealmGame::cancel(&client, first.id).await.expect("cancel");
    svc.sweep(Utc::now()).await.expect("sweep");
    let second = RealmGame::live(&client)
        .await
        .expect("live")
        .expect("the second world stands");
    assert_eq!(second.season, 2);
    let state = parse_state(&second.state).expect("state");
    assert_eq!(
        state.ruleset.map_id, GENERATED_MAP_ID,
        "even seasons are drawn"
    );
    assert!(state.ruleset.map_spec.is_some());

    for (game_id, season) in [(first.id, 1), (second.id, 2)] {
        let opened = activity_where(&mut activity, |event| {
            matches!(event.kind, ActivityKind::RealmSeasonOpened { game_id: g, .. } if g == game_id)
        })
        .await;
        assert_eq!(
            opened.kind,
            ActivityKind::RealmSeasonOpened { game_id, season }
        );
    }
}

/// Two replicas' sweepers both find no world and both open one: the second
/// insert is refused by the table, not by luck, and reads as "somebody got
/// there first" rather than an error.
#[tokio::test]
async fn only_one_world_stands_at_a_time() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    house(&test_db).await;
    let client = test_db.db.get().await.expect("client");
    let first = svc
        .open_world(&client, &spec("First", RESET_HOUR))
        .await
        .expect("open");
    assert!(first.is_some());
    let second = svc
        .open_world(&client, &spec("Second", RESET_HOUR))
        .await
        .expect("the second open is not an error");
    assert!(second.is_none(), "but it opens nothing");
}

#[tokio::test]
async fn the_doors_close_once_the_map_is_half_carved_up() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, _) = started_game(&svc, &test_db, &["realm_door_a", "realm_door_b"]).await;
    let latecomer = create_test_user(&test_db.db, "realm_door_c").await;
    let client = test_db.db.get().await.expect("client");

    let mut state = load_state(&svc, game_id).await;
    let map = crate::app::door::realm::map::map_by_id(&state.ruleset.map_id).expect("map");
    assert!(state.joinable(&map), "a fresh realm takes comers");

    // Carve the world up past the threshold.
    let expected = state.revision as i64;
    let limit = (map.territories.len() as f64 * state.ruleset.join_max_claimed).ceil() as u16;
    let holder = state.players[0].user_id;
    for t in 0..limit {
        state.ownership.insert(t, holder);
    }
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    let err = svc
        .join_game(latecomer.id, &latecomer.username, game_id, None)
        .await
        .expect_err("too late");
    assert!(err.to_string().contains("too late to join"));
}

#[tokio::test]
async fn a_spectator_can_look_but_never_act() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_spec_a", "realm_spec_b"]).await;
    let onlooker = create_test_user(&test_db.db, "realm_spec_c").await;

    // Everything a board needs to draw is readable by anyone: the row, the
    // state, the day logs. Watching is not a privilege.
    let state = load_state(&svc, game_id).await;
    assert!(state.player(onlooker.id).is_none(), "not in the roster");
    assert!(state.ownership.len() >= 2, "there is a world to look at");
    svc.load_day_logs(game_id, None, 5)
        .await
        .expect("logs read");

    // But every way of touching it is refused, whether the realm is still
    // open to newcomers or not.
    let target = nearest_free(&state, ids[0]);
    let err = svc
        .act(onlooker.id, game_id, RealmAction::Claim { target })
        .await
        .expect_err("a spectator cannot claim");
    assert!(err.to_string().contains("not playing"), "{err}");
    let err = svc
        .act(
            onlooker.id,
            game_id,
            RealmAction::Attack {
                target: state.holdings(ids[1])[0],
            },
        )
        .await
        .expect_err("a spectator cannot attack");
    assert!(err.to_string().contains("not playing"), "{err}");
    assert!(
        svc.leave_game(onlooker.id, game_id).await.is_err(),
        "and cannot leave a game they were never in"
    );

    // Nothing they did left a mark.
    let after = load_state(&svc, game_id).await;
    assert_eq!(after.revision, state.revision, "the world did not move");
    assert_eq!(after.players.len(), 2);

    // Once the doors shut, the same holds: still readable, still refused.
    let client = test_db.db.get().await.expect("client");
    let mut locked = load_state(&svc, game_id).await;
    let expected = locked.revision as i64;
    let map = crate::app::door::realm::map::map_by_id(&locked.ruleset.map_id).expect("map");
    let limit = (map.territories.len() as f64 * locked.ruleset.join_max_claimed).ceil() as u16;
    for t in 0..limit {
        locked.ownership.insert(t, ids[0]);
    }
    locked.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&locked).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    let locked = load_state(&svc, game_id).await;
    assert!(!locked.joinable(&map), "the realm is closed now");
    assert!(
        svc.join_game(onlooker.id, &onlooker.username, game_id, None)
            .await
            .is_err(),
        "closed means closed"
    );
    let err = svc
        .act(
            onlooker.id,
            game_id,
            RealmAction::Claim {
                target: nearest_free(&locked, ids[0]),
            },
        )
        .await
        .expect_err("still a spectator");
    assert!(err.to_string().contains("not playing"), "{err}");
}

#[tokio::test]
async fn a_conquered_player_keeps_reading_and_stops_acting() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_gone_a", "realm_gone_b"]).await;
    let (alice, bob) = (ids[0], ids[1]);
    let client = test_db.db.get().await.expect("client");

    // Bob is conquered: still on the roster, no longer in the war.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    for player in &mut state.players {
        if player.user_id == bob {
            player.status = RealmPlayerStatus::Eliminated;
            player.exit_day = Some(realm_day(Utc::now(), RESET_HOUR));
        }
    }
    state.ownership.retain(|_, owner| *owner == alice);
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    let state = load_state(&svc, game_id).await;
    assert!(
        state.player(bob).is_some(),
        "his history stays in the roster"
    );
    let err = svc
        .act(
            bob,
            game_id,
            RealmAction::Claim {
                target: nearest_free(&state, alice),
            },
        )
        .await
        .expect_err("the fallen do not act");
    assert!(err.to_string().contains("not playing"), "{err}");
}

#[tokio::test]
async fn the_final_table_is_what_the_pot_actually_paid() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(
        &svc,
        &test_db,
        &["realm_tbl_a", "realm_tbl_b", "realm_tbl_c"],
    )
    .await;
    let (alice, bob, carol) = (ids[0], ids[1], ids[2]);
    let client = test_db.db.get().await.expect("client");

    // A played-out war: alice wins it, bob was conquered having played, and
    // carol barely turned up.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    let bar = state.ruleset.min_active_days_to_win;
    let today = realm_day(Utc::now(), RESET_HOUR);
    for player in &mut state.players {
        if player.user_id == alice {
            player.active_days = bar;
        } else {
            player.status = RealmPlayerStatus::Eliminated;
            player.exit_day = Some(today);
            player.exit_territories = 4;
            player.active_days = if player.user_id == bob {
                state.ruleset.min_payout_active_days + 2
            } else {
                0
            };
        }
    }
    // Hers except one: a realm ends when a player holds the *whole* map, so
    // the position under test is one territory short of that.
    let map =
        crate::app::door::realm::map::map_handle(&state.ruleset.map_id, None).expect("the map");
    let last = map.territories.len() as u16 - 1;
    state.ownership.clear();
    for territory in 0..last {
        state.ownership.insert(territory, alice);
    }
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    let state = load_state(&svc, game_id).await;
    let table = crate::app::door::realm::svc::final_table(&state, game_id, Some(alice));
    assert_eq!(table.len(), 3, "everyone is on the board, paid or not");
    assert_eq!(table[0].user_id, alice);
    assert_eq!(table[0].place, Some(1));
    assert!(table[0].chips > 0);
    // Carol played nothing, so she takes no place and the chips do not slide
    // to her — the table says why.
    let carol_row = table.iter().find(|p| p.user_id == carol).expect("carol");
    assert_eq!(carol_row.place, None);
    assert_eq!(carol_row.chips, 0);
    assert_eq!(carol_row.note, "too few days played");

    // Now finish it for real — the last territory on the map — and check the
    // ledger agrees with the table.
    // One action is all it takes: with the others conquered, the realm ends
    // whether or not this particular roll lands.
    svc.act(alice, game_id, RealmAction::Claim { target: last })
        .await
        .expect("act");
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row.status, RealmGame::STATUS_FINISHED);
    for place in &table {
        let paid = wait_for_payout(&test_db, place.user_id).await;
        if place.chips > 0 {
            assert_eq!(
                paid,
                Some(place.chips),
                "{} was shown {} and paid {paid:?}",
                place.username,
                place.chips
            );
        } else {
            assert_eq!(paid, None, "{} was shown nothing", place.username);
        }
    }
}

#[tokio::test]
async fn a_finished_realm_lingers_a_day_and_then_leaves_the_lobby() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, _) = started_game(&svc, &test_db, &["realm_gone_x", "realm_gone_y"]).await;
    let client = test_db.db.get().await.expect("client");

    client
        .execute(
            "UPDATE realm_games SET status = 'finished', updated = current_timestamp
             WHERE id = $1",
            &[&game_id],
        )
        .await
        .expect("finish it");
    let listed =
        RealmGame::list_finished_recent(&client, crate::app::door::realm::svc::REALM_RESULTS_HOURS)
            .await
            .expect("list");
    assert!(
        listed.iter().any(|g| g.id == game_id),
        "a result you can still read"
    );

    // A day older, and the Lobby is about games being played again.
    client
        .execute(
            "UPDATE realm_games
             SET updated = current_timestamp - make_interval(hours => $2::int)
             WHERE id = $1",
            &[
                &game_id,
                &(crate::app::door::realm::svc::REALM_RESULTS_HOURS as i32 + 1),
            ],
        )
        .await
        .expect("age it");
    let listed =
        RealmGame::list_finished_recent(&client, crate::app::door::realm::svc::REALM_RESULTS_HOURS)
            .await
            .expect("list");
    assert!(!listed.iter().any(|g| g.id == game_id));
    // The row itself is still there: the history and its logs outlive the
    // listing.
    assert!(svc.load_game(game_id).await.expect("load").is_some());
}

#[tokio::test]
async fn an_action_resolves_immediately_and_is_logged() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_now_a", "realm_now_b"]).await;
    let alice = ids[0];
    let today = realm_day(Utc::now(), RESET_HOUR);

    let state = load_state(&svc, game_id).await;
    let cap = state.ruleset.actions_per_day(2);
    let target = nearest_free(&state, alice);

    svc.act(alice, game_id, RealmAction::Claim { target })
        .await
        .expect("a legal claim resolves");

    let state = load_state(&svc, game_id).await;
    // The point is gone whether or not the roll landed, and the attempt is in
    // today's log the moment it happened — no midnight involved.
    assert_eq!(state.points_left(alice, today), cap - 1);
    let day = state.last_day.as_ref().expect("a live day log");
    assert_eq!(day.day, today);
    assert_eq!(moves(&day.entries), 1);

    // The archive has it too, so a reconnecting session sees the same history.
    let archived = svc
        .load_day_logs(game_id, None, 5)
        .await
        .expect("load logs");
    assert_eq!(archived.first().map(|d| moves(&d.result.entries)), Some(1));
}

#[tokio::test]
async fn the_daily_budget_runs_out_and_refills_on_the_games_own_hour() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_pts_a", "realm_pts_b"]).await;
    let alice = ids[0];
    let today = realm_day(Utc::now(), RESET_HOUR);
    let cap = load_state(&svc, game_id).await.ruleset.actions_per_day(2);

    for _ in 0..cap {
        let state = load_state(&svc, game_id).await;
        let target = nearest_free(&state, alice);
        svc.act(alice, game_id, RealmAction::Claim { target })
            .await
            .expect("point spent");
    }
    let state = load_state(&svc, game_id).await;
    assert_eq!(state.points_left(alice, today), 0);

    let target = nearest_free(&state, alice);
    let err = svc
        .act(alice, game_id, RealmAction::Claim { target })
        .await
        .expect_err("out of points");
    assert!(err.to_string().contains("no action points left"));

    // Tomorrow the same stored state hands the budget back — nothing had to
    // run at the turn of the day.
    assert_eq!(state.points_left(alice, today + 1), cap);
}

#[tokio::test]
async fn acting_on_a_territory_someone_just_took_is_refused_not_wasted() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_race_a", "realm_race_b"]).await;
    let (alice, bob) = (ids[0], ids[1]);
    let today = realm_day(Utc::now(), RESET_HOUR);

    // Hand alice a territory outright, then have bob try to claim it as if
    // his screen were a second out of date.
    let state = load_state(&svc, game_id).await;
    let contested = nearest_free(&state, alice);
    svc.act(alice, game_id, RealmAction::Claim { target: contested })
        .await
        .expect("alice acts first");

    let after = load_state(&svc, game_id).await;
    let bob_points_before = after.points_left(bob, today);
    if after.ownership.get(&contested) == Some(&alice) {
        let err = svc
            .act(bob, game_id, RealmAction::Claim { target: contested })
            .await
            .expect_err("the ground moved under bob");
        assert!(err.to_string().contains("claimed that territory first"));
        let after = load_state(&svc, game_id).await;
        assert_eq!(
            after.points_left(bob, today),
            bob_points_before,
            "a refused action costs nothing"
        );
    }
}

#[tokio::test]
async fn a_distant_strike_is_allowed_at_long_odds() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_far_a", "realm_far_b"]).await;
    let alice = ids[0];
    let state = load_state(&svc, game_id).await;
    let map = crate::app::door::realm::map::map_by_id(&state.ruleset.map_id).expect("map");

    // Something far away and unowned: legal to go for, just very unlikely.
    // Far, not unreachable — a target on another landmass with no way to it
    // costs infinity, which passes a "cost >= 4" filter and is a different
    // test (it is refused outright, see `ActionRejected::NoRoute`). Whether
    // the first match was one of those depended on where the spawn landed,
    // which made this flaky rather than wrong.
    let index = crate::app::door::realm::resolver::ReachIndex::build(
        &state,
        &map,
        alice,
        realm_day(Utc::now(), RESET_HOUR),
    );
    let far = map
        .territories
        .iter()
        .filter(|t| !state.ownership.contains_key(&t.id))
        .find(|t| {
            let reach = index.reach(&map, &state.ruleset, t.id);
            reach.cost >= 4.0 && !reach.is_unreachable()
        })
        .expect("earth has somewhere far away");

    let reach = crate::app::door::realm::resolver::reach_for(
        &state,
        &map,
        alice,
        far.id,
        realm_day(Utc::now(), RESET_HOUR),
    );
    assert!(reach.cost >= 4.0);
    let odds = crate::app::door::realm::resolver::projected_probability(
        &state,
        &map,
        alice,
        far.id,
        reach,
        realm_day(Utc::now(), RESET_HOUR),
    )
    .expect("a price for the gamble");
    assert!(odds >= state.ruleset.far_prob_floor && odds < 0.2);

    svc.act(alice, game_id, RealmAction::Claim { target: far.id })
        .await
        .expect("the long shot is a legal action");
    let after = load_state(&svc, game_id).await;
    assert_eq!(moves(&after.last_day.as_ref().expect("log").entries), 1);
}

#[tokio::test]
async fn the_sweeper_rolls_the_day_over_and_archives_it() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_roll_a", "realm_roll_b"]).await;
    let alice = ids[0];

    let state = load_state(&svc, game_id).await;
    let target = nearest_free(&state, alice);
    svc.act(alice, game_id, RealmAction::Claim { target })
        .await
        .expect("an action on day one");
    let day_one = realm_day(Utc::now(), RESET_HOUR);

    // Tomorrow, from the sweeper's point of view.
    svc.sweep(Utc::now() + Duration::days(1))
        .await
        .expect("sweep");

    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row.last_resolved_day, day_one + 1);
    let state = parse_state(&row.state).expect("state");
    assert_eq!(
        state.last_day.as_ref().map(|d| d.day),
        Some(day_one + 1),
        "the live log is the new day"
    );
    assert!(state.last_day.as_ref().unwrap().entries.is_empty());

    // Day one survived in the archive with its entry.
    let logs = svc.load_day_logs(game_id, None, 10).await.expect("logs");
    let archived_one = logs
        .iter()
        .find(|d| d.day() == day_one)
        .expect("day one archived");
    assert_eq!(moves(&archived_one.result.entries), 1);

    // Sweeping the same day twice changes nothing further.
    svc.sweep(Utc::now() + Duration::days(1))
        .await
        .expect("idempotent sweep");
    let row_again = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row_again.last_resolved_day, day_one + 1);
}

#[tokio::test]
async fn a_game_nobody_plays_dissolves_without_paying() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_idle_a", "realm_idle_b"]).await;
    let abandon_days = i64::from(crate::app::door::realm::resolver::REALM_ABANDON_DAYS);

    // A quiet spell is not an exit: everyone is still in and still holding.
    svc.sweep(Utc::now() + Duration::days(abandon_days - 1))
        .await
        .expect("sweep");
    let state = load_state(&svc, game_id).await;
    assert!(
        state
            .players
            .iter()
            .all(|p| p.status == RealmPlayerStatus::Alive)
    );
    assert_eq!(state.ownership.len(), 2, "quiet players keep their land");

    // Long enough and the row is cleared away, with no winner and no chips.
    svc.sweep(Utc::now() + Duration::days(abandon_days + 1))
        .await
        .expect("sweep");
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row.status, RealmGame::STATUS_FINISHED);
    assert_eq!(row.winner_user_id, None);
    for id in ids {
        assert_eq!(wait_for_payout(&test_db, id).await, None);
    }
    assert_eq!(
        parse_state(&row.state).expect("state").news.map(|n| n.kind),
        Some(RealmNewsKind::Ended {
            winner_user_id: None,
            winner_username: None,
        }),
        "the strip reads a world that fell silent"
    );
}

#[tokio::test]
async fn you_can_withdraw_on_your_first_day_and_not_after() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_w_a", "realm_w_b"]).await;
    let (alice, bob) = (ids[0], ids[1]);
    let client = test_db.db.get().await.expect("client");

    // Fresh arrival, untouchable and uncommitted: he can still walk away.
    svc.leave_game(bob, game_id).await.expect("bob withdraws");
    let state = load_state(&svc, game_id).await;
    assert!(!state.players.iter().any(|p| p.user_id == bob));
    assert_eq!(state.territory_count(bob), 0, "his spawn goes back");

    // Alice has been here a while — she is committed.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    for player in &mut state.players {
        if player.user_id == alice {
            player.joined_day -= 30;
        }
    }
    state.players.push(
        serde_json::from_value(serde_json::json!({
            "user_id": bob,
            "username": "b",
            "status": "alive",
            "joined_day": state.start_day - 30,
            "exit_day": null,
            "exit_territories": 0,
            "active_days": 4
        }))
        .expect("player"),
    );
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    let err = svc
        .leave_game(alice, game_id)
        .await
        .expect_err("quitting a war you are in is refused");
    assert!(err.to_string().contains("first day"));
}

#[tokio::test]
async fn the_last_player_can_pack_the_realm_up() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "realm_solo").await;
    let game = create_game(&svc, &test_db, &alice, &spec("", RESET_HOUR)).await;

    // Nobody else ever turned up, so there is nobody to hand it to.
    svc.leave_game(alice.id, game.id)
        .await
        .expect("alice packs up");
    let row = svc.load_game(game.id).await.expect("load").expect("row");
    assert_eq!(row.status, RealmGame::STATUS_CANCELLED);
}

#[tokio::test]
async fn a_conquest_after_a_week_of_play_pays_the_pot() {
    let test_db = new_test_db().await;
    let (svc, mut activity) = realm_service_watching(&test_db);
    let (game_id, ids) =
        started_game(&svc, &test_db, &["realm_p_a", "realm_p_b", "realm_p_c"]).await;
    let (alice, bob, carol) = (ids[0], ids[1], ids[2]);
    let client = test_db.db.get().await.expect("client");

    // Stand the game where a real one would be after a week: alice has
    // played her days and conquered the other two.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    let bar = state.ruleset.min_active_days_to_win;
    for player in &mut state.players {
        if player.user_id == alice {
            player.active_days = bar;
        } else {
            // Conquered, having played the week, so the pot is sized on
            // all three of them.
            player.status = RealmPlayerStatus::Eliminated;
            player.exit_day = Some(realm_day(Utc::now(), RESET_HOUR));
            player.active_days = bar;
        }
    }
    // Hers except one: a realm ends when a player holds the *whole* map, so
    // the position under test is one territory short of that.
    let map =
        crate::app::door::realm::map::map_handle(&state.ruleset.map_id, None).expect("the map");
    let last = map.territories.len() as u16 - 1;
    state.ownership.clear();
    for territory in 0..last {
        state.ownership.insert(territory, alice);
    }
    state.revision += 1;
    let updated = RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");
    assert_eq!(updated, 1);

    // The last territory on the map closes it out: the world is hers and the
    // week is in.
    let state = load_state(&svc, game_id).await;
    assert_eq!(state.ownership.len(), map.territories.len() - 1);
    // One action is all it takes: with the others conquered, the realm ends
    // whether or not this particular roll lands.
    svc.act(alice, game_id, RealmAction::Claim { target: last })
        .await
        .expect("act");

    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row.status, RealmGame::STATUS_FINISHED);
    assert_eq!(row.winner_user_id, Some(alice));

    // Three players at start: the whole pot, winner takes all.
    let pot: i64 = crate::app::door::realm::svc::payout_plan(3, state.ruleset.pot_scale)
        .iter()
        .map(|(_, chips)| chips)
        .sum();
    assert_eq!(wait_for_payout(&test_db, alice).await, Some(pot));
    assert!(pot >= 9_000, "a three-player pot should be worth winning");
    // The three-player tier pays one place: the runners-up grew the pot and
    // take nothing from it.
    assert_eq!(wait_for_payout(&test_db, bob).await, None);
    assert_eq!(wait_for_payout(&test_db, carol).await, None);

    // The house hears the world fell: who took it and what it paid, and
    // the row's last moment is the end, for the strip.
    let fell = activity_where(&mut activity, |event| {
        matches!(event.kind, ActivityKind::RealmSeasonEnded { game_id: g, .. } if g == game_id)
    })
    .await;
    assert_eq!(
        fell.kind,
        ActivityKind::RealmSeasonEnded {
            game_id,
            season: row.season,
            winner: Some("realm_p_a".to_string()),
            pot,
            players: 3,
        }
    );
    let state = load_state(&svc, game_id).await;
    assert_eq!(
        state.news.map(|n| n.kind),
        Some(RealmNewsKind::Ended {
            winner_user_id: Some(alice),
            winner_username: Some("realm_p_a".to_string()),
        })
    );
}

#[tokio::test]
async fn conquering_everyone_early_does_not_end_the_game() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_q_a", "realm_q_b"]).await;
    let (alice, bob) = (ids[0], ids[1]);
    let client = test_db.db.get().await.expect("client");

    // Alice has eliminated bob on day two — the shape a collusion farm wants.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    for player in &mut state.players {
        if player.user_id == bob {
            player.status = RealmPlayerStatus::Eliminated;
        } else {
            player.active_days = 2;
        }
    }
    state.ownership.retain(|_, owner| *owner == alice);
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    let state = load_state(&svc, game_id).await;
    let target = nearest_free(&state, alice);
    svc.act(alice, game_id, RealmAction::Claim { target })
        .await
        .expect("act");

    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(
        row.status,
        RealmGame::STATUS_ACTIVE,
        "holding the field is not winning until the week is in"
    );
    assert_eq!(wait_for_payout(&test_db, alice).await, None);
}

#[test]
fn realm_days_follow_the_games_own_reset_hour() {
    use chrono::{TimeZone, Utc};
    // A game refilling at 18:00 UTC: 17:59 is still the previous realm day.
    let before = Utc.timestamp_opt(86_400 * 100 + 17 * 3_600, 0).unwrap();
    let after = Utc.timestamp_opt(86_400 * 100 + 18 * 3_600, 0).unwrap();
    assert_eq!(realm_day(before, 18), realm_day(after, 18) - 1);
    // At hour 0 a realm day is just the UTC day.
    assert_eq!(realm_day(after, 0), 100);
    // The next refill is the coming 18:00 either way.
    assert_eq!(next_reset(before, 18), after);
    assert_eq!(
        next_reset(after, 18),
        after + chrono::Duration::days(1),
        "on the hour itself the next one is tomorrow"
    );
}

#[test]
fn the_reset_hour_reads_in_both_clocks() {
    let utc_only = reset_hour_label(18, None);
    assert_eq!(utc_only, "18:00 UTC");
    let warsaw: chrono_tz::Tz = "Europe/Warsaw".parse().unwrap();
    let both = reset_hour_label(18, Some(warsaw));
    assert!(both.starts_with("18:00 UTC · "), "{both}");
    assert!(both.contains(" local"), "{both}");
}

/// Warsaw is UTC+1 in winter and UTC+2 in summer. A label sampled on a fixed
/// instant gets one of those wrong for half the year, which is the one error
/// this label exists to stop somebody making by hand.
#[test]
fn the_local_half_follows_summer_time() {
    let warsaw: chrono_tz::Tz = "Europe/Warsaw".parse().unwrap();
    let winter = chrono::DateTime::parse_from_rfc3339("2026-01-10T09:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let summer = chrono::DateTime::parse_from_rfc3339("2026-07-10T09:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(
        reset_hour_label_at(18, Some(warsaw), winter),
        "18:00 UTC · 19:00 local"
    );
    assert_eq!(
        reset_hour_label_at(18, Some(warsaw), summer),
        "18:00 UTC · 20:00 local"
    );
}

/// An hour near the ends of the UTC day is a different date for most of the
/// world, and which day your points come back on is half of what a creator
/// is choosing.
#[test]
fn a_reset_that_lands_on_another_date_says_so() {
    let now = chrono::DateTime::parse_from_rfc3339("2026-07-10T09:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let auckland: chrono_tz::Tz = "Pacific/Auckland".parse().unwrap();
    assert_eq!(
        reset_hour_label_at(23, Some(auckland), now),
        "23:00 UTC · 11:00 local next day"
    );
    let los_angeles: chrono_tz::Tz = "America/Los_Angeles".parse().unwrap();
    assert_eq!(
        reset_hour_label_at(2, Some(los_angeles), now),
        "02:00 UTC · 19:00 local prev day"
    );
}

/// Colour is identity on a realm map: two players in the same one would make
/// the board unreadable for everybody, including them. So a pick somebody
/// already has is refused outright rather than quietly swapped — arriving as
/// a colour you did not choose is its own kind of wrong.
#[tokio::test]
async fn two_players_cannot_wear_the_same_colour() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "alice").await;
    let bob = create_test_user(&test_db.db, "bob").await;
    let carol = create_test_user(&test_db.db, "carol").await;

    let game = open(&svc, &test_db, &spec("Colours", RESET_HOUR)).await;
    svc.join_game(alice.id, &alice.username, game.id, Some(4))
        .await
        .expect("alice joins in her colour");
    let state = load_state(&svc, game.id).await;
    assert_eq!(
        state.player(alice.id).expect("alice").color,
        Some(4),
        "she plays in what she picked"
    );

    assert!(
        svc.join_game(bob.id, &bob.username, game.id, Some(4))
            .await
            .is_err(),
        "the colour is spoken for"
    );
    svc.join_game(bob.id, &bob.username, game.id, Some(7))
        .await
        .expect("a free colour is fine");
    // No pick at all takes the first colour going, which is never one on the
    // board already.
    svc.join_game(carol.id, &carol.username, game.id, None)
        .await
        .expect("join");

    let state = load_state(&svc, game.id).await;
    let mut worn: Vec<u8> = state.taken_colors();
    let count = worn.len();
    worn.sort_unstable();
    worn.dedup();
    assert_eq!(worn.len(), count, "no two players share a colour");
    assert_eq!(state.player(bob.id).expect("bob").color, Some(7));
}

/// The map is the opener's choice, frozen into the game like every other
/// rule: not something the ruleset carries, and not something a later
/// roster change can move under a game being played.
#[tokio::test]
async fn the_world_is_picked_at_the_opening_and_frozen_with_the_rules() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let client = test_db.db.get().await.expect("client");

    house(&test_db).await;
    assert!(
        svc.open_world(
            &client,
            &NewRealm {
                map_id: "atlantis".to_string(),
                ..spec("Nowhere", RESET_HOUR)
            }
        )
        .await
        .is_err(),
        "a world that does not exist is not a game"
    );

    let game = open(&svc, &test_db, &spec("Earth", RESET_HOUR)).await;
    let state = load_state(&svc, game.id).await;
    assert_eq!(state.ruleset.map_id, "earth");
}

/// A generated world is stored as the numbers it was drawn from, not as a
/// megabyte of cells. What has to hold is that those numbers come back as the
/// same world: the seed is the server's, the names are settled once, and the
/// board the game is played on is rebuilt from the snapshot every time.
#[tokio::test]
async fn a_generated_world_is_frozen_as_the_recipe_that_draws_it() {
    use crate::app::door::realm::map::{GENERATED_MAP_ID, map_handle};
    use crate::app::door::realm::mapgen::GeneratedMapSpec;

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "alice").await;

    let game = create_game(
        &svc,
        &test_db,
        &alice,
        &crate::app::door::realm::svc::NewRealm {
            map_id: GENERATED_MAP_ID.to_string(),
            map_spec: Some(GeneratedMapSpec {
                // A seed the creator would like: it must not be the one
                // used, or a creator could shop for a favourable world by
                // making games until they liked their own corner.
                seed: 1,
                continents: 3,
                islands: 4,
                territories: 60,
                names: Vec::new(),
            }),
            ..spec("Uncharted", RESET_HOUR)
        },
    )
    .await;

    let state = load_state(&svc, game.id).await;
    assert_eq!(state.ruleset.map_id, GENERATED_MAP_ID);
    let stored = state.ruleset.map_spec.clone().expect("a spec is frozen");
    assert_ne!(stored.seed, 1, "the server picks the seed, not the client");
    assert_eq!((stored.continents, stored.islands), (3, 4));
    assert_eq!(stored.territories, 60);
    assert_eq!(
        stored.names.len(),
        60,
        "names are settled at creation, not re-invented per rebuild"
    );

    // The board is rebuilt from the snapshot, and it is the same board twice.
    let map = map_handle(&state.ruleset.map_id, Some(&stored)).expect("map");
    assert_eq!(map.territories.len(), 60);
    let again = map_handle(&state.ruleset.map_id, Some(&stored)).expect("map");
    assert_eq!(map.territories[0].name, again.territories[0].name);
    assert_eq!(map.grid.cells, again.grid.cells);

    // And the creator is standing on it.
    assert_eq!(state.ownership.len(), 1);
    let (territory, owner) = state.ownership.iter().next().expect("a spawn");
    assert_eq!(*owner, alice.id);
    assert!(map.territory(*territory).is_some(), "spawned off the map");
}

/// The shape a creator dials in is not trusted: the service clamps it the
/// same way the generator does, so an out-of-range client cannot ask for a
/// world that cannot be built.
#[tokio::test]
async fn a_generated_world_clamps_the_shape_it_was_asked_for() {
    use crate::app::door::realm::map::GENERATED_MAP_ID;
    use crate::app::door::realm::mapgen::{
        GEN_MAX_CONTINENTS, GEN_MAX_TERRITORIES, GeneratedMapSpec,
    };

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "alice").await;
    let game = create_game(
        &svc,
        &test_db,
        &alice,
        &crate::app::door::realm::svc::NewRealm {
            map_id: GENERATED_MAP_ID.to_string(),
            map_spec: Some(GeneratedMapSpec {
                seed: 0,
                continents: 250,
                islands: 0,
                territories: 9_000,
                names: Vec::new(),
            }),
            ..spec("Too much world", RESET_HOUR)
        },
    )
    .await;
    let stored = load_state(&svc, game.id)
        .await
        .ruleset
        .map_spec
        .expect("a spec is frozen");
    assert_eq!(stored.continents, GEN_MAX_CONTINENTS);
    assert_eq!(stored.territories, GEN_MAX_TERRITORIES);
}

/// Two realms made from the same request are still two different worlds —
/// the seed is drawn per game, so "another go at this shape" means another
/// map rather than the same one again.
#[tokio::test]
async fn two_generated_realms_are_two_different_worlds() {
    use crate::app::door::realm::map::GENERATED_MAP_ID;
    use crate::app::door::realm::mapgen::GeneratedMapSpec;

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "alice").await;
    let ask = |name: &'static str| crate::app::door::realm::svc::NewRealm {
        map_id: GENERATED_MAP_ID.to_string(),
        map_spec: Some(GeneratedMapSpec {
            seed: 0,
            continents: 2,
            islands: 2,
            territories: 50,
            names: Vec::new(),
        }),
        ..spec(name, RESET_HOUR)
    };
    let one = create_game(&svc, &test_db, &alice, &ask("First")).await;
    // One world at a time: the first has to end before the next is drawn.
    let client = test_db.db.get().await.expect("client");
    RealmGame::cancel(&client, one.id).await.expect("cancel");
    let two = create_game(&svc, &test_db, &alice, &ask("Second")).await;
    let a = load_state(&svc, one.id).await.ruleset.map_spec.unwrap();
    let b = load_state(&svc, two.id).await.ruleset.map_spec.unwrap();
    assert_ne!(a.seed, b.seed);
    assert_ne!(a.key(), b.key());
}

/// How carved up the world was when somebody joined is a fact about their
/// arrival, so it is recorded at the join and never recomputed — it is what
/// their new-arrival shield is measured from, and a number that kept moving
/// would keep moving their protection with it.
#[tokio::test]
async fn joining_records_how_much_of_the_world_was_gone() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["alice", "bob"]).await;
    let state = load_state(&svc, game_id).await;

    // Both arrived to an all-but-empty world: two spawns out of 242.
    for id in &ids {
        let player = state.player(*id).expect("player");
        assert_eq!(player.joined_claimed, 0, "nobody was here yet");
        assert_eq!(
            state.shield_days(*id),
            i32::from(state.ruleset.attack_grace_days),
            "an early arrival gets the plain opening truce"
        );
    }

    // Hand most of the map to alice, then let a third player in: they arrive
    // somewhere quite different, and the record says so.
    let map = crate::app::door::realm::map::map_handle(&state.ruleset.map_id, None).expect("map");
    let mut carved = state.clone();
    for t in map.territories.iter().take(map.territories.len() / 3) {
        carved.ownership.insert(t.id, ids[0]);
    }
    carved.revision += 1;
    let client = test_db.db.get().await.expect("client");
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&carved).expect("state"),
        state.revision as i64,
    )
    .await
    .expect("carve the map up");

    let carol = create_test_user(&test_db.db, "carol").await;
    svc.join_game(carol.id, &carol.username, game_id, None)
        .await
        .expect("join");
    let state = load_state(&svc, game_id).await;
    let carol_player = state.player(carol.id).expect("carol is in");
    assert!(
        carol_player.joined_claimed >= 30,
        "a third of the map was gone, got {}",
        carol_player.joined_claimed
    );
    assert!(
        state.shield_days(carol.id) > i32::from(state.ruleset.attack_grace_days),
        "arriving late is what the longer shield is for"
    );
}

/// A realm action resolves in the service, not in the keystroke that asked
/// for it: the event comes back first and the reloaded board a moment later.
/// If that second arrival does not report itself as a change, the frame that
/// would draw it never happens — you claim a territory, are told you have
/// it, and the map goes on showing it as somebody else's until you move the
/// cursor and trigger a render for another reason.
#[tokio::test]
async fn a_resolved_action_asks_for_the_frame_that_shows_it() {
    use crate::app::door::realm::resolver::RealmAction;
    use crate::app::door::realm::state::RealmState;

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["alice", "bob"]).await;

    let mut session = RealmState::new(svc.clone(), ids[0], "alice".to_string());
    session.open_board(game_id, crate::app::common::primitives::Screen::Dashboard);
    // Settle the opening load, so what follows is only about the action.
    let board_loaded = wait_for_tick(&mut session, |s| {
        s.board.as_ref().is_some_and(|b| b.detail.is_some())
    })
    .await;
    assert!(board_loaded, "the board never loaded");

    // Claim something reachable, the way the board does.
    let state = load_state(&svc, game_id).await;
    let map = crate::app::door::realm::map::map_handle(&state.ruleset.map_id, None).expect("map");
    let mine = *state
        .ownership
        .iter()
        .find(|(_, owner)| **owner == ids[0])
        .expect("alice holds something")
        .0;
    let target = map
        .territory(mine)
        .expect("held territory")
        .neighbors
        .iter()
        .copied()
        .find(|n| !state.ownership.contains_key(n))
        .expect("free land next door");
    let held_before = state.territory_count(ids[0]);

    svc.act(ids[0], game_id, RealmAction::Claim { target })
        .await
        .expect("claim");

    // The tick that carries the new board has to be the tick that says so.
    // An earlier `changed` (the event, the banner) does not help: by then the
    // board still holds the old world.
    // The revision, not the territory count: a claim is a roll, and a failed
    // one still spends the point, still writes the row, and still has to
    // reach the screen. Keying on "did I win the roll" would make this test
    // fail a third of the time for the wrong reason.
    let revision = |session: &RealmState| {
        session
            .board
            .as_ref()
            .and_then(|b| b.detail.as_ref())
            .map(|d| d.state.revision)
            .unwrap_or(0)
    };
    let _ = held_before;
    let mut arrived = false;
    for _ in 0..200 {
        let before = revision(&session);
        let tick = session.tick();
        let after = revision(&session);
        if after > before {
            assert!(
                tick.changed,
                "the board moved on this tick without asking to be drawn"
            );
            arrived = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        arrived,
        "the resolved claim never reached the session's board"
    );
}

/// Poll `tick` until `done`, the way the app's frame loop does.
async fn wait_for_tick(
    session: &mut crate::app::door::realm::state::RealmState,
    done: impl Fn(&crate::app::door::realm::state::RealmState) -> bool,
) -> bool {
    for _ in 0..200 {
        session.tick();
        if done(session) {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    false
}

/// The daily refill is the game asking for you, and it has to reach people
/// who are not looking at the board: the event carries the realm's name so a
/// session anywhere in the app can say which one is calling.
#[tokio::test]
async fn a_rolled_day_calls_the_players_by_name() {
    let test_db = new_test_db().await;
    let (svc, _activity) = realm_service_watching(&test_db);
    let mut events = svc.subscribe_events();
    let (game_id, _ids) = started_game(&svc, &test_db, &["realm_call_a", "realm_call_b"]).await;

    svc.sweep(Utc::now() + Duration::days(1))
        .await
        .expect("sweep");

    let mut called = None;
    while let Ok(event) = events.try_recv() {
        if let crate::app::door::realm::svc::RealmEvent::DayRolled {
            game_id: id, name, ..
        } = event
            && id == game_id
        {
            called = Some(name);
        }
    }
    assert_eq!(
        called.as_deref(),
        Some("The Long War"),
        "the summons says which realm is calling"
    );
}

/// The lounge hears about a war that has players in it, and hears nothing
/// from a world somebody sits in alone: that would be a daily line about
/// nothing, every day, for as long as the row lives.
#[tokio::test]
async fn only_a_realm_with_players_calls_the_lounge() {
    let test_db = new_test_db().await;
    let (svc, mut activity) = realm_service_watching(&test_db);
    let alice = create_test_user(&test_db.db, "realm_lounge_solo").await;
    let solo = create_game(&svc, &test_db, &alice, &spec("Alone", RESET_HOUR)).await;
    svc.sweep(Utc::now() + Duration::days(1))
        .await
        .expect("sweep");
    let mut called = Vec::new();
    while let Ok(event) = activity.try_recv() {
        if let ActivityKind::RealmCalls { game_id, .. } = event.kind {
            called.push((game_id, event.username.clone(), event.action.clone()));
        }
    }
    assert!(
        !called.iter().any(|(id, _, _)| *id == solo.id),
        "a world sat in alone says nothing: {called:?}"
    );

    // One world at a time: the lonely one ends before the shared one rises.
    let client = test_db.db.get().await.expect("client");
    RealmGame::cancel(&client, solo.id).await.expect("cancel");
    let (shared_id, _ids) =
        started_game(&svc, &test_db, &["realm_lounge_a", "realm_lounge_b"]).await;
    svc.sweep(Utc::now() + Duration::days(1))
        .await
        .expect("sweep");
    while let Ok(event) = activity.try_recv() {
        if let ActivityKind::RealmCalls { game_id, .. } = event.kind {
            called.push((game_id, event.username.clone(), event.action.clone()));
        }
    }
    assert!(
        called.iter().any(|(id, who, what)| *id == shared_id
            && who == "realm"
            && what == "The Long War calls its players"),
        "a war with players in it calls the lounge: {called:?}"
    );
}

/// A realm stands frozen for its first couple of minutes. The creator used
/// to be able to spend a whole day's points before anybody else had heard
/// the game existed; now the announcement goes out, people arrive, and the
/// first point in the realm is spent with everyone already in it.
#[tokio::test]
async fn a_new_realm_is_frozen_until_its_muster_ends() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let alice = create_test_user(&test_db.db, "realm_muster_a").await;
    let bob = create_test_user(&test_db.db, "realm_muster_b").await;
    let game = create_game(&svc, &test_db, &alice, &spec("Muster", RESET_HOUR)).await;

    let state = load_state(&svc, game.id).await;
    let target = nearest_free(&state, alice.id);
    let err = svc
        .act_at(alice.id, game.id, RealmAction::Claim { target }, Utc::now())
        .await
        .expect_err("the realm has not opened");
    assert!(
        err.to_string().contains("opens in"),
        "the refusal says how long is left: {err}"
    );

    // Everything else about the realm works while it is frozen: this is a
    // window for arriving and looking around, not a closed door.
    svc.join_game(bob.id, &bob.username, game.id, None)
        .await
        .expect("join during the muster");
    let state = load_state(&svc, game.id).await;
    assert_eq!(state.players.len(), 2);
    assert!(
        state.territory_count(bob.id) > 0,
        "a joiner sees their own land straight away"
    );

    // And once it opens, the game is an ordinary realm.
    let target = nearest_free(&state, alice.id);
    svc.act_at(
        alice.id,
        game.id,
        RealmAction::Claim { target },
        Utc::now() + crate::app::door::realm::svc::REALM_MUSTER,
    )
    .await
    .expect("the realm is open");
}

/// The muster is only worth anything if people hear about it, so creation
/// announces itself — named, attributed to whoever raised it, and with the
/// deadline in the line.
#[tokio::test]
async fn opening_a_world_announces_it_to_the_house() {
    let test_db = new_test_db().await;
    let (svc, mut activity) = realm_service_watching(&test_db);
    let game = open(&svc, &test_db, &spec("", RESET_HOUR)).await;

    let announced = activity_where(&mut activity, |event| {
        matches!(event.kind, ActivityKind::RealmSeasonOpened { game_id, .. } if game_id == game.id)
    })
    .await;
    assert_eq!(announced.username, "realm", "the invitation is the house's");
    assert_eq!(
        announced.kind,
        ActivityKind::RealmSeasonOpened {
            game_id: game.id,
            season: game.season,
        }
    );
    assert!(
        announced.action.contains("season 1") && announced.action.contains("gates open in a day"),
        "the line names the season and its deadline: {}",
        announced.action
    );
}

/// A finished realm should be walkable day by day, and that needs two things
/// the archive never had: the positions themselves, and the events that move
/// them. Spawns were never logged and a withdrawal freed land the log only
/// counted, so the map could not be rebuilt from the actions however complete
/// they looked.
#[tokio::test]
async fn every_day_keeps_the_board_it_closed_on() {
    use crate::app::door::realm::resolver::LogEntry;

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["alice", "bob"]).await;
    let (alice, bob) = (ids[0], ids[1]);

    // Both arrivals are on the record, which is where a replay has to start.
    let today = realm_day(Utc::now(), RESET_HOUR);
    let live = load_state(&svc, game_id).await;
    let spawned: Vec<Uuid> = live
        .last_day
        .as_ref()
        .expect("a day is open")
        .entries
        .iter()
        .filter_map(|entry| match entry {
            LogEntry::Spawned { user_id, .. } => Some(*user_id),
            _ => None,
        })
        .collect();
    assert!(
        spawned.contains(&alice) && spawned.contains(&bob),
        "{spawned:?}"
    );

    // Play a little, then let the day turn over.
    for _ in 0..3 {
        let state = load_state(&svc, game_id).await;
        let target = nearest_free(&state, alice);
        svc.act(alice, game_id, RealmAction::Claim { target })
            .await
            .expect("claim");
    }
    let before_rollover = load_state(&svc, game_id).await;
    let held_at_close = before_rollover.ownership.len();

    svc.sweep(Utc::now() + Duration::days(1))
        .await
        .expect("roll the day");

    let archived = svc
        .load_day_logs(game_id, None, 14)
        .await
        .expect("load the archive");
    let closed = archived
        .iter()
        .find(|day| day.day() == today)
        .expect("the day that just closed is archived");
    let board = closed
        .board
        .as_ref()
        .expect("a closed day keeps the board it closed on");

    assert_eq!(
        board.owned.len(),
        held_at_close,
        "the board should hold exactly what the map held when the day ended"
    );
    // Self-describing: names and colours travel with it, so a replay does not
    // depend on the live roster still remembering everybody.
    assert_eq!(board.players.len(), 2);
    assert!(board.players.iter().all(|p| !p.username.is_empty()));
    assert!(board.players.iter().all(|p| p.color.is_some()));
    let standings = board.standings();
    assert_eq!(
        standings.iter().map(|(_, held)| *held).sum::<u16>() as usize,
        held_at_close
    );
    assert!(
        standings[0].1 >= standings[1].1,
        "standings run busiest first"
    );
    // And every owned territory names a player that snapshot knows.
    for (territory, _) in &board.owned {
        assert!(board.owner(*territory).is_some());
    }

    // The day still being played has no board yet: a mid-day snapshot would
    // be a different question from the one this answers.
    let live_day = archived.iter().find(|day| day.day() == today + 1);
    assert!(
        live_day.is_none_or(|day| day.board.is_none()),
        "today is not finished, so it has no closing board"
    );
}

/// Withdrawing is the other hole: the player is dropped from the roster
/// outright, so without a log line the land they held goes back to nobody
/// with no record of how.
#[tokio::test]
async fn withdrawing_says_what_it_let_go_of() {
    use crate::app::door::realm::resolver::LogEntry;

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["alice", "bob"]).await;
    let bob = ids[1];
    let before = load_state(&svc, game_id).await;
    let bobs_land: Vec<u16> = before.holdings(bob);
    assert!(!bobs_land.is_empty());

    svc.leave_game(bob, game_id).await.expect("withdraw");

    let state = load_state(&svc, game_id).await;
    let freed = state
        .last_day
        .as_ref()
        .expect("a day is open")
        .entries
        .iter()
        .find_map(|entry| match entry {
            LogEntry::Left { user_id, freed } if *user_id == bob => Some(freed.clone()),
            _ => None,
        })
        .expect("the withdrawal is on the record");
    assert_eq!(freed, bobs_land, "and it says which ground went back");
    assert!(state.players.iter().all(|p| p.user_id != bob));
}

/// Quiet days must not cost a busy day its board. The sweeper rolls a day
/// whether or not anybody played, and a day that turned over with nobody
/// looking still closed on a real position.
#[tokio::test]
async fn a_busy_day_keeps_its_board_through_the_quiet_ones() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["alice", "bob"]).await;
    let alice = ids[0];
    let busy_day = realm_day(Utc::now(), RESET_HOUR);

    // Taken, not merely attempted: a claim is a roll, and this test is about
    // what the board holds rather than about luck.
    for _ in 0..2 {
        let state = load_state(&svc, game_id).await;
        let target = nearest_free(&state, alice);
        claim_until_taken(&svc, game_id, alice, target).await;
    }

    // Then nobody plays for a while, and the sweeper walks the days on.
    for ahead in 1..=4 {
        svc.sweep(Utc::now() + Duration::days(ahead))
            .await
            .expect("sweep");
    }

    let archived = svc
        .load_day_logs(game_id, None, 20)
        .await
        .expect("load the archive");
    let busy = archived
        .iter()
        .find(|day| day.day() == busy_day)
        .expect("the day that was played is archived");
    assert!(
        busy.board.is_some(),
        "the day people played on lost its board to the quiet days after it"
    );
    assert_eq!(
        busy.board.as_ref().map(|board| board.owned.len()),
        Some(4),
        "two spawns and two claims"
    );
}

/// The history opens on day one and says "of N", so N has to be the whole
/// run rather than the newest page of it — the archive pages fourteen days at
/// a time, and a two-month game is five pages.
#[tokio::test]
async fn the_history_view_pages_the_whole_archive_in() {
    use crate::app::door::realm::state::{RealmState, RealmView};

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["alice", "bob"]).await;
    let alice = ids[0];

    // Twenty days of history: more than one page of the archive.
    for ahead in 0..20 {
        let at = Utc::now() + Duration::days(ahead);
        let state = load_state(&svc, game_id).await;
        if state.points_left(alice, realm_day(at, RESET_HOUR)) > 0 {
            let target = nearest_free(&state, alice);
            let _ = svc
                .act_at(alice, game_id, RealmAction::Claim { target }, at)
                .await;
        }
        svc.sweep(at + Duration::days(1)).await.expect("roll");
    }

    let mut session = RealmState::new(svc.clone(), alice, "alice".to_string());
    session.open_board(game_id, crate::app::common::primitives::Screen::Dashboard);
    for _ in 0..200 {
        session.tick();
        if session.board.as_ref().is_some_and(|b| b.detail.is_some()) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Straight into the history without visiting the log view first — which
    // is the path that was broken: the chase for the rest of the archive used
    // to start only when a page landed while this view was already open, and
    // the first page lands while the board is still showing the map.
    if let Some(board) = session.board.as_mut() {
        board.view = RealmView::History;
    }

    // Tick until the archive stops growing: the view chases the rest of it.
    let mut settled = 0;
    let mut seen = 0;
    for _ in 0..400 {
        session.tick();
        let days = session.history_days().len();
        if days == seen && days > 0 {
            settled += 1;
            if settled > 20 {
                break;
            }
        } else {
            settled = 0;
            seen = days;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    assert!(
        seen > 14,
        "a page is fourteen days; the whole run should be here, got {seen}"
    );
    // And it lands on the first of them.
    let (index, total) = session.history_position().expect("a run to walk");
    assert_eq!(index, 0, "the history opens at the beginning");
    assert_eq!(total, seen);
}

/// A worker fed by hand: what another replica's write looks like from here is
/// a notify arriving on the channel and the worker re-reading. Returns once
/// that re-read has published a snapshot.
async fn notified(
    worker: &tokio::sync::mpsc::UnboundedSender<crate::pg_listener::Signal>,
    snapshot: &mut tokio::sync::watch::Receiver<
        std::sync::Arc<crate::app::door::realm::svc::RealmSnapshot>,
    >,
) {
    worker
        .send(crate::pg_listener::Signal::Notify {
            channel: crate::pg_listener::Channel::RealmGameChanged,
            payload: String::new(),
        })
        .expect("worker alive");
    tokio::time::timeout(std::time::Duration::from_secs(5), snapshot.changed())
        .await
        .expect("the re-read published in time")
        .expect("service alive");
}

/// Drain the events a session would see, up to a short deadline.
async fn drained(
    events: &mut broadcast::Receiver<crate::app::door::realm::svc::RealmEvent>,
) -> Vec<crate::app::door::realm::svc::RealmEvent> {
    let mut seen = Vec::new();
    while let Ok(Ok(event)) =
        tokio::time::timeout(std::time::Duration::from_millis(300), events.recv()).await
    {
        seen.push(event);
    }
    seen
}

/// Nothing this replica tells a session about a write comes from the process
/// that wrote it. A row written behind this service's back (another replica,
/// here plain SQL) reaches its sessions through the notify and the re-read:
/// the reload, the daily call by name, and the finish.
#[tokio::test]
async fn a_write_from_another_replica_reaches_this_one_on_the_re_read() {
    use crate::app::door::realm::svc::RealmEvent;

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_rep_a", "realm_rep_b"]).await;
    let client = test_db.db.get().await.expect("client");
    let (worker, signals) = tokio::sync::mpsc::unbounded_channel();
    let _worker = svc.start_notify_worker(signals);
    let mut snapshot = svc.subscribe_snapshot();
    snapshot.borrow_and_update();
    let mut events = svc.subscribe_events();

    // Somebody acted on another replica: the revision moved, nothing else.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write from elsewhere");
    notified(&worker, &mut snapshot).await;
    assert_eq!(
        drained(&mut events).await,
        vec![RealmEvent::GameChanged { game_id }],
        "a moved revision is a reload and nothing more"
    );

    // Its day rolled over there: the players here are called, by name.
    client
        .execute(
            "UPDATE realm_games SET last_resolved_day = last_resolved_day + 1 WHERE id = $1",
            &[&game_id],
        )
        .await
        .expect("roll the day from elsewhere");
    notified(&worker, &mut snapshot).await;
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(
        drained(&mut events).await,
        vec![
            RealmEvent::GameChanged { game_id },
            RealmEvent::DayRolled {
                game_id,
                day: row.last_resolved_day,
                name: "The Long War".to_string(),
            },
        ]
    );

    // And it finished there.
    client
        .execute(
            "UPDATE realm_games SET status = 'finished', winner_user_id = $2, updated = current_timestamp WHERE id = $1",
            &[&game_id, &ids[0]],
        )
        .await
        .expect("finish from elsewhere");
    notified(&worker, &mut snapshot).await;
    assert_eq!(
        drained(&mut events).await,
        vec![
            RealmEvent::GameChanged { game_id },
            RealmEvent::GameFinished {
                game_id,
                winner_user_id: Some(ids[0]),
            },
        ]
    );
}

/// The finish is durable before the chips move, so a credit that fails must
/// not lose the pot: the row stays unsettled, the sweeper comes back for it,
/// and when it lands it lands once.
#[tokio::test]
async fn a_pot_that_could_not_be_paid_is_settled_by_the_sweeper() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(
        &svc,
        &test_db,
        &["realm_owe_a", "realm_owe_b", "realm_owe_c"],
    )
    .await;
    let alice = ids[0];
    let client = test_db.db.get().await.expect("client");

    // A played-out war, one territory short of the whole map.
    let mut state = load_state(&svc, game_id).await;
    let expected = state.revision as i64;
    let bar = state.ruleset.min_active_days_to_win;
    for player in &mut state.players {
        if player.user_id == alice {
            player.active_days = bar;
        } else {
            player.status = RealmPlayerStatus::Eliminated;
            player.exit_day = Some(realm_day(Utc::now(), RESET_HOUR));
            player.active_days = bar;
        }
    }
    let map =
        crate::app::door::realm::map::map_handle(&state.ruleset.map_id, None).expect("the map");
    let last = map.territories.len() as u16 - 1;
    state.ownership.clear();
    for territory in 0..last {
        state.ownership.insert(territory, alice);
    }
    state.revision += 1;
    RealmGame::update_state_cas(
        &client,
        game_id,
        &serde_json::to_value(&state).expect("state"),
        expected,
    )
    .await
    .expect("write state");

    // The template the pot pays through is gone when the game ends.
    client
        .execute(
            "UPDATE reward_templates SET active = false WHERE key = 'realm_win_small'",
            &[],
        )
        .await
        .expect("retire the template");
    svc.act(alice, game_id, RealmAction::Claim { target: last })
        .await
        .expect("act");
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row.status, RealmGame::STATUS_FINISHED);

    let ledger_rows = || async {
        client
            .query(
                "SELECT delta FROM chip_ledger WHERE user_id = $1 AND reason = 'realm_conquest'",
                &[&alice],
            )
            .await
            .expect("ledger")
            .len()
    };

    // The sweeper tries and cannot: nothing paid, nothing recorded.
    svc.sweep(Utc::now()).await.expect("sweep");
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(
        row.payout, None,
        "an unpaid pot is not recorded as anything"
    );
    assert_eq!(ledger_rows().await, 0);

    // The template is back: the next tick settles it.
    client
        .execute(
            "UPDATE reward_templates SET active = true WHERE key = 'realm_win_small'",
            &[],
        )
        .await
        .expect("restore the template");
    svc.sweep(Utc::now()).await.expect("sweep");
    let row = svc.load_game(game_id).await.expect("load").expect("row");
    assert_eq!(row.payout.as_deref(), Some(RealmGame::PAYOUT_PAID));
    let pot: i64 = crate::app::door::realm::svc::payout_plan(3, state.ruleset.pot_scale)
        .iter()
        .map(|(_, chips)| chips)
        .sum();
    assert_eq!(wait_for_payout(&test_db, alice).await, Some(pot));

    // Settled is settled: another tick pays nothing twice.
    svc.sweep(Utc::now()).await.expect("sweep");
    assert_eq!(ledger_rows().await, 1);
}

/// A row this build cannot read leaves the listing and takes nothing with
/// it: the other games stay listed and the service keeps publishing.
#[tokio::test]
async fn a_corrupt_row_leaves_the_listing_without_taking_it_down() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let client = test_db.db.get().await.expect("client");
    // The rotten row is a world that just ended, still in the listing; the
    // sound one is the world standing after it.
    let carol = create_test_user(&test_db.db, "realm_rot_c").await;
    let rotten = create_game(&svc, &test_db, &carol, &spec("Rotten", RESET_HOUR))
        .await
        .id;
    client
        .execute(
            "UPDATE realm_games SET status = 'finished', updated = current_timestamp WHERE id = $1",
            &[&rotten],
        )
        .await
        .expect("end the rotten world");
    let (sound, _) = started_game(&svc, &test_db, &["realm_rot_a", "realm_rot_b"]).await;
    let (worker, signals) = tokio::sync::mpsc::unbounded_channel();
    let _worker = svc.start_notify_worker(signals);
    let mut snapshot = svc.subscribe_snapshot();
    snapshot.borrow_and_update();

    client
        .execute(
            "UPDATE realm_games SET state = '{\"nope\": true}'::jsonb WHERE id = $1",
            &[&rotten],
        )
        .await
        .expect("corrupt the row");
    notified(&worker, &mut snapshot).await;

    let listed: Vec<Uuid> = snapshot
        .borrow()
        .active_games
        .iter()
        .chain(snapshot.borrow().finished_games.iter())
        .map(|g| g.id)
        .collect();
    assert!(listed.contains(&sound), "the readable game is still listed");
    assert!(
        !listed.contains(&rotten),
        "the unreadable one is left out rather than guessed at"
    );
}

/// The board's "left of" figure is the day's real allowance. It used to be
/// the tier for the roster's high-water mark, so five arrivals on a fresh
/// world read "7/5 points".
#[tokio::test]
async fn the_board_counts_points_against_the_days_real_allowance() {
    use crate::app::door::realm::state::{RealmState, points_left};

    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(
        &svc,
        &test_db,
        &[
            "realm_cap_a",
            "realm_cap_b",
            "realm_cap_c",
            "realm_cap_d",
            "realm_cap_e",
        ],
    )
    .await;
    let state = load_state(&svc, game_id).await;
    let allowance = state
        .standing(ids[0], realm_day(Utc::now(), RESET_HOUR))
        .allowance;

    let mut session = RealmState::new(svc.clone(), ids[0], "realm_cap_a".to_string());
    session.open_board(game_id, crate::app::common::primitives::Screen::Dashboard);
    let loaded = wait_for_tick(&mut session, |s| {
        s.board.as_ref().is_some_and(|b| b.detail.is_some())
    })
    .await;
    assert!(loaded, "the board never loaded");

    let board = session.board.as_ref().expect("board");
    assert_eq!(
        points_left(board, ids[0]),
        Some((usize::from(allowance), usize::from(allowance))),
        "nothing spent yet: every point of the day's allowance is left"
    );
}

/// The lounge call names the day that just opened, however long the realm
/// went without a rollover before this action brought it up to date.
#[tokio::test]
async fn the_lounge_call_names_the_day_that_opened() {
    use crate::app::activity::event::ActivityKind;

    let test_db = new_test_db().await;
    let (svc, mut activity) = realm_service_watching(&test_db);
    let (game_id, ids) =
        started_game(&svc, &test_db, &["realm_call_day_a", "realm_call_day_b"]).await;

    // Three days with no sweeper (a deploy that stayed down), then a move.
    let later = Utc::now() + Duration::days(3);
    let state = load_state(&svc, game_id).await;
    let target = nearest_free(&state, ids[0]);
    svc.act_at(ids[0], game_id, RealmAction::Claim { target }, later)
        .await
        .expect("act");

    let mut called = Vec::new();
    while let Ok(event) = activity.try_recv() {
        if let ActivityKind::RealmCalls { game_id: id, day } = event.kind
            && id == game_id
        {
            called.push(day);
        }
    }
    assert_eq!(called, vec![realm_day(later, RESET_HOUR)]);
}

/// Withdrawing gives the ground back as it was found: free land carries no
/// walls, or the map draws a fortress nobody holds and the next claimant
/// inherits it in the history.
#[tokio::test]
async fn withdrawing_takes_your_walls_with_you() {
    let test_db = new_test_db().await;
    let svc = realm_service(&test_db);
    let (game_id, ids) = started_game(&svc, &test_db, &["realm_wall_a", "realm_wall_b"]).await;
    let bob = ids[1];
    let state = load_state(&svc, game_id).await;
    let spawn = state.holdings(bob)[0];

    svc.act(bob, game_id, RealmAction::Fortify { target: spawn })
        .await
        .expect("dig in");
    assert_eq!(load_state(&svc, game_id).await.fort_level(spawn), 2);

    svc.leave_game(bob, game_id).await.expect("bob withdraws");
    let state = load_state(&svc, game_id).await;
    assert_eq!(state.ownership.get(&spawn), None, "the ground went back");
    assert_eq!(state.fort_level(spawn), 1, "and so did the walls on it");
    assert!(
        state.forts.is_empty(),
        "nothing left standing: {:?}",
        state.forts
    );
}
