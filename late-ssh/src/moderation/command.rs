use anyhow::Result;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ModCommand {
    Help {
        topic: Option<String>,
    },
    User {
        username: String,
    },
    RoomInfo {
        slug: String,
    },
    Bans {
        scope: BanListScope,
        page: i64,
    },
    Audit {
        page: i64,
    },
    ArtboardSnapshots {
        page: i64,
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
        "view" => parse_view_mod_command(&rest),
        "rename-room" => parse_rename_room_mod_command(&rest),
        "rename-user" => parse_rename_user_mod_command(&rest),
        "kick" => parse_kick_mod_command(&rest),
        "ban" => parse_ban_mod_command(&rest),
        "unban" => parse_unban_mod_command(&rest),
        "artboard" => parse_artboard_mod_command(&rest),
        "admin" => parse_admin_mod_command(&rest),
        _ => anyhow::bail!("unknown mod command: {head}"),
    }
}

fn parse_view_mod_command(parts: &[&str]) -> Result<ModCommand> {
    const USAGE: &str = "usage: view <@user|#room|bans|audit|artboard|help> [pagenumber]";
    let Some(target) = parts.first().copied() else {
        anyhow::bail!(USAGE);
    };
    match target {
        "help" => {
            if parts.len() > 1 {
                anyhow::bail!(USAGE);
            }
            Ok(ModCommand::Help {
                topic: Some("view".to_string()),
            })
        }
        "bans" => parse_bans_mod_command(&parts[1..]),
        "audit" => parse_audit_mod_command(&parts[1..]),
        "artboard" => {
            if parts.len() > 2 {
                anyhow::bail!("usage: view artboard [pagenumber]");
            }
            Ok(ModCommand::ArtboardSnapshots {
                page: optional_page(parts.get(1).copied())?,
            })
        }
        _ if target.starts_with('@') => {
            if parts.len() > 1 {
                anyhow::bail!("usage: view @name");
            }
            Ok(ModCommand::User {
                username: required_username(Some(target), "usage: view @name")?,
            })
        }
        _ if target.starts_with('#') => {
            if parts.len() > 1 {
                anyhow::bail!("usage: view #roomname");
            }
            Ok(ModCommand::RoomInfo {
                slug: required_room_target(target, "usage: view #roomname")?,
            })
        }
        _ => anyhow::bail!(USAGE),
    }
}

fn parse_bans_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        return Ok(ModCommand::Bans {
            scope: BanListScope::All,
            page: DEFAULT_PAGE,
        });
    };

    if let Some(page) = parse_page(first)? {
        if parts.len() > 1 {
            anyhow::bail!("usage: view bans [server|artboard|#roomname] [pagenumber]");
        }
        return Ok(ModCommand::Bans {
            scope: BanListScope::All,
            page,
        });
    }

    match first {
        "server" => {
            if parts.len() > 2 {
                anyhow::bail!("usage: view bans server [pagenumber]");
            }
            Ok(ModCommand::Bans {
                scope: BanListScope::Server,
                page: optional_page(parts.get(1).copied())?,
            })
        }
        "artboard" => {
            if parts.len() > 2 {
                anyhow::bail!("usage: view bans artboard [pagenumber]");
            }
            Ok(ModCommand::Bans {
                scope: BanListScope::Artboard,
                page: optional_page(parts.get(1).copied())?,
            })
        }
        _ if first.starts_with('#') => {
            if parts.len() > 2 {
                anyhow::bail!("usage: view bans #roomname [pagenumber]");
            }
            Ok(ModCommand::Bans {
                scope: BanListScope::Room {
                    slug: required_room_target(first, "usage: view bans #roomname [pagenumber]")?,
                },
                page: optional_page(parts.get(1).copied())?,
            })
        }
        _ => anyhow::bail!("unknown bans scope: {first}"),
    }
}

fn parse_audit_mod_command(parts: &[&str]) -> Result<ModCommand> {
    if parts.len() > 1 {
        anyhow::bail!("usage: view audit [pagenumber]");
    }
    Ok(ModCommand::Audit {
        page: optional_page(parts.first().copied())?,
    })
}

fn parse_rename_room_mod_command(parts: &[&str]) -> Result<ModCommand> {
    const USAGE: &str = "usage: rename-room #oldname #newname";
    if parts.len() != 2 {
        anyhow::bail!(USAGE);
    }
    Ok(ModCommand::RenameRoom {
        slug: required_slug(parts.first().copied(), USAGE)?,
        new_slug: required_slug(parts.get(1).copied(), USAGE)?,
    })
}

