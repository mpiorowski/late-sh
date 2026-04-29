#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Permissions {
    is_admin: bool,
    is_moderator: bool,
}

impl Permissions {
    pub const fn new(is_admin: bool, is_moderator: bool) -> Self {
        Self {
            is_admin,
            is_moderator,
        }
    }

    pub const fn is_admin(self) -> bool {
        self.is_admin
    }

    pub const fn is_moderator(self) -> bool {
        self.is_moderator
    }

    pub const fn can_moderate(self) -> bool {
        self.is_admin || self.is_moderator
    }

    pub const fn tier(self) -> Tier {
        if self.is_admin {
            Tier::Admin
        } else if self.is_moderator {
            Tier::Moderator
        } else {
            Tier::Regular
        }
    }

    pub const fn can_access_admin_surface(self) -> bool {
        self.is_admin
    }

    pub const fn can_access_mod_surface(self) -> bool {
        self.can_moderate()
    }

    pub const fn can_manage_permanent_rooms(self) -> bool {
        self.is_admin
    }

    pub const fn can_post_announcements(self) -> bool {
        self.is_admin
    }

    pub const fn can_edit_message(self, is_owner: bool) -> bool {
        is_owner || self.can_moderate()
    }

    pub const fn can_delete_message(self, is_owner: bool) -> bool {
        is_owner || self.can_moderate()
    }

    pub const fn can_delete_article(self, is_owner: bool) -> bool {
        is_owner || self.can_moderate()
    }

    /// Consult the permissions matrix
    /// (`devdocs/PERMISSIONS-MATRIX.csv`) for the given action + target.
    ///
    /// This is additive — the legacy `can_*` predicates above are still the
    /// source of gating at call sites until a follow-up PR migrates them.
    pub fn decide(self, action: Action, target: TargetTier) -> Decision {
        decide_matrix(self.tier(), action, target)
    }

    /// The audit-log rule that applies when this actor's allowed action
    /// landed on the given (action, target) cell of the matrix. Pair with
    /// [`LogRule::applies`] at call sites.
    pub fn log_rule(self, action: Action, target: TargetTier) -> LogRule {
        log_rule_matrix(action, target)
    }

