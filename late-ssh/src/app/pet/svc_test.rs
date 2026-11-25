use chrono::NaiveDate;
use late_core::models::chips::UserChips;
use late_core::models::pet::{
    Fed, Form, PetCompanion, PetMood, PetSpecies, Pick, Stage, TreatKind, Trick, VisitKind,
};
use late_core::test_utils::{TestDb, create_test_user};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{Did, PET_CHIP_BONUS, PetCommand, PetService, Refusal, Visited};
use crate::app::activity::event::{ActivityEvent, ActivityKind};
use crate::app::pet::growth::{DayCounted, Practiced, TRICK_DAYS};
use crate::test_helpers::new_test_db;

fn service(test_db: &TestDb) -> (PetService, broadcast::Receiver<ActivityEvent>) {
    let (tx, rx) = broadcast::channel::<ActivityEvent>(16);
    (PetService::new(test_db.db.clone(), tx), rx)
}

/// A user who has signed in: every account has a pet, an egg, out.
async fn owner(test_db: &TestDb, svc: &PetService, handle: &str) -> Uuid {
    let user = create_test_user(&test_db.db, handle).await;
    svc.bootstrap(user.id).await.expect("bootstrap");
    user.id
}

/// An owner whose egg hatched on `day(1)` as `species`.
async fn hatched(test_db: &TestDb, svc: &PetService, handle: &str, species: PetSpecies) -> Uuid {
    let user_id = owner(test_db, svc, handle).await;
    live(svc, user_id, 0..2).await;
    assert_eq!(
        did(svc, user_id, PetCommand::Hatch(species), day(1)).await,
        Did::Hatched(species)
    );
    user_id
}

fn counted(days_away: i64) -> Did {
    Did::DayCounted(DayCounted { days_away })
}

fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 3, 1).expect("a date") + chrono::Duration::days(n)
}

/// Count a day together on each of `days`.
async fn live(svc: &PetService, user_id: Uuid, days: std::ops::Range<i64>) {
    for n in days {
        svc.act(user_id, PetCommand::CountDay, day(n))
            .await
            .expect("count a day");
    }
}

async fn did(svc: &PetService, user_id: Uuid, command: PetCommand, today: NaiveDate) -> Did {
    svc.act(user_id, command, today).await.expect("act").did
}

async fn balance(test_db: &TestDb, user_id: Uuid) -> i64 {
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user_id)
        .await
        .expect("chips")
        .balance
}

#[tokio::test]
async fn bootstrap_creates_a_sleeping_egg_and_is_idempotent_across_reconnects() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-svc-new").await;
    let (svc, _rx) = service(&test_db);

    let first = svc.bootstrap(user.id).await.expect("bootstrap");
    let second = svc.bootstrap(user.id).await.expect("second bootstrap");

    assert_eq!(first.row.user_id, user.id);
    assert_eq!(first.row.species(), PetSpecies::Cat);
    assert_eq!(first.row.mood(), PetMood::Asleep);
    assert_eq!(first.row.stage(), Stage::Egg);
    assert!(first.waiting.is_none() && first.visitors.is_empty());
    assert_eq!(
        first.row.id, second.row.id,
        "reconnecting must return the same pet row, not create a new one"
    );
}

#[tokio::test]
async fn the_mood_lands_on_the_row() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let (svc, _rx) = service(&test_db);
    let user_id = owner(&test_db, &svc, "pet-svc-mood").await;

    svc.set_mood(user_id, PetMood::Vibing)
        .await
        .expect("set mood");

    let stored = PetCompanion::ensure(&client, user_id)
        .await
        .expect("companion");
    assert_eq!(stored.mood(), PetMood::Vibing);
}

