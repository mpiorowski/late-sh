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
            Self::Sheet => "view a character sheet (/sheet @user)",
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
///
/// A description carries the argument shape inline (`/gift @user 50 [note]`)
/// whenever the command takes one, since the popup is where a user learns the
/// syntax: the usage banner only shows up after they have already got it
/// wrong. Keep them at 46 columns or under. The popup sizes itself to the
/// longest description in the match set and then clips to the composer width,
/// so a long one silently truncates on an 80-col terminal.
const COMMANDS: &[Command] = &[
    global("active", "list users online right now"),
    global("aquarium", "toggle aquarium (/aquarium feed to feed)"),
    global("ban", "ban from your room (/ban @user [7d] [reason])"),
    global("binds", "open the chat guide (same as ?)"),
    global("brb", "go AFK and mute audio (/brb back in 5)"),
    global("bug", "report a bug to #bugs (/bug <what broke>)"),
    global("coffee", "post coffee cup"),
    global("crown", "the crown (/crown; /crown take to buy it)"),
    global("cs", "cyberspace (/cs post, chat, link, unlink)"),
    global("cyberspace", "open the cyberspace tab (alias /cs)"),
    global("dm", "open a DM (/dm @user)"),
    global("exit", "quit confirm"),
    global("friend", "mark a friend (/friend @user; bare lists)"),
    global("friends", "list friends"),
    global("gift", "send chips (/gift @user 50 [note])"),
    global("golive", "stream your screen (/golive <title>; stop)"),
    global("history", "browse this room's full history"),
    global("icons", "open icon picker"),
    global("ignore", "mute a user (/ignore @user; bare lists)"),
    global("invite", "add a user to this room (/invite @user)"),
    global("join", "open/create a public room (/join #room)"),
    global("kick", "remove a user from your room (/kick @user)"),
    global("leave", "leave room"),
    global("list", "list public rooms"),
    global("me", "send an action line (/me waves)"),
    global("members", "room members"),
    global("pair", "shared coding scratchpad; both run /pair @user"),
    global("paste-image", "upload image from CLI clipboard"),
    global("pet", "toggle the pet strip (/pet feed, /pet water)"),
    global("petname", "name your pet (/petname Mochi; bare shows)"),
    global("poll", "start a Home room poll (2-3 options)"),
    global("pomodoro", "focus countdown (/pomodoro 50 deep work; stop)"),
    global("pot", "the weekly pot (/pot; /pot buy N for tickets)"),
    global("private", "create a private room (/private #room)"),
    global("profile", "view a profile (/profile @user; bare = you)"),
    global("public", "open/create a public room (/public #room)"),
    global("roll", "roll dice (/roll 3d6 2d20; default d20)"),
    global("roominfo", "set this room's topic and rules"),
    global("rules", "show this room's rules"),
    global("search", "search messages (?query in Ctrl+/)"),
    global("settings", "open settings"),
    global("shop", "open the shop (badges, effects, companions)"),
    global("suggest", "send an idea to #suggestions (/suggest <idea>)"),
    global("summary", "AI catch-up of this room, or /summary 6h"),
    global("tea", "post tea cup"),
    global("unban", "lift a room ban (/unban @user)"),
    global("unfriend", "remove a friend mark (/unfriend @user)"),
    global("unignore", "unmute a user (/unignore @user)"),
    global("upload", "upload an image by url (/upload <url>)"),
    global("watch", "open someone's live stream (/watch @user)"),
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
