use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use late_core::{
    api_types::NowPlaying, db::Db, icecast, models::chat_room::ChatRoom, rate_limit::IpRateLimiter,
    shutdown::CancellationToken,
};
use late_ssh::{
    api,
    app::ai::{ghost::GhostService, svc::AiService},
    app::chat::news::svc::ArticleService,
    app::chat::notifications::svc::NotificationService,
    app::chat::showcase::svc::ShowcaseService,
    app::chat::svc::ChatService,
    app::profile::svc::ProfileService,
    app::vote::svc::VoteService,
    config::Config,
    session::SessionRegistry,
    ssh,
    state::{ActivityEvent, State},
};
use tokio::{
    sync::{Semaphore, broadcast, watch},
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
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(64);
    let (now_playing_tx, now_playing_rx) = watch::channel::<Option<NowPlaying>>(None);
    let vote_service = VoteService::new(
        db.clone(),
        config.liquidsoap_addr.clone(),
        Duration::from_secs(config.vote_switch_interval_secs),
        active_users.clone(),
        activity_tx.clone(),
    );
    let notification_service = NotificationService::new(db.clone());
    let chat_service = ChatService::new(db.clone(), notification_service.clone());
    let ai_service = AiService::new(
        config.ai.enabled,
        config.ai.api_key.clone(),
        config.ai.model.clone(),
    );
    let profile_service = ProfileService::new(db.clone(), active_users.clone());
    let article_service = ArticleService::new(db.clone(), ai_service.clone(), chat_service.clone());
    let showcase_service = ShowcaseService::new(db.clone());
    let twenty_forty_eight_service =
        late_ssh::app::games::twenty_forty_eight::svc::TwentyFortyEightService::new(db.clone());
    let tetris_service = late_ssh::app::games::tetris::svc::TetrisService::new(db.clone());
    let chip_service = late_ssh::app::games::chips::svc::ChipService::new(db.clone());
    let rooms_service = late_ssh::app::rooms::svc::RoomsService::new(db.clone());
    rooms_service.refresh_task();
    let blackjack_table_manager =
        late_ssh::app::rooms::blackjack::manager::BlackjackTableManager::new(chip_service.clone());
    let (blackjack_event_tx, _) =
        broadcast::channel::<late_ssh::app::rooms::blackjack::svc::BlackjackEvent>(64);
    let blackjack_service = late_ssh::app::rooms::blackjack::svc::BlackjackService::new(
        chip_service.clone(),
        blackjack_event_tx,
    );
    let sudoku_service = late_ssh::app::games::sudoku::svc::SudokuService::new(
        db.clone(),
        activity_tx.clone(),
        chip_service.clone(),
    );
    let nonogram_service = late_ssh::app::games::nonogram::svc::NonogramService::new(
        db.clone(),
        activity_tx.clone(),
        chip_service.clone(),
    );
    let solitaire_service = late_ssh::app::games::solitaire::svc::SolitaireService::new(
        db.clone(),
        activity_tx.clone(),
        chip_service.clone(),
    );
    let minesweeper_service = late_ssh::app::games::minesweeper::svc::MinesweeperService::new(
        db.clone(),
        activity_tx.clone(),
        chip_service.clone(),
    );
    let bonsai_service =
        late_ssh::app::bonsai::svc::BonsaiService::new(db.clone(), activity_tx.clone());
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
    let leaderboard_service =
        late_ssh::app::games::leaderboard::svc::LeaderboardService::new(db.clone());
    let nonogram_library = match late_ssh::app::games::nonogram::state::load_default_library() {
        Ok(library) => library,
        Err(err) => {
            tracing::warn!(error = ?err, "failed to load nonogram asset packs; continuing with empty library");
            late_ssh::app::games::nonogram::state::Library::default()
        }
    };
    let ghost_service = GhostService::new(
        db.clone(),
        chat_service.clone(),
        ai_service.clone(),
        active_users.clone(),
        activity_tx.clone(),
    );
    let session_registry = SessionRegistry::new();
    let paired_client_registry = late_ssh::session::PairedClientRegistry::new();
    let web_chat_registry = late_ssh::web::WebChatRegistry::new();
    let icecast_url = config.icecast_url.clone();
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
        vote_service: vote_service.clone(),
        chat_service: chat_service.clone(),
        notification_service: notification_service.clone(),
        article_service,
        showcase_service,
        profile_service,
        twenty_forty_eight_service,
        tetris_service,
        sudoku_service,
        nonogram_service,
        solitaire_service,
        minesweeper_service,
        bonsai_service,
        nonogram_library,
        chip_service,
        rooms_service,
        blackjack_table_manager,
        blackjack_service,
        dartboard_server,
        dartboard_provenance,
        leaderboard_service: leaderboard_service.clone(),
        conn_limit,
        conn_counts,
        active_users,
        activity_feed: activity_tx,
        now_playing_rx: now_playing_rx.clone(),
        session_registry,
        paired_client_registry,
        web_chat_registry,
        ssh_attempt_limiter,
        ws_pair_limiter,
        is_draining: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let session_shutdown = CancellationToken::new();
    let accept_shutdown = CancellationToken::new();
    let singleton_shutdown = CancellationToken::new();

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

    let now_playing_shutdown = session_shutdown.clone();
    tasks.spawn_blocking(move || {
        let mut last_title: Option<String> = None;
        loop {
            if now_playing_shutdown.is_cancelled() {
                tracing::info!("now playing fetcher shutting down");
                break;
            }
            let result = icecast::fetch_track(&icecast_url);
            match result {
                Ok(track) => {
                    tracing::debug!(track = %track, "fetched now playing");
                    // Only update if track changed (to reset started_at correctly)
                    let current_title = track.to_string();
                    if last_title.as_ref() != Some(&current_title) {
                        tracing::info!(track = %track, "now playing changed");
                        last_title = Some(current_title);
                        let now_playing = NowPlaying::new(track);
                        if let Err(err) = now_playing_tx.send(Some(now_playing)) {
                            tracing::error!(error = ?err, "failed to publish now playing update");
                            break;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!(error = ?e, "failed to fetch now playing, retrying in 5s");
                }
            }

            for _ in 0..10 {
                if now_playing_shutdown.is_cancelled() {
                    tracing::info!("now playing fetcher shutting down");
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
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

    // Server-side audio analyzer (disabled - using browser-side viz only)
    // To re-enable: add viz_tx to State, subscribe in ssh.rs, uncomment below
    // let (viz_tx, _) = tokio::sync::broadcast::channel::<VizFrame>(32);
    // let analyzer_tx = viz_tx.clone();
    // tokio::task::spawn_blocking(move || {
    //     let decoder = match SymphoniaStreamDecoder::new_http(&icecast_url) {
    //         Ok(d) => d,
    //         Err(e) => {
    //             tracing::error!(error = ?e, "failed to create decoder");
    //             return;
    //         }
    //     };
    //     let sample_rate = decoder.sample_rate as f32;
    //     if let Err(e) = run_analyzer(AnalyzerConfig::default(), analyzer_tx, decoder, sample_rate) {
    //         tracing::error!(error = ?e, "audio analyzer failed");
    //     }
    // });

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