#[tokio::test]
async fn petting_pays_an_owner_once_a_day() {
    let test_db = new_test_db().await;
    let (svc, mut rx) = service(&test_db);
    let today = day(0);
    let user_id = owner(&test_db, &svc, "pet-svc-chips-owner").await;
    let before = balance(&test_db, user_id).await;
    let (svc_a, svc_b) = (svc.clone(), svc.clone());
    let (first, second) = tokio::join!(
        svc_a.act(user_id, PetCommand::Pat, today),
        svc_b.act(user_id, PetCommand::Pat, today),
    );
    let mut dids = [
        first.expect("first pat").did,
        second.expect("second pat").did,
    ];
    dids.sort_by_key(|did| *did != Did::Patted);
    assert_eq!(
        dids,
        [Did::Patted, Did::AlreadyPatted],
        "two sessions racing: one pays"
    );
    assert_eq!(
        balance(&test_db, user_id).await - before,
        PET_CHIP_BONUS,
        "one payout a day"
    );

    svc.announce(Did::Patted, user_id).await;
    let event = rx.try_recv().expect("the first pat tells the session");
    assert!(matches!(event.kind, ActivityKind::PetPetted));
    svc.announce(Did::AlreadyPatted, user_id).await;
    assert!(rx.try_recv().is_err(), "and only the first");

    assert_eq!(
        did(&svc, user_id, PetCommand::Pat, day(1)).await,
        Did::Patted,
        "a new day pays again"
    );
}

/// The switch reaches an owner's other sessions only at their next connect,
/// so one of them can still act on a pet put away. The row decides, not the
/// session: nothing is paid, counted or earned, and no day is spent.
#[tokio::test]
async fn a_pet_put_away_takes_nothing_from_a_session_that_has_not_heard() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let (svc, _rx) = service(&test_db);
    let user_id = owner(&test_db, &svc, "pet-svc-away").await;
    PetCompanion::set_enabled(&client, user_id, false)
        .await
        .expect("put the pet away");
    let today = day(0);
    let before = balance(&test_db, user_id).await;

    for command in [
        PetCommand::Pat,
        PetCommand::CountDay,
        PetCommand::Earn(TreatKind::Puzzle),
        PetCommand::Play,
    ] {
        assert_eq!(
            did(&svc, user_id, command, today).await,
            Did::Refused(Refusal::Away),
            "{command:?}"
        );
    }
    assert_eq!(balance(&test_db, user_id).await, before);

    PetCompanion::set_enabled(&client, user_id, true)
        .await
        .expect("bring the pet back");
    assert_eq!(
        did(&svc, user_id, PetCommand::Pat, today).await,
        Did::Patted,
        "the refused click did not spend the day"
    );
    let acted = svc
        .act(user_id, PetCommand::CountDay, today)
        .await
        .expect("count");
    assert_eq!(acted.did, counted(0));
    assert_eq!(acted.pet.row.days_together, 1, "nothing counted while away");
    assert!(acted.pet.waiting.is_none(), "nothing earned while away");
}

#[tokio::test]
async fn two_sessions_pressing_a_key_count_one_day() {
    let test_db = new_test_db().await;
    let (svc, _rx) = service(&test_db);
    let user_id = owner(&test_db, &svc, "pet-svc-day").await;

    let (svc_a, svc_b) = (svc.clone(), svc.clone());
    let (first, second) = tokio::join!(
        svc_a.act(user_id, PetCommand::CountDay, day(0)),
        svc_b.act(user_id, PetCommand::CountDay, day(0)),
    );
    let mut dids = [first.expect("first").did, second.expect("second").did];
    dids.sort_by_key(|did| *did == Did::DayAlreadyCounted);
    assert_eq!(dids, [counted(0), Did::DayAlreadyCounted]);

    let acted = svc
        .act(user_id, PetCommand::CountDay, day(4))
        .await
        .expect("back after three days");
    assert_eq!(acted.did, counted(3));
    assert_eq!(
        (acted.pet.row.days_together, acted.pet.row.last_day),
        (2, Some(day(4)))
    );
}

