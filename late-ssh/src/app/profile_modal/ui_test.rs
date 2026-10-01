//! The profile modal drawn for real: a user with a bio, a ledger, and a
//! gift, rendered into a test terminal at two sizes. One layout, so the
//! same sections must appear at both, in the same order; the narrow one
//! scrolls, and `/chips` lands on the ledger.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use late_core::models::chips::{ChipMove, UserChips};
use late_core::test_utils::create_test_user;
use ratatui::{Terminal, backend::TestBackend};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use crate::app::profile::svc::ProfileService;
use crate::test_helpers::new_test_db;

use super::state::ProfileModalState;
use super::ui::draw;

struct Fixture {
    _test_db: late_core::test_utils::TestDb,
    state: ProfileModalState,
    user_id: Uuid,
}

/// A user with the stipend, a dozen earned rows, and one gift received,
/// with the modal open on them and the snapshot already drained.
async fn fixture(slug: &str) -> Fixture {
    let test_db = new_test_db().await;
    let db = test_db.db.clone();
    let client = db.get().await.expect("db client");
    let user = create_test_user(&db, &format!("{slug}-viewed")).await;
    let friend = create_test_user(&db, &format!("{slug}-friend")).await;

    UserChips::ensure(&client, user.id).await.expect("chips");
    // A dozen earned rows: enough ledger that the chips section is taller
    // than a short terminal, so the `/chips` jump has somewhere to go.
    for index in 0..12 {
        UserChips::apply(
            &**client,
            user.id,
            ChipMove::QuestReward,
            500,
            &format!("assignment-{index}"),
        )
        .await
        .expect("quest")
        .expect("credited");
    }
    drop(client);
    {
        let mut client = db.get().await.expect("db client");
        let tx = client.transaction().await.expect("tx");
        UserChips::transfer_gift(&tx, friend.id, user.id, 300)
            .await
            .expect("gift")
            .expect("affordable");
        tx.commit().await.expect("commit");
    }

    let profile_service = ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new())));
    let mut snapshot_rx = profile_service.subscribe_snapshot(user.id);
    let mut state = ProfileModalState::new(profile_service);
    state.open(user.id, user.username.clone());
    timeout(Duration::from_secs(5), async {
        loop {
            snapshot_rx.changed().await.expect("watch open");
            if snapshot_rx.borrow().profile.is_some() {
                break;
            }
        }
    })
    .await
    .expect("profile snapshot");
    assert!(state.tick(), "the open modal drains the snapshot");

    Fixture {
        _test_db: test_db,
        state,
        user_id: user.id,
    }
}

fn render(state: &ProfileModalState, width: u16, height: u16) -> Vec<String> {
    render_as(state, width, height, false)
}

fn render_as(
    state: &ProfileModalState,
    width: u16,
    height: u16,
    viewer_is_runner: bool,
) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw(frame, frame.area(), state, 0, viewer_is_runner))
        .expect("draw");
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect()
}

fn row_of(lines: &[String], needle: &str) -> Option<usize> {
    lines.iter().position(|line| line.contains(needle))
}

