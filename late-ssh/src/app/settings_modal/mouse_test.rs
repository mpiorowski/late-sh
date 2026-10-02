use super::*;

#[test]
fn wheel_batches_clamp_each_pane_and_invalidate_only_clicks() {
    let state = MouseState::default();
    state.begin((80, 24));
    let left = Rect::new(2, 3, 20, 5);
    let right = Rect::new(24, 3, 30, 5);
    state.pane(left, Pane::StatusList, 30, 0);
    state.pane(right, Pane::StatusDetail, 12, 0);
    state.hit(left, Target::Status(0));
    state.finish();
    state.scroll(3, 4, 3, (80, 24));
    state.scroll(3, 4, 3, (80, 24));
    assert_eq!(state.offsets.get()[Pane::StatusList as usize], 6);
    assert_eq!(state.offsets.get()[Pane::StatusDetail as usize], 0);
    assert_eq!(state.target(3, 4, (80, 24)), None);
    state.scroll(25, 4, 100, (80, 24));
    assert_eq!(state.offsets.get()[Pane::StatusDetail as usize], 7);
    state.scroll(3, 4, -100, (80, 24));
    assert_eq!(state.offsets.get()[Pane::StatusList as usize], 0);
    state.scroll(1, 1, 3, (80, 24));
    assert_eq!(state.offsets.get()[Pane::StatusList as usize], 0);
}

#[test]
fn virtual_hits_are_clipped_and_normalized_to_viewport() {
    let state = MouseState::default();
    state.begin((80, 24));
    state.hit(Rect::new(0, 1, 20, 1), Target::Row(Row::Username));
    state.hit(Rect::new(0, 4, 20, 1), Target::Row(Row::Country));
    state.hit(Rect::new(0, 6, 20, 1), Target::Row(Row::Timezone));
    state.translate(0, Rect::new(10, 10, 12, 2), 4);
    assert_eq!(
        state.target(10, 10, (80, 24)),
        Some(Target::Row(Row::Country))
    );
    assert_eq!(state.target(22, 10, (80, 24)), None);
    assert_eq!(state.target(10, 12, (80, 24)), None);
    assert_eq!(state.hits.borrow().len(), 1);
}

#[test]
fn resize_mutation_and_foreground_dialog_discard_stale_hits() {
    let state = MouseState::default();
    state.begin((80, 24));
    state.hit(Rect::new(3, 4, 10, 1), Target::Tab(Tab::Themes));
    assert_eq!(state.target(3, 4, (40, 12)), None);
    state.invalidate();
    assert_eq!(state.target(3, 4, (80, 24)), None);
    state.begin((80, 24));
    state.hit(Rect::new(3, 4, 10, 1), Target::Tab(Tab::Themes));
    state.clear_surface();
    state.hit(Rect::new(5, 6, 4, 1), Target::Close);
    assert_eq!(state.target(3, 4, (80, 24)), None);
    assert_eq!(state.target(5, 6, (80, 24)), Some(Target::Close));
}

#[test]
fn keyboard_reveals_selection_after_independent_wheel_scroll() {
    let state = MouseState::default();
    state.begin((80, 24));
    let area = Rect::new(3, 4, 20, 5);
    state.pane(area, Pane::Settings, 27, 0);
    state.finish();
    state.scroll(4, 5, 12, (80, 24));
    state.reveal_selection();
    state.begin((80, 24));
    assert_eq!(state.pane(area, Pane::Settings, 27, 1), 1);
    assert_eq!(state.pane(area, Pane::Settings, 27, 24), 20);
}