#[tokio::test]
async fn an_egg_hatches_as_its_owners_pick_and_grows_by_its_owners_picks() {
    let test_db = new_test_db().await;
    let (svc, _rx) = service(&test_db);
    let user_id = owner(&test_db, &svc, "pet-svc-grow").await;

    live(&svc, user_id, 0..1).await;
    assert_eq!(
        did(&svc, user_id, PetCommand::Hatch(PetSpecies::Dog), day(0)).await,
        Did::Refused(Refusal::NotReady),
        "the day it arrived"
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Give, day(0)).await,
        Did::Refused(Refusal::Egg)
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Species(PetSpecies::Bird), day(0)).await,
        Did::Refused(Refusal::Egg),
        "an egg is no animal yet"
    );
    live(&svc, user_id, 1..2).await;
    let acted = svc
        .act(user_id, PetCommand::Hatch(PetSpecies::Bird), day(1))
        .await
        .expect("hatch");
    assert_eq!(acted.did, Did::Hatched(PetSpecies::Bird));
    assert_eq!(
        (acted.pet.row.species(), acted.pet.row.stage()),
        (PetSpecies::Bird, Stage::Baby),
        "what hatches is its owner's pick"
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Hatch(PetSpecies::Cat), day(1)).await,
        Did::Refused(Refusal::NotReady),
        "once"
    );

    // Its days make it ready and enter no stage: young is its owner's
    // pick of a top, from the seventh day.
    assert_eq!(
        did(&svc, user_id, PetCommand::Grow(Pick::One), day(1)).await,
        Did::Refused(Refusal::NotReady),
        "a baby without its days"
    );
    live(&svc, user_id, 2..7).await;
    assert_eq!(
        svc.bootstrap(user_id).await.expect("reload").row.form(),
        Form::Baby,
        "the seventh day alone grows nothing"
    );
    let acted = svc
        .act(user_id, PetCommand::Grow(Pick::Two), day(6))
        .await
        .expect("grow");
    assert_eq!(acted.did, Did::Grew(Stage::Young));
    assert_eq!(acted.pet.row.form(), Form::Young { top: Pick::Two });
    assert_eq!(
        did(&svc, user_id, PetCommand::Grow(Pick::One), day(6)).await,
        Did::Refused(Refusal::NotReady),
        "an adult is twenty three days off"
    );

    // `/pet dog`: another animal, the same pet.
    let before = acted.pet.row.revision;
    let acted = svc
        .act(user_id, PetCommand::Species(PetSpecies::Dog), day(6))
        .await
        .expect("change species");
    assert_eq!(acted.did, Did::Became(PetSpecies::Dog));
    assert_eq!(
        (
            acted.pet.row.species(),
            acted.pet.row.form(),
            acted.pet.row.days_together
        ),
        (PetSpecies::Dog, Form::Young { top: Pick::Two }, 7),
        "the pick it made carries over as the dog's drawing"
    );
    assert!(
        acted.pet.row.revision > before,
        "a write a mirror orders by"
    );
}