fn parse_rename_user_mod_command(parts: &[&str]) -> Result<ModCommand> {
    const USAGE: &str = "usage: rename-user @oldname @newname";
    if parts.len() != 2 {
        anyhow::bail!(USAGE);
    }
    Ok(ModCommand::RenameUser {
        username: required_username(parts.first().copied(), USAGE)?,
        new_username: required_username(parts.get(1).copied(), USAGE)?,
    })
}

fn parse_kick_mod_command(parts: &[&str]) -> Result<ModCommand> {
    const USAGE: &str = "usage: kick <server|#roomname> @name [reason...]";
    let Some(target) = parts.first().copied() else {
        anyhow::bail!(USAGE);
    };
    let username = required_username(parts.get(1).copied(), USAGE)?;
    let reason = parts.get(2..).unwrap_or_default().join(" ");
    if target == "server" {
        return Ok(ModCommand::ServerUser {
            action: ServerUserAction::Kick,
            username,
            duration: None,
            reason,
        });
    }
    if target.starts_with('#') {
        return Ok(ModCommand::RoomAction {
            action: RoomModAction::Kick,
            slug: required_room_target(target, USAGE)?,
            username,
            duration: None,
            reason,
        });
    }
    anyhow::bail!(USAGE)
}

fn parse_ban_mod_command(parts: &[&str]) -> Result<ModCommand> {
    const USAGE: &str = "usage: ban <server|#roomname|artboard> @name [duration] [reason...]";
    let Some(target) = parts.first().copied() else {
        anyhow::bail!(USAGE);
    };
    let username = required_username(parts.get(1).copied(), USAGE)?;
    let (duration, reason_start) = parse_optional_duration(parts.get(2).copied(), 2)?;
    let reason = parts.get(reason_start..).unwrap_or_default().join(" ");
    match target {
        "server" => Ok(ModCommand::ServerUser {
            action: ServerUserAction::Ban,
            username,
            duration,
            reason,
        }),
        "artboard" => Ok(ModCommand::Artboard {
            action: ArtboardAction::Ban,
            username,
            duration,
            reason,
        }),
        _ if target.starts_with('#') => Ok(ModCommand::RoomAction {
            action: RoomModAction::Ban,
            slug: required_room_target(target, USAGE)?,
            username,
            duration,
            reason,
        }),
        _ => anyhow::bail!(USAGE),
    }
}

fn parse_unban_mod_command(parts: &[&str]) -> Result<ModCommand> {
    const USAGE: &str = "usage: unban <server|#roomname|artboard> @name [reason...]";
    let Some(target) = parts.first().copied() else {
        anyhow::bail!(USAGE);
    };
    let username = required_username(parts.get(1).copied(), USAGE)?;
    let reason = parts.get(2..).unwrap_or_default().join(" ");
    match target {
        "server" => Ok(ModCommand::ServerUser {
            action: ServerUserAction::Unban,
            username,
            duration: None,
            reason,
        }),
        "artboard" => Ok(ModCommand::Artboard {
            action: ArtboardAction::Unban,
            username,
            duration: None,
            reason,
        }),
        _ if target.starts_with('#') => Ok(ModCommand::RoomAction {
            action: RoomModAction::Unban,
            slug: required_room_target(target, USAGE)?,
            username,
            duration: None,
            reason,
        }),
        _ => anyhow::bail!(USAGE),
    }
}

fn parse_artboard_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(first) = parts.first().copied() else {
        anyhow::bail!("usage: artboard restore [YYYY-MM-DD] [reason...]");
    };
    if first == "restore" {
        return parse_artboard_restore_mod_command(&parts[1..]);
    }
    anyhow::bail!("usage: artboard restore [YYYY-MM-DD] [reason...]")
}

fn required_room_target(value: &str, usage: &str) -> Result<String> {
    if !value.starts_with('#') {
        anyhow::bail!(usage.to_string());
    }
    required_slug(Some(value), usage)
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
    Ok(ModCommand::ArtboardRestore { date, reason })
}

fn parse_admin_mod_command(parts: &[&str]) -> Result<ModCommand> {
    let Some(action) = parts.first().copied() else {
        return Ok(ModCommand::Help {
            topic: Some("admin".to_string()),
        });
    };
    match action {
        "grant" => parse_role_mod_command(RoleAction::GrantMod, &parts[1..]),
        "revoke" => parse_role_mod_command(RoleAction::RevokeMod, &parts[1..]),
        _ => anyhow::bail!("unknown admin command: {action}"),
    }
}

