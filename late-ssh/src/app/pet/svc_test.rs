use late_core::models::pet::{PetCompanion, PetMood, PetSpecies};
use late_core::test_utils::create_test_user;

use crate::app::pet::svc::PetService;
use crate::test_helpers::new_test_db;

#[tokio::test]
async fn ensure_pet_creates_a_sleeping_cat_for_a_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-svc-new").await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );

    let pet = svc.ensure_pet(user.id).await.expect("ensure pet");

    assert_eq!(pet.user_id, user.id);
    assert_eq!(pet.species(), PetSpecies::Cat);
    assert_eq!(pet.mood(), PetMood::Asleep);
}

#[tokio::test]
async fn ensure_pet_is_idempotent_across_reconnects() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-svc-reconnect").await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );

    let first = svc.ensure_pet(user.id).await.expect("first ensure");
    let second = svc.ensure_pet(user.id).await.expect("second ensure");

    assert_eq!(
        first.id, second.id,
        "reconnecting must return the same pet row, not create a new one"
    );
}

#[tokio::test]
async fn the_mood_and_the_species_land_on_the_row() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "pet-svc-mood").await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    svc.ensure_pet(user.id).await.expect("ensure pet");

    svc.set_mood(user.id, PetMood::Vibing)
        .await
        .expect("set mood");
    svc.set_species(user.id, PetSpecies::Bird)
        .await
        .expect("set species");

    let stored = PetCompanion::ensure(&client, user.id)
        .await
        .expect("companion");
    assert_eq!(stored.mood(), PetMood::Vibing);
    assert_eq!(stored.species(), PetSpecies::Bird);
}

#[tokio::test]
async fn petting_pays_an_owner_once_a_day() {
    use late_core::models::chips::UserChips;
    use late_core::models::marketplace::{PET_COMPANION_SKU, purchase_durable_item_by_sku};

    use crate::app::activity::event::{ActivityEvent, ActivityKind};
    use crate::app::pet::svc::{PET_CHIP_BONUS, PetOutcome};

    let test_db = new_test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "pet-svc-chips").await;
    let (tx, mut rx) = tokio::sync::broadcast::channel::<ActivityEvent>(16);
    let svc = PetService::new(test_db.db.clone(), tx);
    svc.ensure_pet(user.id).await.expect("ensure pet");
    let today = chrono::Utc::now().date_naive();

    assert_eq!(
        svc.pet_on(user.id, today).await.expect("pet without the unlock"),
        PetOutcome::NoPet
    );
    assert!(rx.try_recv().is_err(), "a pet nobody bought pays nothing");

    UserChips::admin_grant(&**client, user.id, 1_000_000)
        .await
        .expect("fund chips");
    purchase_durable_item_by_sku(&mut client, user.id, PET_COMPANION_SKU)
        .await
        .expect("buy the pet");
    let before = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;

    assert_eq!(
        svc.pet_on(user.id, today).await.expect("first pet"),
        PetOutcome::Petted
    );
    assert_eq!(
        svc.pet_on(user.id, today).await.expect("second pet"),
        PetOutcome::AlreadyPettedToday
    );
    let after = UserChips::ensure(&client, user.id)
        .await
        .expect("chips")
        .balance;
    assert_eq!(after - before, PET_CHIP_BONUS, "one payout a day");
    let event = rx.try_recv().expect("the first pet tells the session");
    assert!(matches!(event.kind, ActivityKind::PetPetted));
    assert!(rx.try_recv().is_err(), "and only the first");

    let tomorrow = today.succ_opt().expect("a tomorrow");
    assert_eq!(
        svc.pet_on(user.id, tomorrow).await.expect("next day"),
        PetOutcome::Petted,
        "a new day pays again"
    );
}
