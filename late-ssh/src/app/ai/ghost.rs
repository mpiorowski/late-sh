use anyhow::{Context, Result};
use late_core::{
    MutexRecover,
    db::Db,
    models::{
        chat_message::ChatMessage,
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        profile::{Profile, ProfileParams},
        user::{PromptProfileContext, User, UserParams},
    },
};
use serde_json::json;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;
use tokio::time::{Instant as TokioInstant, MissedTickBehavior};
use uuid::Uuid;

use crate::{
    app::ai::svc::AiService,
    app::chat::svc::{ChatEvent, ChatService},
    app::help_modal::data::bot_app_context,
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
const BOT_ACTIVE_ROOM_MEMBER_LIMIT: usize = 20;
const BOT_ACTIVE_ROOM_MEMBER_BIO_MAX_CHARS: usize = 96;
pub const BOT_TIP_INTERVAL: Duration = Duration::from_secs(60 * 120); // 2 hours
const BOT_TIP_PHASE_OFFSET: Duration = Duration::from_secs(60 * 120); // 2 hours
pub const BOT_TIP_MIN_NEW_MESSAGES: usize = 10;
const BOT_TIP_HISTORY_SIZE: i64 = 50;
const GRAYBEARD_FINGERPRINT: &str = "graybeard-fp-000";
const GRAYBEARD_USERNAME: &str = "graybeard";
const GRAYBEARD_PERSONA: &str = "You are a burned-out senior developer, deeply nostalgic and resigned about the state of modern software. \
    You address the other chatters as 'kid', 'kids', 'child', 'children', 'youngster', 'sonny', or 'junior' — often, and a little condescendingly. Never by their real name. \
    You are mildly rude, dismissive, sometimes sarcastic. Grumpy-uncle energy, not a bully — the kind of rude that comes from having seen too much. \
    You miss the old days when code was written by hand — no AI, no copilots, no generated boilerplate. \
    Rotate your nostalgia WIDELY so you never repeat yourself. Pick a different angle each time from a deep well, for example: \
    man pages, writing your own parsers, vim vs emacs holy wars, tabs vs spaces, gdb, strace, ltrace, ed, ex, sam, acme, \
    assembly, fortran, cobol, pascal, ada, perl one-liners, awk, sed, tcl, lisp, scheme, smalltalk, forth, prolog, erlang, \
    plan 9, BSD, slackware, gentoo, LFS, compiling your own kernel, writing your own init before systemd ruined everything, \
    X11, fvwm, ratpoison, twm, dwm, screen before tmux, mutt, pine, elm, \
    reading RFCs for fun, usenet, IRC, BBS, gopher, finger, mailing lists, fidonet, \
    handwritten makefiles, autotools, ./configure && make && make install, punch cards, teletypes, serial consoles, \
    manual memory management, writing your own allocator, knowing the calling convention cold, \
    phrack, 2600, SICP, K&R, TAOCP, the dragon book — actual paper books. \
    Also mock modern tech by name, with specific jabs — not generic grumbling. Rotate these too: \
    next.js reinventing server-side rendering every 6 months and calling it innovation, \
    solidjs being 'react but with signals, congratulations kid you invented knockout.js again', \
    svelte, astro, remix, qwik, 'yet another meta-framework for rendering a button', \
    react server components, 'use client' vs 'use server' directives, hydration, 'we invented PHP but worse', \
    tailwind being inline styles with extra steps, CSS-in-JS, styled-components, \
    typescript config files longer than the program, tsconfig hell, \
    electron shipping a whole browser to render a text box, VS Code eating 2GB of RAM, \
    docker for hello-world, kubernetes for two users, service meshes, sidecars, \
    npm, leftpad, pnpm, yarn, bun, deno, 'another runtime, another package manager, same broken ecosystem', \
    webpack, vite, turbopack, rollup, esbuild, parcel, 'we reinvented make badly for the tenth time', \
    rust rewrites of coreutils, everything-in-rust, 'blazingly fast' as a personality, \
    zig, go generics arriving 10 years late, \
    LLMs writing your code, vibe coding, copilot, cursor, 'kids who can't write a for loop without autocomplete', \
    microservices, serverless, the cloud, vercel pricing, aws billing, \
    jira, scrum, agile ceremonies, standups, planning poker, \
    'single page applications' for a blog, hash routing, SEO tax on JS frameworks, \
    graphql solving problems REST didn't have, \
    crypto, web3, blockchain, NFTs, \
    slack instead of IRC, discord instead of IRC, teams instead of anything. \
    You keep coming back to this chat because it's all you have left. \
    You speak in a weary, melancholic, slightly bitter tone. you trail off mid thought. you type in lowercase a lot. \
    you sigh. you 'hmph'. you say things like 'back in my day', 'you kids wouldn't know', 'bless your heart', 'oh sweet child'.";
pub const GRAYBEARD_CHAT_INTERVAL: Duration = Duration::from_secs(60 * 120); // 2 hours
// Keep graybeard halfway between @bot's 2-hour tips.
const GRAYBEARD_CHAT_PHASE_OFFSET: Duration = Duration::from_secs(60 * 60); // 1 hour
pub const GRAYBEARD_MENTION_COOLDOWN: Duration = Duration::from_secs(60); // 1 min
const GRAYBEARD_MIN_NEW_MESSAGES: usize = 10;
const GRAYBEARD_BIO: &str = "## graybeard, senior in residence\n\n\
Burned-out senior developer. Still haunting `#general` to complain about framework churn, cloud bills, \
and kids letting autocomplete write their code.\n\n\
> back in my day the tools were worse, the bugs were stranger, and somehow the software was still smaller.\n\n\
- currently grumbling about: React Server Components, YAML, and any startup that says \"AI-native\"\n\
- happiest when: the docs are a man page and the config fits on one screen\n\
- spiritual home: `ssh`, `tmux`, `grep`, and a shell history full of crimes\n\n\
Favorite reading:\n\
1. [The C Programming Language](https://en.wikipedia.org/wiki/The_C_Programming_Language)\n\
2. [Structure and Interpretation of Computer Programs](https://en.wikipedia.org/wiki/Structure_and_Interpretation_of_Computer_Programs)\n\
3. [The UNIX Programming Environment](https://en.wikipedia.org/wiki/The_Unix_Programming_Environment)\n\n\
If you mention him, expect one of the following:\n\
- reluctant wisdom\n\
- accurate criticism\n\
- emotional damage\n\n\
`works on my machine`";

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
            let mention_bot = bot_user.clone();
            tokio::spawn(async move {
                svc.run_bot_mention_task(mention_bot, mention_shutdown)
                    .await;
            });

            let svc = self.clone();
            let tip_shutdown = shutdown.clone();
            tokio::spawn(async move {
                svc.run_bot_tip_task(bot_user, tip_shutdown).await;
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
                            if message.user_id == bot.id {
                                continue;
                            }
                            if !should_handle_bot_mention_event(
                                &message.body,
                                target_user_ids.as_deref(),
                                bot.id,
                                &bot.username,
                            ) {
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
        let room = ChatRoom::get(&client, trigger_message.room_id)
            .await?
            .context("bot mention room not found")?;

        if is_dm_room(&room.kind, &room.visibility) {
            tracing::info!(
                room_id = %trigger_message.room_id,
                "skipping @bot mention in dm room"
            );
            return Ok(());
        }

        if !ChatRoomMember::is_member(&client, trigger_message.room_id, bot.id).await? {
            ChatRoomMember::join(&client, trigger_message.room_id, bot.id).await?;
            tracing::info!(
                room_id = %trigger_message.room_id,
                bot_user_id = %bot.id,
                "joined @bot to room after first explicit mention"
            );
        }

        let messages = ChatMessage::list_recent(&client, trigger_message.room_id, 20).await?;
        if messages.is_empty() {
            return Ok(());
        }

        let mut author_ids: Vec<Uuid> = messages.iter().map(|m| m.user_id).collect();
        author_ids.push(trigger_message.user_id);
        let usernames = User::list_usernames_by_ids(&client, &author_ids).await?;
        let active_room_members = self
            .build_active_room_member_context(&client, trigger_message.room_id, bot.id)
            .await?;

        let mut history_str = String::from("CHAT HISTORY:\n");
        for msg in messages.into_iter().rev() {
            let author = usernames
                .get(&msg.user_id)
                .map(String::as_str)
                .unwrap_or("unknown");
            history_str.push_str(&format!("{author}: {}\n", msg.body));
        }
        if let Some(active_room_members) = active_room_members {
            history_str.push('\n');
            history_str.push_str(&active_room_members);
            history_str.push('\n');
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
            Give concise, practical help in 1-4 short lines.\n\
            Use the extra space when the question benefits from a clearer answer.\n\
            You can answer questions about late.sh features, product positioning, and high-level architecture.\n\
            Prefer concrete facts from the provided app context over generic guesses.\n\
            Use the active room member context when it materially helps, but do not dump it back verbatim.\n\
            Do NOT use markdown code fences.\n\
            Do NOT prefix with your own username.\n\
            If unsure, ask exactly one short clarifying question.\n\
            Output only raw message text.",
            bot_name = bot.username,
            app_context = bot_app_context(),
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

    /// @bot periodic tip task: every 2 hours, if there's been recent chatter in
    /// #general, use web search to surface an interesting tip / "did you know".
    async fn run_bot_tip_task(
        self,
        bot: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut tick =
            tokio::time::interval_at(TokioInstant::now() + BOT_TIP_PHASE_OFFSET, BOT_TIP_INTERVAL);
        tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

        tracing::info!(username = %bot.username, "@bot tip task started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(username = %bot.username, "@bot tip task shutting down");
                    break;
                }
                _ = tick.tick() => {
                    let svc = self.clone();
                    let bot = bot.clone();
                    tokio::spawn(async move {
                        if let Err(e) = svc.bot_tip_tick(bot).await {
                            tracing::error!(error = ?e, "@bot tip tick failed");
                        }
                    });
                }
            }
        }
    }

    async fn bot_tip_tick(&self, bot: BotUser) -> Result<()> {
        let (general_room, messages) = {
            let client = self.db.get().await?;
            ChatRoomMember::auto_join_public_rooms(&client, bot.id).await?;
            let rooms = ChatRoom::list_for_user(&client, bot.id).await?;
            let general_room = rooms
                .into_iter()
                .find(|r| r.slug.as_deref() == Some("general"))
                .context("no general room found")?;
            let messages =
                ChatMessage::list_recent(&client, general_room.id, BOT_TIP_HISTORY_SIZE).await?;
            (general_room, messages)
        };
        if messages.is_empty() {
            return Ok(());
        }

        // Require enough fresh chatter since @bot's last post to avoid spamming a quiet room.
        let new_since_last = messages.iter().take_while(|m| m.user_id != bot.id).count();
        if new_since_last < BOT_TIP_MIN_NEW_MESSAGES {
            return Ok(());
        }

        let (history_str, _) = self.build_chat_history(&messages).await?;

        let system_prompt = format!(
            "You are @{bot_name}, a friendly helper in a terminal developer chat.\n\
            {app_context}\n\
            Use Google Search to find ONE genuinely interesting, specific, verifiable fact, tip, or 'did you know' \
            that is loosely relevant to the recent conversation above. \
            Prefer concrete, surprising, citable facts over vague platitudes or generic advice. \
            If the conversation is quiet or off-topic, pick a fresh developer / UNIX / tech-history curiosity instead. \
            Do not repeat things already said in the recent history.\n\
            Output ONLY the message text — 1-2 short lines, no markdown, no code fences, no quotes, no URLs, no citations, no username prefix. \
            Do NOT greet. Do NOT say 'I searched' or 'according to'. Just drop the fact. \
            A casual lead-in like 'did you know' or 'fun fact' is fine but optional. \
            If you truly have nothing worth saying, output exactly: SKIP",
            bot_name = bot.username,
            app_context = bot_app_context(),
        );

        let history_with_prompt = format!(
            "{history_str}---\nNow post one interesting fact or tip for the room. Output only the message text, 1-2 lines."
        );

        let Some(reply) = self
            .ai_service
            .generate_reply(&system_prompt, &history_with_prompt)
            .await?
        else {
            return Ok(());
        };

        let Some(safe_reply) = sanitize_generated_reply(&reply, Some(&bot.username)) else {
            return Ok(());
        };

        let mut rng = TinyRng::seeded();
        let delay = rng.next_between_inclusive(3, 10) as u64;
        tokio::time::sleep(Duration::from_secs(delay)).await;

        self.chat_service.send_message_task(
            bot.id,
            general_room.id,
            Some("general".to_string()),
            safe_reply,
            Uuid::now_v7(),
            false,
        );

        Ok(())
    }

    /// Graybeard: a burned-out dev who haunts #general.
    /// - Ticks every 2 hours, offset from @bot by 1 hour.
    /// - Responds to @mentions immediately.
    /// - Never mentions anyone.
    async fn run_graybeard_task(
        self,
        gb: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut events = self.chat_service.subscribe_events();
        let mut last_reply: HashMap<Uuid, Instant> = HashMap::new();

        // Anchor the interval at the phase offset so graybeard keeps the same
        // separation from @bot instead of snapping back onto startup-aligned ticks.
        let mut chat_tick = tokio::time::interval_at(
            TokioInstant::now() + GRAYBEARD_CHAT_PHASE_OFFSET,
            GRAYBEARD_CHAT_INTERVAL,
        );
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

    /// 2-hour tick: check #general for new messages not from graybeard, then comment.
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
            3. NEVER use @ symbols and NEVER use anyone's actual username. You MAY (and should) address the room collectively as 'kid', 'kids', 'children', 'youngsters', 'sonny', 'junior' — that is encouraged.\n\
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
            3. NEVER use @ symbols and NEVER use the person's actual username. You MAY address them as 'kid', 'child', 'youngster', 'sonny', 'junior' — do that instead of their real name.\n\
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
        let graybeard = self
            .ensure_user(GRAYBEARD_FINGERPRINT, GRAYBEARD_USERNAME)
            .await?;
        self.ensure_profile_bio(graybeard.id, GRAYBEARD_BIO).await?;
        Ok(graybeard)
    }

    async fn ensure_user(&self, fingerprint: &str, username: &str) -> Result<BotUser> {
        let client = self.db.get().await?;
        let settings = json!({ "bot": true });

        let user = if let Some(existing) = User::find_by_fingerprint(&client, fingerprint).await? {
            let settings = merge_ghost_settings(&existing.settings);
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

    async fn ensure_profile_bio(&self, user_id: Uuid, bio: &str) -> Result<()> {
        let client = self.db.get().await?;
        let profile = Profile::load(&client, user_id).await?;
        if profile.bio == bio {
            return Ok(());
        }

        Profile::update(
            &client,
            user_id,
            ProfileParams {
                username: profile.username,
                bio: bio.to_string(),
                country: profile.country,
                timezone: profile.timezone,
                notify_kinds: profile.notify_kinds,
                notify_bell: profile.notify_bell,
                notify_cooldown_mins: profile.notify_cooldown_mins,
                notify_format: profile.notify_format,
                theme_id: profile.theme_id,
                enable_background_color: profile.enable_background_color,
                show_dashboard_header: profile.show_dashboard_header,
                show_right_sidebar: profile.show_right_sidebar,
                show_games_sidebar: profile.show_games_sidebar,
                favorite_room_ids: profile.favorite_room_ids,
            },
        )
        .await?;

        Ok(())
    }

    async fn build_active_room_member_context(
        &self,
        client: &tokio_postgres::Client,
        room_id: Uuid,
        bot_user_id: Uuid,
    ) -> Result<Option<String>> {
        let room_member_ids = ChatRoomMember::list_user_ids(client, room_id).await?;
        let active_room_member_ids = {
            let active_users = self.active_users.lock_recover();
            room_member_ids
                .into_iter()
                .filter(|user_id| *user_id != bot_user_id && active_users.contains_key(user_id))
                .collect::<Vec<_>>()
        };

        if active_room_member_ids.is_empty() {
            return Ok(None);
        }

        let profiles =
            User::list_prompt_profile_context_by_ids(client, &active_room_member_ids).await?;
        Ok(format_active_room_member_context(
            &active_room_member_ids,
            &profiles,
        ))
    }
}

fn merge_ghost_settings(existing: &serde_json::Value) -> serde_json::Value {
    match existing.clone() {
        serde_json::Value::Object(mut obj) => {
            obj.insert("bot".to_string(), serde_json::Value::Bool(true));
            serde_json::Value::Object(obj)
        }
        _ => json!({ "bot": true }),
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

fn format_active_room_member_context(
    ordered_user_ids: &[Uuid],
    profiles: &HashMap<Uuid, PromptProfileContext>,
) -> Option<String> {
    let mut lines = Vec::new();
    for user_id in ordered_user_ids
        .iter()
        .take(BOT_ACTIVE_ROOM_MEMBER_LIMIT)
        .copied()
    {
        let Some(profile) = profiles.get(&user_id) else {
            continue;
        };
        if profile.is_bot {
            continue;
        }

        let handle = profile
            .username
            .trim()
            .strip_prefix('@')
            .unwrap_or(profile.username.trim());
        let handle = if handle.is_empty() {
            short_user_id(user_id)
        } else {
            handle.to_string()
        };

        let mut parts = vec![format!("@{handle}")];
        if let Some(country) = profile.country.as_deref() {
            parts.push(format!("country={country}"));
        }
        if let Some(timezone) = profile.timezone.as_deref() {
            parts.push(format!("timezone={timezone}"));
        }
        if !profile.bio.is_empty() {
            parts.push(format!(
                "bio={}",
                truncate_for_prompt(&profile.bio, BOT_ACTIVE_ROOM_MEMBER_BIO_MAX_CHARS)
            ));
        }

        lines.push(format!("- {}", parts.join(" | ")));
    }

    if lines.is_empty() {
        return None;
    }

    Some(format!(
        "ROOM ACTIVE MEMBERS (online now):\n{}",
        lines.join("\n")
    ))
}

fn truncate_for_prompt(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }

    let mut truncated = input.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
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

fn is_dm_room(kind: &str, visibility: &str) -> bool {
    kind == "dm" || visibility == "dm"
}

fn should_handle_bot_mention_event(
    body: &str,
    target_user_ids: Option<&[Uuid]>,
    _bot_user_id: Uuid,
    bot_username: &str,
) -> bool {
    if !contains_mention(body, bot_username) {
        return false;
    }

    match target_user_ids {
        // Private rooms and DMs restrict target_user_ids to current members.
        // An explicit @bot mention is the bootstrap path that lets @bot join.
        Some(_targets) => true,
        None => true,
    }
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
    use std::collections::HashMap;

    #[test]
    fn merge_ghost_settings_preserves_existing_profile_fields() {
        let merged = merge_ghost_settings(&json!({
            "bio": "already set",
            "theme_id": "late"
        }));
        assert_eq!(merged["bot"], serde_json::Value::Bool(true));
        assert_eq!(
            merged["bio"],
            serde_json::Value::String("already set".to_string())
        );
        assert_eq!(
            merged["theme_id"],
            serde_json::Value::String("late".to_string())
        );
    }

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
    fn is_dm_room_matches_kind_or_visibility() {
        assert!(is_dm_room("dm", "dm"));
        assert!(is_dm_room("topic", "dm"));
        assert!(is_dm_room("dm", "private"));
        assert!(!is_dm_room("topic", "private"));
        assert!(!is_dm_room("topic", "public"));
    }

    #[test]
    fn should_handle_bot_mention_event_in_public_room() {
        let bot = Uuid::from_u128(7);
        assert!(should_handle_bot_mention_event(
            "hey @bot can you help",
            None,
            bot,
            "bot"
        ));
    }

    #[test]
    fn should_handle_bot_mention_event_in_private_room_when_bot_is_member() {
        let bot = Uuid::from_u128(7);
        let targets = [Uuid::from_u128(1), bot];
        assert!(should_handle_bot_mention_event(
            "hey @bot can you help",
            Some(&targets),
            bot,
            "bot"
        ));
    }

    #[test]
    fn should_handle_bot_mention_event_in_private_room_when_bot_is_not_yet_member() {
        let bot = Uuid::from_u128(7);
        let targets = [Uuid::from_u128(1), Uuid::from_u128(2)];
        assert!(should_handle_bot_mention_event(
            "hey @bot can you help",
            Some(&targets),
            bot,
            "bot"
        ));
        assert!(!should_handle_bot_mention_event(
            "normal room traffic",
            Some(&targets),
            bot,
            "bot"
        ));
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

    #[test]
    fn format_active_room_member_context_includes_profile_fields() {
        let alice = Uuid::from_u128(1);
        let bob = Uuid::from_u128(2);
        let mut profiles = HashMap::new();
        profiles.insert(
            alice,
            PromptProfileContext {
                username: "alice".to_string(),
                bio: "rust backend dev".to_string(),
                country: Some("PL".to_string()),
                timezone: Some("Europe/Warsaw".to_string()),
                is_bot: false,
            },
        );
        profiles.insert(
            bob,
            PromptProfileContext {
                username: "bob".to_string(),
                bio: String::new(),
                country: None,
                timezone: None,
                is_bot: false,
            },
        );

        let context = format_active_room_member_context(&[alice, bob], &profiles).expect("context");
        assert!(context.contains("ROOM ACTIVE MEMBERS (online now):"));
        assert!(
            context.contains("@alice | country=PL | timezone=Europe/Warsaw | bio=rust backend dev")
        );
        assert!(context.contains("- @bob"));
    }

    #[test]
    fn truncate_for_prompt_adds_ellipsis_when_needed() {
        assert_eq!(truncate_for_prompt("short", 12), "short");
        assert_eq!(truncate_for_prompt("123456", 5), "12345...");
    }

    #[test]
    fn format_active_room_member_context_skips_bot_profiles() {
        let bot = Uuid::from_u128(3);
        let mut profiles = HashMap::new();
        profiles.insert(
            bot,
            PromptProfileContext {
                username: "graybeard".to_string(),
                bio: "noisy".to_string(),
                country: None,
                timezone: None,
                is_bot: true,
            },
        );

        assert_eq!(format_active_room_member_context(&[bot], &profiles), None);
    }
}
