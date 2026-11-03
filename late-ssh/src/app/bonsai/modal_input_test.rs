use late_core::test_utils::create_test_user;

use crate::app::bonsai::state::BranchStatus;
use crate::app::state::App;
use crate::test_helpers::{make_app, new_test_db, wait_for_app};

fn selected_tip_bend(app: &App) -> Option<(BranchStatus, i8, i8)> {
    let id = app.bonsai.tree.selected_branch_id?;
    let branch = app.bonsai.tree.graph.branch(id)?;
    Some((branch.status, branch.bend_x, branch.bend_y))
}

async fn press_and_wait_for_wire(app: &mut App, key: &[u8], direction: &str) {
    let message = format!("Wire set: future growth will lean {direction}");
    app.handle_input(key);
    wait_for_app(app, &message, |app| {
        app.bonsai.tree.message.as_deref() == Some(message.as_str())
    })
    .await;
}

// The two upward diagonals have their own keys, through the real input path
// and the service, and the wire lands on the stored tree.
#[tokio::test]
async fn y_and_u_wire_the_selected_tip_up_left_and_up_right() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "bonsai-diagonal-steer").await;
    let mut app = make_app(test_db.db.clone(), user.id, "bonsai-diagonal-steer-flow");
    app.show_bonsai_modal = true;

    // A fresh tree is only the trunk, which will not wire: water grows a tip.
    app.handle_input(b"w");
    wait_for_app(&mut app, "the first watering", |app| {
        app.bonsai
            .tree
            .message
            .as_deref()
            .is_some_and(|message| message.starts_with("Watered (+"))
    })
    .await;
    app.bonsai.tree.cycle_selection(1);

    press_and_wait_for_wire(&mut app, b"y", "up-left").await;
    assert_eq!(selected_tip_bend(&app), Some((BranchStatus::Wired, -1, 1)));

    press_and_wait_for_wire(&mut app, b"u", "up-right").await;
    assert_eq!(selected_tip_bend(&app), Some((BranchStatus::Wired, 1, 1)));
}
