use bitflags::bitflags;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    #[default]
    Regular = 0,
    Moderator = 1,
    Admin = 2,
}

impl Tier {
    pub const fn from_user_flags(is_admin: bool, is_moderator: bool) -> Self {
        if is_admin {
            Self::Admin
        } else if is_moderator {
            Self::Moderator
        } else {
            Self::Regular
        }
    }
}

bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct Caps: u64 {
        const EDIT_OTHER_MESSAGE = 1 << 0;
        const DELETE_OTHER_MESSAGE = 1 << 1;
        const KICK_FROM_ROOM = 1 << 2;
        const BAN_FROM_ROOM = 1 << 3;
        const UNBAN_FROM_ROOM = 1 << 4;
        const KICK_USER = 1 << 5;
        const TEMP_BAN_USER = 1 << 6;
        const PERMA_BAN_USER = 1 << 7;
        const UNBAN_USER = 1 << 8;
        const BAN_FROM_ARTBOARD = 1 << 9;
        const UNBAN_FROM_ARTBOARD = 1 << 10;
        const GRANT_MOD = 1 << 11;
        const REVOKE_MOD = 1 << 12;
        const OPEN_MOD_SURFACE = 1 << 13;
        const VIEW_STAFF_INFO = 1 << 14;
        const RENAME_ROOM = 1 << 15;
        const RESTORE_ARTBOARD = 1 << 16;
        const RENAME_USER = 1 << 17;
        const BAN_FROM_AUDIO = 1 << 18;
        const UNBAN_FROM_AUDIO = 1 << 19;
        // Bit 20 belonged to DELETE_PINSTAR_GRAPH (feature removed); caps are
        // derived fresh from Tier each session, so the gap is inert.
        const DELETE_AUDIO_TRACK = 1 << 21;
        const KICK_FROM_VOICE = 1 << 22;
        const UNBLOCK_VOICE = 1 << 23;
        const SET_ROOM_VOICE = 1 << 24;
        const KICK_STREAM = 1 << 25;
        const BAN_FROM_STREAM = 1 << 26;
        const UNBAN_FROM_STREAM = 1 << 27;
    }
}

const REGULAR: Caps = Caps::empty();

/// What the owner of a private room may do inside that one room. Deliberately
/// narrow: an owner keeps the door, staff keep everything else.
const ROOM_OWNER: Caps = Caps::KICK_FROM_ROOM;

const MODERATOR: Caps = Caps::EDIT_OTHER_MESSAGE
    .union(Caps::DELETE_OTHER_MESSAGE)
    .union(Caps::KICK_FROM_ROOM)
    .union(Caps::BAN_FROM_ROOM)
    .union(Caps::UNBAN_FROM_ROOM)
    .union(Caps::KICK_USER)
    .union(Caps::TEMP_BAN_USER)
    .union(Caps::UNBAN_USER)
    .union(Caps::BAN_FROM_ARTBOARD)
    .union(Caps::UNBAN_FROM_ARTBOARD)
    .union(Caps::OPEN_MOD_SURFACE)
    .union(Caps::VIEW_STAFF_INFO)
    .union(Caps::RENAME_ROOM)
    .union(Caps::RESTORE_ARTBOARD)
    .union(Caps::RENAME_USER)
    .union(Caps::BAN_FROM_AUDIO)
    .union(Caps::UNBAN_FROM_AUDIO)
    .union(Caps::DELETE_AUDIO_TRACK)
    .union(Caps::KICK_FROM_VOICE)
    .union(Caps::UNBLOCK_VOICE)
    .union(Caps::SET_ROOM_VOICE)
    .union(Caps::KICK_STREAM)
    .union(Caps::BAN_FROM_STREAM)
    .union(Caps::UNBAN_FROM_STREAM);

const ADMIN: Caps = Caps::all();

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Permissions {
    tier: Tier,
    /// Granted for a single action against a single room the actor owns. Never
    /// part of a session's standing permissions, which is why it is not derived
    /// from the user flags.
    owns_room: bool,
}

impl Permissions {
    pub const fn new(is_admin: bool, is_moderator: bool) -> Self {
        Self {
            tier: Tier::from_user_flags(is_admin, is_moderator),
            owns_room: false,
        }
    }

    /// Add the caps a room owner holds, for one action inside the room they
    /// own. The tier is untouched on purpose: ownership outranks nobody, so the
    /// rank compare in `can` still refuses staff targets.
    pub const fn as_room_owner(self) -> Self {
        Self {
            tier: self.tier,
            owns_room: true,
        }
    }

    pub const fn tier(self) -> Tier {
        self.tier
    }

    pub const fn is_admin(self) -> bool {
        matches!(self.tier, Tier::Admin)
    }

    pub const fn is_moderator(self) -> bool {
        matches!(self.tier, Tier::Moderator)
    }

    pub const fn can_moderate(self) -> bool {
        matches!(self.tier, Tier::Moderator | Tier::Admin)
    }

    pub const fn can_access_admin_surface(self) -> bool {
        self.is_admin()
    }

    pub const fn can_access_mod_surface(self) -> bool {
        self.has(Caps::OPEN_MOD_SURFACE)
    }

    pub const fn can_manage_permanent_rooms(self) -> bool {
        self.is_admin()
    }

    pub const fn can_post_announcements(self) -> bool {
        self.is_admin()
    }

    pub const fn can_edit_message(self, is_owner: bool) -> bool {
        is_owner || self.has(Caps::EDIT_OTHER_MESSAGE)
    }

    pub const fn can_delete_message(self, is_owner: bool) -> bool {
        is_owner || self.has(Caps::DELETE_OTHER_MESSAGE)
    }

    pub const fn can_delete_article(self, is_owner: bool) -> bool {
        is_owner || self.has(Caps::DELETE_OTHER_MESSAGE)
    }

    pub const fn can_delete_audio_track(self, is_owner: bool) -> bool {
        is_owner || self.has(Caps::DELETE_AUDIO_TRACK)
    }

    pub const fn caps(self) -> Caps {
        let tier = match self.tier {
            Tier::Regular => REGULAR,
            Tier::Moderator => MODERATOR,
            Tier::Admin => ADMIN,
        };
        if self.owns_room {
            tier.union(ROOM_OWNER)
        } else {
            tier
        }
    }

    pub const fn has(self, action: Caps) -> bool {
        self.caps().contains(action)
    }

    pub fn can(self, action: Caps, target: Tier) -> bool {
        if !self.has(action) {
            return false;
        }
        // Staff act by rank. An owner holds no rank, so they may only act on
        // regulars, and only with the caps ownership itself grants.
        self.tier > target
            || (self.owns_room && matches!(target, Tier::Regular) && ROOM_OWNER.contains(action))
    }

    /// Owner actions are logged like staff actions: someone was removed from a
    /// room and the record should say who did it.
    pub const fn should_audit(self, target_is_self: bool) -> bool {
        !target_is_self && (self.can_moderate() || self.owns_room)
    }
}

#[cfg(test)]
#[path = "policy_test.rs"]
mod policy_test;
