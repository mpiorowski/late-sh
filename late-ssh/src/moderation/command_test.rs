use super::*;

#[test]
fn parses_optional_mod_prefix() {
    assert_eq!(
        parse_mod_command("/mod help").unwrap(),
        ModCommand::Help { topic: None }
    );
    assert_eq!(
        parse_mod_command("help").unwrap(),
        ModCommand::Help { topic: None }
    );
    assert_eq!(
        parse_mod_command("help ban server").unwrap(),
        ModCommand::Help {
            topic: Some("ban server".to_string())
        }
    );
    assert_eq!(
        parse_mod_command("help ban room").unwrap(),
        ModCommand::Help {
            topic: Some("ban room".to_string())
        }
    );
    assert_eq!(
        parse_mod_command("admin").unwrap(),
        ModCommand::Help {
            topic: Some("admin".to_string())
        }
    );
    assert!(parse_mod_command("/moderator help").is_err());
}

#[test]
fn command_help_explains_audit_arguments() {
    let lines = mod_help_lines(Some("view audit"));

    assert!(
        lines.iter().any(|line| line == "view audit [pagenumber]"),
        "audit help should be available: {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.contains("15 rows per page")),
        "audit help should explain page size: {lines:?}"
    );
}

#[test]
fn command_help_explains_ban_arguments() {
    let lines = mod_help_lines(Some("ban"));

    assert!(
        lines.iter().any(
            |line| line == "ban <server|#room|artboard|audio> @name [duration] [reason...]"
        )
    );
    assert!(
        lines.iter().any(|line| line.contains("s/m/h/d")),
        "ban help should explain duration syntax: {lines:?}"
    );
}

#[test]
fn command_help_uses_limited_grouped_surface() {
    let lines = mod_help_lines(None);

    assert!(
        lines
            .iter()
            .any(|line| line == "rename-room <#oldname> <#newname>"),
        "top-level help should show rename-room command: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line == "rename-user <@oldname> <@newname>"),
        "top-level help should show rename-user command: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line
                == "ban    <server|#room|artboard|audio> @name [duration] [reason...]"),
        "top-level help should show verb-primary ban form: {lines:?}"
    );
}

#[test]
fn command_help_uses_roomname_examples_instead_of_slug_jargon() {
    let lines = [
        mod_help_lines(None),
        mod_help_lines(Some("ban room")),
        mod_help_lines(Some("view bans room")),
    ]
    .concat();

    assert!(
        lines.iter().any(|line| line.contains("#roomname")),
        "help should show room examples with #roomname: {lines:?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains("#slug")),
        "help should avoid #slug wording: {lines:?}"
    );
    assert!(
        lines
            .iter()
            .all(|line| !line.to_ascii_lowercase().contains("room slug")),
        "help should avoid room slug wording: {lines:?}"
    );
}

#[test]
fn normalizes_room_slugs_like_chat_rooms() {
    assert_eq!(normalize_mod_slug("#Rust_Nerds").unwrap(), "rust-nerds");
    assert_eq!(normalize_mod_slug("vps/d9d0").unwrap(), "vps-d9d0");
    assert!(normalize_mod_slug("!!!").is_err());
}

#[test]
fn parses_room_ban_with_duration_and_reason() {
    assert_eq!(
        parse_mod_command("ban #lobby @alice 7d cleanup").unwrap(),
        ModCommand::RoomAction {
            action: RoomModAction::Ban,
            slug: "lobby".to_string(),
            username: "alice".to_string(),
            duration: Some(chrono::Duration::days(7)),
            reason: "cleanup".to_string(),
        }
    );
}

