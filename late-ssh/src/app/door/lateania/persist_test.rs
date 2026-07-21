use crate::app::door::lateania::classes::Class;
use crate::app::door::lateania::persist::*;
use crate::app::door::lateania::stats::AbilityScores;
use uuid::Uuid;

#[test]
fn round_trips_through_json() {
    let scores = AbilityScores {
        dexterity: 16,
        ..Default::default()
    };
    let c = SavedCharacter::new_for(SavedCharacterInit {
        class: Some(Class::Rogue),
        xp: 1234,
        level: 7,
        gold: 560,
        banked_gold: 1400,
        hp: 42,
        room: 18,
        visited: vec![1, 5, 18],
        inventory: vec![1300, 1301],
        equipped: vec![("weapon".to_string(), 1004)],
        scores,
        titles: vec!["Wyrmbane".to_string()],
        title_levels: vec![12],
        active_title: Some(0),
        completed_quests: vec![2],
        board_progress: vec![(4, 2)],
        board_done: vec![1],
        quest_cooldowns: vec![(1, 1_700_000_000)],
        archetype: Some("assassin".to_string()),
        pet: Some("dire_wolf".to_string()),
        pet_loyalty: 250,
        owned_plot: Some(3),
        house_furniture: vec![(9040, "feather_bed".to_string())],
        appearance: vec![1, 2, 3, 4, 5],
        skills: vec![("woodcutting".to_string(), 900), ("mining".to_string(), 40)],
        craft_skills: vec![("smithing".to_string(), 300)],
        taming_xp: 1500,
    });
    let json = c.to_json();
    let back = SavedCharacter::from_json(&json).expect("parses");
    assert_eq!(back.class(), Some(Class::Rogue));
    assert_eq!(back.xp, 1234);
    assert_eq!(back.level, 7);
    assert_eq!(back.gold, 560);
    assert_eq!(back.banked_gold, 1400);
    assert_eq!(back.visited, vec![1, 5, 18]);
    assert_eq!(back.inventory, vec![1300, 1301]);
    assert_eq!(back.equipped, vec![("weapon".to_string(), 1004)]);
    assert_eq!(back.scores.dexterity, 16);
    assert_eq!(back.titles, vec!["Wyrmbane".to_string()]);
    assert_eq!(back.board_progress, vec![(4, 2)]);
    assert_eq!(back.board_done, vec![1]);
    assert_eq!(back.quest_cooldowns, vec![(1, 1_700_000_000)]);
    assert_eq!(back.archetype.as_deref(), Some("assassin"));
    assert_eq!(back.pet.as_deref(), Some("dire_wolf"));
    assert_eq!(back.pet_loyalty, 250);
    assert_eq!(back.owned_plot, Some(3));
    assert_eq!(
        back.house_furniture,
        vec![(9040, "feather_bed".to_string())]
    );
    assert_eq!(back.appearance, vec![1, 2, 3, 4, 5]);
    assert_eq!(
        back.skills,
        vec![("woodcutting".to_string(), 900), ("mining".to_string(), 40)]
    );
    assert_eq!(back.craft_skills, vec![("smithing".to_string(), 300)]);
    assert_eq!(back.taming_xp, 1500);
}

#[test]
fn empty_blob_is_treated_as_no_save() {
    assert!(SavedCharacter::from_json(&serde_json::json!({})).is_none());
    assert!(SavedCharacter::from_json(&serde_json::Value::Null).is_none());
}

#[test]
fn missing_fields_fall_back_to_defaults() {
    // A minimal/old blob with only a class should still load.
    let json = serde_json::json!({ "class": "mage" });
    let c = SavedCharacter::from_json(&json).expect("parses partial");
    assert_eq!(c.class(), Some(Class::Mage));
    assert_eq!(c.level, 1);
    assert_eq!(c.gold, 0);
    assert_eq!(c.banked_gold, 0);
    assert_eq!(c.room, 1);
    assert!(c.visited.is_empty());
    assert!(c.inventory.is_empty());
}

#[test]
fn world_state_round_trips_through_json() {
    let owner = Uuid::nil();
    let world = SavedWorld::new(
        vec![SavedMob {
            id: 42,
            hp: 3,
            alive: false,
            respawn_remaining_secs: Some(17),
        }],
        vec![SavedMobStun {
            mob_id: 42,
            remaining_ticks: 2,
        }],
        vec![SavedMobDot {
            mob_id: 42,
            owner,
            damage: 5,
            remaining_ticks: 3,
        }],
    );
    let json = world.to_json();
    let back = SavedWorld::from_json(&json).expect("parses");
    assert_eq!(back.mobs[0].id, 42);
    assert_eq!(back.mobs[0].respawn_remaining_secs, Some(17));
    assert_eq!(back.mob_stuns[0].remaining_ticks, 2);
    assert_eq!(back.mob_dots[0].owner, owner);
}
