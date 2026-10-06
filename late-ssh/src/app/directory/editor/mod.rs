// The profile editor: one modal for everything a person shows on the
// Profiles page. Three pages, `card` (the work card), `about` (bio and the
// late.fetch fields), and `projects` (the showcase list, each project a
// small form of its own). Opened from page 5; a moderator editing someone
// else's card or project gets that one page alone.
pub(crate) mod input;
pub(crate) mod state;
pub(crate) mod ui;

#[cfg(test)]
mod state_test;
#[cfg(test)]
mod ui_test;