#[test]
fn parses_at_prefixed_usernames_for_all_username_commands() {
    let cases = [
        ("view @alice", "alice"),
        ("rename-user @alice @bob", "alice"),
        ("kick #lobby @alice reason", "alice"),
        ("ban #lobby @alice 7d cleanup", "alice"),
        ("unban #lobby @alice", "alice"),
        ("slow #lobby @alice 90s 1d flood", "alice"),
        ("unslow #lobby @alice", "alice"),
        ("kick server @alice reason", "alice"),
        ("ban server @alice policy", "alice"),
        ("unban server @alice", "alice"),
        ("ban artboard @alice policy", "alice"),
        ("unban artboard @alice", "alice"),
        ("admin grant mod @alice", "alice"),
        ("admin revoke mod @alice", "alice"),
    ];

    for (input, expected_username) in cases {
        assert_eq!(
            primary_username(&parse_mod_command(input).unwrap()),
            expected_username,
            "{input}"
        );
    }

    assert_eq!(
        parse_mod_command("rename-user @alice @bob").unwrap(),
        ModCommand::RenameUser {
            username: "alice".to_string(),
            new_username: "bob".to_string(),
        }
    );
}

#[test]
fn parses_bare_usernames_for_mod_commands() {
    assert_eq!(
        parse_mod_command("ban server alice policy").unwrap(),
        ModCommand::ServerUser {
            action: ServerUserAction::Ban,
            username: "alice".to_string(),
            duration: None,
            reason: "policy".to_string(),
        }
    );
}

#[test]
fn parses_admin_ultimate_cast() {
    assert_eq!(
        parse_mod_command("admin ultimate cast thematrix").unwrap(),
        ModCommand::AdminUltimateCast {
            ultimate_id: "thematrix".to_string()
        }
    );
    assert_eq!(
        parse_mod_command("admin ultimate cast Wonderland").unwrap(),
        ModCommand::AdminUltimateCast {
            ultimate_id: "wonderland".to_string()
        }
    );
    assert!(parse_mod_command("admin ultimate").is_err());
    assert!(parse_mod_command("admin ultimate cast").is_err());
    assert!(parse_mod_command("admin ultimate cast thematrix extra").is_err());
}