#[tokio::test]
async fn a_wide_terminal_shows_every_section_in_order() {
    let fixture = fixture("wide").await;
    let lines = render(&fixture.state, 130, 60);
    let text = lines.join("\n");

    // late.fetch: a heading over the grid naming the facts.
    let heading = row_of(&lines, "late.fetch ─").expect("late.fetch heading");
    // The name is the modal's title and nothing else: not repeated in the grid.
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("wide-viewed"))
            .count(),
        1,
        "{text}"
    );
    let name = row_of(&lines, "country   ").expect("first grid row");
    assert!(heading < name, "{text}");
    for key in [
        "country", "chips", "created", "member", "ide", "os", "terminal", "theme", "langs",
    ] {
        assert!(
            row_of(&lines, &format!("{key:<10}")).is_some(),
            "{key} missing from the late.fetch grid:\n{text}"
        );
    }
    let chips_fact = &lines[row_of(&lines, "chips     ").unwrap()];
    assert!(
        chips_fact.contains("7,300") && !chips_fact.contains("this month"),
        "the chips fact is the balance alone:\n{text}"
    );

    // Then the sections, in the order the design fixes.
    let bio = row_of(&lines, "bio ─").expect("bio heading");
    let bonsai = row_of(&lines, "bonsai ─").expect("bonsai heading");
    let chips = row_of(&lines, "chips ─").expect("chips heading");
    assert!(
        name < bio && bio < bonsai && bonsai < chips,
        "late.fetch, bio, bonsai, chips:\n{text}"
    );
    assert!(
        lines[bio + 1].contains("Not set"),
        "an empty bio says so under its heading:\n{text}"
    );

    // The ledger: newest first, the gift naming its sender, the stipend last.
    let gift = row_of(&lines, "gift received").expect("gift row");
    let quest = row_of(&lines, "quest reward").expect("quest row");
    let stipend = row_of(&lines, "starting chips").expect("stipend row");
    assert!(gift < quest && quest < stipend, "newest first:\n{text}");
    assert!(lines[gift].contains("from @wide-friend"), "{}", lines[gift]);
    assert!(lines[gift].contains("+300") && lines[quest].contains("+500"));

    // The summary agrees with the board rule: the quests count, the gift
    // and the stipend do not. 1,000 stipend + 12 x 500 quests + 300 gift,
    // and the net is all of it.
    let summary = row_of(&lines, "balance 7,300").expect("balance");
    assert!(
        lines[summary].contains("this month +6,000  ·  net +7,300"),
        "{}",
        lines[summary]
    );
}

/// A standing runner's profile grows a runner column beside the late.fetch
/// grid, for a runner looking: its own heading on the grid's heading row
/// carrying the level badge, then the face beside one key column (the
/// signal and exp bars, bits, the kit, the tally), no frame anywhere. A narrow body makes it a section under
/// the grid, still above the bio. A civilian looking sees the profile they
/// always did, and a runner who left has nothing to show.
#[tokio::test]
async fn a_runners_profile_shows_the_runner_to_runners_only() {
    use crate::app::deadchannel::runner::state::Look;
    use late_core::models::deadchannel_runner::DeadchannelRunner;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    let fixture = fixture("runner").await;
    let lines = render_as(&fixture.state, 130, 60, true);
    assert!(
        row_of(&lines, "signal  ").is_none(),
        "no runner column for a civilian:\n{}",
        lines.join("\n")
    );

    // The viewed user joins the row: the next open of their profile
    // carries the runner.
    let db = fixture._test_db.db.clone();
    let client = db.get().await.expect("db client");
    let mut rng = StdRng::seed_from_u64(7);
    let look = Look::random(1, &mut rng);
    DeadchannelRunner::ensure_for_user(&client, fixture.user_id, &look.to_json())
        .await
        .expect("runner row");
    let profile_service = ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new())));
    let mut snapshot_rx = profile_service.subscribe_snapshot(fixture.user_id);
    let mut state = ProfileModalState::new(profile_service);
    state.open(fixture.user_id, "runner-viewed".to_string());
    timeout(Duration::from_secs(5), async {
        loop {
            snapshot_rx.changed().await.expect("watch open");
            if snapshot_rx.borrow().runner.is_some() {
                break;
            }
        }
    })
    .await
    .expect("runner snapshot");
    assert!(state.tick());

    // Wide: the runner's heading shares the late.fetch heading's row, the
    // rows run beside the grid, and all of it sits above the bio.
    let lines = render_as(&state, 130, 60, true);
    let text = lines.join("\n");
    let heading = row_of(&lines, &format!("runner {}1 ─", look.mark)).expect("runner heading");
    assert!(
        lines[heading].contains("late.fetch ─"),
        "one row, two headings:\n{text}"
    );
    assert!(
        lines[heading + 1].contains("country   "),
        "beside the grid:\n{text}"
    );
    let face = look.rows().map(|worn| worn.piece.row);
    let expected = [
        format!("{}  signal  ████████████ 10/10", face[0]),
        format!("{}  exp     ░░░░░░░░░░░░ 0/100", face[1]),
        format!("{}  bits    50", face[2]),
        "       weapon  bare hands".to_string(),
        "       armor   street clothes".to_string(),
        "       glyphs  0 down".to_string(),
    ];
    for (offset, expected) in expected.iter().enumerate() {
        assert!(
            lines[heading + 1 + offset].contains(expected),
            "{expected}:\n{text}"
        );
    }
    let bio = row_of(&lines, "bio ─").expect("bio heading");
    assert!(
        heading + expected.len() < bio,
        "the runner, then bio:\n{text}"
    );
    assert!(
        row_of(&lines, "level   ").is_none(),
        "the level is the badge, not a row:\n{text}"
    );

    // Narrow: the same rows as a section under the grid, above the bio.
    let lines = render_as(&state, 70, 80, true);
    let text = lines.join("\n");
    let langs = row_of(&lines, "langs     ").expect("last grid row");
    let heading = row_of(&lines, &format!("runner {}1 ─", look.mark)).expect("runner heading");
    let bio = row_of(&lines, "bio ─").expect("bio heading");
    assert!(
        langs < heading && heading < bio,
        "grid, runner, bio:\n{text}"
    );
    assert!(
        lines[heading + 1].contains("signal  ████████████ 10/10"),
        "{text}"
    );

    // The same profile to a civilian: the row is not theirs to see.
    let lines = render_as(&state, 130, 60, false);
    assert!(row_of(&lines, "signal  ").is_none(), "{}", lines.join("\n"));
}

