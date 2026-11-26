use crate::{
    models::pet::{
        Fed, Form, PET_NAME_MAX_CHARS, PetCompanion, PetGrowthWrite, PetMood, PetSpecies, Pick,
        Stage, TreatEarned, TreatKind, Trick, VisitKind, normalize_pet_name,
    },
    test_utils::test_db,
};
use chrono::Utc;

#[test]
fn normalize_trims_and_collapses_whitespace() {
    assert_eq!(
        normalize_pet_name("  Whiskers  ").as_deref(),
        Some("Whiskers")
    );
    assert_eq!(
        normalize_pet_name("Mr   Mittens").as_deref(),
        Some("Mr Mittens")
    );
}

#[test]
fn normalize_caps_length_to_max() {
    let very_long = "a".repeat(200);
    let out = normalize_pet_name(&very_long).expect("non-empty");
    assert_eq!(out.chars().count(), PET_NAME_MAX_CHARS);
}

#[test]
fn normalize_returns_none_for_empty_or_whitespace_only() {
    assert!(normalize_pet_name("").is_none());
    assert!(normalize_pet_name("   ").is_none());
}

#[tokio::test]
async fn ensure_creates_a_sleeping_cat_for_a_new_user() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-new").await;

    let pet = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    assert_eq!(pet.user_id, user.id);
    assert_eq!(pet.species(), PetSpecies::Cat);
    assert_eq!(pet.mood(), PetMood::Asleep);
    assert_eq!(pet.name, None);
}

#[tokio::test]
async fn ensure_is_idempotent_and_keeps_the_mood() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-idem").await;

    let first = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    PetCompanion::set_mood(&client, user.id, PetMood::Proud)
        .await
        .expect("set mood");
    let second = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure again");

    assert_eq!(first.id, second.id);
    assert_eq!(
        second.mood(),
        PetMood::Proud,
        "re-ensuring must not wipe the inferred mood"
    );
}

#[tokio::test]
async fn the_mood_clock_moves_only_when_the_mood_does() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "cat-model-clock").await;
    PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");

    PetCompanion::set_mood(&client, user.id, PetMood::Chatty)
        .await
        .expect("chatty");
    let chatty = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    PetCompanion::set_mood(&client, user.id, PetMood::Chatty)
        .await
        .expect("chatty again");
    let still_chatty = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert_eq!(still_chatty.mood_since, chatty.mood_since);

    PetCompanion::set_mood(&client, user.id, PetMood::Vibing)
        .await
        .expect("vibing");
    let vibing = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert_eq!(vibing.mood(), PetMood::Vibing);
    assert!(vibing.mood_since > chatty.mood_since);
}

#[tokio::test]
async fn writes_are_scoped_to_the_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = crate::test_utils::create_test_user(&test_db.db, "cat-model-owner").await;
    let other = crate::test_utils::create_test_user(&test_db.db, "cat-model-other").await;

    let owner_pet = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("ensure owner pet");
    let other_pet = PetCompanion::ensure(&client, other.id)
        .await
        .expect("ensure other pet");
    assert_ne!(owner_pet.id, other_pet.id);

    PetCompanion::set_mood(&client, owner.id, PetMood::Sulking)
        .await
        .expect("owner sulks");
    let bird = PetCompanion::set_species(&**client, owner.id, PetSpecies::Bird)
        .await
        .expect("owner picks the bird");
    assert_eq!(
        (bird.species(), bird.revision),
        (PetSpecies::Bird, owner_pet.revision + 1),
        "a change of species is a write a mirror orders by"
    );

    let other_after = PetCompanion::ensure(&client, other.id)
        .await
        .expect("reload other pet");
    assert_eq!(other_after.id, other_pet.id);
    assert_eq!(other_after.mood(), PetMood::Asleep);
    assert_eq!(other_after.species(), PetSpecies::Cat);
    let owner_after = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("reload owner pet");
    assert_eq!(owner_after.mood(), PetMood::Sulking);
    assert_eq!(owner_after.species(), PetSpecies::Bird);
}

#[tokio::test]
async fn the_switch_starts_on_and_flips_only_the_owners_pet() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = crate::test_utils::create_test_user(&test_db.db, "pet-switch-owner").await;
    let other = crate::test_utils::create_test_user(&test_db.db, "pet-switch-other").await;
    let fresh = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("ensure owner pet");
    assert!(fresh.enabled, "a pet starts out");
    PetCompanion::ensure(&client, other.id)
        .await
        .expect("ensure other pet");

    PetCompanion::set_enabled(&client, owner.id, false)
        .await
        .expect("put away");
    let away = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("reload owner pet");
    assert!(!away.enabled, "the owner's pet is away");
    let other_after = PetCompanion::ensure(&client, other.id)
        .await
        .expect("reload other pet");
    assert!(other_after.enabled, "nobody else's pet moved");

    PetCompanion::set_enabled(&client, owner.id, true)
        .await
        .expect("bring out");
    let back = PetCompanion::ensure(&client, owner.id)
        .await
        .expect("reload owner pet");
    assert!(back.enabled, "the owner's pet is back out");
}

