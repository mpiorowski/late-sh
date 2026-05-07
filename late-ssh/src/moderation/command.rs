use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModCommand {
    Help {
        topic: Option<String>,
    },
    User {
        username: String,
    },
    Bans {
        scope: BanListScope,
        limit: i64,
    },
    Audit {
        limit: i64,
    },
    RenameRoom {
        slug: String,
        new_slug: String,
    },
    RenameUser {
        username: String,
        new_username: String,
    },
    RoomAction {
        action: RoomModAction,
        slug: String,
        username: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    ServerUser {
        action: ServerUserAction,
        username: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    Artboard {
        action: ArtboardAction,
        username: String,
        duration: Option<chrono::Duration>,
        reason: String,
    },
    ArtboardRestore {
        date: Option<chrono::NaiveDate>,
        reason: String,
    },
    Role {
        action: RoleAction,
        username: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BanListScope {
    All,
    Server,
    Room { slug: String },
    Artboard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomModAction {
    Kick,
    Ban,
    Unban,
}

impl RoomModAction {
    pub(crate) const fn past_tense(self) -> &'static str {
        match self {
            Self::Kick => "kicked",
            Self::Ban => "banned",
            Self::Unban => "unbanned",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerUserAction {
    Kick,
    Ban,
    Unban,
}

impl ServerUserAction {
    pub(crate) const fn past_tense(self) -> &'static str {
        match self {
            Self::Kick => "kicked",
            Self::Ban => "banned",
            Self::Unban => "unbanned",
        }
    }

    pub(crate) const fn audit_name(self) -> &'static str {
        match self {
            Self::Kick => "server_kick",
            Self::Ban => "server_ban",
            Self::Unban => "server_unban",
        }
    }

    pub(crate) const fn termination_reason(self) -> &'static str {
        match self {
            Self::Kick => "server kick",
            Self::Ban => "server ban",
            Self::Unban => "server unban",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtboardAction {
    Ban,
    Unban,
}

impl ArtboardAction {
    pub(crate) const fn past_tense(self) -> &'static str {
        match self {
            Self::Ban => "artboard-banned",
            Self::Unban => "removed artboard ban for",
        }
    }

    pub(crate) const fn audit_name(self) -> &'static str {
        match self {
            Self::Ban => "artboard_ban",
            Self::Unban => "artboard_unban",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoleAction {
    GrantMod,
    RevokeMod,
}

impl RoleAction {
    pub(crate) const fn audit_name(self) -> &'static str {
        match self {
            Self::GrantMod => "grant_moderator",
            Self::RevokeMod => "revoke_moderator",
        }
    }
}

pub(crate) fn parse_mod_command(input: &str) -> Result<ModCommand> {
    let input = input.trim();
    let input = if input == "/mod" {
        ""
    } else {
        input.strip_prefix("/mod ").map(str::trim).unwrap_or(input)
    };
    if input.is_empty() {
        return Ok(ModCommand::Help { topic: None });
    }

    let mut parts = input.split_whitespace();
    let Some(head) = parts.next() else {
        return Ok(ModCommand::Help { topic: None });
    };
    let rest = parts.collect::<Vec<_>>();

    match head {
        "help" => Ok(ModCommand::Help {
            topic: nonempty(rest.join(" ")),
        }),
        "user" => Ok(ModCommand::User {
            username: required_username(rest.first().copied(), "usage: user @name")?,
        }),
        "bans" => parse_bans_mod_command(&rest),
        "audit" => parse_audit_mod_command(&rest),
        "rename-room" => parse_rename_room_mod_command(&rest),
        "rename-user" => parse_rename_user_mod_command(&rest),
        "room" => parse_room_mod_command(&rest),
        "server" => parse_server_mod_command(&rest),
        "artboard" => parse_artboard_mod_command(&rest),
        "grant" => parse_role_mod_command(RoleAction::GrantMod, &rest),
        "revoke" => parse_role_mod_command(RoleAction::RevokeMod, &rest),
        _ => anyhow::bail!("unknown mod command: {head}"),
    }
}

fn parse_bans_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        return Ok(ModCommand::Bans {
            scope: BanListScope::All,
            limit: DEFAULT_LIST_LIMIT,
        });
    };

    if let Some(limit) = parse_limit(first)? {
        if parts.len() > 1 {
            anyhow::bail!("usage: bans [server|artboard|room #slug] [limit]");
        }
        return Ok(ModCommand::Bans {
            scope: BanListScope::All,
            limit,
        });
    }

    match first {
        "server" => {
            if parts.len() > 2 {
                anyhow::bail!("usage: bans server [limit]");
            }
            Ok(ModCommand::Bans {
                scope: BanListScope::Server,
                limit: optional_limit(parts.get(1).copied())?,
            })
        }
        "artboard" => {
            if parts.len() > 2 {
                anyhow::bail!("usage: bans artboard [limit]");
            }
            Ok(ModCommand::Bans {
                scope: BanListScope::Artboard,
                limit: optional_limit(parts.get(1).copied())?,
            })
        }
        "room" => {
            if parts.len() > 3 {
                anyhow::bail!("usage: bans room #slug [limit]");
            }
            Ok(ModCommand::Bans {
                scope: BanListScope::Room {
                    slug: required_slug(parts.get(1).copied(), "usage: bans room #slug [limit]")?,
                },
                limit: optional_limit(parts.get(2).copied())?,
            })
        }
        _ => anyhow::bail!("unknown bans scope: {first}"),
    }
}

fn parse_audit_mod_command(parts: &[&str]) -> Result<ModCommand> {
    if parts.len() > 1 {
        anyhow::bail!("usage: audit [limit]");
    }
    Ok(ModCommand::Audit {
        limit: optional_limit(parts.first().copied())?,
    })
}

fn parse_rename_room_mod_command(parts: &[&str]) -> Result<ModCommand> {
    if parts.len() != 2 {
        anyhow::bail!("usage: rename-room #old #new");
    }
    Ok(ModCommand::RenameRoom {
        slug: required_slug(parts.first().copied(), "usage: rename-room #old #new")?,
        new_slug: required_slug(parts.get(1).copied(), "usage: rename-room #old #new")?,
    })
}

fn parse_rename_user_mod_command(parts: &[&str]) -> Result<ModCommand> {
    if parts.len() != 2 {
        anyhow::bail!("usage: rename-user @old @new");
    }
    Ok(ModCommand::RenameUser {
        username: required_username(parts.first().copied(), "usage: rename-user @old @new")?,
        new_username: required_username(parts.get(1).copied(), "usage: rename-user @old @new")?,
    })
}

fn parse_room_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: room #slug | room <action> ...");
    };
    match first {
        "kick" | "ban" | "unban" => {
            let action = match first {
                "kick" => RoomModAction::Kick,
                "ban" => RoomModAction::Ban,
                "unban" => RoomModAction::Unban,
                _ => unreachable!(),
            };
            let slug = required_slug(parts.get(1).copied(), "usage: room kick #slug @name")?;
            let username =
                required_username(parts.get(2).copied(), "usage: room kick #slug @name")?;
            let (duration, reason_start) = if matches!(action, RoomModAction::Ban) {
                parse_optional_duration(parts.get(3).copied(), 3)?
            } else {
                (None, 3)
            };
            Ok(ModCommand::RoomAction {
                action,
                slug,
                username,
                duration,
                reason: parts.get(reason_start..).unwrap_or_default().join(" "),
            })
        }
        _ => anyhow::bail!("unknown room action: {first}"),
    }
}

fn parse_server_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: server <kick|ban|unban> @name");
    };
    let action = match first {
        "kick" => ServerUserAction::Kick,
        "ban" => ServerUserAction::Ban,
        "unban" => ServerUserAction::Unban,
        _ => anyhow::bail!("unknown server action: {first}"),
    };
    let username = required_username(parts.get(1).copied(), "usage: server <action> @name")?;
    let (duration, reason_start) = if matches!(action, ServerUserAction::Ban) {
        parse_optional_duration(parts.get(2).copied(), 2)?
    } else {
        (None, 2)
    };
    Ok(ModCommand::ServerUser {
        action,
        username,
        duration,
        reason: parts.get(reason_start..).unwrap_or_default().join(" "),
    })
}

fn parse_artboard_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: artboard <ban|unban|restore> ...");
    };
    if first == "restore" {
        return parse_artboard_restore_mod_command(&parts[1..]);
    }
    let action = match first {
        "ban" => ArtboardAction::Ban,
        "unban" => ArtboardAction::Unban,
        _ => anyhow::bail!("unknown artboard action: {first}"),
    };
    let username = required_username(parts.get(1).copied(), "usage: artboard <action> @name")?;
    let (duration, reason_start) = if matches!(action, ArtboardAction::Ban) {
        parse_optional_duration(parts.get(2).copied(), 2)?
    } else {
        (None, 2)
    };
    Ok(ModCommand::Artboard {
        action,
        username,
        duration,
        reason: parts.get(reason_start..).unwrap_or_default().join(" "),
    })
}

