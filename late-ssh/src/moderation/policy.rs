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
    }
}

const REGULAR: Caps = Caps::empty();

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
    .union(Caps::RENAME_ROOM);

const ADMIN: Caps = Caps::all();

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Permissions {
    tier: Tier,
}

impl Permissions {
    pub const fn new(is_admin: bool, is_moderator: bool) -> Self {
        Self {
            tier: Tier::from_user_flags(is_admin, is_moderator),
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

    pub const fn caps(self) -> Caps {
        match self.tier {
            Tier::Regular => REGULAR,
            Tier::Moderator => MODERATOR,
            Tier::Admin => ADMIN,
        }
    }

    pub const fn has(self, action: Caps) -> bool {
        self.caps().contains(action)
    }

    pub fn can(self, action: Caps, target: Tier) -> bool {
        self.has(action) && self.tier > target
    }

    pub const fn should_audit(self, target_is_self: bool) -> bool {
        !target_is_self && self.can_moderate()
    }
}

#[cfg(test)]
mod tests {
    use super::{Caps, Permissions, Tier};

    #[test]
    fn tier_from_flags() {
        assert_eq!(Permissions::new(false, false).tier(), Tier::Regular);
        assert_eq!(Permissions::new(false, true).tier(), Tier::Moderator);
        assert_eq!(Permissions::new(true, false).tier(), Tier::Admin);
        assert_eq!(Permissions::new(true, true).tier(), Tier::Admin);
    }

    #[test]
    fn moderators_have_staff_caps_without_admin_caps() {
        let permissions = Permissions::new(false, true);
        assert!(permissions.has(Caps::OPEN_MOD_SURFACE));
        assert!(permissions.has(Caps::TEMP_BAN_USER));
        assert!(permissions.has(Caps::RENAME_ROOM));
        assert!(!permissions.has(Caps::PERMA_BAN_USER));
        assert!(!permissions.has(Caps::GRANT_MOD));
    }

    #[test]
    fn targeted_actions_require_higher_tier() {
        let moderator = Permissions::new(false, true);
        let admin = Permissions::new(true, false);

        assert!(moderator.can(Caps::BAN_FROM_ROOM, Tier::Regular));
        assert!(!moderator.can(Caps::BAN_FROM_ROOM, Tier::Moderator));
        assert!(!moderator.can(Caps::BAN_FROM_ROOM, Tier::Admin));
        assert!(admin.can(Caps::BAN_FROM_ROOM, Tier::Moderator));
        assert!(!admin.can(Caps::BAN_FROM_ROOM, Tier::Admin));
    }

    #[test]
    fn audit_only_privileged_actions_against_others() {
        assert!(!Permissions::default().should_audit(false));
        assert!(!Permissions::new(false, true).should_audit(true));
        assert!(Permissions::new(false, true).should_audit(false));
        assert!(Permissions::new(true, false).should_audit(false));
    }
}
