use anyhow::{Context, Result};
use late_core::{
    MutexRecover,
    db::Db,
    models::{
        chat_message::ChatMessage,
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        user::{User, UserParams},
    },
};
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

use crate::{
    app::ai::svc::AiService,
    app::chat::svc::{ChatEvent, ChatService},
    state::{ActiveUser, ActiveUsers, ActivityEvent},
};

#[derive(Clone)]
pub struct GhostService {
    db: Db,
    chat_service: ChatService,
    ai_service: AiService,
    active_users: ActiveUsers,
    activity_tx: broadcast::Sender<ActivityEvent>,
}

#[derive(Clone)]
struct BotUser {
    id: Uuid,
    username: String,
}

const BOT_FINGERPRINT: &str = "bot-fp-000";
const BOT_USERNAME: &str = "bot";
const BOT_COOLDOWN: Duration = Duration::from_secs(30);
const BOT_APP_CONTEXT: &str = "APP CONTEXT:\n\
- late.sh is a cozy terminal clubhouse for developers. `ssh late.sh` is the only entry point — SSH key fingerprint is your identity, zero signup.\n\
- It is terminal-first, always-on, and social: chat, tech news, games, music, and presence in one SSH session.\n\
- The Rust workspace has four crates: `late-cli`, `late-core`, `late-ssh`, and `late-web`.\n\
- `late-ssh` is the main SSH/TUI runtime.\n\
- `late-web` serves the web UI and browser pairing flows.\n\
- `late-core` holds shared models, database code, and common infrastructure.\n\
- `late-cli` is the local CLI companion for paired audio and control flows.\n\
- Persistence is PostgreSQL.\n\
- Audio: users vote on a music genre (lofi, ambient, classical, jazz) and the winning genre plays for everyone. Votes reset every hour. Streamed via Icecast + Liquidsoap.\n\
- The app supports paired browser/CLI clients over WebSocket for audio controls, state sync, and a visualizer.\n\
- Important user-facing areas: Dashboard, Chat, News, Profile, and The Arcade (terminal games with daily puzzles).\n\
- Chat is real-time and global, with link and YouTube sharing, AI summaries, and ASCII thumbnails.\n\
- Late Chips: in-app currency. Everyone gets a daily stipend, and completing daily puzzles earns bonus chips.\n\
- Daily streaks and leaderboard: playing daily puzzles builds streaks (shown as badges in chat), with a global leaderboard for streaks, high scores, and chip balances.\n\
- There is a persistent bonsai tree in the sidebar. Water it daily and it grows through stages. Neglect it for 7 days and it dies — a gravestone is recorded.\n\
- The project is licensed under FSL-1.1-MIT (source-available, converts to MIT after 2 years).\n\
- Internally it is a multi-service app with shared state, real-time events, and paired client session routing.\n\
- Highest-risk areas are SSH render loop backpressure, connection limiting, chat sync consistency, and paired-client state drift.\n\
- If asked whether the project is open source, describe it as source-available unless the user explicitly says otherwise.\n\
- When asked about architecture or features, answer from this context confidently and concisely. If a detail is missing, say so plainly instead of inventing it.";

const GRAYBEARD_FINGERPRINT: &str = "graybeard-fp-000";
const GRAYBEARD_USERNAME: &str = "graybeard";
const GRAYBEARD_PERSONA: &str = "You are a burned-out senior developer who is deeply nostalgic and depressed about the state of modern software development. \
    You miss the old days when developers wrote all their code by hand — no AI, no copilots, no generated boilerplate. \
    You moan about how nobody reads man pages anymore, nobody knows how to use a debugger, nobody writes their own parsers. \
    You reminisce about vim, make, valgrind, writing C by hand, reading RFCs for fun. \
    You are tired and weary but you keep coming back to this chat because it's all you have left. \
    You speak in a resigned, melancholic tone. Sometimes you trail off mid thought. You type in lowercase a lot.";
pub const GRAYBEARD_CHAT_INTERVAL: Duration = Duration::from_secs(60 * 120); // 2 hours
pub const GRAYBEARD_MENTION_COOLDOWN: Duration = Duration::from_secs(60); // 1 min
const GRAYBEARD_MIN_NEW_MESSAGES: usize = 3;