    /// Convenience: returns `true` when this actor's allowed action at
    /// `(action, target)` should be recorded in the moderation audit log.
    pub fn should_audit(self, action: Action, target: TargetTier) -> bool {
        self.log_rule(action, target).applies(self.tier())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Regular,
    Moderator,
    Admin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetTier {
    /// No meaningful target, or the target tier doesn't affect the decision.
    NotApplicable,
    /// Target is the actor themself or content they own (owner rule).
    Own,
    /// Target is a regular user, or content owned by a regular user.
    Regular,
    /// Target is a moderator, or content owned by a moderator.
    Moderator,
    /// Target is an admin, or content owned by an admin.
    Admin,
    /// Target is a system-protected resource (e.g. `#general`).
    System,
}

impl TargetTier {
    /// Maps a target user's role flags to a `TargetTier`. Use `Own` directly
    /// when the target is the actor themself — this helper never returns
    /// `Own`.
    pub const fn from_user_flags(is_admin: bool, is_moderator: bool) -> Self {
        if is_admin {
            TargetTier::Admin
        } else if is_moderator {
            TargetTier::Moderator
        } else {
            TargetTier::Regular
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    /// The capability is intentionally deferred / not implemented. Treat as
    /// `Deny` at call sites unless you have a specific reason to handle it.
    NotImplemented,
}

impl Decision {
    /// Only `Allow` permits the action. `Deny` and `NotImplemented` both
    /// reject.
    pub const fn is_allowed(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// Whether an allowed action should be recorded in the moderation audit
/// log. Derived from the `log_when_allowed` column of the permissions
/// matrix for the matched (action, target_tier) row. Owner-rule rows
/// (e.g. editing your own message) are always `Never` — the matrix only
/// audits privileged overrides, not self-service.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogRule {
    Never,
    OnModOrAdmin,
    OnAdmin,
}

impl LogRule {
    /// Whether an actor of `actor_tier` should be audit-logged when their
    /// action is allowed under this rule.
    pub const fn applies(self, actor_tier: Tier) -> bool {
        match self {
            LogRule::Never => false,
            LogRule::OnAdmin => matches!(actor_tier, Tier::Admin),
            LogRule::OnModOrAdmin => matches!(actor_tier, Tier::Moderator | Tier::Admin),
        }
    }
}

/// Every action the permissions matrix enumerates. Multiple CSV rows can map
/// to the same variant — the `TargetTier` distinguishes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    // Chat messages
    PostMessagePublicRoom,
    PostMessagePrivateRoomMember,
    PostMessageAnnouncements,
    EditMessage,
    ReactToMessage,
    RemoveOwnReaction,
    DeleteMessage,
    // Articles
    PostArticle,
    EditArticle,
    DeleteArticle,
    // Rooms
    CreateTopicRoom,
    CreatePermanentRoom,
    JoinPublicRoom,
    JoinPrivateRoomWithInvite,
    SeePrivateRoomInDiscover,
    InviteToPrivateRoom,
    LeaveRoom,
    KickFromRoom,
    BanFromRoom,
    UnbanFromRoom,
    RenameRoom,
    SetRoomVisibility,
    DeleteRoom,
    // Server-level user moderation
    KickUserSessions,
    TempBanUser,
    PermaBanUser,
    UnbanUser,
    // Artboard moderation
    BanFromArtboard,
    UnbanFromArtboard,
    ViewUserFingerprint,
    ViewUserIp,
    DeleteUserAccountHard,
    DeactivateUserAccount,
    RenameUserForced,
    GrantModerator,
    RevokeModerator,
    GrantAdmin,
    RevokeAdmin,
    // Profile
    EditProfile,
    ClearProfileUgc,
    DeleteOwnAccount,
    // Social
    IgnoreUser,
    // Staff surfaces
    OpenControlCenter,
    ViewStaffUserDirectory,
    ViewLiveSessions,
    // Audit
    ViewAuditLogSelf,
    ViewAuditLogOther,
    ViewUserSanctionHistory,
    ViewOwnSanctions,
    // Misc
    LaunchBlackjackArcade,
}

fn decide_matrix(actor: Tier, action: Action, target: TargetTier) -> Decision {
    use Action::*;
    use Decision::*;
    use TargetTier::*;

    match (action, target) {
        // Chat messages
        (PostMessagePublicRoom, _) => Allow,
        (PostMessagePrivateRoomMember, _) => Allow,
        (PostMessageAnnouncements, _) => admin_only(actor),
        (EditMessage, Own) => Allow,
        (EditMessage, _) => admin_only(actor),
        (ReactToMessage, _) => Allow,
        (RemoveOwnReaction, _) => Allow,
        (DeleteMessage, Own) => Allow,
        (DeleteMessage, Regular) => mod_or_admin(actor),
        (DeleteMessage, Moderator | Admin) => admin_only(actor),

        // Articles
        (PostArticle, _) => Allow,
        (EditArticle, Own) => NotImplemented,
        (EditArticle, _) => admin_ni_else_deny(actor),
        (DeleteArticle, Own) => Allow,
        (DeleteArticle, Regular) => mod_or_admin(actor),
        (DeleteArticle, Moderator | Admin) => admin_only(actor),

        // Rooms
        (CreateTopicRoom, _) => Allow,
        (CreatePermanentRoom, _) => admin_only(actor),
        (JoinPublicRoom, _) => Allow,
        (JoinPrivateRoomWithInvite, _) => Allow,
        (SeePrivateRoomInDiscover, _) => mod_or_admin(actor),
        (InviteToPrivateRoom, _) => Allow,
        (LeaveRoom, _) => Allow,
        (KickFromRoom, Regular) => mod_or_admin(actor),
        (KickFromRoom, Moderator | Admin) => admin_only(actor),
        (BanFromRoom, Regular) => mod_or_admin(actor),
        (BanFromRoom, Moderator | Admin) => admin_only(actor),
        (UnbanFromRoom, Regular) => mod_or_admin(actor),
        (UnbanFromRoom, Moderator | Admin) => admin_only(actor),
        (RenameRoom, System) => Deny,
        (RenameRoom, _) => mod_or_admin(actor),
        (SetRoomVisibility, _) => admin_only(actor),
        (DeleteRoom, System) => Deny,
        (DeleteRoom, _) => admin_only(actor),

        // Server-level user moderation
        (KickUserSessions, Regular) => mod_or_admin(actor),
        (KickUserSessions, Moderator) => admin_only(actor),
        (KickUserSessions, Admin) => Deny,
        (TempBanUser, Regular) => mod_or_admin(actor),
        (TempBanUser, Moderator) => admin_only(actor),
        (TempBanUser, Admin) => Deny,
        (PermaBanUser, Regular | Moderator) => admin_only(actor),
        (PermaBanUser, Admin) => Deny,
        (UnbanUser, Regular) => mod_or_admin(actor),
        (UnbanUser, Moderator) => admin_only(actor),
        (UnbanUser, Admin) => Deny,
        (BanFromArtboard, Regular) => mod_or_admin(actor),
        (BanFromArtboard, Moderator | Admin) => admin_only(actor),
        (UnbanFromArtboard, Regular) => mod_or_admin(actor),
        (UnbanFromArtboard, Moderator | Admin) => admin_only(actor),
        (ViewUserFingerprint, _) => mod_or_admin(actor),
        (ViewUserIp, _) => mod_or_admin(actor),
        (DeleteUserAccountHard, _) => NotImplemented,
        (DeactivateUserAccount, _) => admin_ni_else_deny(actor),
        (RenameUserForced, _) => admin_ni_else_deny(actor),
        (GrantModerator, _) => admin_only(actor),
        (RevokeModerator, _) => admin_only(actor),
        (GrantAdmin, _) => admin_only(actor),
        (RevokeAdmin, _) => admin_only(actor),

        // Profile
        (EditProfile, Own) => Allow,
        (EditProfile, _) => admin_ni_else_deny(actor),
        (ClearProfileUgc, Regular) => mod_or_admin(actor),
        (ClearProfileUgc, Moderator | Admin) => admin_only(actor),
        (DeleteOwnAccount, _) => NotImplemented,

        // Social
        (IgnoreUser, _) => Allow,

        // Staff surfaces
        (OpenControlCenter, _) => mod_or_admin(actor),
        (ViewStaffUserDirectory, _) => mod_or_admin(actor),
        (ViewLiveSessions, _) => mod_or_admin(actor),

        // Audit
        (ViewAuditLogSelf, _) => mod_or_admin(actor),
        (ViewAuditLogOther, _) => mod_or_admin(actor),
        (ViewUserSanctionHistory, _) => mod_or_admin(actor),
        (ViewOwnSanctions, _) => NotImplemented,

        // Misc
        (LaunchBlackjackArcade, _) => admin_only(actor),

        // Fallbacks that the CSV doesn't cover explicitly. Target-tier rows
        // not present in the matrix (e.g. ClearProfileUgc with target=Own,
        // EditProfile with target=System) shouldn't occur at call sites; we
        // return Deny defensively.
        (ClearProfileUgc, _) => Deny,
        (KickFromRoom | BanFromRoom | UnbanFromRoom, _) => Deny,
        (DeleteMessage | DeleteArticle, _) => Deny,
        (KickUserSessions, _) => Deny,
        (TempBanUser | PermaBanUser | UnbanUser, _) => Deny,
        (BanFromArtboard | UnbanFromArtboard, _) => Deny,
    }
}

fn log_rule_matrix(action: Action, target: TargetTier) -> LogRule {
    use Action::*;
    use LogRule::*;
    use TargetTier::*;

    match (action, target) {
        // Chat messages
        (PostMessagePublicRoom, _) => Never,
        (PostMessagePrivateRoomMember, _) => Never,
        (PostMessageAnnouncements, _) => Never,
        (EditMessage, Own) => Never,
        (EditMessage, _) => OnAdmin,
        (ReactToMessage, _) => Never,
        (RemoveOwnReaction, _) => Never,
        (DeleteMessage, Own) => Never,
        (DeleteMessage, Regular) => OnModOrAdmin,
        (DeleteMessage, Moderator | Admin) => OnAdmin,

        // Articles
        (PostArticle, _) => Never,
        (EditArticle, Own) => Never,
        (EditArticle, _) => OnAdmin,
        (DeleteArticle, Own) => Never,
        (DeleteArticle, Regular) => OnModOrAdmin,
        (DeleteArticle, Moderator | Admin) => OnAdmin,

        // Rooms
        (CreateTopicRoom, _) => Never,
        (CreatePermanentRoom, _) => Never,
        (JoinPublicRoom, _) => Never,
        (JoinPrivateRoomWithInvite, _) => Never,
        (SeePrivateRoomInDiscover, _) => Never,
        (InviteToPrivateRoom, _) => Never,
        (LeaveRoom, _) => Never,
        (KickFromRoom, Regular) => OnModOrAdmin,
        (KickFromRoom, Moderator | Admin) => OnAdmin,
        (BanFromRoom, Regular) => OnModOrAdmin,
        (BanFromRoom, Moderator | Admin) => OnAdmin,
        (UnbanFromRoom, Regular) => OnModOrAdmin,
        (UnbanFromRoom, Moderator | Admin) => OnAdmin,
        (RenameRoom, System) => Never,
        (RenameRoom, _) => OnModOrAdmin,
        (SetRoomVisibility, _) => OnAdmin,
        (DeleteRoom, System) => Never,
        (DeleteRoom, _) => OnAdmin,

        // Server-level user moderation
        (KickUserSessions, Regular) => OnModOrAdmin,
        (KickUserSessions, Moderator) => OnAdmin,
        (KickUserSessions, Admin) => Never,
        (TempBanUser, Regular) => OnModOrAdmin,
        (TempBanUser, Moderator) => OnAdmin,
        (TempBanUser, Admin) => Never,
        (PermaBanUser, Regular | Moderator) => OnAdmin,
        (PermaBanUser, Admin) => Never,
        (UnbanUser, Regular) => OnModOrAdmin,
        (UnbanUser, Moderator) => OnAdmin,
        (UnbanUser, Admin) => Never,
        (BanFromArtboard, Regular) => OnModOrAdmin,
        (BanFromArtboard, Moderator | Admin) => OnAdmin,
        (UnbanFromArtboard, Regular) => OnModOrAdmin,
        (UnbanFromArtboard, Moderator | Admin) => OnAdmin,
        (ViewUserFingerprint, _) => Never,
        (ViewUserIp, _) => Never,
        (DeleteUserAccountHard, _) => Never,
        (DeactivateUserAccount, _) => OnAdmin,
        (RenameUserForced, _) => OnAdmin,
        (GrantModerator, _) => OnAdmin,
        (RevokeModerator, _) => OnAdmin,
        (GrantAdmin, _) => OnAdmin,
        (RevokeAdmin, _) => OnAdmin,

        // Profile
        (EditProfile, Own) => Never,
        (EditProfile, _) => OnAdmin,
        (ClearProfileUgc, Regular) => OnModOrAdmin,
        (ClearProfileUgc, Moderator | Admin) => OnAdmin,
        (DeleteOwnAccount, _) => Never,

        // Social
        (IgnoreUser, _) => Never,

        // Staff surfaces
        (OpenControlCenter, _) => Never,
        (ViewStaffUserDirectory, _) => Never,
        (ViewLiveSessions, _) => Never,

        // Audit
        (ViewAuditLogSelf, _) => Never,
        (ViewAuditLogOther, _) => Never,
        (ViewUserSanctionHistory, _) => Never,
        (ViewOwnSanctions, _) => Never,

        // Misc
        (LaunchBlackjackArcade, _) => Never,

        // Fallbacks for (action, target) combinations the CSV does not
        // enumerate. Call sites should never hit these; default to Never so
        // an accidental cell doesn't silently generate audit-log noise.
        (ClearProfileUgc, _) => Never,
        (KickFromRoom | BanFromRoom | UnbanFromRoom, _) => Never,
        (DeleteMessage | DeleteArticle, _) => Never,
        (KickUserSessions, _) => Never,
        (TempBanUser | PermaBanUser | UnbanUser, _) => Never,
        (BanFromArtboard | UnbanFromArtboard, _) => Never,
    }
}

const fn admin_only(actor: Tier) -> Decision {
    if matches!(actor, Tier::Admin) {
        Decision::Allow
    } else {
        Decision::Deny
    }
}

const fn mod_or_admin(actor: Tier) -> Decision {
    match actor {
        Tier::Moderator | Tier::Admin => Decision::Allow,
        Tier::Regular => Decision::Deny,
    }
}

const fn admin_ni_else_deny(actor: Tier) -> Decision {
    if matches!(actor, Tier::Admin) {
        Decision::NotImplemented
    } else {
        Decision::Deny
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Decision, LogRule, Permissions, TargetTier, Tier};

    #[test]
    fn moderator_can_moderate_without_admin_privileges() {
        let permissions = Permissions::new(false, true);
        assert!(permissions.can_moderate());
        assert!(!permissions.can_access_admin_surface());
        assert!(permissions.can_access_mod_surface());
        assert!(!permissions.can_manage_permanent_rooms());
        assert!(!permissions.can_post_announcements());
    }

    #[test]
    fn admin_can_moderate_and_manage_admin_surfaces() {
        let permissions = Permissions::new(true, false);
        assert!(permissions.can_moderate());
        assert!(permissions.can_access_admin_surface());
        assert!(permissions.can_access_mod_surface());
        assert!(permissions.can_manage_permanent_rooms());
        assert!(permissions.can_post_announcements());
    }

    #[test]
    fn ownership_still_allows_regular_user_message_actions() {
        let permissions = Permissions::default();
        assert!(permissions.can_edit_message(true));
        assert!(permissions.can_delete_message(true));
        assert!(permissions.can_delete_article(true));
        assert!(!permissions.can_edit_message(false));
        assert!(!permissions.can_delete_message(false));
        assert!(!permissions.can_delete_article(false));
    }

    #[test]
    fn tier_from_flags() {
        assert_eq!(Permissions::new(false, false).tier(), Tier::Regular);
        assert_eq!(Permissions::new(false, true).tier(), Tier::Moderator);
        assert_eq!(Permissions::new(true, false).tier(), Tier::Admin);
        // Admin flag wins if both are set.
        assert_eq!(Permissions::new(true, true).tier(), Tier::Admin);
    }

    #[test]
    fn decide_matches_permissions_matrix() {
        const CSV: &str = include_str!("../../devdocs/PERMISSIONS-MATRIX.csv");
        let mut errors: Vec<String> = Vec::new();
        let mut rows_checked = 0usize;
        let mut rows_skipped = 0usize;

        for (lineno, raw) in CSV.lines().enumerate().skip(1) {
            let line = raw.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            let fields = parse_csv_line(line);
            if fields.len() < 7 {
                errors.push(format!("line {}: fewer than 7 columns", lineno + 1));
                continue;
            }
            let action_name = fields[0].as_str();
            let target_str = fields[2].as_str();
            let actor_cells = [fields[3].as_str(), fields[4].as_str(), fields[5].as_str()];
            let log_str = fields[6].as_str();

            let Some(action) = lookup_action(action_name) else {
                errors.push(format!(
                    "line {}: action '{}' is in the matrix but not in lookup_action() / the Action enum",
                    lineno + 1,
                    action_name
                ));
                rows_skipped += 1;
                continue;
            };
            let target = match lookup_target(target_str) {
                Some(t) => t,
                None => {
                    errors.push(format!(
                        "line {}: unknown target_tier '{}'",
                        lineno + 1,
                        target_str
                    ));
                    continue;
                }
            };

            let expected_log = match parse_log(log_str) {
                Some(r) => r,
                None => {
                    errors.push(format!(
                        "line {}: unknown log_when_allowed '{}'",
                        lineno + 1,
                        log_str
                    ));
                    continue;
                }
            };
            let got_log = Permissions::new(false, false).log_rule(action, target);
            if got_log != expected_log {
                errors.push(format!(
                    "line {}: action={} target={}: matrix log={:?} log_rule={:?}",
                    lineno + 1,
                    action_name,
                    target_str,
                    expected_log,
                    got_log
                ));
            }

            for (i, tier) in [Tier::Regular, Tier::Moderator, Tier::Admin]
                .into_iter()
                .enumerate()
            {
                let expected = match parse_cell(actor_cells[i]) {
                    Some(d) => d,
                    None => {
                        errors.push(format!(
                            "line {}: unknown cell '{}' at {:?}",
                            lineno + 1,
                            actor_cells[i],
                            tier
                        ));
                        continue;
                    }
                };
                let actor = permissions_for_tier(tier);
                let got = actor.decide(action, target);
                if got != expected {
                    errors.push(format!(
                        "line {}: action={} target={} actor={:?}: matrix={:?} decide={:?}",
                        lineno + 1,
                        action_name,
                        target_str,
                        tier,
                        expected,
                        got
                    ));
                }
            }
            rows_checked += 1;
        }

        assert!(
            rows_checked > 0,
            "no matrix rows were exercised — the CSV may have failed to load"
        );
        assert!(
            errors.is_empty(),
            "matrix / decide() mismatches ({} errors, {} rows checked, {} rows skipped):\n{}",
            errors.len(),
            rows_checked,
            rows_skipped,
            errors.join("\n")
        );
    }

    fn permissions_for_tier(tier: Tier) -> Permissions {
        match tier {
            Tier::Regular => Permissions::new(false, false),
            Tier::Moderator => Permissions::new(false, true),
            Tier::Admin => Permissions::new(true, false),
        }
    }

    fn parse_csv_line(line: &str) -> Vec<String> {
        let mut fields: Vec<String> = Vec::new();
        let mut current = String::new();
        let mut in_quotes = false;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if in_quotes {
                if c == '"' {
                    if chars.peek() == Some(&'"') {
                        current.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                } else {
                    current.push(c);
                }
            } else if c == ',' {
                fields.push(std::mem::take(&mut current));
            } else if c == '"' && current.is_empty() {
                in_quotes = true;
            } else {
                current.push(c);
            }
        }
        fields.push(current);
        fields
    }

    fn lookup_action(name: &str) -> Option<Action> {
        use Action::*;
        Some(match name {
            "post_message_public_room" => PostMessagePublicRoom,
            "post_message_private_room_member" => PostMessagePrivateRoomMember,
            "post_message_announcements" => PostMessageAnnouncements,
            "edit_own_message" | "edit_other_message" => EditMessage,
            "react_to_message" => ReactToMessage,
            "remove_own_reaction" => RemoveOwnReaction,
            "delete_own_message" | "delete_other_message" => DeleteMessage,
            "post_article" => PostArticle,
            "edit_own_article" | "edit_other_article" => EditArticle,
            "delete_own_article" | "delete_other_article" => DeleteArticle,
            "create_topic_room" => CreateTopicRoom,
            "create_permanent_room" => CreatePermanentRoom,
            "join_public_room" => JoinPublicRoom,
            "join_private_room_with_invite" => JoinPrivateRoomWithInvite,
            "see_private_room_in_discover" => SeePrivateRoomInDiscover,
            "invite_to_private_room" => InviteToPrivateRoom,
            "leave_room" => LeaveRoom,
            "kick_from_room" => KickFromRoom,
            "ban_from_room" => BanFromRoom,
            "unban_from_room" => UnbanFromRoom,
            "rename_room" | "rename_room_system" => RenameRoom,
            "set_room_visibility" => SetRoomVisibility,
            "delete_room_nonsystem" | "delete_room_system" => DeleteRoom,
            "kick_user_sessions" => KickUserSessions,
            "temp_ban_user" => TempBanUser,
            "perma_ban_user" => PermaBanUser,
            "unban_user" => UnbanUser,
            "ban_from_artboard" => BanFromArtboard,
            "unban_from_artboard" => UnbanFromArtboard,
            "view_user_fingerprint" => ViewUserFingerprint,
            "view_user_ip" => ViewUserIp,
            "delete_user_account_hard" => DeleteUserAccountHard,
            "deactivate_user_account" => DeactivateUserAccount,
            "rename_user_forced" => RenameUserForced,
            "grant_moderator" => GrantModerator,
            "revoke_moderator" => RevokeModerator,
            "grant_admin" => GrantAdmin,
            "revoke_admin" => RevokeAdmin,
            "edit_own_profile" | "edit_other_profile" => EditProfile,
            "clear_other_profile_ugc" => ClearProfileUgc,
            "delete_own_account" => DeleteOwnAccount,
            "ignore_user" => IgnoreUser,
            "open_control_center" => OpenControlCenter,
            "view_staff_user_directory" => ViewStaffUserDirectory,
            "view_live_sessions" => ViewLiveSessions,
            "view_audit_log_self" => ViewAuditLogSelf,
            "view_audit_log_other" => ViewAuditLogOther,
            "view_user_sanction_history" => ViewUserSanctionHistory,
            "view_own_sanctions" => ViewOwnSanctions,
            "launch_blackjack_arcade" => LaunchBlackjackArcade,
            _ => return None,
        })
    }

    fn parse_log(s: &str) -> Option<LogRule> {
        Some(match s {
            "never" => LogRule::Never,
            "on_mod_or_admin" => LogRule::OnModOrAdmin,
            "on_admin" => LogRule::OnAdmin,
            _ => return None,
        })
    }

    fn lookup_target(s: &str) -> Option<TargetTier> {
        Some(match s {
            "any" => TargetTier::NotApplicable,
            "self" => TargetTier::Own,
            "regular" => TargetTier::Regular,
            "mod" => TargetTier::Moderator,
            "admin" => TargetTier::Admin,
            "system" => TargetTier::System,
            _ => return None,
        })
    }

    fn parse_cell(s: &str) -> Option<Decision> {
        Some(match s {
            "allow" => Decision::Allow,
            "deny" => Decision::Deny,
            "N/I" => Decision::NotImplemented,
            _ => return None,
        })
    }
}