#[tokio::test]
async fn one_treat_a_day_from_any_game_waits_alone_and_bonds_once_a_day() {
    let test_db = new_test_db().await;
    let (svc, _rx) = service(&test_db);
    let user_id = hatched(&test_db, &svc, "pet-svc-treat", PetSpecies::Cat).await;
    let today = day(2);

    // Two sessions win in two families at once: the day has one treat.
    let (svc_a, svc_b) = (svc.clone(), svc.clone());
    let (first, second) = tokio::join!(
        svc_a.act(user_id, PetCommand::Earn(TreatKind::Puzzle), today),
        svc_b.act(user_id, PetCommand::Earn(TreatKind::Table), today),
    );
    let dids = [first.expect("first").did, second.expect("second").did];
    let earned = match dids {
        [Did::Earned(kind), Did::NotEarned(_)] | [Did::NotEarned(_), Did::Earned(kind)] => kind,
        other => panic!("two sessions winning earn one treat, got {other:?}"),
    };
    assert_eq!(
        did(&svc, user_id, PetCommand::Earn(TreatKind::Door), today).await,
        Did::NotEarned(TreatKind::Door),
        "a third family the same day earns nothing more"
    );

    let gave = svc
        .act(user_id, PetCommand::Give, today)
        .await
        .expect("give");
    assert_eq!(
        gave.did,
        Did::Gave {
            kind: earned,
            bonded: true
        }
    );
    assert_eq!(gave.pet.waiting, None);
    assert_eq!(
        gave.pet.fed,
        Some(Fed {
            kind: earned,
            on: today
        }),
        "what it goes by today"
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Give, today).await,
        Did::NothingToGive
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Earn(TreatKind::Door), today).await,
        Did::NotEarned(TreatKind::Door),
        "one a day, given or not"
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Play, today).await,
        Did::Played { bonded: false },
        "play shares the day's bond with the treat"
    );

    // A treat left in the pouch refuses the next day's; given late, it
    // makes room, and the second one eaten that day bonds nothing more.
    let (next, later) = (day(3), day(4));
    assert_eq!(
        did(&svc, user_id, PetCommand::Earn(TreatKind::Arcade), next).await,
        Did::Earned(TreatKind::Arcade)
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Earn(TreatKind::Door), later).await,
        Did::NotEarned(TreatKind::Door),
        "one waits"
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Give, later).await,
        Did::Gave {
            kind: TreatKind::Arcade,
            bonded: true
        }
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Earn(TreatKind::Door), later).await,
        Did::Earned(TreatKind::Door)
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Give, later).await,
        Did::Gave {
            kind: TreatKind::Door,
            bonded: false
        }
    );
    let pet = svc.bootstrap(user_id).await.expect("reload");
    assert_eq!((pet.row.bond_days, pet.row.last_bond_day), (2, Some(later)));
    assert_eq!(
        pet.fed,
        Some(Fed {
            kind: TreatKind::Door,
            on: later
        }),
        "the last one eaten names it"
    );
}

#[tokio::test]
async fn a_reset_turns_it_back_into_an_egg_and_keeps_its_name() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let (svc, _rx) = service(&test_db);
    let user_id = hatched(&test_db, &svc, "pet-svc-reset", PetSpecies::Dog).await;
    PetCompanion::set_name(&client, user_id, Some("Rex"))
        .await
        .expect("name");
    for n in 2..2 + TRICK_DAYS as i64 {
        live(&svc, user_id, n..n + 1).await;
        did(&svc, user_id, PetCommand::Practice(Trick::Spin), day(n)).await;
    }
    let today = day(6);
    live(&svc, user_id, 5..7).await;
    assert_eq!(
        did(&svc, user_id, PetCommand::Grow(Pick::Two), today).await,
        Did::Grew(Stage::Young)
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Pat, today).await,
        Did::Patted
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Earn(TreatKind::Table), today).await,
        Did::Earned(TreatKind::Table)
    );
    let before = svc.bootstrap(user_id).await.expect("before");
    assert_eq!(before.row.tricks(), vec![Trick::Spin]);

    let acted = svc
        .act(user_id, PetCommand::Reset, today)
        .await
        .expect("reset");
    assert_eq!(acted.did, Did::StartedOver);
    let row = &acted.pet.row;
    assert_eq!(
        (
            row.form(),
            row.days_together,
            row.last_day,
            row.bond_days,
            row.tricks(),
            row.learning()
        ),
        (Form::Egg, 0, None, 0, Vec::new(), None),
        "the drawings it picked go with the rest"
    );
    assert_eq!(row.name.as_deref(), Some("Rex"), "the name stays");
    assert_eq!(
        row.last_petted,
        Some(today),
        "the day's pat is not sold twice"
    );
    assert!(row.revision > before.row.revision);
    assert_eq!(
        acted.pet.waiting,
        Some(TreatKind::Table),
        "the treat waits for the next hatch"
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Pat, today).await,
        Did::AlreadyPatted
    );

    // The same day is its first day again.
    assert_eq!(
        did(&svc, user_id, PetCommand::CountDay, today).await,
        counted(0)
    );
    assert_eq!(
        did(&svc, user_id, PetCommand::Hatch(PetSpecies::Cat), today).await,
        Did::Refused(Refusal::NotReady)
    );
}

