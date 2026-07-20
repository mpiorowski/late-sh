use super::*;
use late_core::db::{Db, DbConfig};
use tokio::sync::broadcast;
use uuid::Uuid;

fn solved_state() -> State {
    let (activity_feed, _) = broadcast::channel(1);
    let svc = RubiksCubeService::new(
        Db::new(&DbConfig::default()).expect("test db pool"),
        activity_feed,
    );
    State {
        user_id: Uuid::now_v7(),
        stickers: solved_stickers(),
        user_moves: 0,
        view: CubeView::default(),
        puzzle_date: NaiveDate::from_ymd_opt(2026, 6, 18).unwrap(),
        solved_reported: true,
        reset_pending: false,
        message: String::new(),
        svc,
    }
}

#[test]
fn sticker_string_round_trips() {
    let mut state = solved_state();
    state.apply_move(CubeMove {
        face: Face::Right,
        inverse: false,
    });
    state.apply_move(CubeMove {
        face: Face::Up,
        inverse: true,
    });
    let encoded = stickers_to_string(&state.stickers);
    assert_eq!(encoded.len(), 54);
    assert_eq!(stickers_from_string(&encoded), Some(state.stickers));
}

#[test]
fn sticker_string_rejects_bad_input() {
    assert_eq!(stickers_from_string(""), None);
    assert_eq!(stickers_from_string(&"W".repeat(53)), None);
    assert_eq!(stickers_from_string(&"W".repeat(55)), None);
    assert_eq!(stickers_from_string(&"X".repeat(54)), None);
}

#[test]
fn four_turns_restore_cube() {
    for face in FACES {
        let mut state = solved_state();
        for _ in 0..4 {
            state.apply_move(CubeMove {
                face,
                inverse: false,
            });
        }
        assert!(state.is_solved(), "{face:?} did not restore");
    }
}

#[test]
fn move_and_inverse_restore_cube() {
    for face in FACES {
        let mut state = solved_state();
        state.apply_move(CubeMove {
            face,
            inverse: false,
        });
        state.apply_move(CubeMove {
            face,
            inverse: true,
        });
        assert!(state.is_solved(), "{face:?} inverse did not restore");
    }
}

#[test]
fn view_arrows_rotate_in_requested_direction() {
    let view = CubeView::default();
    assert_eq!(
        view.turned(ViewTurn::Right).visible_faces(),
        (Face::Up, Face::Right, Face::Back)
    );
    assert_eq!(
        view.turned(ViewTurn::Left).visible_faces(),
        (Face::Up, Face::Left, Face::Front)
    );
    assert_eq!(
        view.turned(ViewTurn::Up).visible_faces(),
        (Face::Back, Face::Up, Face::Right)
    );
    assert_eq!(
        view.turned(ViewTurn::Down).visible_faces(),
        (Face::Front, Face::Down, Face::Right)
    );
}

#[test]
fn resolve_face_follows_the_view() {
    // Default view: slots map straight onto their like-named faces.
    let view = CubeView::default();
    for slot in FACES {
        assert_eq!(view.resolve_face(slot), slot, "default {slot:?}");
    }

    // After turning right, the old right face is now the front slot, so the
    // viewer-relative `f` control acts on it instead of the absolute front.
    let turned = view.turned(ViewTurn::Right);
    let (top, front, right) = turned.visible_faces();
    assert_eq!(turned.resolve_face(Face::Up), top);
    assert_eq!(turned.resolve_face(Face::Front), front);
    assert_eq!(turned.resolve_face(Face::Right), right);
    assert_eq!(turned.resolve_face(Face::Down), opposite(top));
    assert_eq!(turned.resolve_face(Face::Back), opposite(front));
    assert_eq!(turned.resolve_face(Face::Left), opposite(right));
}

#[test]
fn net_slots_are_pinned_to_the_view() {
    // The front slot is always labeled F regardless of which face occupies it.
    let stickers = solved_stickers();
    let view = CubeView::default().turned(ViewTurn::Right);
    let net = net_view(&stickers, view);
    assert_eq!(net.up.slot, "U");
    assert_eq!(net.down.slot, "D");
    assert_eq!(net.left.slot, "L");
    assert_eq!(net.right.slot, "R");
    assert_eq!(net.front.slot, "F");
    assert_eq!(net.back.slot, "B");
    // ...even though the front slot now holds the absolute Right face.
    assert_eq!(net.front.face, Face::Right);
}

#[test]
fn opposite_view_turns_restore_orientation() {
    for (first, second) in [
        (ViewTurn::Right, ViewTurn::Left),
        (ViewTurn::Left, ViewTurn::Right),
        (ViewTurn::Up, ViewTurn::Down),
        (ViewTurn::Down, ViewTurn::Up),
    ] {
        let view = CubeView::default().turned(first).turned(second);
        assert_eq!(view, CubeView::default());
    }
}