#[tokio::test]
async fn a_fresh_row_is_an_egg_and_a_stored_growth_bumps_the_revision() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "pet-model-grow").await;
    let fresh = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    assert_eq!(fresh.stage(), Stage::Egg);
    assert_eq!((fresh.days_together, fresh.revision), (0, 0));
    PetCompanion::set_name(&client, user.id, Some("Mochi"))
        .await
        .expect("name");

    let today = Utc::now().date_naive();
    let tx = client.transaction().await.expect("tx");
    PetCompanion::lock(&*tx, user.id).await.expect("lock");
    let stored = PetCompanion::store(
        &*tx,
        PetGrowthWrite {
            user_id: user.id,
            days_together: 8,
            last_day: Some(today),
            form: Form::Adult {
                top: Pick::Two,
                feet: Pick::One,
            },
            bond_days: 3,
            last_bond_day: Some(today),
            tricks: vec![Trick::Spin],
            learning: Some(Trick::Bow),
            learning_days: 2,
            last_practice: Some(today),
        },
    )
    .await
    .expect("store");
    tx.commit().await.expect("commit");

    assert_eq!(stored.revision, 1);
    assert_eq!(stored.stage(), Stage::Adult);
    assert_eq!(
        stored.form(),
        Form::Adult {
            top: Pick::Two,
            feet: Pick::One
        },
        "the picks come back in the order they were made"
    );
    assert_eq!(stored.tricks(), vec![Trick::Spin]);
    assert_eq!(stored.learning(), Some(Trick::Bow));
    assert_eq!(
        (stored.days_together, stored.bond_days, stored.learning_days),
        (8, 3, 2)
    );
    assert_eq!(
        stored.name.as_deref(),
        Some("Mochi"),
        "a growth write leaves the name alone"
    );
}