fn parse_artboard_restore_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let (date, reason_start) = match parts.first().copied() {
        Some(value) => match chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d") {
            Ok(date) => (Some(date), 1),
            Err(_) => (None, 0),
        },
        None => (None, 0),
    };
    let reason = parts.get(reason_start..).unwrap_or_default().join(" ");
    if reason.trim().is_empty() {
        anyhow::bail!("usage: artboard restore [YYYY-MM-DD] <reason...>");
    }
    Ok(ModCommand::ArtboardRestore { date, reason })
}

fn parse_role_mod_command(mod_action: RoleAction, parts: &[&str]) -> Result<ModCommand> {
    let Some(role) = parts.first().copied() else {
        anyhow::bail!("usage: grant mod @name | revoke mod @name");
    };
    let action = match role {
        "mod" | "moderator" => mod_action,
        "admin" => anyhow::bail!("grant admin is deferred"),
        _ => anyhow::bail!("unknown role: {role}"),
    };
    Ok(ModCommand::Role {
        action,
        username: required_username(parts.get(1).copied(), "usage: grant mod @name")?,
    })
}

fn parse_optional_duration(
    value: Option<&str>,
    duration_index: usize,
) -> Result<(Option<chrono::Duration>, usize)> {
    let Some(value) = value else {
        return Ok((None, duration_index));
    };
    if let Some(duration) = parse_mod_duration(value)? {
        Ok((Some(duration), duration_index + 1))
    } else {
        Ok((None, duration_index))
    }
}

