// A small form modal for a room's "about" info: name, what it's about, and the
// general rules. Opened when a user creates a room with `/public` or `/private`,
// and reopenable by the owner via `/roominfo` to edit. The collected info is
// shown at the top of every room.
pub(crate) mod input;
pub(crate) mod state;
pub(crate) mod ui;

#[cfg(test)]
mod state_test;
