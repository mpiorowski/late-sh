use super::*;

fn names(matches: &[MentionMatch]) -> Vec<&str> {
    matches.iter().map(|m| m.name.as_str()).collect()
}

/// Minimal `ChatRoom` for scope tests; only `slug` affects command matching.
fn room_with_slug(slug: Option<&str>) -> ChatRoom {
    ChatRoom {
        id: uuid::Uuid::from_u128(1),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        kind: "topic".to_string(),
        visibility: "public".to_string(),
        auto_join: false,
        permanent: false,
        slug: slug.map(str::to_string),
        language_code: None,
        dm_user_a: None,
        dm_user_b: None,
        title: None,
        about: None,
        rules: None,
        created_by: None,
    }
}

#[test]
fn rank_command_matches_lists_user_commands_for_empty_query() {
    let ranked = rank_command_matches("", None);
    let ranked_names = names(&ranked);
    assert_eq!(
        ranked_names.iter().copied().take(4).collect::<Vec<_>>(),
        vec!["active", "aquarium", "binds", "brb"]
    );
    let mut sorted = ranked_names.clone();
    sorted.sort_unstable();
    assert_eq!(ranked_names, sorted);
    assert!(ranked.iter().all(|m| m.prefix == "/"));
    assert!(ranked.iter().all(|m| m.description.is_some()));
    assert!(ranked_names.contains(&"petname"));
    assert!(ranked_names.contains(&"poll"));
    assert!(!ranked_names.contains(&"create-room"));
    assert!(!ranked_names.contains(&"delete-room"));
    assert!(!ranked_names.contains(&"fill-room"));
    assert!(!ranked_names.contains(&"music"));
}

#[test]
fn rank_command_matches_excludes_admin_commands() {
    assert!(rank_command_matches("delete", None).is_empty());
    assert!(rank_command_matches("fill", None).is_empty());
}

#[test]
fn rank_command_matches_hides_exact_command() {
    assert!(rank_command_matches("exit", None).is_empty());
    assert_eq!(names(&rank_command_matches("ex", None)), vec!["exit"]);
}

#[test]
fn command_scope_availability() {
    let dnd = room_with_slug(Some("dnd"));
    let other = room_with_slug(Some("lounge"));
    let no_slug = room_with_slug(None);

    let room = CommandScope::Room(RoomScopedCommand::Sheet);
    assert!(room.available_in(Some(&dnd)));
    assert!(!room.available_in(Some(&other)));
    assert!(!room.available_in(Some(&no_slug)));
    assert!(!room.available_in(None));

    // Global is available everywhere, including with no resolvable room.
    assert!(CommandScope::Global.available_in(None));
    assert!(CommandScope::Global.available_in(Some(&other)));
}

#[test]
fn rank_command_matches_includes_room_command_in_owning_room() {
    let dnd = room_with_slug(Some("dnd"));
    let ranked = rank_command_matches("sh", Some(&dnd));
    let sheet = ranked
        .iter()
        .find(|m| m.name == "sheet")
        .expect("/sheet should be available in #dnd");
    assert_eq!(sheet.prefix, "/");
    assert_eq!(sheet.description, Some("view character sheets"));
}

#[test]
fn rank_command_matches_excludes_room_command_elsewhere() {
    let other = room_with_slug(Some("lounge"));
    assert!(!names(&rank_command_matches("sh", Some(&other))).contains(&"sheet"));
    assert!(!names(&rank_command_matches("sh", None)).contains(&"sheet"));
}

#[test]
fn rank_command_matches_hides_exact_room_command() {
    let dnd = room_with_slug(Some("dnd"));
    assert!(rank_command_matches("sheet", Some(&dnd)).is_empty());
}

#[test]
fn room_owns_command_only_in_owning_room() {
    let dnd = room_with_slug(Some("dnd"));
    let other = room_with_slug(Some("lounge"));

    assert!(room_owns_command(&dnd, "sheet"));
    assert!(!room_owns_command(&other, "sheet"));
    // global commands are never "owned" by a room
    assert!(!room_owns_command(&dnd, "active"));
    // unknown command name
    assert!(!room_owns_command(&dnd, "nope"));
}

#[test]
fn room_scoped_command_metadata_is_consistent() {
    let command = room_scoped_command_named("sheet").expect("sheet command");
    assert_eq!(command.name(), "sheet");
    assert_eq!(command.description(), "view character sheets");
    assert_eq!(command.room_slug(), "dnd");
}

#[test]
fn room_scoped_commands_are_registered() {
    for command in RoomScopedCommand::ALL {
        assert!(
            COMMANDS.iter().any(
                |entry| matches!(entry.scope, CommandScope::Room(registered) if registered == *command)
            ),
            "room-scoped command /{} is missing from COMMANDS",
            command.name()
        );
    }

    for entry in COMMANDS.iter().filter_map(|entry| match entry.scope {
        CommandScope::Room(command) => Some(command),
        CommandScope::Global => None,
    }) {
        assert!(
            RoomScopedCommand::ALL.contains(&entry),
            "COMMANDS contains untracked room-scoped command /{}",
            entry.name()
        );
    }
}

#[test]
fn room_commands_do_not_shadow_global_commands() {
    // A room command sharing a name with a global command would be matched
    // by the global handler in `submit_composer` first, silently defeating
    // room scoping. Keep the two command namespaces disjoint.
    let globals: Vec<&str> = COMMANDS
        .iter()
        .filter(|cmd| matches!(cmd.scope, CommandScope::Global))
        .map(|cmd| cmd.name)
        .collect();
    for cmd in COMMANDS
        .iter()
        .filter(|cmd| matches!(cmd.scope, CommandScope::Room(_)))
    {
        assert!(
            !globals.contains(&cmd.name),
            "room command /{} collides with a global command",
            cmd.name
        );
    }
}
