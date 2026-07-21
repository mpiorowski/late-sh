use crate::app::directory::state::*;

#[test]
fn search_buffer_methods_reset_selection() {
    let mut state = DirectoryState::new();
    state.enter_search();
    state.search_push('r');
    state.move_search_selection(1, 3);
    assert_eq!(state.search_selected(), 1);
    state.search_push('s');
    assert_eq!(state.search_query(), "rs");
    assert_eq!(state.search_selected(), 0);
    state.search_backspace();
    assert_eq!(state.search_query(), "r");
}