/// A profile with a tank and a pet draws them as one row on a wide body:
/// the reef under its heading, the pet under its own beside it, name and
/// mood on a row each. A narrow body keeps them two sections, pet first.
#[tokio::test]
async fn a_pet_sits_beside_its_owners_reef() {
    use late_core::models::marketplace::{
        AQUARIUM_SKU, PET_COMPANION_SKU, purchase_durable_item_by_sku,
    };

    let fixture = fixture("reef").await;
    let db = fixture._test_db.db.clone();
    {
        let mut client = db.get().await.expect("db client");
        UserChips::admin_grant(&**client, fixture.user_id, 1_000_000)
            .await
            .expect("fund chips");
        // The tank arrives with a welcome fry swimming, the pet with its row.
        for sku in [AQUARIUM_SKU, PET_COMPANION_SKU] {
            purchase_durable_item_by_sku(&mut client, fixture.user_id, sku)
                .await
                .expect("purchase")
                .expect("affordable");
        }
    }
    let profile_service = ProfileService::new(db.clone(), Arc::new(Mutex::new(HashMap::new())));
    let mut snapshot_rx = profile_service.subscribe_snapshot(fixture.user_id);
    let mut state = ProfileModalState::new(profile_service);
    state.open(fixture.user_id, "reef-viewed".to_string());
    timeout(Duration::from_secs(5), async {
        loop {
            snapshot_rx.changed().await.expect("watch open");
            if snapshot_rx.borrow().pet.is_some() {
                break;
            }
        }
    })
    .await
    .expect("pet snapshot");
    assert!(state.tick());
    let (species, mood) = {
        let snapshot = snapshot_rx.borrow();
        let pet = snapshot.pet.as_ref().expect("pet");
        assert!(!snapshot.aquarium_fish.is_empty(), "the welcome fry swims");
        (pet.species.as_str(), pet.mood.as_str())
    };
    // Columns, not bytes: the rules are multi-byte.
    let col_of = |line: &str, needle: &str| -> Option<usize> {
        line.find(needle).map(|at| line[..at].chars().count())
    };
    let cells = |line: &str, from: usize, len: usize| -> String {
        line.chars().skip(from).take(len).collect()
    };

    // Wide: one heading row for both, the pet's rows inside the band's
    // eleven, and the reef stopping short of the pet's column.
    let lines = render_as(&state, 130, 70, false);
    let text = lines.join("\n");
    let heading = row_of(&lines, "aquarium ─").expect("aquarium heading");
    let pet_col = col_of(&lines[heading], "pet ─").expect("pet heading on the reef's row");
    assert_eq!(
        lines.iter().filter(|line| line.contains("pet ─")).count(),
        1,
        "{text}"
    );
    let band = &lines[heading + 1..heading + 12];
    let name = band
        .iter()
        .position(|line| cells(line, pet_col, species.len() + 1).trim_end() == species)
        .unwrap_or_else(|| panic!("the pet's name under its heading:\n{text}"));
    assert_eq!(
        cells(&band[name + 1], pet_col, mood.len() + 1).trim_end(),
        mood,
        "the mood under the name:\n{text}"
    );
    assert!(name >= 3, "three art rows above the name:\n{text}");
    for row in band {
        assert_eq!(
            cells(row, pet_col - 3, 3),
            "   ",
            "the reef keeps out of the gap:\n{text}"
        );
    }
    assert!(
        !cells(&band[0], pet_col - 10, 7).trim().is_empty(),
        "the reef's surface runs up to the gap:\n{text}"
    );

    // Narrow: two sections, the pet's above the reef's, one caption row.
    let lines = render_as(&state, 70, 70, false);
    let text = lines.join("\n");
    let pet = row_of(&lines, "pet ─").expect("pet heading");
    let reef = row_of(&lines, "aquarium ─").expect("aquarium heading");
    assert!(pet < reef, "pet, then aquarium:\n{text}");
    assert!(
        lines[pet + 4].contains(&format!("{species} · {mood}")),
        "{text}"
    );
}

