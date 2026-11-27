use late_core::models::pet::{Form, PetVisitor, Pick, VisitKind};
use late_core::test_utils::create_test_user;
use ratatui::{Terminal, backend::TestBackend, layout::Rect};

use super::draw;
use crate::app::pet::session::PetState;
use crate::app::pet::svc::PetService;
use crate::app::pet::ui::PetView;
use crate::test_helpers::{new_test_db, pet_grown_to};

/// The care box as `width` by 40 cells of text, one string a row.
fn care_box(state: &PetState, width: u16) -> Vec<String> {
    let area = Rect::new(0, 0, width, 40);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal
        .draw(|frame| {
            draw(
                frame,
                area,
                &PetView {
                    state,
                    pet_rect_slot: None,
                    frame_slot: None,
                },
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// A long name and a crowd of visitors flow onto more rows under their
/// label instead of running past the border, and the box grows to hold
/// them with its key row still showing.
#[tokio::test]
async fn long_care_rows_wrap_inside_the_box() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "pet-modal-wrap").await;
    let svc = PetService::new(
        test_db.db.clone(),
        tokio::sync::broadcast::channel::<crate::app::activity::event::ActivityEvent>(16).0,
    );
    let pet = svc.bootstrap(user.id).await.expect("bootstrap");
    let mut state = PetState::new(
        user.id,
        svc,
        Some(pet_grown_to(
            pet,
            Form::Adult {
                top: Pick::One,
                feet: Pick::One,
            },
        )),
    );
    state.name = Some("Sir Reginald Whiskerton".to_string());
    state.care.came_by = ["puddington", "nebuchadnezzar", "marmalade_queen", "x", "y"]
        .into_iter()
        .enumerate()
        .map(|(at, username)| PetVisitor {
            visitor_id: uuid::Uuid::now_v7(),
            username: username.to_string(),
            kind: match at {
                1 => VisitKind::Call,
                _ => VisitKind::Pat,
            },
            at: chrono::Utc::now(),
        })
        .collect();

    for width in [76, 60] {
        let rows = care_box(&state, width);
        let plain = rows.join("\n");
        for shown in [
            "puddington petted it",
            "nebuchadnezzar's pet called",
            "marmalade_queen petted it",
            "and 2 more",
            "/petname <name>",
            "/pet reset",
            "q close",
        ] {
            assert!(
                plain.contains(shown),
                "{width} wide shows {shown:?} whole:\n{plain}"
            );
        }
        // A row that wraps carries on under its own value column.
        let column = |name: &str| {
            rows.iter()
                .find_map(|row| row.find(name).map(|at| row[..at].chars().count()))
                .expect("named")
        };
        assert_eq!(
            column("puddington"),
            column("marmalade_queen"),
            "{width} wide:\n{plain}"
        );
    }
}
