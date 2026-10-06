use serde_json::json;

use super::data::{Building, Job, Resource};
use super::model::Builder;
use super::persist;

#[test]
fn a_village_era_save_still_loads() {
    // A blob written before the trading post and the workshop existed: no
    // `seen_crafts`, no `seen_trades`, and none of the new stores.
    let blob = json!({
        "schema_version": 1,
        "game": {
            "stores": { "wood": 120, "fur": 30, "cured_meat": 4 },
            "carry": { "wood": 0.5 },
            "buildings": { "hut": 2, "trading_post": 1 },
            "workers": { "hunter": 1 },
            "population": 5,
            "seen_buildings": ["trap", "hut", "trading_post"],
            "seen_jobs": ["hunter"],
            "forest_unlocked": true,
            "seen_forest": true,
            "fire": "burning",
            "temperature": "warm",
            "builder": "helping",
            "last_settled": 1_800_000_000_i64
        }
    });

    let game = persist::from_json(&blob);
    assert_eq!(game.store(Resource::Wood), 120);
    assert_eq!(game.store(Resource::CuredMeat), 4);
    assert_eq!(game.building_count(Building::TradingPost), 1);
    assert_eq!(game.worker_count(Job::Hunter), 1);
    assert_eq!(game.population, 5);
    assert_eq!(game.builder, Builder::Helping);
    assert!(
        game.seen_crafts.is_empty() && game.seen_trades.is_empty(),
        "the new offer sets default to empty rather than failing the load"
    );
    assert_eq!(
        game.store(Resource::Iron),
        0,
        "a store the save never knew about reads as none"
    );
}