#[tokio::test]
async fn a_narrow_terminal_scrolls_the_same_column() {
    let fixture = fixture("narrow").await;
    // Too short for the whole column: the top is late.fetch, and the ledger
    // is below the fold.
    let top = render(&fixture.state, 70, 18);
    assert!(
        row_of(&top, "narrow-viewed").is_some(),
        "{}",
        top.join("\n")
    );
    assert!(
        row_of(&top, "chips ─").is_none(),
        "the ledger starts below the fold:\n{}",
        top.join("\n")
    );
    assert!(
        row_of(&top, "j/k").is_some(),
        "a scrollable body shows the scroll hint:\n{}",
        top.join("\n")
    );

    // `/chips` lands on the ledger heading as soon as the body is measured.
    fixture.state.jump_to_chips();
    let jumped = render(&fixture.state, 70, 18);
    let heading = row_of(&jumped, "chips ─").expect("chips heading in view");
    let border = row_of(&jumped, "profile ·").expect("title border");
    // Border, breathing row, the section's own blank row, then the heading.
    assert_eq!(
        heading,
        border + 3,
        "the chips heading is at the top of the viewport:\n{}",
        jumped.join("\n")
    );
    assert!(
        row_of(&jumped, "gift received").is_some(),
        "{}",
        jumped.join("\n")
    );

    // Scrolling is clamped to the content: the bottom row is the last
    // ledger row, never blank space past it.
    fixture.state.scroll_by(500);
    let bottom = render(&fixture.state, 70, 18);
    assert!(
        row_of(&bottom, "starting chips").is_some(),
        "the stipend row closes the column:\n{}",
        bottom.join("\n")
    );
    fixture.state.scroll_to_top();
    let again = render(&fixture.state, 70, 18);
    assert_eq!(again, top, "g returns to the same first page");
    let _ = fixture.user_id;
}