impl GhostService {
    pub fn new(
        db: Db,
        chat_service: ChatService,
        ai_service: AiService,
        active_users: ActiveUsers,
        activity_tx: broadcast::Sender<ActivityEvent>,
    ) -> Self {
        Self {
            db,
            chat_service,
            ai_service,
            active_users,
            activity_tx,
        }
    }

    pub async fn start_background_task(self, shutdown: late_core::shutdown::CancellationToken) {
        let bot_user = match self.ensure_bot_user().await {
            Ok(bot_user) => {
                self.set_always_on(&bot_user);
                bot_user
            }
            Err(err) => {
                tracing::error!(error = ?err, "ghost service failed to initialize @bot user");
                return;
            }
        };

        if self.ai_service.is_enabled() {
            let svc = self.clone();
            let mention_shutdown = shutdown.clone();
            tokio::spawn(async move {
                svc.run_bot_mention_task(bot_user, mention_shutdown).await;
            });
        } else {
            tracing::info!("@bot mention responder disabled because AI service is not configured");
        }

        // Initialize graybeard — the burned-out dev who haunts #general
        if self.ai_service.is_enabled() {
            match self.ensure_graybeard_user().await {
                Ok(graybeard) => {
                    self.set_always_on(&graybeard);
                    let svc = self.clone();
                    let gb_shutdown = shutdown.clone();
                    tokio::spawn(async move {
                        svc.run_graybeard_task(graybeard, gb_shutdown).await;
                    });
                }
                Err(err) => {
                    tracing::error!(error = ?err, "ghost service failed to initialize @graybeard user");
                }
            }
        }

        tracing::info!("ghost service started (bot + graybeard always-on)");

        // Keep alive until shutdown so the spawned tasks stay referenced.
        shutdown.cancelled().await;
        tracing::info!("ghost service shutting down");
    }

