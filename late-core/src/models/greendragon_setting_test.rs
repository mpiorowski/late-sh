use crate::{models::greendragon_setting::GreenDragonSetting, test_utils::test_db};

const KEY: &str = "fivesix_jackpot";

#[tokio::test]
async fn five_sixes_settles_each_throw_against_the_shared_pot() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    // (sixes, pot after the stake, pot left after the payout). The seed is 100
    // and every stake is 10, so each row starts from the one above it.
    let throws = [
        (0, 110, 110),
        (2, 120, 120),
        (3, 130, 123), // 5% of 130 rounds up to 7
        (4, 133, 120), // 10% of 133 rounds down to 13
        (5, 130, 100), // the jackpot takes all of it and the pot resets
    ];

    for (sixes, pot, left_over) in throws {
        let settled = GreenDragonSetting::settle_fivesix(&client, 10, 1_000, sixes)
            .await
            .expect("settle");
        assert_eq!(settled, (pot, left_over), "{sixes} sixes");
        let stored = GreenDragonSetting::get(&client, KEY).await.expect("read");
        assert_eq!(stored, Some(left_over), "{sixes} sixes");
    }
}

#[tokio::test]
async fn five_sixes_caps_the_pot_at_the_house_maximum() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let settled = GreenDragonSetting::settle_fivesix(&client, 10, 105, 0)
        .await
        .expect("settle");

    assert_eq!(settled, (105, 105));
}
