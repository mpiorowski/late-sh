use crate::app::quit_confirm::input::*;

#[test]
fn second_q_confirms_and_escape_dismisses() {
    assert_eq!(action_for(false), QuitAction::OpenConfirm);
    assert_eq!(action_for(true), QuitAction::QuitNow);
}
