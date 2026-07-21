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
    assert!(permissions.has(Caps::RENAME_USER));
    assert!(permissions.has(Caps::RESTORE_ARTBOARD));
    assert!(permissions.has(Caps::DELETE_PINSTAR_GRAPH));
    assert!(permissions.has(Caps::DELETE_AUDIO_TRACK));
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
    assert!(moderator.can_delete_pinstar_graph(false, Tier::Regular));
    assert!(!moderator.can_delete_pinstar_graph(false, Tier::Moderator));
    assert!(moderator.can_delete_audio_track(false));
    assert!(admin.can(Caps::BAN_FROM_ROOM, Tier::Moderator));
    assert!(!admin.can(Caps::BAN_FROM_ROOM, Tier::Admin));
    assert!(admin.can_delete_pinstar_graph(false, Tier::Moderator));
    assert!(!admin.can_delete_pinstar_graph(false, Tier::Admin));
    assert!(admin.can_delete_audio_track(false));
    assert!(Permissions::default().can_delete_audio_track(true));
    assert!(!Permissions::default().can_delete_audio_track(false));
}

#[test]
fn audit_only_privileged_actions_against_others() {
    assert!(!Permissions::default().should_audit(false));
    assert!(!Permissions::new(false, true).should_audit(true));
    assert!(Permissions::new(false, true).should_audit(false));
    assert!(Permissions::new(true, false).should_audit(false));
}
