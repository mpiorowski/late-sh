//! Chat slash-command registry and matching.
//!
//! [`COMMANDS`] is the single registry of slash commands. Each command's
//! [`CommandScope`] decides where it is offered and dispatched: `Global`
//! commands are available everywhere, while room-scoped commands appear only
//! inside the room matching their slug. [`rank_command_matches`] filters the
//! registry for autocomplete; [`room_owns_command`] gates dispatch of
//! room-scoped commands in `ChatState::submit_composer`.

use late_core::models::chat_room::ChatRoom;

use super::state::MentionMatch;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RoomScopedCommand {
    Sheet,
}

impl RoomScopedCommand {
    pub(crate) const ALL: &'static [Self] = &[Self::Sheet];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Sheet => "sheet",
        }
    }

    pub(crate) const fn description(self) -> &'static str {
        match self {
            Self::Sheet => "view character sheets",
        }
    }

    pub(crate) const fn room_slug(self) -> &'static str {
        match self {
            Self::Sheet => "dnd",
        }
    }

    pub(crate) fn available_in(self, room: &ChatRoom) -> bool {
        room.slug.as_deref() == Some(self.room_slug())
    }
}

/// Where a [`Command`] is offered and dispatched.
#[derive(Clone, Copy)]
enum CommandScope {
    /// Available in every room.
    Global,
    /// Available only in the room owned by this room-scoped command.
    Room(RoomScopedCommand),
}

impl CommandScope {
    /// Whether a command with this scope is available in `room` (`None` means
    /// the composer is not focused on a resolvable room).
    fn available_in(&self, room: Option<&ChatRoom>) -> bool {
        match self {
            CommandScope::Global => true,
            CommandScope::Room(command) => room.is_some_and(|room| command.available_in(room)),
        }
    }
}

struct Command {
    name: &'static str,
    description: &'static str,
    scope: CommandScope,
}

/// Terse constructor for the common [`CommandScope::Global`] case.
const fn global(name: &'static str, description: &'static str) -> Command {
    Command {
        name,
        description,
        scope: CommandScope::Global,
    }
}

/// Terse constructor for room-scoped commands. The enum carries the command
/// name, description, and owning room slug so autocomplete, dispatch, and
/// service authorization all share one source of truth.
const fn room(command: RoomScopedCommand) -> Command {
    Command {
        name: command.name(),
        description: command.description(),
        scope: CommandScope::Room(command),
    }
}

/// All slash commands: globals (kept alphabetical for readability) followed by
/// room-scoped commands. `rank_command_matches` sorts matches before returning,
/// so registry order does not affect the autocomplete display.
const COMMANDS: &[Command] = &[
    global("active", "list active users"),
    global("aquarium", "toggle aquarium (/aquarium feed to feed)"),
    global("binds", "chat guide"),
    global("brb", "go AFK and mute audio"),
    global("bug", "report a bug to #bugs"),
    global(
        "challenge",
        "post daily challenge (chess, battleship, connect4, reversi, checkers, backgammon)",
    ),
    global("coffee", "post coffee cup"),
    global("dm", "open DM"),
    global("exit", "quit confirm"),
    global("feed", "feed your pet with pet food"),
    global("friend", "mark user"),
    global("friends", "list friends"),
    global("gift", "send chips"),
    global("icons", "open icon picker"),
    global("ignore", "mute user"),
    global("invite", "add user"),
    global("leave", "leave room"),
    global("list", "public rooms"),
    global("me", "send action"),
    global("members", "room members"),
    global("paste-image", "upload image from CLI clipboard"),
    global("pet", "toggle the pet strip"),
    global("petname", "name your pet"),
    global("poll", "start room poll"),
    global("private", "new private room"),
    global("profile", "view user profile"),
    global("public", "open public room for everyone"),
    global("roll", "roll dice (e.g. /roll 3d6)"),
    global("search", "search messages (?query in Ctrl+/)"),
    global("settings", "open settings"),
    global("suggest", "send a suggestion to #suggestions"),
    global("tea", "post tea cup"),
    global("unfriend", "unmark user"),
    global("unignore", "unmute user"),
    global("upload", "upload image from url"),
    global("water", "water your pet"),
    room(RoomScopedCommand::Sheet),
];

/// True when `room` owns a room-scoped command named `name`. Used to gate
/// dispatch (in `submit_composer`) and to keep wrong-room commands unrecognized.
/// Global commands are never "owned" by a room — they have their own
/// unconditional dispatch branches.
pub(crate) fn room_owns_command(room: &ChatRoom, name: &str) -> bool {
    room_scoped_command_named(name).is_some_and(|command| command.available_in(room))
}

pub(crate) fn room_scoped_command_named(name: &str) -> Option<RoomScopedCommand> {
    RoomScopedCommand::ALL
        .iter()
        .copied()
        .find(|command| command.name() == name)
}

pub(crate) fn rank_command_matches(
    query_lower: &str,
    room: Option<&ChatRoom>,
) -> Vec<MentionMatch> {
    let available = || COMMANDS.iter().filter(|cmd| cmd.scope.available_in(room));

    // A fully typed command name needs no suggestions.
    if !query_lower.is_empty() && available().any(|cmd| cmd.name == query_lower) {
        return Vec::new();
    }

    let mut matches: Vec<MentionMatch> = available()
        .filter(|cmd| cmd.name.starts_with(query_lower))
        .map(|cmd| MentionMatch {
            name: cmd.name.to_string(),
            online: true,
            prefix: "/",
            description: Some(cmd.description),
        })
        .collect();
    matches.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    matches
}

#[cfg(test)]
#[path = "commands_test.rs"]
mod commands_test;
