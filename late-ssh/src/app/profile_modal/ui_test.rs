//! The profile modal drawn for real: a user with a bio, a ledger, and a
//! gift, rendered into a test terminal at two sizes. One layout, so the
//! same sections must appear at both, in the same order; the narrow one
//! scrolls, and `/chips` lands on the ledger.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use late_core::models::chips::{ChipMove, UserChips};
use late_core::test_utils::create_test_user;
use ratatui::{Terminal, backend::TestBackend};
use tokio::sync::broadcast;
use tokio::time::{Duration, timeout};
use uuid::Uuid;

use crate::app::bonsai::svc::BonsaiService;
use crate::app::chat::showcase::svc::ShowcaseService;
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
    let (activity_tx, _activity_rx) = broadcast::channel(8);
    let mut state = ProfileModalState::new(
        profile_service,
        ShowcaseService::new(db.clone()),
        BonsaiService::new(db.clone(), activity_tx),
    );
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
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw(frame, frame.area(), state))
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

    // The hero: the name heads the grid, and the grid names the facts.
    let name = row_of(&lines, "wide-viewed").expect("name in the grid");
    for key in [
        "country", "chips", "created", "ide", "os", "terminal", "theme", "langs",
    ] {
        assert!(
            row_of(&lines, &format!("{key:<10}")).is_some(),
            "{key} missing from the late.fetch grid:\n{text}"
        );
    }
    assert!(
        lines[row_of(&lines, "chips     ").unwrap()].contains("this month"),
        "the chips fact carries the board figure:\n{text}"
    );

    // Then the sections, in the order the design fixes.
    let bio = row_of(&lines, "bio ─").expect("bio heading");
    let chips = row_of(&lines, "chips ─").expect("chips heading");
    assert!(name < bio && bio < chips, "hero, bio, chips:\n{text}");
    assert!(
        lines[bio + 1].contains("Not set"),
        "an empty bio says so under its heading:\n{text}"
    );

    // The ledger: newest first, the gift naming its sender and marked off
    // the board, the quest row unmarked, the stipend last.
    let gift = row_of(&lines, "gift received").expect("gift row");
    let quest = row_of(&lines, "quest reward").expect("quest row");
    let stipend = row_of(&lines, "starting chips").expect("stipend row");
    assert!(gift < quest && quest < stipend, "newest first:\n{text}");
    assert!(lines[gift].contains("from @wide-friend"), "{}", lines[gift]);
    assert!(lines[gift].contains("  off"), "{}", lines[gift]);
    assert!(!lines[quest].contains("  off"), "{}", lines[quest]);
    assert!(lines[gift].contains("+300") && lines[quest].contains("+500"));

    // The summary agrees with the board rule: the gift is out, the rest in.
    // 1,000 stipend + 12 x 500 quests + 300 gift; the gift is off the board.
    let summary = row_of(&lines, "balance 7,300").expect("balance");
    assert!(
        lines[summary].contains("this month +7,000"),
        "{}",
        lines[summary]
    );
}

#[tokio::test]
async fn a_narrow_terminal_scrolls_the_same_column() {
    let fixture = fixture("narrow").await;
    // Too short for the whole column: the top is the hero, and the ledger
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
