use late_core::models::pet::{PetCompanion, PetMood, PetSpecies};
use late_core::test_utils::create_test_user;

use crate::app::pet::svc::PetService;
use crate::test_helpers::new_test_db;

#[tokio::test]
async fn ensure_pet_creates_a_sleeping_cat_for_a_new_user() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-svc-new").await;
    let svc = PetService::new(test_db.db.clone());

    let pet = svc.ensure_pet(user.id).await.expect("ensure pet");

    assert_eq!(pet.user_id, user.id);
    assert_eq!(pet.species(), PetSpecies::Cat);
    assert_eq!(pet.mood(), PetMood::Asleep);
}

#[tokio::test]
async fn ensure_pet_is_idempotent_across_reconnects() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-svc-reconnect").await;
    let svc = PetService::new(test_db.db.clone());

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
    let svc = PetService::new(test_db.db.clone());
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
