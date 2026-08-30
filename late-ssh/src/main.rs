use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use anyhow::Context;
use late_core::{
    db::Db, models::chat_room::ChatRoom, rate_limit::IpRateLimiter, shutdown::CancellationToken,
};
use late_ssh::{
    api,
    app::ai::{ghost::GhostService, svc::AiService},
    app::audio::now_playing::svc::NowPlayingService,
    app::audio::svc::AudioService,
    app::chat::feeds::svc::FeedService,
    app::chat::news::svc::ArticleService,
    app::chat::notifications::svc::NotificationService,
    app::chat::showcase::svc::ShowcaseService,
    app::chat::svc::ChatService,
    app::chat::work::svc::WorkService,
    app::profile::svc::ProfileService,
    app::voice::svc::VoiceService,
    config::Config,
    moderation::service::ModerationInfra,
    session::SessionRegistry,
    ssh,
    state::State,
};
use tokio::{sync::Semaphore, task::JoinSet};

fn begin_drain(
    state: &State,
    accept_shutdown: &CancellationToken,
    singleton_shutdown: &CancellationToken,
) {
    state
        .is_draining
        .store(true, std::sync::atomic::Ordering::Relaxed);
    accept_shutdown.cancel();
    singleton_shutdown.cancel();
}

async fn finish_ssh_drain(
    ssh_task: &mut tokio::task::JoinHandle<anyhow::Result<()>>,
    fatal_error: &mut Option<anyhow::Error>,
) {
    tracing::info!("waiting for active ssh sessions to drain...");
    match ssh_task.await {
        Ok(Err(err)) => {
            tracing::error!(error = ?err, "ssh task failed during drain");
            *fatal_error = Some(err);
        }
        Ok(Ok(())) => tracing::info!("ssh task finished draining"),
        Err(err) => {
            tracing::error!(error = ?err, "ssh task panicked during drain");
            *fatal_error = Some(anyhow::Error::new(err).context("ssh task panicked"));
        }
    }
}

async fn flush_dartboard_snapshot(state: &State, fatal_error: &mut Option<anyhow::Error>) {
    match late_ssh::dartboard::flush_server_snapshot(
        &state.db,
        &state.dartboard_server,
        &state.dartboard_provenance,
    )
    .await
    {
        Ok(()) => tracing::info!("flushed artboard snapshot during shutdown"),
        Err(err) => {
            tracing::error!(error = ?err, "failed to flush artboard snapshot during shutdown");
            if fatal_error.is_none() {
                *fatal_error =
                    Some(err.context("failed to flush artboard snapshot during shutdown"));
            }
        }
    }
}

async fn flush_lateania_characters(state: &State, fatal_error: &mut Option<anyhow::Error>) {
    match state.lateania_service.flush_all().await {
        Ok(()) => tracing::info!("flushed lateania characters during shutdown"),
        Err(err) => {
            tracing::error!(error = ?err, "failed to flush lateania characters during shutdown");
            if fatal_error.is_none() {
                *fatal_error =
                    Some(err.context("failed to flush lateania characters during shutdown"));
            }
        }
    }
}