/// A stage is held with exactly the picks made on the way to it: the
/// typed read refuses anything else, and so does the row.
#[tokio::test]
async fn a_form_is_a_stage_with_its_picks_and_the_row_refuses_a_misfit() {
    let (one, two) = (Pick::One, Pick::Two);
    let elder = Form::Elder {
        top: one,
        feet: two,
        face: two,
    };
    assert_eq!(elder.stage(), Stage::Elder);
    assert_eq!(elder.picks(), vec![one, two, two]);
    assert_eq!(
        Form::from_parts(Stage::Elder, &[one, two, two]),
        Some(elder)
    );
    assert_eq!(Form::from_parts(Stage::Egg, &[]), Some(Form::Egg));
    assert_eq!(Form::from_parts(Stage::Baby, &[one]), None);
    assert_eq!(Form::from_parts(Stage::Adult, &[one]), None);

    // Each stage adds one pick; an egg hatches by no pick and an elder is
    // done.
    assert_eq!(Form::Egg.grown(two), None);
    assert_eq!(Form::Baby.grown(two), Some(Form::Young { top: two }));
    assert_eq!(
        Form::Young { top: two }.grown(one),
        Some(Form::Adult {
            top: two,
            feet: one
        })
    );
    assert_eq!(
        Form::Adult {
            top: one,
            feet: two
        }
        .grown(two),
        Some(elder)
    );
    assert_eq!(elder.grown(one), None);

    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = crate::test_utils::create_test_user(&test_db.db, "pet-model-form").await;
    let fresh = PetCompanion::ensure(&client, user.id)
        .await
        .expect("ensure");
    assert_eq!(fresh.form(), Form::Egg);
    for (stage, looks) in [
        ("young", "{}"),
        ("baby", "{one}"),
        ("elder", "{one,three,two}"),
    ] {
        let refused = client
            .execute(
                "UPDATE pet_companions SET stage = $2, looks = $3::text::text[] WHERE user_id = $1",
                &[&user.id, &stage, &looks],
            )
            .await;
        assert!(refused.is_err(), "{stage} with {looks} is no form");
    }
    // The same write with picks that fit goes through.
    client
        .execute(
            "UPDATE pet_companions SET stage = 'young', looks = '{two}' WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("a young pet with its one pick");
    let young = PetCompanion::ensure(&client, user.id)
        .await
        .expect("reload");
    assert_eq!(young.form(), Form::Young { top: Pick::Two });
}

#[tokio::test]
async fn one_treat_a_day_whatever_the_kind_and_one_waiting_at_a_time() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = crate::test_utils::create_test_user(&test_db.db, "pet-model-treat").await;
    let other = crate::test_utils::create_test_user(&test_db.db, "pet-model-treat-2").await;
    let today = Utc::now().date_naive();
    let tomorrow = today.succ_opt().expect("a tomorrow");

    let earn = |kind, day| PetCompanion::earn_treat(&**client, owner.id, kind, day);
    let last_fed = |user_id| PetCompanion::last_fed(&**client, user_id);
    assert_eq!(
        earn(TreatKind::Puzzle, today).await.expect("first"),
        TreatEarned::Earned
    );
    assert_eq!(
        earn(TreatKind::Door, today).await.expect("another kind"),
        TreatEarned::Waiting,
        "the day has its treat, and it still waits"
    );
    assert_eq!(
        earn(TreatKind::Door, tomorrow).await.expect("next day"),
        TreatEarned::Waiting,
        "a waiting treat refuses another, whatever the day"
    );
    assert_eq!(
        PetCompanion::waiting_treat(&**client, owner.id)
            .await
            .expect("pouch"),
        Some(TreatKind::Puzzle)
    );
    assert_eq!(
        PetCompanion::waiting_treat(&**client, other.id)
            .await
            .expect("other pouch"),
        None,
        "a pouch is its owner's"
    );

    assert_eq!(
        PetCompanion::give_treat(&**client, other.id, today)
            .await
            .expect("give from another pouch"),
        None,
        "nobody gives another owner's treat"
    );
    assert_eq!(
        PetCompanion::give_treat(&**client, owner.id, today)
            .await
            .expect("give"),
        Some(TreatKind::Puzzle)
    );
    assert_eq!(
        last_fed(owner.id).await.expect("fed"),
        Some(Fed {
            kind: TreatKind::Puzzle,
            on: today
        })
    );
    assert_eq!(
        last_fed(other.id).await.expect("other fed"),
        None,
        "what a pet ate is its owner's"
    );
    assert_eq!(
        PetCompanion::give_treat(&**client, owner.id, today)
            .await
            .expect("give again"),
        None,
        "nothing waits"
    );
    assert_eq!(
        earn(TreatKind::Door, today).await.expect("same day"),
        TreatEarned::AlreadyToday,
        "one treat a day, wherever it would come from"
    );
    assert_eq!(
        earn(TreatKind::Door, tomorrow).await.expect("next day"),
        TreatEarned::Earned
    );
    assert_eq!(
        last_fed(owner.id).await.expect("fed"),
        Some(Fed {
            kind: TreatKind::Puzzle,
            on: today
        }),
        "a treat still in the pouch is not eaten"
    );
    assert_eq!(
        PetCompanion::give_treat(&**client, owner.id, tomorrow)
            .await
            .expect("give the next day"),
        Some(TreatKind::Door)
    );
    assert_eq!(
        last_fed(owner.id).await.expect("fed"),
        Some(Fed {
            kind: TreatKind::Door,
            on: tomorrow
        }),
        "the latest one eaten"
    );
}

#[tokio::test]
async fn a_visit_is_claimed_once_a_day_and_a_pet_calls_once_a_day() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = crate::test_utils::create_test_user(&test_db.db, "pet-model-host").await;
    let second = crate::test_utils::create_test_user(&test_db.db, "pet-model-host-2").await;
    let visitor = crate::test_utils::create_test_user(&test_db.db, "pet-model-guest").await;
    let today = Utc::now().date_naive();
    let before = Utc::now() - chrono::Duration::minutes(1);

    let claim = |host, kind| PetCompanion::claim_visit(&**client, host, visitor.id, kind, today);
    assert!(claim(owner.id, VisitKind::Pat).await.expect("pat"));
    assert!(!claim(owner.id, VisitKind::Pat).await.expect("pat again"));
    assert!(claim(second.id, VisitKind::Pat).await.expect("another pet"));
    assert!(claim(owner.id, VisitKind::Call).await.expect("call"));
    assert!(
        !claim(second.id, VisitKind::Call)
            .await
            .expect("second call"),
        "one call a day, whoever it is on"
    );

    let came = PetCompanion::visitors_since(&**client, owner.id, before)
        .await
        .expect("visitors");
    assert_eq!(came.len(), 2);
    assert!(
        came.iter()
            .all(|visit| visit.visitor_id == visitor.id && visit.username == visitor.username)
    );
    assert!(
        PetCompanion::visitors_since(
            &**client,
            owner.id,
            Utc::now() + chrono::Duration::minutes(1)
        )
        .await
        .expect("seen")
        .is_empty(),
        "visits before the mark are seen"
    );
}