#[tokio::test]
async fn a_trick_is_learned_over_separate_days() {
    let test_db = new_test_db().await;
    let (svc, _rx) = service(&test_db);
    let user_id = owner(&test_db, &svc, "pet-svc-trick").await;
    let spin = Trick::Spin;
    assert_eq!(
        did(&svc, user_id, PetCommand::Practice(spin), day(0)).await,
        Did::Practiced {
            trick: spin,
            practiced: Practiced::Egg
        }
    );
    live(&svc, user_id, 0..2).await;
    assert_eq!(
        did(&svc, user_id, PetCommand::Hatch(PetSpecies::Cat), day(1)).await,
        Did::Hatched(PetSpecies::Cat)
    );

    for n in 1..TRICK_DAYS as i64 {
        assert_eq!(
            did(&svc, user_id, PetCommand::Practice(spin), day(n)).await,
            Did::Practiced {
                trick: spin,
                practiced: Practiced::Progress { days: n as i32 }
            }
        );
    }
    assert_eq!(
        did(
            &svc,
            user_id,
            PetCommand::Practice(spin),
            day(TRICK_DAYS as i64 - 1)
        )
        .await,
        Did::Practiced {
            trick: spin,
            practiced: Practiced::AlreadyToday
        }
    );
    let acted = svc
        .act(user_id, PetCommand::Practice(spin), day(TRICK_DAYS as i64))
        .await
        .expect("the last day");
    assert_eq!(
        acted.did,
        Did::Practiced {
            trick: spin,
            practiced: Practiced::Learned
        }
    );
    assert_eq!(acted.pet.row.tricks(), vec![spin]);
}

#[tokio::test]
async fn a_visitor_pets_once_a_day_and_never_their_own_or_one_put_away() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let (svc, _rx) = service(&test_db);
    let host = owner(&test_db, &svc, "pet-svc-host").await;
    let guest = owner(&test_db, &svc, "pet-svc-guest").await;
    let never_here = create_test_user(&test_db.db, "pet-svc-never").await.id;
    PetCompanion::set_name(&client, host, Some("Mochi"))
        .await
        .expect("name");
    let before = svc.bootstrap(host).await.expect("host").row.revision;

    assert_eq!(
        svc.visit(guest, host, VisitKind::Pat, day(0))
            .await
            .expect("pat"),
        Visited::Patted {
            pet: "Mochi".to_string()
        }
    );
    assert_eq!(
        svc.visit(guest, host, VisitKind::Pat, day(0))
            .await
            .expect("pat again"),
        Visited::AlreadyToday(VisitKind::Pat)
    );
    assert_eq!(
        svc.visit(host, host, VisitKind::Pat, day(0))
            .await
            .expect("own pet"),
        Visited::OwnPet
    );
    assert_eq!(
        svc.visit(host, never_here, VisitKind::Pat, day(0))
            .await
            .expect("no pet there"),
        Visited::NobodyHome,
        "an account that never signed in has no pet to visit"
    );

    let seen = svc.bootstrap(host).await.expect("host");
    assert_eq!(seen.visitors.len(), 1, "the owner sees who came by");
    assert_eq!(seen.visitors[0].visitor_id, guest);
    assert_eq!(
        seen.row.revision,
        before + 1,
        "one visit, one write a mirror orders by"
    );
    assert_eq!(
        did(&svc, host, PetCommand::SeenVisits, day(0)).await,
        Did::VisitsSeen
    );
    assert!(
        svc.bootstrap(host).await.expect("host").visitors.is_empty(),
        "looked at"
    );

    PetCompanion::set_enabled(&client, host, false)
        .await
        .expect("put away");
    assert_eq!(
        svc.visit(guest, host, VisitKind::Pat, day(1))
            .await
            .expect("pat a pet put away"),
        Visited::NobodyHome
    );
}