#[test]
fn parses_rename_room_command() {
    assert_eq!(
        parse_mod_command("rename-room #Old_Room #New.Room").unwrap(),
        ModCommand::RenameRoom {
            slug: "Old_Room".to_string(),
            new_slug: "New.Room".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("rename-room #old #new").unwrap(),
        ModCommand::RenameRoom {
            slug: "old".to_string(),
            new_slug: "new".to_string(),
        }
    );
    assert!(parse_mod_command("rename-room #old").is_err());
    assert!(parse_mod_command("rename-room #old #new extra").is_err());
    assert!(parse_mod_command("rename room #old #new").is_err());
}

#[test]
fn parses_rename_user_command() {
    assert_eq!(
        parse_mod_command("rename-user @Old @New.Name").unwrap(),
        ModCommand::RenameUser {
            username: "Old".to_string(),
            new_username: "New.Name".to_string(),
        }
    );
    assert!(parse_mod_command("rename-user @old").is_err());
    assert!(parse_mod_command("rename-user @old @new extra").is_err());
    assert!(parse_mod_command("rename user @old @new").is_err());
}

#[test]
fn parses_server_permanent_ban_without_duration() {
    assert_eq!(
        parse_mod_command("ban server @alice policy").unwrap(),
        ModCommand::ServerUser {
            action: ServerUserAction::Ban,
            username: "alice".to_string(),
            duration: None,
            reason: "policy".to_string(),
        }
    );
}

#[test]
fn parses_reason_that_looks_like_duration_suffix() {
    assert_eq!(
        parse_mod_command("ban server @alice spam wave").unwrap(),
        ModCommand::ServerUser {
            action: ServerUserAction::Ban,
            username: "alice".to_string(),
            duration: None,
            reason: "spam wave".to_string(),
        }
    );
}

#[test]
fn parses_server_kick() {
    assert_eq!(
        parse_mod_command("kick server @alice go outside").unwrap(),
        ModCommand::ServerUser {
            action: ServerUserAction::Kick,
            username: "alice".to_string(),
            duration: None,
            reason: "go outside".to_string(),
        }
    );
    assert!(parse_mod_command("disconnect server @alice").is_err());
}

#[test]
fn parses_ban_listing_commands() {
    assert_eq!(
        parse_mod_command("view bans").unwrap(),
        ModCommand::Bans {
            scope: BanListScope::All,
            page: DEFAULT_PAGE,
        }
    );
    assert_eq!(
        parse_mod_command("view bans #lobby 200").unwrap(),
        ModCommand::Bans {
            scope: BanListScope::Room {
                slug: "lobby".to_string()
            },
            page: 200,
        }
    );
    assert_eq!(
        parse_mod_command("view bans server 3").unwrap(),
        ModCommand::Bans {
            scope: BanListScope::Server,
            page: 3,
        }
    );
    assert!(parse_mod_command("view bans topic").is_err());
    assert!(parse_mod_command("bans").is_err());
}

#[test]
fn parses_slow_mode_commands() {
    assert_eq!(
        parse_mod_command("slow #lobby @alice 90s 1d high volume").unwrap(),
        ModCommand::Slow {
            scope: SlowScope::Room {
                slug: "lobby".to_string()
            },
            username: "alice".to_string(),
            interval_secs: 90,
            expires_in: Some(chrono::Duration::days(1)),
            reason: "high volume".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("slow #lobby @alice 5m permanent").unwrap(),
        ModCommand::Slow {
            scope: SlowScope::Room {
                slug: "lobby".to_string()
            },
            username: "alice".to_string(),
            interval_secs: 300,
            expires_in: None,
            reason: String::new(),
        }
    );
    assert_eq!(
        parse_mod_command("unslow #lobby @alice improved").unwrap(),
        ModCommand::Unslow {
            scope: SlowScope::Room {
                slug: "lobby".to_string()
            },
            username: "alice".to_string(),
            reason: "improved".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("slow server @alice 90s 1d high volume").unwrap(),
        ModCommand::Slow {
            scope: SlowScope::Server,
            username: "alice".to_string(),
            interval_secs: 90,
            expires_in: Some(chrono::Duration::days(1)),
            reason: "high volume".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("unslow server @alice improved").unwrap(),
        ModCommand::Unslow {
            scope: SlowScope::Server,
            username: "alice".to_string(),
            reason: "improved".to_string(),
        }
    );
    assert!(parse_mod_command("slow #lobby @alice 90s").is_err());
    assert!(parse_mod_command("slow #lobby @alice 2d 1d").is_err());
    assert!(parse_mod_command("slow #lobby @alice 90s forever").is_err());
}

#[test]
fn parses_slow_listing_commands() {
    assert_eq!(
        parse_mod_command("view slows").unwrap(),
        ModCommand::Slows {
            scope: SlowListScope::All,
            page: DEFAULT_PAGE,
        }
    );
    assert_eq!(
        parse_mod_command("view slows #lobby 2").unwrap(),
        ModCommand::Slows {
            scope: SlowListScope::Room {
                slug: "lobby".to_string()
            },
            page: 2,
        }
    );
    assert_eq!(
        parse_mod_command("view slows server 2").unwrap(),
        ModCommand::Slows {
            scope: SlowListScope::Server,
            page: 2,
        }
    );
    assert!(parse_mod_command("view slows lounge").is_err());
    assert!(parse_mod_command("slows").is_err());
}

#[test]
fn parses_audit_listing_commands() {
    assert_eq!(
        parse_mod_command("view audit").unwrap(),
        ModCommand::Audit { page: DEFAULT_PAGE }
    );
    assert_eq!(
        parse_mod_command("view audit 5").unwrap(),
        ModCommand::Audit { page: 5 }
    );
    assert!(parse_mod_command("view audit nope").is_err());
    assert!(parse_mod_command("audit").is_err());
}

#[test]
fn parses_artboard_restore_command() {
    assert_eq!(
        parse_mod_command("artboard restore 2026-05-06 rollback vandalism").unwrap(),
        ModCommand::ArtboardRestore {
            date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 6).unwrap()),
            reason: "rollback vandalism".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("artboard restore rollback latest").unwrap(),
        ModCommand::ArtboardRestore {
            date: None,
            reason: "rollback latest".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("artboard restore").unwrap(),
        ModCommand::ArtboardRestore {
            date: None,
            reason: String::new(),
        }
    );
    assert_eq!(
        parse_mod_command("artboard restore 2026-05-06").unwrap(),
        ModCommand::ArtboardRestore {
            date: Some(chrono::NaiveDate::from_ymd_opt(2026, 5, 6).unwrap()),
            reason: String::new(),
        }
    );
}

#[test]
fn parses_artboard_curate_command() {
    assert_eq!(
        parse_mod_command("artboard curate 2026-05-25 saved before cleanup").unwrap(),
        ModCommand::ArtboardCurate {
            source: ArtboardCurateSource::Daily(
                chrono::NaiveDate::from_ymd_opt(2026, 5, 25).unwrap()
            ),
            reason: "saved before cleanup".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("artboard curate live save current").unwrap(),
        ModCommand::ArtboardCurate {
            source: ArtboardCurateSource::Live,
            reason: "save current".to_string(),
        }
    );
    assert!(parse_mod_command("artboard curate").is_err());
    assert!(parse_mod_command("artboard curate save current").is_err());
}

#[test]
fn rejects_deferred_server_ip_commands() {
    assert!(parse_mod_command("server ban-ip 203.0.113.10 2h subnet abuse").is_err());
    assert!(parse_mod_command("server unban-ip 2001:db8::1").is_err());
}

#[test]
fn parses_voice_moderation_commands() {
    assert_eq!(
        parse_mod_command("kick voice @spammer too loud").unwrap(),
        ModCommand::Voice {
            action: VoiceAction::Kick,
            username: "spammer".to_string(),
            reason: "too loud".to_string(),
        }
    );
    assert_eq!(
        parse_mod_command("unban voice @spammer").unwrap(),
        ModCommand::Voice {
            action: VoiceAction::Allow,
            username: "spammer".to_string(),
            reason: String::new(),
        }
    );
    // A target user is required.
    assert!(parse_mod_command("kick voice").is_err());
}

#[test]
fn parses_room_voice_commands() {
    assert_eq!(
        parse_mod_command("room-voice #general on").unwrap(),
        ModCommand::RoomVoice {
            slug: "general".to_string(),
            enabled: true,
        }
    );
    assert_eq!(
        parse_mod_command("room-voice #general off").unwrap(),
        ModCommand::RoomVoice {
            slug: "general".to_string(),
            enabled: false,
        }
    );
    // Needs a room and an on/off state.
    assert!(parse_mod_command("room-voice #general").is_err());
    assert!(parse_mod_command("room-voice #general maybe").is_err());
}

fn primary_username(command: &ModCommand) -> &str {
    match command {
        ModCommand::User { username }
        | ModCommand::RenameUser { username, .. }
        | ModCommand::RoomAction { username, .. }
        | ModCommand::Slow { username, .. }
        | ModCommand::Unslow { username, .. }
        | ModCommand::ServerUser { username, .. }
        | ModCommand::Artboard { username, .. }
        | ModCommand::Audio { username, .. }
        | ModCommand::Voice { username, .. }
        | ModCommand::Role { username, .. } => username,
        ModCommand::Help { .. }
        | ModCommand::AdminUltimateCast { .. }
        | ModCommand::RoomInfo { .. }
        | ModCommand::Bans { .. }
        | ModCommand::Slows { .. }
        | ModCommand::Audit { .. }
        | ModCommand::ArtboardSnapshots { .. }
        | ModCommand::RenameRoom { .. }
        | ModCommand::RoomVoice { .. }
        | ModCommand::ArtboardRestore { .. }
        | ModCommand::ArtboardCurate { .. } => {
            panic!("command does not have a primary username: {command:?}")
        }
    }
}