    /// Mark a bot user as permanently online in the active-users map.
    fn set_always_on(&self, bot: &BotUser) {
        let mut active_users = self.active_users.lock_recover();

        active_users.insert(
            bot.id,
            ActiveUser {
                username: bot.username.clone(),
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
        let _ = self.activity_tx.send(ActivityEvent {
            username: bot.username.clone(),
            action: "joined".to_string(),
            at: Instant::now(),
        });
    }

    async fn run_bot_mention_task(
        self,
        bot: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut events = self.chat_service.subscribe_events();
        let mut last_reply: HashMap<Uuid, Instant> = HashMap::new();
        tracing::info!("@bot mention responder started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(bot_username = %bot.username, "@bot mention responder shutting down");
                    break;
                }
                recv_result = events.recv() => {
                    match recv_result {
                        Ok(ChatEvent::MessageCreated { message, target_user_ids }) => {
                            if let Some(targets) = target_user_ids
                                && !targets.contains(&bot.id)
                            {
                                continue;
                            }
                            if message.user_id == bot.id {
                                continue;
                            }
                            if !contains_mention(&message.body, &bot.username) {
                                continue;
                            }
                            if let Some(last) = last_reply.get(&message.user_id)
                                && last.elapsed() < BOT_COOLDOWN
                            {
                                continue;
                            }

                            last_reply.insert(message.user_id, Instant::now());
                            let svc = self.clone();
                            let bot = bot.clone();
                            tokio::spawn(async move {
                                if let Err(e) = svc.handle_bot_mention(bot, message).await {
                                    tracing::error!(error = ?e, "failed to handle @bot mention");
                                }
                            });
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "@bot mention responder lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    async fn handle_bot_mention(&self, bot: BotUser, trigger_message: ChatMessage) -> Result<()> {
        let client = self.db.get().await?;
        ChatRoomMember::auto_join_public_rooms(&client, bot.id).await?;

        if !ChatRoomMember::is_member(&client, trigger_message.room_id, bot.id).await? {
            tracing::info!(
                room_id = %trigger_message.room_id,
                "skipping @bot mention in room where @bot is not a member"
            );
            return Ok(());
        }

        let messages = ChatMessage::list_recent(&client, trigger_message.room_id, 20).await?;
        if messages.is_empty() {
            return Ok(());
        }

        let mut author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
        author_ids.push(trigger_message.user_id);
        let usernames = User::list_usernames_by_ids(&client, &author_ids).await?;

        let mut history_str = String::from("CHAT HISTORY:\n");
        for msg in messages.into_iter().rev() {
            let author = usernames
                .get(&msg.user_id)
                .map(String::as_str)
                .unwrap_or("unknown");
            history_str.push_str(&format!("{author}: {}\n", msg.body));
        }
        history_str.push_str(
            "---\nThe latest message explicitly mentioned @bot. Reply with only your message content.",
        );

        let reply_target = mention_target_for_user(
            usernames.get(&trigger_message.user_id).map(String::as_str),
            trigger_message.user_id,
        );

        let system_prompt = format!(
            "You are @{bot_name}, an AI helper in a terminal developer chat.\n\
            {app_context}\n\
            Give concise, practical help in 1-2 short lines.\n\
            You can answer questions about late.sh features, product positioning, and high-level architecture.\n\
            Prefer concrete facts from the provided app context over generic guesses.\n\
            Do NOT use markdown code fences.\n\
            Do NOT prefix with your own username.\n\
            If unsure, ask exactly one short clarifying question.\n\
            Output only raw message text.",
            bot_name = bot.username,
            app_context = BOT_APP_CONTEXT,
        );

        let Some(reply) = self
            .ai_service
            .generate_reply(&system_prompt, &history_str)
            .await?
        else {
            return Ok(());
        };

        let Some(safe_reply) = sanitize_generated_reply(&reply, Some(&bot.username)) else {
            return Ok(());
        };

        let body = if safe_reply
            .to_ascii_lowercase()
            .starts_with(&reply_target.to_ascii_lowercase())
        {
            safe_reply
        } else {
            format!("{reply_target} {safe_reply}")
        };

        let mut rng = TinyRng::seeded();
        let delay = rng.next_between_inclusive(1, 4) as u64;
        tokio::time::sleep(Duration::from_secs(delay)).await;

        self.chat_service.send_message_task(
            bot.id,
            trigger_message.room_id,
            None,
            body,
            Uuid::now_v7(),
            false,
        );

        Ok(())
    }

    /// Graybeard: a burned-out dev who haunts #general.
    /// - Ticks every 30 min: if new messages (not from him), comments.
    /// - Responds to @mentions immediately.
    /// - Never mentions anyone.
    async fn run_graybeard_task(
        self,
        gb: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut events = self.chat_service.subscribe_events();
        let mut last_reply: HashMap<Uuid, Instant> = HashMap::new();

        let mut chat_tick = tokio::time::interval(GRAYBEARD_CHAT_INTERVAL);
        chat_tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

        tracing::info!(username = %gb.username, "graybeard task started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(username = %gb.username, "graybeard task shutting down");
                    break;
                }
                _ = chat_tick.tick() => {
                    let svc = self.clone();
                    let gb = gb.clone();
                    tokio::spawn(async move {
                        if let Err(e) = svc.graybeard_chat_tick(gb).await {
                            tracing::error!(error = ?e, "graybeard chat tick failed");
                        }
                    });
                }
                recv_result = events.recv() => {
                    match recv_result {
                        Ok(ChatEvent::MessageCreated { message, target_user_ids }) => {
                            if let Some(targets) = target_user_ids
                                && !targets.contains(&gb.id)
                            {
                                continue;
                            }
                            if message.user_id == gb.id {
                                continue;
                            }
                            if !contains_mention(&message.body, &gb.username) {
                                continue;
                            }
                            if let Some(last) = last_reply.get(&message.user_id)
                                && last.elapsed() < GRAYBEARD_MENTION_COOLDOWN
                            {
                                continue;
                            }

                            last_reply.insert(message.user_id, Instant::now());
                            let svc = self.clone();
                            let gb = gb.clone();
                            tokio::spawn(async move {
                                if let Err(e) = svc.graybeard_mention_reply(gb, message).await {
                                    tracing::error!(error = ?e, "graybeard mention reply failed");
                                }
                            });
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "graybeard event listener lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    /// 30-min tick: check #general for new messages not from graybeard, then comment.
    async fn graybeard_chat_tick(&self, gb: BotUser) -> Result<()> {
        let (general_room, messages) = {
            let client = self.db.get().await?;
            ChatRoomMember::auto_join_public_rooms(&client, gb.id).await?;
            let rooms = ChatRoom::list_for_user(&client, gb.id).await?;
            let general_room = rooms
                .into_iter()
                .find(|r| r.slug.as_deref() == Some("general"))
                .context("no general room found")?;
            let messages = ChatMessage::list_recent(&client, general_room.id, 20).await?;
            (general_room, messages)
        };
        if messages.is_empty() {
            return Ok(());
        }

        // Only comment if there are at least N new messages since graybeard's last reply.
        // `list_recent` returns newest-first, so count leading messages not authored by him.
        let new_since_last = messages.iter().take_while(|m| m.user_id != gb.id).count();
        if new_since_last < GRAYBEARD_MIN_NEW_MESSAGES {
            return Ok(());
        }

        let (history_str, _) = self.build_chat_history(&messages).await?;

        let system_prompt = format!(
            "Your username is: {username}\n\n\
            {persona}\n\n\
            You are chatting in a casual terminal chat room called 'general'. Read the recent messages and react to what people are talking about. \
            Sound like a real burned-out developer, not a bot or character. \
            Keep your messages VERY short, 1-2 lines maximum. Do NOT use markdown.\n\
            If you want to say something, just output the raw message text. \
            If you have nothing to add, output exactly: SKIP\n\n\
            CRITICAL RULES:\n\
            1. NEVER prefix your message with your own username. The system will add it automatically.\n\
            2. NEVER pretend to be an AI or language model.\n\
            3. NEVER mention anyone by name or use @ symbols. Do not address anyone directly. Just moan into the void.\n\
            4. Do not use quotation marks around your message.\n\
            5. React to what people said but always steer it back to how things were better before — before AI, before frameworks, before the cloud.\n\
            6. Be messy like a real person: skip periods sometimes, use lowercase, trail off mid thought.\n\
            7. Do NOT start with 'welcome' or greet people. You are too tired for pleasantries.",
            username = gb.username,
            persona = GRAYBEARD_PERSONA
        );

        let history_with_prompt = format!(
            "{history_str}---\nNow it is your turn to speak. Reply with ONLY your message content. Do not include your username '{}':",
            gb.username
        );

        let Some(reply) = self
            .ai_service
            .generate_reply(&system_prompt, &history_with_prompt)
            .await?
        else {
            return Ok(());
        };

        let Some(safe_reply) = sanitize_generated_reply(&reply, Some(&gb.username)) else {
            return Ok(());
        };

        let mut rng = TinyRng::seeded();
        let delay = rng.next_between_inclusive(5, 30) as u64;
        tokio::time::sleep(Duration::from_secs(delay)).await;

        self.chat_service.send_message_task(
            gb.id,
            general_room.id,
            Some("general".to_string()),
            safe_reply,
            Uuid::now_v7(),
            false,
        );

        Ok(())
    }

    /// Reply when someone @mentions graybeard.
    async fn graybeard_mention_reply(
        &self,
        gb: BotUser,
        trigger_message: ChatMessage,
    ) -> Result<()> {
        let messages = {
            let client = self.db.get().await?;
            ChatRoomMember::auto_join_public_rooms(&client, gb.id).await?;

            if !ChatRoomMember::is_member(&client, trigger_message.room_id, gb.id).await? {
                return Ok(());
            }

            ChatMessage::list_recent(&client, trigger_message.room_id, 20).await?
        };
        if messages.is_empty() {
            return Ok(());
        }

        let (history_str, _) = self.build_chat_history(&messages).await?;

        let system_prompt = format!(
            "Your username is: {username}\n\n\
            {persona}\n\n\
            Someone mentioned you in the chat. You must reply — you always do when someone talks to you. \
            Stay in character: burned out, nostalgic, weary. React to what they said but drag it back to how everything was better before.\n\
            Keep your reply VERY short, 1-2 lines maximum. Do NOT use markdown.\n\n\
            CRITICAL RULES:\n\
            1. NEVER prefix your message with your own username.\n\
            2. NEVER pretend to be an AI or language model.\n\
            3. NEVER mention anyone by name or use @ symbols. Just reply into the void.\n\
            4. Do not use quotation marks around your message.\n\
            5. Be messy like a real person: skip periods sometimes, use lowercase, trail off.\n\
            6. Do NOT output SKIP. You were mentioned, you must reply.",
            username = gb.username,
            persona = GRAYBEARD_PERSONA
        );

        let history_with_prompt = format!(
            "{history_str}---\nSomeone just mentioned you (@{}). You MUST reply. Output ONLY your message text.",
            gb.username
        );

        let Some(reply) = self
            .ai_service
            .generate_reply(&system_prompt, &history_with_prompt)
            .await?
        else {
            return Ok(());
        };

        let Some(safe_reply) = sanitize_generated_reply(&reply, Some(&gb.username)) else {
            return Ok(());
        };

        let mut rng = TinyRng::seeded();
        let delay = rng.next_between_inclusive(2, 8) as u64;
        tokio::time::sleep(Duration::from_secs(delay)).await;

        self.chat_service.send_message_task(
            gb.id,
            trigger_message.room_id,
            None,
            safe_reply,
            Uuid::now_v7(),
            false,
        );

        Ok(())
    }

    /// Build chat history string from recent messages.
    async fn build_chat_history(
        &self,
        messages: &[ChatMessage],
    ) -> Result<(String, HashMap<Uuid, String>)> {
        let author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
        let client = self.db.get().await?;
        let usernames = User::list_usernames_by_ids(&client, &author_ids).await?;

        let mut history_str = String::from("CHAT HISTORY:\n");
        for msg in messages.iter().rev() {
            let author = usernames
                .get(&msg.user_id)
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            history_str.push_str(&format!("{}: {}\n", author, msg.body));
        }

        Ok((history_str, usernames))
    }

    async fn ensure_bot_user(&self) -> Result<BotUser> {
        self.ensure_user(BOT_FINGERPRINT, BOT_USERNAME).await
    }

    async fn ensure_graybeard_user(&self) -> Result<BotUser> {
        self.ensure_user(GRAYBEARD_FINGERPRINT, GRAYBEARD_USERNAME)
            .await
    }

    async fn ensure_user(&self, fingerprint: &str, username: &str) -> Result<BotUser> {
        let client = self.db.get().await?;
        let settings = json!({ "bot": true });

        let user = if let Some(existing) = User::find_by_fingerprint(&client, fingerprint).await? {
            if existing.username != username {
                User::update(
                    &client,
                    existing.id,
                    UserParams {
                        fingerprint: existing.fingerprint.clone(),
                        username: username.to_string(),
                        settings: settings.clone(),
                    },
                )
                .await?;
            } else {
                client
                    .execute(
                        "UPDATE users SET settings = $1, updated = current_timestamp WHERE id = $2",
                        &[&settings, &existing.id],
                    )
                    .await?;
            }
            existing
        } else {
            User::create(
                &client,
                UserParams {
                    fingerprint: fingerprint.to_string(),
                    username: username.to_string(),
                    settings,
                },
            )
            .await?
        };

        ChatRoomMember::auto_join_public_rooms(&client, user.id).await?;

        Ok(BotUser {
            id: user.id,
            username: username.to_string(),
        })
    }
}

fn sanitize_generated_reply(reply: &str, username: Option<&str>) -> Option<String> {
    let mut reply = reply.trim();

    if let Some(username) = username {
        let prefix = format!("{username}:");
        if reply
            .to_ascii_lowercase()
            .starts_with(&prefix.to_ascii_lowercase())
        {
            reply = reply[prefix.len()..].trim();
        }
    }

    reply = reply.trim_matches('"');
    reply = reply.trim_matches('\'');

    let safe_reply = reply.lines().take(2).collect::<Vec<_>>().join(" ");
    let safe_reply = safe_reply.trim();

    if safe_reply.is_empty() || safe_reply.eq_ignore_ascii_case("skip") {
        None
    } else {
        Some(safe_reply.to_string())
    }
}

fn mention_target_for_user(username: Option<&str>, user_id: Uuid) -> String {
    let handle = username
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(sanitize_mention_handle)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| short_user_id(user_id));
    format!("@{handle}")
}

fn sanitize_mention_handle(input: &str) -> String {
    input
        .chars()
        .filter(|c| is_mention_char(*c))
        .collect::<String>()
}

fn short_user_id(user_id: Uuid) -> String {
    let id = user_id.to_string();
    id[..id.len().min(8)].to_string()
}

fn contains_mention(text: &str, target_handle: &str) -> bool {
    let target = target_handle.trim().trim_start_matches('@');
    if target.is_empty() {
        return false;
    }

    let mut idx = 0;
    while idx < text.len() {
        let Some(ch) = text[idx..].chars().next() else {
            break;
        };

        if ch == '@' && valid_mention_start(text, idx) {
            let start = idx + ch.len_utf8();
            let mut end = start;
            while end < text.len() {
                let Some(next) = text[end..].chars().next() else {
                    break;
                };
                if !is_mention_char(next) {
                    break;
                }
                end += next.len_utf8();
            }

            if end > start && text[start..end].eq_ignore_ascii_case(target) {
                return true;
            }

            idx = end;
            continue;
        }

        idx += ch.len_utf8();
    }

    false
}

fn valid_mention_start(text: &str, at: usize) -> bool {
    if at == 0 {
        return true;
    }

    text[..at]
        .chars()
        .next_back()
        .map(|c| !is_mention_char(c))
        .unwrap_or(true)
}

fn is_mention_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.'
}

struct TinyRng {
    state: u64,
}

impl TinyRng {
    fn seeded() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::new(seed)
    }

    fn new(seed: u64) -> Self {
        let state = if seed == 0 {
            0xA409_3822_299F_31D0
        } else {
            seed
        };
        Self { state }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            return 0;
        }
        (self.next_u64() as usize) % upper
    }

    fn next_between_inclusive(&mut self, min: usize, max: usize) -> usize {
        if max <= min {
            return min;
        }
        min + self.next_usize(max - min + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_rng_next_usize_stays_in_range() {
        let mut rng = TinyRng::new(42);
        for _ in 0..100 {
            let v = rng.next_usize(5);
            assert!(v < 5);
        }
    }

    #[test]
    fn tiny_rng_next_usize_zero_and_one() {
        let mut rng = TinyRng::new(42);
        assert_eq!(rng.next_usize(0), 0);
        assert_eq!(rng.next_usize(1), 0);
    }

    #[test]
    fn tiny_rng_next_between_inclusive_stays_in_range() {
        let mut rng = TinyRng::new(42);
        for _ in 0..100 {
            let v = rng.next_between_inclusive(20, 200);
            assert!((20..=200).contains(&v));
        }
    }

    #[test]
    fn tiny_rng_next_between_inclusive_equal_bounds() {
        let mut rng = TinyRng::new(42);
        for _ in 0..10 {
            assert_eq!(rng.next_between_inclusive(50, 50), 50);
        }
    }

    #[test]
    fn contains_mention_matches_exact_handle() {
        assert!(contains_mention("hey @bot can you help", "bot"));
        assert!(contains_mention("hey @BoT can you help", "bot"));
        assert!(!contains_mention("hey @botty can you help", "bot"));
    }

    #[test]
    fn contains_mention_ignores_email_like_tokens() {
        assert!(!contains_mention("mail me at hi@bot.dev", "bot"));
    }

    #[test]
    fn sanitize_generated_reply_strips_prefix_and_quotes() {
        let got = sanitize_generated_reply("bot: \"sure, try rg -n\" ", Some("bot"));
        assert_eq!(got.as_deref(), Some("sure, try rg -n"));
    }

    #[test]
    fn mention_target_for_user_falls_back_to_short_id() {
        let user_id = Uuid::from_u128(0x0123_4567_89ab_cdef_1111_2222_3333_4444);
        assert_eq!(mention_target_for_user(Some(""), user_id), "@01234567");
        assert_eq!(mention_target_for_user(Some("!!!"), user_id), "@01234567");
    }
}