#[tokio::test]
async fn a_pet_goes_calling_once_a_day_and_an_egg_stays_home() {
    let test_db = new_test_db().await;
    let (svc, _rx) = service(&test_db);
    let host = owner(&test_db, &svc, "pet-svc-called").await;
    let second_host = hatched(&test_db, &svc, "pet-svc-called-2", PetSpecies::Bird).await;
    let caller = owner(&test_db, &svc, "pet-svc-caller").await;

    assert_eq!(
        svc.visit(caller, host, VisitKind::Call, day(0))
            .await
            .expect("an egg calling"),
        Visited::NobodyToSend
    );
    live(&svc, caller, 0..2).await;
    assert_eq!(
        did(&svc, caller, PetCommand::Hatch(PetSpecies::Cat), day(1)).await,
        Did::Hatched(PetSpecies::Cat)
    );

    assert_eq!(
        svc.visit(caller, host, VisitKind::Call, day(1))
            .await
            .expect("call"),
        Visited::Called {
            caller: "the cat".to_string(),
            host: "the egg".to_string(),
            line: "it kept the egg warm",
        },
        "the host is still an egg"
    );

    // The call is a stay: the host's pet has a guest, as the caller's pet
    // looks, and the caller's own pet knows where it is.
    let home = svc.bootstrap(host).await.expect("the host");
    assert_eq!(home.calling, None, "having a guest is not being one");
    assert_eq!(home.guests.len(), 1);
    assert_eq!(
        (
            home.guests[0].visitor_id,
            home.guests[0].species,
            home.guests[0].form
        ),
        (caller, PetSpecies::Cat, Form::Baby)
    );
    let away = svc.bootstrap(caller).await.expect("the caller");
    assert!(away.guests.is_empty());
    let stay = away.calling.expect("its pet is out calling");
    assert_eq!(stay.since, home.guests[0].since);
    assert!(
        svc.bootstrap(second_host)
            .await
            .expect("the pet nobody called on")
            .guests
            .is_empty()
    );
    assert_eq!(
        svc.visit(caller, second_host, VisitKind::Call, day(1))
            .await
            .expect("a second call"),
        Visited::AlreadyToday(VisitKind::Call),
        "one call a day, whoever it is on"
    );

    // The next day both ate a door treat: the call reads what they share.
    for pet in [caller, second_host] {
        did(&svc, pet, PetCommand::Earn(TreatKind::Door), day(2)).await;
        did(&svc, pet, PetCommand::Give, day(2)).await;
    }
    assert_eq!(
        svc.visit(caller, second_host, VisitKind::Call, day(2))
            .await
            .expect("the next day"),
        Visited::Called {
            caller: "the cat".to_string(),
            host: "the bird".to_string(),
            line: "they went exploring and came back muddy",
        }
    );
    assert_eq!(
        svc.visit(second_host, caller, VisitKind::Call, day(3))
            .await
            .expect("a day later"),
        Visited::Called {
            caller: "the bird".to_string(),
            host: "the cat".to_string(),
            line: "they sat in the sun together",
        },
        "yesterday's treat names nobody today"
    );
}

#[tokio::test]
async fn a_change_notice_reaches_this_replicas_sessions() {
    let test_db = new_test_db().await;
    let (svc, _rx) = service(&test_db);
    let mut changes = svc.subscribe_changes();
    let user_id = Uuid::now_v7();

    svc.publish_change(&user_id.to_string());
    svc.publish_change("not a user id");

    assert_eq!(changes.try_recv().expect("the owner's id"), user_id);
    assert!(
        changes.try_recv().is_err(),
        "an unreadable payload is dropped"
    );
}