/// A call makes the caller's pet its host's guest, as it looks now, until
/// the window the reader asks for has passed. A pat brings nobody, and a
/// pet put away or back in its egg is no guest.
#[tokio::test]
async fn a_calling_pet_is_its_hosts_guest_as_it_looks_now() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let host = crate::test_utils::create_test_user(&test_db.db, "pet-guest-host").await;
    let other = crate::test_utils::create_test_user(&test_db.db, "pet-guest-other").await;
    let caller = crate::test_utils::create_test_user(&test_db.db, "pet-guest-caller").await;
    let patter = crate::test_utils::create_test_user(&test_db.db, "pet-guest-patter").await;
    for user in [&host, &other, &caller, &patter] {
        PetCompanion::ensure(&client, user.id)
            .await
            .expect("ensure");
    }
    let today = Utc::now().date_naive();
    let before = Utc::now() - chrono::Duration::minutes(1);
    let grown = |form| PetGrowthWrite {
        user_id: caller.id,
        days_together: 30,
        last_day: Some(today),
        form,
        bond_days: 0,
        last_bond_day: None,
        tricks: Vec::new(),
        learning: None,
        learning_days: 0,
        last_practice: None,
    };
    let adult = Form::Adult {
        top: Pick::Two,
        feet: Pick::One,
    };
    let tx = client.transaction().await.expect("tx");
    PetCompanion::store(&*tx, grown(adult)).await.expect("grow");
    PetCompanion::set_species(&*tx, caller.id, PetSpecies::Dog)
        .await
        .expect("species");
    tx.commit().await.expect("commit");
    PetCompanion::set_name(&client, caller.id, Some("Biscuit"))
        .await
        .expect("name");

    assert!(
        PetCompanion::claim_visit(&**client, host.id, patter.id, VisitKind::Pat, today)
            .await
            .expect("pat")
    );
    assert!(
        PetCompanion::guests_since(&**client, host.id, before)
            .await
            .expect("guests")
            .is_empty(),
        "a pat brings no pet over"
    );
    assert_eq!(
        PetCompanion::stay_since(&**client, caller.id, before)
            .await
            .expect("stay"),
        None
    );

    assert!(
        PetCompanion::claim_visit(&**client, host.id, caller.id, VisitKind::Call, today)
            .await
            .expect("call")
    );
    let guests = PetCompanion::guests_since(&**client, host.id, before)
        .await
        .expect("guests");
    assert_eq!(guests.len(), 1);
    assert_eq!(
        (
            guests[0].visitor_id,
            guests[0].username.as_str(),
            guests[0].species,
            guests[0].form,
            guests[0].name.as_deref()
        ),
        (
            caller.id,
            caller.username.as_str(),
            PetSpecies::Dog,
            adult,
            Some("Biscuit")
        )
    );
    let stay = PetCompanion::stay_since(&**client, caller.id, before)
        .await
        .expect("stay")
        .expect("the caller's pet is out");
    assert_eq!(
        (stay.host.as_str(), stay.since),
        (host.username.as_str(), guests[0].since)
    );

    // Only its host has it, and only inside the window asked for.
    assert!(
        PetCompanion::guests_since(&**client, other.id, before)
            .await
            .expect("another pet's guests")
            .is_empty()
    );
    assert_eq!(
        PetCompanion::stay_since(&**client, host.id, before)
            .await
            .expect("the host's own stay"),
        None,
        "having a guest is not being one"
    );
    let later = Utc::now() + chrono::Duration::minutes(1);
    assert!(
        PetCompanion::guests_since(&**client, host.id, later)
            .await
            .expect("after the window")
            .is_empty()
    );
    assert_eq!(
        PetCompanion::stay_since(&**client, caller.id, later)
            .await
            .expect("after the window"),
        None
    );

    // Put away, it is nobody's guest; out again it is back; reset to an
    // egg, it is gone.
    PetCompanion::set_enabled(&client, caller.id, false)
        .await
        .expect("away");
    assert!(
        PetCompanion::guests_since(&**client, host.id, before)
            .await
            .expect("put away")
            .is_empty()
    );
    PetCompanion::set_enabled(&client, caller.id, true)
        .await
        .expect("out");
    assert_eq!(
        PetCompanion::guests_since(&**client, host.id, before)
            .await
            .expect("out again")
            .len(),
        1
    );
    let tx = client.transaction().await.expect("tx");
    PetCompanion::store(&*tx, grown(Form::Egg))
        .await
        .expect("reset");
    tx.commit().await.expect("commit");
    assert!(
        PetCompanion::guests_since(&**client, host.id, before)
            .await
            .expect("an egg")
            .is_empty()
    );
}