fn parse_mod_duration(value: &str) -> Result<Option<chrono::Duration>> {
    if value.is_empty() {
        return Ok(None);
    }
    let Some(unit) = value.chars().last() else {
        return Ok(None);
    };
    if !matches!(unit, 's' | 'm' | 'h' | 'd' | 'S' | 'M' | 'H' | 'D') {
        return Ok(None);
    }
    let amount_text = &value[..value.len() - unit.len_utf8()];
    let Ok(amount) = amount_text.parse::<i64>() else {
        return Ok(None);
    };
    if amount <= 0 {
        anyhow::bail!("duration must be positive");
    }
    let duration = match unit.to_ascii_lowercase() {
        's' => chrono::Duration::seconds(amount),
        'm' => chrono::Duration::minutes(amount),
        'h' => chrono::Duration::hours(amount),
        'd' => chrono::Duration::days(amount),
        _ => unreachable!(),
    };
    Ok(Some(duration))
}

const DEFAULT_LIST_LIMIT: i64 = 25;
const MAX_LIST_LIMIT: i64 = 100;

fn optional_limit(value: Option<&str>) -> Result<i64> {
    let Some(value) = value else {
        return Ok(DEFAULT_LIST_LIMIT);
    };
    parse_limit(value)?.ok_or_else(|| anyhow::anyhow!("limit must be a positive number"))
}

fn parse_limit(value: &str) -> Result<Option<i64>> {
    let Ok(limit) = value.parse::<i64>() else {
        return Ok(None);
    };
    if limit <= 0 {
        anyhow::bail!("limit must be positive");
    }
    Ok(Some(limit.min(MAX_LIST_LIMIT)))
}

fn nonempty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn required_username(value: Option<&str>, usage: &str) -> Result<String> {
    let Some(value) = value else {
        anyhow::bail!("{usage}");
    };
    let username = strip_user_prefix(value);
    if username.is_empty() {
        anyhow::bail!("{usage}");
    }
    Ok(username)
}

fn required_slug(value: Option<&str>, usage: &str) -> Result<String> {
    let Some(value) = value else {
        anyhow::bail!("{usage}");
    };
    let slug = strip_slug_prefix(value);
    if slug.is_empty() {
        anyhow::bail!("{usage}");
    }
    Ok(slug)
}

pub(crate) fn strip_user_prefix(value: &str) -> String {
    value.trim().trim_start_matches('@').to_string()
}

fn strip_slug_prefix(value: &str) -> String {
    value.trim().trim_start_matches('#').to_string()
}

pub(crate) fn normalize_mod_slug(slug: &str) -> Result<String> {
    let slug = strip_slug_prefix(slug).to_ascii_lowercase();
    let slug = slug.trim();
    if slug.is_empty() {
        anyhow::bail!("room slug cannot be empty");
    }

    let mut normalized = String::with_capacity(slug.len());
    let mut last_was_dash = false;
    for ch in slug.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            normalized.push(ch);
            last_was_dash = false;
        } else if ch.is_whitespace() || matches!(ch, '-' | '_' | '.' | '/' | '\\') {
            if !normalized.is_empty() && !last_was_dash {
                normalized.push('-');
                last_was_dash = true;
            }
        } else if !normalized.is_empty() && !last_was_dash {
            normalized.push('-');
            last_was_dash = true;
        }
    }

    let normalized = normalized.trim_matches('-').to_string();
    if normalized.is_empty() {
        anyhow::bail!("room slug cannot be empty");
    }
    Ok(normalized)
}

