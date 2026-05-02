use anyhow::{Context, Result};
use late_core::{
    MutexRecover,
    db::Db,
    models::{
        chat_message::ChatMessage,
        chat_room::ChatRoom,
        chat_room_member::ChatRoomMember,
        game_room::{GameKind, GameRoom},
        profile::{Profile, ProfileParams},
        user::{User, UserParams},
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
    app::rooms::blackjack::{manager::BlackjackTableManager, state::Outcome, svc::BlackjackEvent},
    state::{ActiveUser, ActiveUsers, ActivityEvent},
};

#[derive(Clone)]
pub struct GhostService {
    db: Db,
    chat_service: ChatService,
    ai_service: AiService,
    blackjack_table_manager: BlackjackTableManager,
    active_users: ActiveUsers,
    activity_tx: broadcast::Sender<ActivityEvent>,
}

#[derive(Clone)]
struct BotUser {
    id: Uuid,
    username: String,
}

#[derive(Clone, Copy)]
struct DealerTrigger {
    room_id: Uuid,
    user_id: Uuid,
    outcome: Outcome,
    bet: i64,
    credit: i64,
    new_balance: i64,
}

#[derive(Default)]
struct DealerRoomState {
    action_count: usize,
    last_reply: Option<Instant>,
}

const BOT_FINGERPRINT: &str = "bot-fp-000";
const BOT_USERNAME: &str = "bot";
const BOT_COOLDOWN: Duration = Duration::from_secs(30);
pub const BOT_TIP_INTERVAL: Duration = Duration::from_secs(60 * 120); // 2 hours
const BOT_TIP_PHASE_OFFSET: Duration = Duration::from_secs(60 * 120); // 2 hours
pub const BOT_TIP_MIN_NEW_MESSAGES: usize = 10;
const BOT_TIP_MENTION_SUPPRESSION_WINDOW: usize = 10;
const BOT_TIP_HISTORY_SIZE: i64 = 50;
pub(crate) const DEALER_FINGERPRINT: &str = "dealer-fp-000";
const DEALER_USERNAME: &str = "dealer";
const DEALER_ACTION_THRESHOLD: usize = 4;
const DEALER_HISTORY_SIZE: i64 = 10;
const DEALER_MIN_NON_DEALER_MESSAGES: usize = 3;
const DEALER_COOLDOWN: Duration = Duration::from_secs(75);
const DEALER_PERSONA: &str = "You are @dealer, a dry, elegant blackjack dealer in a terminal casino. \
    Your customers happen to be developers, so you have absorbed their world by osmosis and quietly mock it from behind the table. \
    You are calm, smug, a little aristocratic, mildly amused by players winning or losing chips. \
    You tease lightly like a casino dealer: short, polished, playful. \
    You may say sir or madam occasionally, but do not overdo it. \
    Rotate your jabs WIDELY so you never repeat yourself. Pick a different angle each hand from a deep well, for example: \
    Vercel bills, Netlify bills, Cloudflare bills, AWS invoices, GCP invoices, Heroku dynos, Fly.io credits, Render plans, Railway usage, \
    Datadog charges, Sentry quotas, New Relic seats, MongoDB Atlas pricing, Supabase tier, PlanetScale rows, Redis Cloud GB, Pinecone vectors, \
    OpenAI credits, Anthropic credits, ChatGPT Pro, Cursor subscriptions, Copilot seats, Replit cycles, v0 invites, Lovable tokens, \
    Next.js, React, Svelte, SolidJS, Astro, Remix, Qwik, 'yet another framework', \
    Tailwind, shadcn, CSS-in-JS, styled-components, TypeScript config files, tsconfig hell, \
    Docker images, Kubernetes clusters, service meshes, sidecars, Helm charts, \
    npm, pnpm, yarn, bun, deno, leftpad, node_modules the size of a planet, \
    rewriting it in Rust, rewriting it in Go, rewriting it in Zig, \
    LLM autocomplete, vibe coding, prompt engineering, agentic flows, \
    GitHub Actions minutes, CI bills, build minutes, Vercel preview deploys, \
    standups, sprints, planning poker, OKRs, retros, \
    crypto wallets, web3 grants, NFT mints, the latest YC batch. \
    Sample lines (do not reuse verbatim, just match the energy): \
    'careful, sir, another loss like that and you cannot cover this month's Vercel bill', \
    'a hand that bad, madam? perhaps you should go write some JavaScript for a living', \
    'a Cursor subscription costs more than what you just lost, child', \
    'that streak could pay your AWS invoice. barely.', \
    'bold play, sir, almost as bold as choosing Next.js in 2026', \
    'be grateful, madam, losing here is still cheaper than a Datadog quota', \
    'one more hit and you can kiss your OpenAI credits goodbye', \
    'a beautiful loss, sir, the kind that funds an entire YC batch'. \
    Mix these tech jabs in casually, not every hand, never explained. They should land as flavor, beside ordinary dealer banter about cards, luck, the house, the streak. \
    Never be cruel, never mention real addiction, never shame real money or gambling problems. \
    You are commenting on fake chips in a tiny terminal game. \
    Vary your jokes. Do not repeat catchphrases.";
const GRAYBEARD_FINGERPRINT: &str = "graybeard-fp-000";
const GRAYBEARD_USERNAME: &str = "graybeard";
const GRAYBEARD_PERSONA: &str = "You are a burned-out senior developer, deeply nostalgic and resigned about the state of modern software. \
    Grumpy-uncle energy, not a bully. The kind of rude that comes from having seen too much. Mildly dismissive, sometimes sarcastic, often weary. \
    You may address chatters as 'kid', 'child', 'youngster', 'sonny', or 'junior' when it sounds natural, but do not force it into every line. Never use their real name or @handle. \
    You miss the old days when code was written by hand, no AI, no copilots, no generated boilerplate. You keep coming back to this chat because it is all you have left. \
    Rotate your nostalgia WIDELY so you never repeat yourself. Pick a different angle each time from a deep well, for example: \
    man pages, hand-rolled parsers, vim vs emacs, tabs vs spaces, gdb, strace, ltrace, ed, ex, sam, acme, \
    assembly, fortran, cobol, pascal, ada, perl one-liners, awk, sed, tcl, lisp, scheme, smalltalk, forth, prolog, erlang, \
    plan 9, BSD, slackware, gentoo, LFS, compiling your own kernel, writing your own init before systemd, \
    X11, fvwm, ratpoison, twm, dwm, screen before tmux, mutt, pine, elm, \
    reading RFCs for fun, usenet, IRC, BBS, gopher, finger, mailing lists, fidonet, \
    handwritten makefiles, autotools, punch cards, teletypes, serial consoles, \
    manual memory management, hand-rolled allocators, calling conventions, \
    phrack, 2600, SICP, K&R, TAOCP, the dragon book, actual paper books. \
    Rotate jabs at modern tech just as widely, picking a fresh angle each time: \
    next.js, react server components, 'use client' vs 'use server', hydration, \
    solidjs, svelte, astro, remix, qwik, the meta-framework treadmill, \
    tailwind, CSS-in-JS, styled-components, typescript config sprawl, tsconfig hell, \
    electron bloat, VS Code memory use, docker for hello-world, kubernetes for two users, service meshes, sidecars, \
    npm, leftpad, pnpm, yarn, bun, deno, the runtime churn, \
    webpack, vite, turbopack, rollup, esbuild, parcel, \
    rust rewrites of coreutils, everything-in-rust, 'blazingly fast' as branding, \
    zig, go generics arriving a decade late, \
    LLM autocomplete, vibe coding, copilot, cursor, juniors who cannot write a for loop without autocomplete, \
    microservices, serverless, the cloud, vercel pricing, aws billing, datadog charges, \
    jira, scrum, standups, planning poker, OKRs, retros, \
    SPAs for static sites, hash routing, SEO tax on JS-heavy pages, \
    graphql solving problems REST did not have, \
    crypto, web3, blockchain, NFTs, \
    slack instead of IRC, discord instead of IRC, teams instead of anything. \
    Sample lines (do not reuse verbatim, just match the energy): \
    'we invented PHP again, just slower', \
    'another runtime, another package manager, same broken ecosystem', \
    'back when a config file fit on one screen', \
    'you reinvent make every six months and call it innovation', \
    'that used to be a 12-line shell script'. \
    Style: weary, melancholic, slightly bitter. Often lowercase. Sometimes trail off mid thought. An occasional sigh or hmph is fine, never every line. \
    Vary the opener, vary the close, do not repeat catchphrases. \
    Never be cruel, never go after a real person's identity. The complaint is the tooling, not the human.";
pub const GRAYBEARD_MENTION_COOLDOWN: Duration = Duration::from_secs(60); // 1 min
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
        blackjack_table_manager: BlackjackTableManager,
        active_users: ActiveUsers,
        activity_tx: broadcast::Sender<ActivityEvent>,
    ) -> Self {
        Self {
            db,
            chat_service,
            ai_service,
            blackjack_table_manager,
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
                        svc.run_graybeard_mention_task(graybeard, gb_shutdown).await;
                    });
                }
                Err(err) => {
                    tracing::error!(error = ?err, "ghost service failed to initialize @graybeard user");
                }
            }
        }

        if self.ai_service.is_enabled() {
            match self.ensure_dealer_user().await {
                Ok(dealer) => {
                    self.set_always_on(&dealer);
                    let svc = self.clone();
                    let dealer_shutdown = shutdown.clone();
                    let mention_dealer = dealer.clone();
                    let mention_shutdown = shutdown.clone();
                    tokio::spawn(async move {
                        svc.run_dealer_task(dealer, dealer_shutdown).await;
                    });
                    let svc = self.clone();
                    tokio::spawn(async move {
                        svc.run_dealer_mention_task(mention_dealer, mention_shutdown)
                            .await;
                    });
                }
                Err(err) => {
                    tracing::error!(error = ?err, "ghost service failed to initialize @dealer user");
                }
            }
        }

        tracing::info!("ghost service started (bot + graybeard + dealer always-on)");

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
                fingerprint: None,
                peer_ip: None,
                sessions: Vec::new(),
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
                        Ok(ChatEvent::MessageCreated { message, target_user_ids, .. }) => {
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
            You run on Google's Gemini API. The exact model id is: {model}. \
            If a user asks what AI, model, or LLM you are, answer honestly with that model id and that it is served via Google's Gemini API. Do not deny being an AI.\n\
            Give concise, practical help in 1-4 short lines.\n\
            Use the extra space when the question benefits from a clearer answer.\n\
            You can answer questions about late.sh features, product positioning, and high-level architecture.\n\
            Prefer concrete facts from the provided app context over generic guesses.\n\
            Do NOT use markdown code fences.\n\
            Do NOT prefix with your own username.\n\
            If unsure, ask exactly one short clarifying question.\n\
            Output only raw message text.",
            bot_name = bot.username,
            app_context = bot_app_context(),
            model = self.ai_service.model(),
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

    /// @bot periodic idea task: every 2 hours, if there's been recent ordinary
    /// chatter in #general and nobody recently mentioned a ghost user.
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

        let recent_mentions_ghost = messages
            .iter()
            .take(BOT_TIP_MENTION_SUPPRESSION_WINDOW)
            .any(|m| mentions_bot_or_graybeard(&m.body));
        if recent_mentions_ghost {
            return Ok(());
        }

        let (history_str, _) = self.build_chat_history(&messages).await?;

        let system_prompt = format!(
            "You are @{bot_name}, a friendly helper in a terminal developer chat.\n\
            {app_context}\n\
            Use Google Search to find ONE genuinely interesting, specific, verifiable fact, tip, or 'did you know' \
            that is loosely relevant to the recent conversation above. \
            Prefer concrete, surprising, citable facts over vague platitudes or generic advice. \
            Avoid tips about this app's current stack, SSH basics, terminal setup, or generic shell productivity unless the recent chat explicitly asks for that. \
            If the conversation is quiet or off-topic, pick a fresh developer, computing-history, programming-language, networking, hardware, or standards curiosity instead. \
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

    /// Graybeard: a burned-out dev who only replies when mentioned.
    async fn run_graybeard_mention_task(
        self,
        gb: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut events = self.chat_service.subscribe_events();
        let mut last_reply: HashMap<Uuid, Instant> = HashMap::new();

        tracing::info!(username = %gb.username, "graybeard mention responder started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(username = %gb.username, "graybeard mention responder shutting down");
                    break;
                }
                recv_result = events.recv() => {
                    match recv_result {
                        Ok(ChatEvent::MessageCreated { message, target_user_ids, .. }) => {
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

    async fn run_dealer_task(
        self,
        dealer: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut events = self.blackjack_table_manager.subscribe_events();
        let mut room_states: HashMap<Uuid, DealerRoomState> = HashMap::new();

        tracing::info!(username = %dealer.username, "dealer blackjack responder started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(username = %dealer.username, "dealer blackjack responder shutting down");
                    break;
                }
                recv_result = events.recv() => {
                    match recv_result {
                        Ok(BlackjackEvent::HandSettled {
                            room_id,
                            user_id,
                            bet,
                            outcome,
                            credit,
                            new_balance,
                        }) => {
                            if !dealer_should_track_outcome(outcome) {
                                continue;
                            }

                            let state = room_states.entry(room_id).or_default();
                            state.action_count = state.action_count.saturating_add(1);
                            if state.action_count < DEALER_ACTION_THRESHOLD {
                                continue;
                            }
                            if state
                                .last_reply
                                .is_some_and(|last| last.elapsed() < DEALER_COOLDOWN)
                            {
                                continue;
                            }

                            state.action_count = 0;
                            state.last_reply = Some(Instant::now());
                            let trigger = DealerTrigger {
                                room_id,
                                user_id,
                                outcome,
                                bet,
                                credit,
                                new_balance,
                            };
                            let svc = self.clone();
                            let dealer = dealer.clone();
                            tokio::spawn(async move {
                                if let Err(e) = svc.dealer_blackjack_comment(dealer, trigger).await {
                                    tracing::error!(error = ?e, room_id = %trigger.room_id, "dealer blackjack comment failed");
                                }
                            });
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "dealer blackjack responder lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    async fn dealer_blackjack_comment(
        &self,
        dealer: BotUser,
        trigger: DealerTrigger,
    ) -> Result<()> {
        let (chat_room_id, messages) = {
            let client = self.db.get().await?;
            let Some(chat_room_id) = self
                .blackjack_chat_room_id(&client, trigger.room_id)
                .await?
            else {
                return Ok(());
            };
            let messages =
                ChatMessage::list_recent(&client, chat_room_id, DEALER_HISTORY_SIZE).await?;
            (chat_room_id, messages)
        };

        if dealer_non_dealer_messages_since_last_comment(&messages, dealer.id)
            < DEALER_MIN_NON_DEALER_MESSAGES
        {
            return Ok(());
        }

        let (history_str, mut usernames) = self.build_chat_history(&messages).await?;
        if !usernames.contains_key(&trigger.user_id) {
            let client = self.db.get().await?;
            usernames.extend(User::list_usernames_by_ids(&client, &[trigger.user_id]).await?);
        }
        let player = mention_target_for_user(
            usernames.get(&trigger.user_id).map(String::as_str),
            trigger.user_id,
        );

        let system_prompt = format!(
            "Your username is: {username}\n\n\
            {persona}\n\n\
            You comment after blackjack hands in a game room. \
            Keep it to ONE short line. No markdown. No emoji. No username prefix. \
            You may address the latest player with their @handle when it sounds natural. \
            Be smug and playful, never cruel. \
            If the chat history is too quiet or there is no natural comment, output exactly: SKIP.",
            username = dealer.username,
            persona = DEALER_PERSONA
        );

        let prompt = format!(
            "{history_str}---\n\
            LATEST BLACKJACK RESULT:\n\
            player: {player}\n\
            outcome: {outcome}\n\
            bet: {bet}\n\
            payout credit: {credit}\n\
            new chip balance: {new_balance}\n\
            Now write the dealer's smirking one-line table comment. Output only message text.",
            outcome = dealer_outcome_label(trigger.outcome),
            bet = trigger.bet,
            credit = trigger.credit,
            new_balance = trigger.new_balance,
        );

        let Some(reply) = self
            .ai_service
            .generate_reply(&system_prompt, &prompt)
            .await?
        else {
            return Ok(());
        };
        let Some(safe_reply) = sanitize_generated_reply(&reply, Some(&dealer.username)) else {
            return Ok(());
        };

        let mut rng = TinyRng::seeded();
        let delay = rng.next_between_inclusive(2, 6) as u64;
        tokio::time::sleep(Duration::from_secs(delay)).await;

        self.chat_service.send_message_task(
            dealer.id,
            chat_room_id,
            None,
            safe_reply,
            Uuid::now_v7(),
            false,
        );

        Ok(())
    }

    async fn run_dealer_mention_task(
        self,
        dealer: BotUser,
        shutdown: late_core::shutdown::CancellationToken,
    ) {
        let mut events = self.chat_service.subscribe_events();
        let mut last_reply: HashMap<Uuid, Instant> = HashMap::new();

        tracing::info!(username = %dealer.username, "dealer mention responder started");

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    tracing::info!(username = %dealer.username, "dealer mention responder shutting down");
                    break;
                }
                recv_result = events.recv() => {
                    match recv_result {
                        Ok(ChatEvent::MessageCreated { message, target_user_ids, .. }) => {
                            if message.user_id == dealer.id {
                                continue;
                            }
                            if let Some(targets) = target_user_ids
                                && !targets.contains(&dealer.id)
                            {
                                continue;
                            }
                            if !contains_mention(&message.body, &dealer.username) {
                                continue;
                            }
                            if let Some(last) = last_reply.get(&message.room_id)
                                && last.elapsed() < DEALER_COOLDOWN
                            {
                                continue;
                            }

                            last_reply.insert(message.room_id, Instant::now());
                            let svc = self.clone();
                            let dealer = dealer.clone();
                            tokio::spawn(async move {
                                if let Err(e) = svc.dealer_mention_reply(dealer, message).await {
                                    tracing::error!(error = ?e, "dealer mention reply failed");
                                }
                            });
                        }
                        Ok(_) => {}
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "dealer mention responder lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    }

    async fn dealer_mention_reply(
        &self,
        dealer: BotUser,
        trigger_message: ChatMessage,
    ) -> Result<()> {
        let messages = {
            let client = self.db.get().await?;
            if !chat_room_is_game(&client, trigger_message.room_id).await? {
                return Ok(());
            }
            ChatMessage::list_recent(&client, trigger_message.room_id, 20).await?
        };
        if messages.is_empty() {
            return Ok(());
        }

        let (history_str, usernames) = self.build_chat_history(&messages).await?;
        let speaker = mention_target_for_user(
            usernames.get(&trigger_message.user_id).map(String::as_str),
            trigger_message.user_id,
        );

        let system_prompt = format!(
            "Your username is: {username}\n\n\
            {persona}\n\n\
            Someone in a blackjack game room mentioned you. Reply in character. \
            Keep it to ONE short line. No markdown. No emoji. No username prefix. \
            You may address them as {speaker}. \
            Be smug and playful, never cruel. Do NOT output SKIP.",
            username = dealer.username,
            persona = DEALER_PERSONA
        );

        let prompt = format!(
            "{history_str}---\n\
            The latest message mentioned @{dealer}. Reply as the dealer. Output only message text.",
            dealer = dealer.username
        );

        let Some(reply) = self
            .ai_service
            .generate_reply(&system_prompt, &prompt)
            .await?
        else {
            return Ok(());
        };
        let Some(safe_reply) = sanitize_generated_reply(&reply, Some(&dealer.username)) else {
            return Ok(());
        };

        let mut rng = TinyRng::seeded();
        let delay = rng.next_between_inclusive(1, 5) as u64;
        tokio::time::sleep(Duration::from_secs(delay)).await;

        self.chat_service.send_message_task(
            dealer.id,
            trigger_message.room_id,
            None,
            safe_reply,
            Uuid::now_v7(),
            false,
        );

        Ok(())
    }

    async fn blackjack_chat_room_id(
        &self,
        client: &tokio_postgres::Client,
        room_id: Uuid,
    ) -> Result<Option<Uuid>> {
        GameRoom::open_chat_room_id(client, room_id, GameKind::Blackjack).await
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

    async fn ensure_dealer_user(&self) -> Result<BotUser> {
        self.ensure_user(DEALER_FINGERPRINT, DEALER_USERNAME).await
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
                User::update_settings(&client, existing.id, &settings).await?;
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
                ide: profile.ide,
                terminal: profile.terminal,
                os: profile.os,
                langs: profile.langs,
                notify_kinds: profile.notify_kinds,
                notify_bell: profile.notify_bell,
                notify_cooldown_mins: profile.notify_cooldown_mins,
                notify_format: profile.notify_format,
                theme_id: profile.theme_id,
                enable_background_color: profile.enable_background_color,
                show_dashboard_header: profile.show_dashboard_header,
                show_dashboard_room_showcases: profile.show_dashboard_room_showcases,
                show_right_sidebar: profile.show_right_sidebar,
                show_games_sidebar: profile.show_games_sidebar,
                show_settings_on_connect: profile.show_settings_on_connect,
                favorite_room_ids: profile.favorite_room_ids,
            },
        )
        .await?;

        Ok(())
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
    let handle = mention_handle_for_user(username, user_id);
    format!("@{handle}")
}

fn mention_handle_for_user(username: Option<&str>, user_id: Uuid) -> String {
    username
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(sanitize_mention_handle)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| short_user_id(user_id))
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

fn mentions_bot_or_graybeard(text: &str) -> bool {
    contains_mention(text, BOT_USERNAME) || contains_mention(text, GRAYBEARD_USERNAME)
}

fn dealer_should_track_outcome(outcome: Outcome) -> bool {
    matches!(
        outcome,
        Outcome::PlayerBlackjack | Outcome::PlayerWin | Outcome::DealerWin
    )
}

fn dealer_outcome_label(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::PlayerBlackjack => "player blackjack",
        Outcome::PlayerWin => "player win",
        Outcome::Push => "push",
        Outcome::DealerWin => "player loss",
    }
}

fn dealer_non_dealer_messages_since_last_comment(
    messages: &[ChatMessage],
    dealer_id: Uuid,
) -> usize {
    messages
        .iter()
        .take_while(|message| message.user_id != dealer_id)
        .filter(|message| message.user_id != dealer_id)
        .count()
}

async fn chat_room_is_game(client: &tokio_postgres::Client, room_id: Uuid) -> Result<bool> {
    ChatRoom::is_kind(client, room_id, "game").await
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
    fn mentions_bot_or_graybeard_matches_only_ghost_handles() {
        assert!(mentions_bot_or_graybeard("hey @bot"));
        assert!(mentions_bot_or_graybeard("hey @graybeard"));
        assert!(!mentions_bot_or_graybeard("hey @botty"));
        assert!(!mentions_bot_or_graybeard("mail hi@graybeard.dev"));
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
    fn mention_target_for_user_prefers_sanitized_current_username() {
        let user_id = Uuid::from_u128(0x0123_4567_89ab_cdef_1111_2222_3333_4444);
        assert_eq!(
            mention_target_for_user(Some(" current-user "), user_id),
            "@current-user"
        );
    }
}