async fn flush_online_time(state: &State, fatal_error: &mut Option<anyhow::Error>) {
    match state.leaderboard_service.flush_online_time().await {
        Ok(()) => tracing::info!("flushed online time during shutdown"),
        Err(err) => {
            tracing::error!(error = ?err, "failed to flush online time during shutdown");
            if fatal_error.is_none() {
                *fatal_error = Some(err.context("failed to flush online time during shutdown"));
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = late_core::telemetry::init_telemetry("late-ssh")
        .context("failed to initialize telemetry")?;

    // Load configuration from environment
    let config = Config::load().context("failed to load configuration")?;
    config.log_startup();

    // Init database connection pool
    let db = Db::new(&config.db).context("failed to initialize database")?;
    db.health().await.context("database health check failed")?;
    db.migrate().await.context("database migration failed")?;
    {
        let client = db.get().await.context("failed to get db client")?;
        let lounge = ChatRoom::ensure_lounge(&client)
            .await
            .context("failed to ensure lounge chat room")?;
        tracing::info!(room_id = %lounge.id, "ensured lounge chat room");
    }
    tracing::info!("database initialized and migrations applied");

    // Initialize shared state
    let conn_limit = Arc::new(Semaphore::new(config.max_conns_global));
    let conn_counts = Arc::new(Mutex::new(HashMap::new()));
    let active_users = Arc::new(Mutex::new(HashMap::new()));
    let afk_users = late_ssh::state::new_afk_users();
    let username_directory = late_ssh::usernames::load(&db)
        .await
        .context("failed to load username directory")?;
    let (activity_tx, _activity_rx) = late_ssh::app::activity::channel::new(512);
    let activity_publisher =
        late_ssh::app::activity::publisher::ActivityPublisher::new(db.clone(), activity_tx.clone())
            .with_username_directory(username_directory.clone());
    let now_playing_service = NowPlayingService::new(config.icecast_url.clone());
    let now_playing_rx = now_playing_service.subscribe_state();
    let radio_meta_service = late_ssh::app::audio::radio_meta::svc::RadioMetaService::new();
    let radio_meta_rx = radio_meta_service.subscribe_state();
    let public_stream_base_url = format!("{}/stream", config.web_url.trim_end_matches('/'));
    let paired_client_registry =
        late_ssh::paired_clients::PairedClientRegistry::new(public_stream_base_url);
    let audio_service = AudioService::new(
        db.clone(),
        config.youtube_api_key.clone(),
        paired_client_registry.clone(),
        active_users.clone(),
    );
    let voice_service = VoiceService::new(config.voice.clone()).with_db(db.clone());
    let stream_service = late_ssh::app::stream::svc::StreamService::new(
        db.clone(),
        voice_service.clone(),
        activity_publisher.clone(),
        config.web_url.clone(),
    );
    let session_registry = SessionRegistry::new();
    let irc_registry = late_ssh::ircd::registry::IrcRegistry::new();
    let notification_service = NotificationService::new(db.clone());
    let ai_service = AiService::new(config.ai.enabled, config.ai.api_key.clone());
    let translation_service =
        late_ssh::app::ai::translate::TranslationService::new(db.clone(), ai_service.clone());
    let summary_service =
        late_ssh::app::ai::summary::SummaryService::new(db.clone(), ai_service.clone());
    let chat_service = ChatService::new_with_active_users(
        db.clone(),
        notification_service.clone(),
        active_users.clone(),
    )
    .with_username_directory(username_directory.clone())
    .with_session_registry(session_registry.clone())
    .with_irc_registry(irc_registry.clone())
    .with_force_admin(config.force_admin)
    .with_translation_service(translation_service.clone());
    let _poll_finalizer_recovery_task = chat_service.start_poll_finalizer_recovery_task();
    let _lounge_feed_task = late_ssh::app::activity::lounge::start_lounge_feed_task(
        db.clone(),
        chat_service.clone(),
        username_directory.clone(),
        activity_tx.subscribe(),
    );
    let profile_service = ProfileService::new(db.clone(), active_users.clone())
        .with_username_directory(username_directory.clone())
        .with_session_registry(session_registry.clone())
        .with_irc_registry(irc_registry.clone());
    let article_service = ArticleService::new(db.clone(), ai_service.clone(), chat_service.clone());
    let feed_service = FeedService::new(db.clone());
    feed_service.start_poll_task();
    let cyberspace_service = late_ssh::app::chat::cyberspace::svc::CyberspaceService::new(
        db.clone(),
        late_ssh::app::chat::cyberspace::api::BASE_URL.to_string(),
    )
    .with_activity(activity_publisher.clone());
    let showcase_service = ShowcaseService::new(db.clone());
    let work_service = WorkService::new(db.clone());
    let twenty_forty_eight_service =
        late_ssh::app::arcade::twenty_forty_eight::svc::TwentyFortyEightService::new(db.clone())
            .with_activity_feed(activity_tx.clone());
    let tetris_service = late_ssh::app::arcade::tetris::svc::LaterisService::new(db.clone())
        .with_activity_feed(activity_tx.clone());
    let snake_service = late_ssh::app::arcade::snake::svc::SnakeService::new(db.clone())
        .with_activity_feed(activity_tx.clone());
    let traffic_service = late_ssh::app::arcade::traffic::svc::TrafficService::new(db.clone())
        .with_activity_feed(activity_tx.clone());
    let rubiks_cube_service = late_ssh::app::arcade::rubiks_cube::svc::RubiksCubeService::new(
        db.clone(),
        activity_tx.clone(),
    );
    let sliding_puzzle_service =
        late_ssh::app::arcade::sliding_puzzle::svc::SlidingPuzzleService::new(
            db.clone(),
            activity_tx.clone(),
        );
    let le_word_service =
        late_ssh::app::arcade::le_word::svc::LeWordService::new(db.clone(), activity_tx.clone());
    let chip_service = late_ssh::app::games::chips::svc::ChipService::new(db.clone());
    let _chip_activity_reward_task = chip_service.start_activity_reward_task(activity_tx.clone());
    let daily_service = late_ssh::app::lobby::daily::svc::DailyService::new(
        db.clone(),
        chip_service.clone(),
        activity_publisher.clone(),
    );
    daily_service.refresh_task();
    daily_service.start_sweeper_task();
    let lateania_service = late_ssh::app::door::lateania::svc::LateaniaService::new(
        activity_publisher.clone(),
        chip_service.clone(),
        db.clone(),
    );
    let greendragon_service = late_ssh::app::door::greendragon::svc::GreenDragonService::new(
        activity_publisher.clone(),
        chip_service.clone(),
        db.clone(),
    );
    let darkroom_service = late_ssh::app::door::darkroom::svc::DarkroomService::new(
        activity_publisher.clone(),
        chip_service.clone(),
        db.clone(),
    );
    let arcade_handle_service = late_ssh::app::door::arcade::ArcadeHandleService::new(db.clone());
    let door_rc_service = late_ssh::app::door::rc::DoorRcService::new(db.clone());
    let house_registry = late_ssh::app::lobby::house::registry::HouseTableRegistry::new(
        chip_service.clone(),
        late_ssh::app::lobby::house::blackjack::player::BlackjackPlayerDirectory::new(db.clone()),
        activity_publisher.clone(),
        db.clone(),
    );
    house_registry
        .ensure_chat_rooms()
        .await
        .context("failed to ensure house table chat rooms")?;
    house_registry.start_seat_activity_task();
    let sudoku_service =
        late_ssh::app::arcade::sudoku::svc::SudokuService::new(db.clone(), activity_tx.clone());
    let nonogram_service =
        late_ssh::app::arcade::nonogram::svc::NonogramService::new(db.clone(), activity_tx.clone());
    let solitaire_service = late_ssh::app::arcade::solitaire::svc::SolitaireService::new(
        db.clone(),
        activity_tx.clone(),
    );
    let minesweeper_service = late_ssh::app::arcade::minesweeper::svc::MinesweeperService::new(
        db.clone(),
        activity_tx.clone(),
    );
    let bonsai_service =
        late_ssh::app::bonsai::svc::BonsaiService::new(db.clone(), activity_tx.clone());
    let pet_service = late_ssh::app::pet::svc::PetService::new(db.clone());
    let initial_dartboard = match late_ssh::dartboard::load_persisted_artboard(&db).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            tracing::warn!(error = ?error, "failed to restore artboard snapshot");
            None
        }
    };
    let dartboard_provenance = initial_dartboard
        .as_ref()
        .map(|snapshot| snapshot.provenance.clone())
        .unwrap_or_default()
        .shared();
    let dartboard_server = late_ssh::dartboard::spawn_persistent_server(
        db.clone(),
        initial_dartboard.map(|snapshot| snapshot.canvas),
        dartboard_provenance.clone(),
    );
    let chat_service = chat_service
        .with_moderation_infra(
            ModerationInfra::default()
                .with_force_admin(config.force_admin)
                .with_artboard_handles(dartboard_server.clone(), dartboard_provenance.clone())
                .with_voice(voice_service.clone())
                .with_stream(stream_service.clone()),
        )
        .with_chip_service(chip_service.clone())
        .with_activity(activity_publisher.clone());
    // Gild markers cross replicas over Postgres, not over this process's
    // chat broadcast; see `ChatService::start_gild_listener_task`.
    let _chat_gild_listener_task = chat_service.start_gild_listener_task(config.db.clone());
    // The crown's glyph crosses replicas over Postgres, not over any
    // in-process broadcast; the listener also seeds this replica's holder on
    // every (re)connect. See `app/crown/svc.rs`.
    let crown_service = late_ssh::app::crown::svc::CrownService::new(db.clone())
        .with_activity(activity_publisher.clone());
    let _crown_listener_task = crown_service.start_listener_task(config.db.clone());
    // The pot's panel and its winner banner cross replicas over Postgres,
    // and the draw is settled by a status transition so exactly one replica
    // pays however many are sweeping. See `app/pot/svc.rs`.
    let pot_service = late_ssh::app::pot::svc::PotService::new(db.clone())
        .with_activity(activity_publisher.clone());
    let _pot_listener_task = pot_service.start_listener_task(config.db.clone());
    let _pot_sweeper_task = pot_service.start_sweeper_task();
    let leaderboard_service = late_ssh::app::LeaderboardService::new(db.clone());
    let _profile_award_snapshot_task = leaderboard_service
        .clone()
        .start_profile_award_snapshot_loop();
    let quest_service = late_ssh::app::QuestService::new(db.clone(), activity_tx.clone());
    let _quest_activity_task = quest_service.start_activity_task();
    let _quest_listener_task = quest_service.start_listener_task(config.db.clone());
    let flair_directory = late_ssh::app::common::username_effect::new_directory();
    let shop_service = late_ssh::app::ShopService::new(db.clone())
        .with_flair_directory(flair_directory.clone())
        .with_activity(activity_publisher.clone())
        .with_ai_service(ai_service.clone());
    let _shop_listener_task = shop_service.start_listener_task(config.db.clone());
    let ultimate_service = late_ssh::app::UltimateService::new(db.clone());
    let nonogram_library = match late_ssh::app::arcade::nonogram::state::load_default_library() {
        Ok(library) => library,
        Err(err) => {
            tracing::warn!(error = ?err, "failed to load nonogram asset packs; continuing with empty library");
            late_ssh::app::arcade::nonogram::state::Library::default()
        }
    };
    let clubhouse_lobby = late_ssh::app::clubhouse::lobby::SharedLobby::new();
    let scratchpad_registry = late_ssh::app::scratchpad::registry::SharedScratchpadRegistry::new();
    let mention_ladders = late_ssh::app::ai::ladder::MentionLadders::new();
    let ghost_service = GhostService::new(
        db.clone(),
        chat_service.clone(),
        ai_service.clone(),
        active_users.clone(),
        activity_tx.clone(),
        username_directory.clone(),
        chip_service.clone(),
        clubhouse_lobby.clone(),
        mention_ladders.clone(),
    );
    let ssh_attempt_limiter = IpRateLimiter::new(
        config.ssh_max_attempts_per_ip,
        config.ssh_rate_limit_window_secs,
    );
    let ws_pair_limiter = IpRateLimiter::new(
        config.ws_pair_max_attempts_per_ip,
        config.ws_pair_rate_limit_window_secs,
    );
    // Initialize app state
    let state = State {
        config: config.clone(),
        db: db.clone(),
        ai_service: ai_service.clone(),
        translation_service: translation_service.clone(),
        summary_service: summary_service.clone(),
        audio_service: audio_service.clone(),
        voice_service,
        stream_service,
        chat_service: chat_service.clone(),
        notification_service: notification_service.clone(),
        article_service,
        feed_service,
        cyberspace_service,
        showcase_service,
        work_service,
        profile_service,
        twenty_forty_eight_service,
        tetris_service,
        snake_service,
        traffic_service,
        rubiks_cube_service,
        sliding_puzzle_service,
        le_word_service,
        sudoku_service,
        nonogram_service,
        solitaire_service,
        minesweeper_service,
        lateania_service,
        greendragon_service,
        darkroom_service,
        arcade_handle_service,
        door_rc_service,
        daily_service,
        bonsai_service,
        pet_service,
        nonogram_library,
        chip_service,
        house_registry,
        dartboard_server,
        dartboard_provenance,
        leaderboard_service: leaderboard_service.clone(),
        quest_service,
        shop_service,
        ultimate_service,
        conn_limit,
        conn_counts,
        pair_ws_counts: Arc::new(Mutex::new(HashMap::new())),
        active_users,
        clubhouse_lobby,
        mention_ladders,
        scratchpad_registry,
        afk_users,
        username_directory: username_directory.clone(),
        flair_directory: flair_directory.clone(),
        pomodoro_directory: late_ssh::app::common::pomodoro::new_directory(),
        crown_service: crown_service.clone(),
        pot_service: pot_service.clone(),
        activity_feed: activity_tx,
        now_playing_rx: now_playing_rx.clone(),
        radio_meta_rx: radio_meta_rx.clone(),
        session_registry,
        paired_client_registry,
        irc_registry: irc_registry.clone(),
        ssh_attempt_limiter,
        ws_pair_limiter,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let session_shutdown = CancellationToken::new();
    let accept_shutdown = CancellationToken::new();
    let singleton_shutdown = CancellationToken::new();
    let _username_directory_refresh_task = late_ssh::usernames::start_refresh_task(
        db.clone(),
        username_directory,
        singleton_shutdown.clone(),
    );

    // The door log pipe: tail each door host's append-only log files over the
    // stats SSH session and land runs/milestones/badges (PLAN-ROGUELIKE-BOARDS
    // Phases 1-3). One task per door, gated on the same flag as that door's
    // client; single-replica by the same assumption as every other
    // process-global singleton here.
    let door_ingest_service = late_ssh::app::door::ingest::svc::DoorIngestService::new(
        db.clone(),
        state.chip_service.clone(),
        activity_publisher.clone(),
    );
    let _dcss_ingest_task = state.config.dcss_enabled.then(|| {
        door_ingest_service.clone().start_dcss_task(
            late_ssh::app::door::ingest::svc::DoorIngestTarget {
                host: state.config.dcss_host.clone(),
                port: state.config.dcss_port,
                secret: state.config.dcss_secret.clone(),
            },
            singleton_shutdown.clone(),
        )
    });
    let _nethack_ingest_task = state.config.nethack_enabled.then(|| {
        door_ingest_service.clone().start_nethack_task(
            late_ssh::app::door::ingest::svc::DoorIngestTarget {
                host: state.config.nethack_host.clone(),
                port: state.config.nethack_port,
                secret: state.config.nethack_secret.clone(),
            },
            singleton_shutdown.clone(),
        )
    });
    let _brogue_ingest_task = state.config.brogue_enabled.then(|| {
        door_ingest_service.clone().start_brogue_task(
            late_ssh::app::door::ingest::svc::DoorIngestTarget {
                host: state.config.brogue_host.clone(),
                port: state.config.brogue_port,
                secret: state.config.brogue_secret.clone(),
            },
            singleton_shutdown.clone(),
        )
    });

    let mut tasks = JoinSet::new();
    let api_state = state.clone();
    let api_shutdown = session_shutdown.clone();
    tasks.spawn(async move {
        api::run_api_server(api_state.config.api_port, api_state, Some(api_shutdown))
            .await
            .context("api server failed")
    });

    tasks.spawn(async move {
        let _ = leaderboard_service.start_refresh_loop().await;
        Ok(())
    });

    let ssh_shutdown = accept_shutdown.clone();
    let ssh_state = state.clone();
    let mut ssh_task = tokio::spawn(async move {
        ssh::run("0.0.0.0", config.ssh_port, ssh_state, Some(ssh_shutdown))
            .await
            .context("ssh server failed")
    });

    if state.config.irc.enabled {
        let irc_state = state.clone();
        let irc_shutdown = accept_shutdown.clone();
        tasks.spawn(async move {
            late_ssh::ircd::serve::run(irc_state, Some(irc_shutdown))
                .await
                .context("irc server failed")
        });
    }

    let now_playing_shutdown = session_shutdown.clone();
    let now_playing_task = now_playing_service.start_poll_task(now_playing_shutdown);
    tasks.spawn(async move {
        now_playing_task
            .await
            .context("now playing task panicked")?;
        Ok(())
    });

    let radio_meta_shutdown = session_shutdown.clone();
    let radio_meta_task = radio_meta_service.start_task(radio_meta_shutdown);
    tasks.spawn(async move {
        radio_meta_task.await.context("radio meta task panicked")?;
        Ok(())
    });

    let meta_forward_task = audio_service.start_meta_forward_task(
        now_playing_rx.clone(),
        radio_meta_rx.clone(),
        session_shutdown.clone(),
    );
    tasks.spawn(async move {
        meta_forward_task
            .await
            .context("meta forward task panicked")?;
        Ok(())
    });

    // Audio rides session_shutdown (fires after ssh drain) rather than
    // singleton_shutdown (fires at drain begin) so paired clients keep
    // hearing music through the entire drain window. Liquidsoap/Icecast
    // streams from a separate process and is unaffected either way.
    let audio_shutdown = session_shutdown.clone();
    tasks.spawn(async move {
        audio_service.start_background_task(audio_shutdown).await;
        Ok(())
    });

    let limiter_cleanup_shutdown = singleton_shutdown.clone();
    let ssh_limiter = state.ssh_attempt_limiter.clone();
    let ws_limiter = state.ws_pair_limiter.clone();
    tasks.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.tick().await; // skip immediate first tick
        loop {
            tokio::select! {
                _ = limiter_cleanup_shutdown.cancelled() => break,
                _ = interval.tick() => {
                    ssh_limiter.cleanup();
                    ws_limiter.cleanup();
                }
            }
        }
        Ok(())
    });

    let voice_prune_shutdown = singleton_shutdown.clone();
    let voice_prune_service = state.voice_service.clone();
    tasks.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        interval.tick().await; // skip immediate first tick
        loop {
            tokio::select! {
                _ = voice_prune_shutdown.cancelled() => break,
                _ = interval.tick() => {
                    voice_prune_service.prune_stale(chrono::Duration::seconds(90));
                }
            }
        }
        Ok(())
    });

    let stream_sweep_shutdown = singleton_shutdown.clone();
    let stream_sweep_service = state.stream_service.clone();
    tasks.spawn(async move {
        // A restart wiped the in-memory registry, so any ingress LiveKit
        // still holds is an orphaned stream key; collect them before the
        // first poll can see them.
        stream_sweep_service.reconcile_ingresses().await;
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.tick().await; // skip immediate first tick
        loop {
            tokio::select! {
                _ = stream_sweep_shutdown.cancelled() => break,
                _ = interval.tick() => {
                    // The poll feeds the OBS streams' publisher reports; the
                    // sweep right after acts on whatever state it left.
                    stream_sweep_service.poll_obs_publishers().await;
                    stream_sweep_service.sweep();
                }
            }
        }
        Ok(())
    });

    let dartboard_rollover_shutdown = singleton_shutdown.clone();
    let dartboard_rollover_db = state.db.clone();
    let dartboard_rollover_server = state.dartboard_server.clone();
    let dartboard_rollover_provenance = state.dartboard_provenance.clone();
    tasks.spawn(async move {
        late_ssh::dartboard::run_daily_snapshot_rollover_task(
            dartboard_rollover_db,
            dartboard_rollover_server,
            dartboard_rollover_provenance,
            dartboard_rollover_shutdown,
        )
        .await;
        Ok(())
    });

    let ghost_task_shutdown = singleton_shutdown.clone();
    tasks.spawn(async move {
        ghost_service
            .start_background_task(ghost_task_shutdown)
            .await;
        Ok(())
    });

    tracing::info!("starting late.sh ssh server");
    let mut fatal_error = None;
    let mut should_finish_ssh_drain = false;
    tokio::select! {
        _ = late_core::shutdown::wait_for_shutdown_signal() => {
            tracing::info!("shutdown signal received, stopping new connections");
            begin_drain(&state, &accept_shutdown, &singleton_shutdown);
            should_finish_ssh_drain = true;
        }
        result = &mut ssh_task => {
            match result {
                Ok(Err(err)) => {
                    tracing::error!(error = ?err, "ssh task failed");
                    fatal_error = Some(err);
                }
                Ok(Ok(())) => tracing::info!("ssh task exited cleanly"),
                Err(err) => {
                    tracing::error!(error = ?err, "ssh task panicked");
                    fatal_error = Some(anyhow::Error::new(err).context("ssh task panicked"));
                }
            }
            tracing::warn!("ssh task exited prematurely, beginning shutdown");
            begin_drain(&state, &accept_shutdown, &singleton_shutdown);
        }
        Some(result) = tasks.join_next() => {
            match result {
                Ok(Err(err)) => {
                    tracing::error!(error = ?err, "task failed");
                    fatal_error = Some(err);
                }
                Ok(Ok(())) => tracing::info!("task exited cleanly"),
                Err(err) => {
                    tracing::error!(error = ?err, "task panicked");
                    fatal_error = Some(anyhow::Error::new(err).context("task panicked"));
                }
            }
            tracing::warn!("a task exited prematurely, beginning shutdown");
            begin_drain(&state, &accept_shutdown, &singleton_shutdown);
            should_finish_ssh_drain = true;
        }
    }

    if should_finish_ssh_drain {
        finish_ssh_drain(&mut ssh_task, &mut fatal_error).await;
    }
    flush_dartboard_snapshot(&state, &mut fatal_error).await;
    flush_lateania_characters(&state, &mut fatal_error).await;
    flush_online_time(&state, &mut fatal_error).await;
    session_shutdown.cancel();

    if tokio::time::timeout(Duration::from_secs(6), async {
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Err(err)) => {
                    tracing::error!(error = ?err, "task failed during shutdown");
                    if fatal_error.is_none() {
                        fatal_error = Some(err);
                    }
                }
                Ok(Ok(())) => tracing::info!("task exited cleanly during shutdown"),
                Err(err) => {
                    tracing::error!(error = ?err, "task panicked during shutdown");
                    if fatal_error.is_none() {
                        fatal_error = Some(anyhow::Error::new(err).context("task panicked"));
                    }
                }
            }
        }
    })
    .await
    .is_err()
    {
        tracing::warn!("shutdown timed out, aborting remaining tasks");
        tasks.abort_all();
    }

    if let Some(err) = fatal_error {
        Err(err)
    } else {
        Ok(())
    }
}
