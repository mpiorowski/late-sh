use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use anyhow::Context;
use late_core::{
    MutexRecover, db::Db, models::chat_room::ChatRoom, rate_limit::IpRateLimiter,
    shutdown::CancellationToken,
};
use late_ssh::{
    api,
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
    app::vote::svc::VoteService,
    app::{
        activity::channel::ACTIVITY_HISTORY_MAX_EVENTS,
        ai::{ghost::GhostService, svc::AiService},
    },
    config::Config,
    moderation::service::ModerationInfra,
    session::SessionRegistry,
    ssh,
    state::State,
};
use tokio::{
    sync::{Semaphore, broadcast},
    task::JoinSet,
};

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

async fn flush_pinstar_diagrams(state: &State, fatal_error: &mut Option<anyhow::Error>) {
    match state.pinstar_registry.flush_all().await {
        Ok(()) => tracing::info!("flushed pinstar diagrams during shutdown"),
        Err(err) => {
            tracing::error!(error = ?err, "failed to flush pinstar diagrams during shutdown");
            if fatal_error.is_none() {
                *fatal_error =
                    Some(err.context("failed to flush pinstar diagrams during shutdown"));
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = late_core::telemetry::init_telemetry("late-ssh")
        .context("failed to initialize telemetry")?;

    // Load configuration from environment
    let config = Config::from_env().context("failed to load configuration")?;
    config.log_startup();

    // Init database connection pool
    let db = Db::new(&config.db).context("failed to initialize database")?;
    db.health().await.context("database health check failed")?;
    db.migrate().await.context("database migration failed")?;
    {
        let client = db.get().await.context("failed to get db client")?;
        let general = ChatRoom::ensure_general(&client)
            .await
            .context("failed to ensure general chat room")?;
        tracing::info!(room_id = %general.id, "ensured general chat room");
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
    let activity_history = Arc::new(Mutex::new(VecDeque::new()));
    let (activity_tx, mut activity_history_rx) = late_ssh::app::activity::channel::new(512);
    let room_join_history = Arc::new(Mutex::new(VecDeque::new()));
    let (room_join_tx, mut room_join_history_rx) = tokio::sync::broadcast::channel(512);
    let activity_publisher =
        late_ssh::app::activity::publisher::ActivityPublisher::new(db.clone(), activity_tx.clone())
            .with_username_directory(username_directory.clone());
    let now_playing_service = NowPlayingService::new(config.icecast_url.clone());
    let now_playing_rx = now_playing_service.subscribe_state();
    let paired_client_registry = late_ssh::paired_clients::PairedClientRegistry::new();
    let audio_service = AudioService::new(
        db.clone(),
        config.youtube_api_key.clone(),
        paired_client_registry.clone(),
        active_users.clone(),
    );
    let voice_service = VoiceService::new(config.voice.clone());
    let session_registry = SessionRegistry::new();
    let vote_service = VoteService::new(
        db.clone(),
        config.liquidsoap_addr.clone(),
        Duration::from_secs(config.vote_switch_interval_secs),
        active_users.clone(),
        activity_tx.clone(),
    );
    let notification_service = NotificationService::new(db.clone());
    let chat_service = ChatService::new_with_active_users(
        db.clone(),
        notification_service.clone(),
        active_users.clone(),
    )
    .with_username_directory(username_directory.clone())
    .with_session_registry(session_registry.clone())
    .with_force_admin(config.force_admin);
    let ai_service = AiService::new(
        config.ai.enabled,
        config.ai.api_key.clone(),
        config.ai.model.clone(),
    );
    let profile_service = ProfileService::new(db.clone(), active_users.clone())
        .with_username_directory(username_directory.clone())
        .with_session_registry(session_registry.clone());
    let article_service = ArticleService::new(db.clone(), ai_service.clone(), chat_service.clone());
    let feed_service = FeedService::new(db.clone());
    feed_service.start_poll_task();
    let showcase_service = ShowcaseService::new(db.clone());
    let work_service = WorkService::new(db.clone());
    let twenty_forty_eight_service =
        late_ssh::app::arcade::twenty_forty_eight::svc::TwentyFortyEightService::new(db.clone())
            .with_activity_feed(activity_tx.clone());
    let tetris_service = late_ssh::app::arcade::tetris::svc::TetrisService::new(db.clone())
        .with_activity_feed(activity_tx.clone());
    let snake_service = late_ssh::app::arcade::snake::svc::SnakeService::new(db.clone())
        .with_activity_feed(activity_tx.clone());
    let chip_service = late_ssh::app::games::chips::svc::ChipService::new(db.clone());
    let _chip_activity_reward_task = chip_service.start_activity_reward_task(activity_tx.clone());
    let rooms_service = late_ssh::app::rooms::svc::RoomsService::new(db.clone());
    rooms_service.refresh_task();
    rooms_service.cleanup_inactive_tables_task();
    let asterion_room_manager = late_ssh::app::rooms::asterion::manager::AsterionRoomManager::new(
        chip_service.clone(),
        activity_publisher.clone(),
        rooms_service.clone(),
        db.clone(),
    );
    let blackjack_table_manager =
        late_ssh::app::rooms::blackjack::manager::BlackjackTableManager::new(
            chip_service.clone(),
            late_ssh::app::rooms::blackjack::player::BlackjackPlayerDirectory::new(db.clone()),
            activity_publisher.clone(),
        );
    let tictactoe_table_manager =
        late_ssh::app::rooms::tictactoe::manager::TicTacToeTableManager::new(
            activity_publisher.clone(),
        );
    let chess_table_manager = late_ssh::app::rooms::chess::manager::ChessTableManager::new(
        chip_service.clone(),
        activity_publisher.clone(),
        rooms_service.clone(),
    );
    let poker_table_manager = late_ssh::app::rooms::poker::manager::PokerTableManager::new(
        chip_service.clone(),
        activity_publisher.clone(),
    );
    let tron_table_manager = late_ssh::app::rooms::tron::manager::TronTableManager::new(
        chip_service.clone(),
        activity_publisher.clone(),
    );
    let lateania_service = late_ssh::app::door::lateania::svc::LateaniaService::new(
        activity_publisher.clone(),
        db.clone(),
    );
    let sshattrick_room_manager =
        late_ssh::app::rooms::sshattrick::manager::SshattrickRoomManager::new(
            rooms_service.clone(),
            chip_service.clone(),
            activity_publisher.clone(),
            db.clone(),
        );
    let room_game_registry = late_ssh::app::rooms::registry::RoomGameRegistry::new(
        asterion_room_manager,
        blackjack_table_manager.clone(),
        chess_table_manager,
        poker_table_manager,
        sshattrick_room_manager,
        tictactoe_table_manager,
        tron_table_manager,
    );
    room_game_registry.start_dashboard_room_join_feed_task(room_join_tx.clone());
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
    let chat_service = chat_service.with_moderation_infra(
        ModerationInfra::default()
            .with_force_admin(config.force_admin)
            .with_artboard_handles(dartboard_server.clone(), dartboard_provenance.clone()),
    );
    let leaderboard_service = late_ssh::app::LeaderboardService::new(db.clone());
    let quest_service = late_ssh::app::QuestService::new(db.clone(), activity_tx.clone());
    let _quest_activity_task = quest_service.start_activity_task();
    let _quest_listener_task = quest_service.start_listener_task(config.db.clone());
    let shop_service = late_ssh::app::ShopService::new(db.clone());
    let _shop_listener_task = shop_service.start_listener_task(config.db.clone());
    let ultimate_service = late_ssh::app::UltimateService::new(db.clone());
    let nonogram_library = match late_ssh::app::arcade::nonogram::state::load_default_library() {
        Ok(library) => library,
        Err(err) => {
            tracing::warn!(error = ?err, "failed to load nonogram asset packs; continuing with empty library");
            late_ssh::app::arcade::nonogram::state::Library::default()
        }
    };
    let ghost_service = GhostService::new(
        db.clone(),
        chat_service.clone(),
        ai_service.clone(),
        blackjack_table_manager.clone(),
        active_users.clone(),
        activity_tx.clone(),
    );
    let ssh_attempt_limiter = IpRateLimiter::new(
        config.ssh_max_attempts_per_ip,
        config.ssh_rate_limit_window_secs,
    );
    let ws_pair_limiter = IpRateLimiter::new(
        config.ws_pair_max_attempts_per_ip,
        config.ws_pair_rate_limit_window_secs,
    );
    let voice_listen_limiter = IpRateLimiter::new(
        config.ws_pair_max_attempts_per_ip,
        config.ws_pair_rate_limit_window_secs,
    );
    let pinstar_registry =
        late_ssh::app::pinstar::svc::PinstarServerRegistry::new(Some(db.clone()));

    // Initialize app state
    let state = State {
        config: config.clone(),
        db: db.clone(),
        ai_service: ai_service.clone(),
        audio_service: audio_service.clone(),
        voice_service,
        vote_service: vote_service.clone(),
        chat_service: chat_service.clone(),
        notification_service: notification_service.clone(),
        article_service,
        feed_service,
        showcase_service,
        work_service,
        profile_service,
        twenty_forty_eight_service,
        tetris_service,
        snake_service,
        sudoku_service,
        nonogram_service,
        solitaire_service,
        minesweeper_service,
        lateania_service,
        bonsai_service,
        pet_service,
        nonogram_library,
        chip_service,
        rooms_service,
        blackjack_table_manager,
        room_game_registry,
        dartboard_server,
        dartboard_provenance,
        leaderboard_service: leaderboard_service.clone(),
        quest_service,
        shop_service,
        ultimate_service,
        conn_limit,
        conn_counts,
        active_users,
        afk_users,
        username_directory: username_directory.clone(),
        activity_feed: activity_tx,
        activity_history: activity_history.clone(),
        room_join_feed: room_join_tx,
        room_join_history: room_join_history.clone(),
        now_playing_rx: now_playing_rx.clone(),
        session_registry,
        paired_client_registry,
        ssh_attempt_limiter,
        ws_pair_limiter,
        voice_listen_limiter,
        pinstar_registry,
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

    let mut tasks = JoinSet::new();
    let activity_history_shutdown = singleton_shutdown.clone();
    tasks.spawn(async move {
        loop {
            tokio::select! {
                _ = activity_history_shutdown.cancelled() => break,
                result = activity_history_rx.recv() => {
                    match result {
                        Ok(event) => {
                            let mut history = activity_history.lock_recover();
                            history.push_back(event);
                            while history.len() > ACTIVITY_HISTORY_MAX_EVENTS {
                                history.pop_front();
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "activity history receiver lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        Ok(())
    });
    let room_join_history_shutdown = singleton_shutdown.clone();
    tasks.spawn(async move {
        loop {
            tokio::select! {
                _ = room_join_history_shutdown.cancelled() => break,
                result = room_join_history_rx.recv() => {
                    match result {
                        Ok(join) => {
                            let mut history = room_join_history.lock_recover();
                            late_ssh::app::dashboard::state::push_recent_room_join(
                                &mut history,
                                join,
                            );
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "room join history receiver lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        Ok(())
    });
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

    let now_playing_shutdown = session_shutdown.clone();
    let now_playing_task = now_playing_service.start_poll_task(now_playing_shutdown);
    tasks.spawn(async move {
        now_playing_task
            .await
            .context("now playing task panicked")?;
        Ok(())
    });

    // Audio rides session_shutdown (fires after ssh drain) rather than
    // singleton_shutdown (fires at drain begin) so paired browsers keep
    // hearing music through the entire drain window. Liquidsoap/Icecast
    // streams from a separate process and is unaffected either way.
    let audio_shutdown = session_shutdown.clone();
    tasks.spawn(async move {
        audio_service.start_background_task(audio_shutdown).await;
        Ok(())
    });

    let pinstar_persist_shutdown = session_shutdown.clone();
    let pinstar_persist_registry = state.pinstar_registry.clone();
    tasks.spawn(async move {
        pinstar_persist_registry
            .run_persist_task(pinstar_persist_shutdown)
            .await;
        Ok(())
    });

    let limiter_cleanup_shutdown = singleton_shutdown.clone();
    let ssh_limiter = state.ssh_attempt_limiter.clone();
    let ws_limiter = state.ws_pair_limiter.clone();
    let voice_listen_limiter = state.voice_listen_limiter.clone();
    tasks.spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.tick().await; // skip immediate first tick
        loop {
            tokio::select! {
                _ = limiter_cleanup_shutdown.cancelled() => break,
                _ = interval.tick() => {
                    ssh_limiter.cleanup();
                    ws_limiter.cleanup();
                    voice_listen_limiter.cleanup();
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

    let vote_shutdown = singleton_shutdown.clone();
    tasks.spawn(async move {
        vote_service.start_background_task(vote_shutdown).await;
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
    flush_pinstar_diagrams(&state, &mut fatal_error).await;
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
