// The tag picker: one filtered multi-select over `late_core::vocab`, opened
// by the settings modal (langs) and the profile editor (skills, langs).
// `state.rs` is the pure picker, `input.rs` the keys and the hand-back to
// whichever field opened it, `ui.rs` the popup.
pub(crate) mod input;
pub(crate) mod state;
pub(crate) mod ui;

#[cfg(test)]
mod state_test;
#[cfg(test)]
mod ui_test;