pub(crate) fn mod_help_lines(topic: Option<&str>) -> Vec<String> {
    let Some(topic) = topic
        .map(normalize_help_topic)
        .filter(|topic| !topic.is_empty())
    else {
        return help_lines(&[
            "help [command]",
            "user @name",
            "bans [server|artboard|room #slug] [limit]",
            "audit [limit]",
            "rename-room #old #new",
            "rename-user @old @new",
            "room kick #slug @name [reason...]",
            "room ban #slug @name [duration] [reason...]",
            "room unban #slug @name",
            "server kick @name [reason...]",
            "server ban @name [duration] [reason...]",
            "server unban @name",
            "artboard ban @name [duration] [reason...]",
            "artboard unban @name",
            "artboard restore [YYYY-MM-DD] [reason...]",
            "grant mod @name",
            "revoke mod @name",
            "",
            "Use help <command> for details, e.g. help room ban.",
        ]);
    };

    let lines: &[&str] = match topic.as_str() {
        "help" => &[
            "help [command]",
            "Shows the command list or focused help for one command.",
            "command: optional command/subcommand, e.g. user, room ban, server ban.",
        ],
        "user" => &[
            "user @name",
            "Shows one user's id, roles, timestamps, and active server/artboard ban flags.",
            "@name: username, with or without @.",
        ],
        "bans" => &[
            "bans [server|artboard|room #slug] [limit]",
            "Lists current active bans. Without a scope, shows server, artboard, and room bans.",
            "limit: optional positive number; capped at 100; default 25.",
        ],
        "bans server" => &[
            "bans server [limit]",
            "Lists active server bans with actor, expiry, and reason.",
        ],
        "bans artboard" => &[
            "bans artboard [limit]",
            "Lists active artboard bans with actor, expiry, and reason.",
        ],
        "bans room" => &["bans room #slug [limit]", "Lists active bans for one room."],
        "audit" => &[
            "audit [limit]",
            "Lists recent moderation audit log entries.",
            "limit: optional positive number; capped at 100; default 25.",
        ],
        "rename-room" => &[
            "rename-room #old #new",
            "Renames a non-DM room by changing its #slug.",
            "Moderator or admin only. #general is reserved and cannot be renamed.",
        ],
        "rename-user" => &[
            "rename-user @old @new",
            "Renames a user account.",
            "@old: existing username. @new: desired username; sanitized with normal username rules.",
            "Moderator or admin only. Writes a moderation audit entry.",
        ],
        "room" => &[
            "room <kick|ban|unban> #slug @name",
            "Subcommands: room kick, room ban, room unban.",
        ],
        "room kick" => &[
            "room kick #slug @name [reason...]",
            "Removes a user from a room without creating a ban.",
            "#slug: room slug. @name: username. reason: optional audit text.",
        ],
        "room ban" => &[
            "room ban #slug @name [duration] [reason...]",
            "Bans a user from a room and removes their membership.",
            "#slug: room slug. @name: username.",
            "duration: optional positive number plus s/m/h/d, e.g. 30m or 7d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "room unban" => &[
            "room unban #slug @name",
            "Removes an active room ban for a user.",
            "#slug: room slug. @name: username.",
        ],
        "server" => &[
            "server <kick|ban|unban> @name",
            "Applies server-wide session removal or bans.",
            "Subcommands: server kick, server ban, server unban.",
        ],
        "server kick" => &[
            "server kick @name [reason...]",
            "Terminates a user's active sessions without creating a ban.",
            "@name: username. reason: optional audit text.",
        ],
        "server ban" => &[
            "server ban @name [duration] [reason...]",
            "Creates a server user ban and terminates active sessions.",
            "@name: username.",
            "duration: optional positive number plus s/m/h/d, e.g. 2h or 7d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "server unban" => &[
            "server unban @name",
            "Removes active server bans for that user.",
            "@name: username.",
        ],
        "artboard" => &[
            "artboard <ban|unban|restore> ...",
            "Controls artboard access and snapshots.",
            "Subcommands: artboard ban, artboard unban, artboard restore.",
        ],
        "artboard ban" => &[
            "artboard ban @name [duration] [reason...]",
            "Bans a user from the artboard.",
            "@name: username.",
            "duration: optional positive number plus s/m/h/d; omit for permanent.",
            "reason: optional audit text after duration.",
        ],
        "artboard unban" => &[
            "artboard unban @name",
            "Removes an artboard ban for a user.",
            "@name: username.",
        ],
        "artboard restore" => &[
            "artboard restore [YYYY-MM-DD] <reason...>",
            "Restores live Artboard from a daily UTC snapshot.",
            "date: optional daily snapshot date; defaults to previous UTC day.",
            "reason: required audit text.",
            "Moderator or admin only. Writes a moderation audit entry and backs up the previous main row.",
        ],
        "grant" => &[
            "grant mod @name",
            "Grants a role to a user.",
            "@name: username.",
        ],
        "grant mod" => &[
            "grant mod @name",
            "Grants moderator role to a user.",
            "@name: username.",
        ],
        "revoke" | "revoke mod" => &[
            "revoke mod @name",
            "Revokes moderator role from a user.",
            "@name: username.",
        ],
        _ => {
            return vec![
                format!("unknown help topic: {topic}"),
                "try: help".to_string(),
            ];
        }
    };
    help_lines(lines)
}