fn parse_role_mod_command(mod_action: RoleAction, parts: &[&str]) -> Result<ModCommand> {
    let Some(role) = parts.first().copied() else {
        anyhow::bail!("usage: admin grant mod @name | admin revoke mod @name");
    };
    let action = match role {
        "mod" | "moderator" => mod_action,
        "admin" => anyhow::bail!("grant admin is deferred"),
        _ => anyhow::bail!("unknown role: {role}"),
    };
    Ok(ModCommand::Role {
        action,
        username: required_username(parts.get(1).copied(), "usage: admin grant mod @name")?,
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

pub(crate) const LIST_PAGE_SIZE: i64 = 15;
const DEFAULT_PAGE: i64 = 1;
const MAX_PAGE: i64 = 1000;

fn optional_page(value: Option<&str>) -> Result<i64> {
    let Some(value) = value else {
        return Ok(DEFAULT_PAGE);
    };
    parse_page(value)?.ok_or_else(|| anyhow::anyhow!("page number must be a positive number"))
}

fn parse_page(value: &str) -> Result<Option<i64>> {
    let Ok(page) = value.parse::<i64>() else {
        return Ok(None);
    };
    if page <= 0 {
        anyhow::bail!("page number must be positive");
    }
    Ok(Some(page.min(MAX_PAGE)))
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
        anyhow::bail!("room name cannot be empty");
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
        anyhow::bail!("room name cannot be empty");
    }
    Ok(normalized)
}

pub(crate) fn mod_help_lines(topic: Option<&str>) -> Vec<String> {
    let Some(topic) = topic
        .map(normalize_help_topic)
        .filter(|topic| !topic.is_empty())
    else {
        return help_lines(&[
            "--- general ---",
            "rename-room <#oldname> <#newname>",
            "rename-user <@oldname> <@newname>",
            "view   <@user|#room|bans|audit|artboard|help> [pagenumber]",
            "artboard restore [YYYY-MM-DD] [reason...]",
            "",
            "--- bans, etc. ---",
            "kick   <server|#room> @name [reason...]",
            "ban    <server|#room|artboard> @name [duration] [reason...]",
            "unban  <server|#room|artboard> @name [reason...]",
            "",
            "--- help & admin ---",
            "admin           - show admin commands",
            "admin <command> - run admin commands",
            "help <command>  - get help with command",
        ]);
    };

    let lines: &[&str] = match topic.as_str() {
        "help" => &[
            "help <command>",
            "Shows the command list or focused help for one command.",
            "command: e.g. rename-room, view, ban, artboard restore, admin grant mod.",
        ],
        "rename" => &[
            "rename-room <#oldname> <#newname>",
            "rename-user <@oldname> <@newname>",
            "Renames rooms or users.",
            "Subtopics: help rename-room, help rename-user.",
        ],
        "rename-room" | "rename room" => &[
            "rename-room #oldname #newname",
            "Renames a non-DM room, e.g. #old-room to #new-room.",
            "Moderator or admin only. #general is reserved and cannot be renamed.",
        ],
        "rename-user" | "rename user" => &[
            "rename-user @oldname @newname",
            "Renames a user account.",
            "@oldname: existing username; bare oldname is also accepted.",
            "@newname: desired username; bare newname is also accepted and sanitized with normal username rules.",
            "Moderator or admin only. Writes a moderation audit entry.",
        ],
        "view" => &[
            "view <@user|#room|bans|audit|artboard|help> [pagenumber]",
            "Views moderation data.",
            "Subtopics: help view user, help view room, help view bans, help view audit, help view artboard.",
        ],
        "view user" => &[
            "view @name",
            "Shows one user's id, roles, timestamps, and active server/artboard ban flags.",
            "@name: username; bare name is also accepted.",
        ],
        "view room" => &[
            "view #roomname",
            "Shows room id, type, visibility, flags, and member count.",
        ],
        "view bans" => &[
            "view bans [server|artboard|#roomname] [pagenumber]",
            "Lists current active bans. Without a scope, shows server, artboard, and room bans.",
            "pagenumber: optional positive page number; 15 rows per page.",
        ],
        "view bans server" => &[
            "view bans server [pagenumber]",
            "Lists active server bans with actor, expiry, and reason.",
        ],
        "view bans artboard" => &[
            "view bans artboard [pagenumber]",
            "Lists active artboard bans with actor, expiry, and reason.",
        ],
        "view bans room" => &[
            "view bans #roomname [pagenumber]",
            "Lists active bans for one room, e.g. #general.",
        ],
        "view audit" => &[
            "view audit [pagenumber]",
            "Lists recent moderation audit log entries.",
            "pagenumber: optional positive page number; 15 rows per page.",
        ],
        "view artboard" => &[
            "view artboard [pagenumber]",
            "Lists daily and monthly Artboard snapshots.",
            "pagenumber: optional positive page number; 15 rows per page.",
        ],
        "kick" => &[
            "kick <server|#room> @name [reason...]",
            "Terminates active sessions for server, or removes a user from a room.",
            "#roomname is required for room operations, e.g. #general.",
            "@name: username; bare name is also accepted. reason: optional audit text.",
            "Subtopics: help kick server, help kick room.",
        ],
        "kick server" => &[
            "kick server @name [reason...]",
            "Terminates active sessions for one user.",
        ],
        "kick room" => &[
            "kick #roomname @name [reason...]",
            "Removes one user from one room.",
        ],
        "ban" => &[
            "ban <server|#room|artboard> @name [duration] [reason...]",
            "Creates a server, artboard, or room ban. Room bans also remove membership.",
            "#roomname is required for room operations, e.g. #general.",
            "@name: username; bare name is also accepted.",
            "duration: optional positive number plus s/m/h/d, e.g. 30m or 7d; omit for permanent.",
            "reason: optional audit text after duration.",
            "Subtopics: help ban server, help ban room, help ban artboard.",
        ],
        "ban server" => &[
            "ban server @name [duration] [reason...]",
            "Creates a server ban and terminates active sessions.",
        ],
        "ban room" => &[
            "ban #roomname @name [duration] [reason...]",
            "Creates a room ban and removes membership.",
        ],
        "ban artboard" => &[
            "ban artboard @name [duration] [reason...]",
            "Creates an Artboard editing ban.",
        ],
        "unban" => &[
            "unban <server|#room|artboard> @name [reason...]",
            "Removes active server, artboard, or room bans for a user.",
            "#roomname is required for room operations, e.g. #general.",
            "@name: username; bare name is also accepted. reason: optional audit text.",
            "Subtopics: help unban server, help unban room, help unban artboard.",
        ],
        "unban server" => &[
            "unban server @name [reason...]",
            "Removes active server bans for one user.",
        ],
        "unban room" => &[
            "unban #roomname @name [reason...]",
            "Removes active room bans for one user in one room.",
        ],
        "unban artboard" => &[
            "unban artboard @name [reason...]",
            "Removes active Artboard editing bans for one user.",
        ],
        "artboard" => &[
            "artboard restore [YYYY-MM-DD] [reason...]",
            "Restores Artboard snapshots.",
            "Subtopics: help artboard restore.",
        ],
        "artboard restore" => &[
            "artboard restore [YYYY-MM-DD] [reason...]",
            "Restores live Artboard from a daily UTC snapshot.",
            "date: optional daily snapshot date; defaults to previous UTC day.",
            "reason: optional audit text.",
            "Moderator or admin only. Writes a moderation audit entry and backs up the previous main row.",
        ],
        "admin" => &[
            "admin <grant|revoke> mod @name",
            "Admin-only role commands.",
            "Subcommands: admin grant mod, admin revoke mod.",
        ],
        "admin grant" | "admin grant mod" => &[
            "admin grant mod @name",
            "Grants moderator role to a user.",
            "@name: username; bare name is also accepted.",
        ],
        "admin revoke" | "admin revoke mod" => &[
            "admin revoke mod @name",
            "Revokes moderator role from a user.",
            "@name: username; bare name is also accepted.",
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
            lines
                .iter()
                .any(|line| line == "ban <server|#room|artboard> @name [duration] [reason...]")
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
                .any(|line| line == "ban    <server|#room|artboard> @name [duration] [reason...]"),
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
    fn rejects_deferred_server_ip_commands() {
        assert!(parse_mod_command("server ban-ip 203.0.113.10 2h subnet abuse").is_err());
        assert!(parse_mod_command("server unban-ip 2001:db8::1").is_err());
    }

    fn primary_username(command: &ModCommand) -> &str {
        match command {
            ModCommand::User { username }
            | ModCommand::RenameUser { username, .. }
            | ModCommand::RoomAction { username, .. }
            | ModCommand::ServerUser { username, .. }
            | ModCommand::Artboard { username, .. }
            | ModCommand::Role { username, .. } => username,
            ModCommand::Help { .. }
            | ModCommand::RoomInfo { .. }
            | ModCommand::Bans { .. }
            | ModCommand::Audit { .. }
            | ModCommand::ArtboardSnapshots { .. }
            | ModCommand::RenameRoom { .. }
            | ModCommand::ArtboardRestore { .. } => {
                panic!("command does not have a primary username: {command:?}")
            }
        }
    }
}