fn normalize_help_topic(topic: &str) -> String {
    let topic = topic
        .trim()
        .strip_prefix("/mod ")
        .map(str::trim)
        .unwrap_or_else(|| topic.trim());
    topic
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn help_lines(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_string()).collect()
}

#[cfg(test)]
mod tests {
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
            parse_mod_command("help server ban").unwrap(),
            ModCommand::Help {
                topic: Some("server ban".to_string())
            }
        );
        assert_eq!(
            parse_mod_command("help room ban").unwrap(),
            ModCommand::Help {
                topic: Some("room ban".to_string())
            }
        );
        assert!(parse_mod_command("/moderator help").is_err());
    }

    #[test]
    fn command_help_explains_audit_arguments() {
        let lines = mod_help_lines(Some("audit"));

        assert!(
            lines.iter().any(|line| line == "audit [limit]"),
            "audit help should be available: {lines:?}"
        );
        assert!(
            lines.iter().any(|line| line.contains("capped at 100")),
            "audit help should explain limit bounds: {lines:?}"
        );
    }

    #[test]
    fn command_help_explains_server_ban_arguments() {
        let lines = mod_help_lines(Some("server ban"));

        assert!(
            lines
                .iter()
                .any(|line| line == "server ban @name [duration] [reason...]")
        );
        assert!(
            lines.iter().any(|line| line.contains("s/m/h/d")),
            "server ban help should explain duration syntax: {lines:?}"
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
            parse_mod_command("room ban #lobby @alice 7d cleanup").unwrap(),
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
    }

    #[test]
    fn parses_server_permanent_ban_without_duration() {
        assert_eq!(
            parse_mod_command("server ban @alice policy").unwrap(),
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
            parse_mod_command("server ban @alice spam wave").unwrap(),
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
            parse_mod_command("server kick @alice go outside").unwrap(),
            ModCommand::ServerUser {
                action: ServerUserAction::Kick,
                username: "alice".to_string(),
                duration: None,
                reason: "go outside".to_string(),
            }
        );
        assert!(parse_mod_command("server disconnect @alice").is_err());
    }

    #[test]
    fn parses_ban_listing_commands() {
        assert_eq!(
            parse_mod_command("bans").unwrap(),
            ModCommand::Bans {
                scope: BanListScope::All,
                limit: DEFAULT_LIST_LIMIT,
            }
        );
        assert_eq!(
            parse_mod_command("bans room #lobby 200").unwrap(),
            ModCommand::Bans {
                scope: BanListScope::Room {
                    slug: "lobby".to_string()
                },
                limit: MAX_LIST_LIMIT,
            }
        );
        assert_eq!(
            parse_mod_command("bans server 3").unwrap(),
            ModCommand::Bans {
                scope: BanListScope::Server,
                limit: 3,
            }
        );
        assert!(parse_mod_command("bans topic").is_err());
    }

    #[test]
    fn parses_audit_listing_commands() {
        assert_eq!(
            parse_mod_command("audit").unwrap(),
            ModCommand::Audit {
                limit: DEFAULT_LIST_LIMIT,
            }
        );
        assert_eq!(
            parse_mod_command("audit 5").unwrap(),
            ModCommand::Audit { limit: 5 }
        );
        assert!(parse_mod_command("audit nope").is_err());
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
        assert!(parse_mod_command("artboard restore").is_err());
        assert!(parse_mod_command("artboard restore 2026-05-06").is_err());
    }

    #[test]
    fn rejects_deferred_server_ip_commands() {
        assert!(parse_mod_command("server ban-ip 203.0.113.10 2h subnet abuse").is_err());
        assert!(parse_mod_command("server unban-ip 2001:db8::1").is_err());
    }
}
