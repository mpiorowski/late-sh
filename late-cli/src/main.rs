use anyhow::{Context, Result};
use std::{
    env,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

mod audio;
mod clipboard;

mod config;
mod identity;
mod latency;
mod mpris;
mod pty;
mod raw_mode;
mod ssh;
mod update;
mod video;
mod voice;
mod ws;

use audio::{AudioRuntime, audio_startup_hint};
use config::{Config, init_logging};
use identity::ensure_client_identity_at;
use raw_mode::{RawModeGuard, SessionModesGuard, enable_ansi_output_if_tty};
use ssh::{SshProcess, flush_stdin_input_queue, forward_resize_events, spawn_subprocess_ssh};
use ws::{
    MAX_CONSECUTIVE_FAILURES, PAIR_RECONNECT_DELAY, PAIR_SLOW_RECONNECT_DELAY, PairAttempt,
    PairClientInfo, PairRetryPolicy, PairRuntime, PlaybackState, ReconnectPlan,
    WebviewPlaybackController, client_platform_label, end_video_without_socket,
    establish_pair_session, run_pair_session,
};

#[tokio::main]
async fn main() -> Result<()> {
    install_rustls_crypto_provider();

    let raw_args: Vec<String> = env::args().skip(1).collect();
    match raw_args.first().map(String::as_str) {
        Some("webview-spike") => return run_webview_spike_subcommand(&raw_args[1..]),
        Some("webview-pair") => return run_webview_pair_subcommand(&raw_args[1..]),
        _ => {}
    }

    let config = Config::from_args(raw_args)?;
    if let Some(path) = init_logging(config.verbose)? {
        eprintln!(
            "late log: {} (set LATE_LOG_STDERR=1 to stream logs to the terminal)",
            path.display()
        );
    }
    debug!(?config, "resolved cli config");
    // Nudge outdated installs before we take over the terminal. Runs only on
    // stamped release builds, is fail-open, and pauses briefly when behind.
    update::check_for_update().await;
    // OpenSSH mode can use normal OpenSSH identity discovery, including
    // ~/.ssh/config and agent-loaded hardware-backed keys. Skip late's key
    // helper in that mode unless the caller explicitly asks for a key.
    let ssh_identity = if config.ssh_mode == config::SshMode::OpenSsh && config.key_file.is_none() {
        None
    } else {
        Some(ensure_client_identity_at(config.key_file.as_deref())?)
    };
    // In OpenSSH mode the system ssh client owns the terminal, so PIN,
    // passphrase, and touch prompts keep OpenSSH's normal echo behavior.
    if config.ssh_mode.uses_cli_raw_mode() {
        enable_ansi_output_if_tty();
    }
    let _raw_mode = config
        .ssh_mode
        .uses_cli_raw_mode()
        .then(RawModeGuard::enable_if_tty);
    // Declared after the raw-mode guard so it drops first: every way out of
    // the session below, clean or not, leaves the shell without mouse
    // reporting or bracketed paste.
    let _session_modes = SessionModesGuard::enable_if_tty();

    if config.ssh_mode == config::SshMode::OpenSsh {
        return run_openssh_mode(config, ssh_identity).await;
    }

    let ssh_identity = ssh_identity.context("embedded SSH modes require a resolved identity")?;

    info!("starting audio runtime");
    let audio = AudioRuntime::start(
        config.audio_base_url.clone(),
        config.audio_output_device.clone(),
    )
    .await
    .map_err(|err| {
        let hint = audio_startup_hint();
        anyhow::anyhow!("failed to start local audio: {err:#}\n\n{hint}")
    })?;
    if audio.enabled {
        info!(sample_rate = audio.sample_rate, "audio runtime ready");
    } else {
        info!("local audio disabled on this platform");
    }
    info!("starting ssh session");
    let ssh_exit = match config.ssh_mode {
        config::SshMode::Native => run_native_session(&config, &ssh_identity, &audio).await?,
        config::SshMode::Subprocess => {
            run_subprocess_session(&config, &ssh_identity, &audio).await?
        }
        config::SshMode::OpenSsh => unreachable!("openssh mode runs through run_openssh_mode"),
    };

    audio.stop.store(true, Ordering::Relaxed);
    debug!(?ssh_exit, "ssh session ended");
    ssh_exit.ensure_success()?;

    Ok(())
}

/// Native mode: one SSH session after another on the same terminal. A drain
/// handoff (`reconnect` from a server that is going away) dials the new
/// server and opens the new session there while the old one is still on
/// screen, then swaps them. The local audio runs underneath the whole time,
/// so the radio never stops. A move that fails at any step before the swap
/// leaves the session where it is, on the server's update banner.
async fn run_native_session(
    config: &Config,
    ssh_identity: &std::path::Path,
    audio: &AudioRuntime,
) -> Result<ssh::SshExit> {
    let (latency_sink, mut latency_samples) = latency::latency_queue();
    let link = ssh::start_native_link(latency_sink);
    let resize_task = tokio::spawn(forward_resize_events(link.resize_handle()));

    let connection = ssh::connect_native(config, ssh_identity).await?;
    let mut token = connection.token().to_string();
    let mut shell = connection.open_shell(&link, ssh::ShellStart::Login).await?;
    flush_stdin_input_queue();
    link.input.attach(&shell);

    let mut policy = PairRetryPolicy::new();
    let exit = loop {
        let (handoff_tx, mut handoff_rx) = tokio::sync::mpsc::channel(1);
        // The pairing is held across a failed move: the old pod is out of the
        // Service endpoints, so a fresh pair socket on this token would only
        // reach the new pod and its 404 (SCALE.md, Pain Point 10).
        let mut pairing = Box::pin(run_ws_pairing(
            config,
            token.clone(),
            audio,
            &mut latency_samples,
            policy,
            &handoff_tx,
        ));
        let moved = loop {
            let resume = tokio::select! {
                result = &mut shell.completion_task => break NativeTurn::Ended(join_ssh_exit(result)),
                () = &mut pairing => unreachable!("the pair loop retries forever"),
                Some(resume) = handoff_rx.recv() => resume,
            };
            info!("drain handoff: dialing the new server");
            let dial = tokio::select! {
                result = &mut shell.completion_task => break NativeTurn::Ended(join_ssh_exit(result)),
                () = &mut pairing => unreachable!("the pair loop retries forever"),
                dial = open_handoff_session(config, ssh_identity, &link, &resume) => dial,
            };
            match dial {
                Ok((token, shell)) => break NativeTurn::Moved { token, shell },
                Err(err) => {
                    warn!(error = ?err, "drain handoff could not open a session on the new server; staying on this one");
                }
            }
        };
        drop(pairing);

        let (next_token, next_shell) = match moved {
            NativeTurn::Ended(exit) => break exit,
            NativeTurn::Moved { token, shell } => (token, shell),
        };
        link.input.detach();
        let old_shell = std::mem::replace(&mut shell, next_shell);
        old_shell.retire().await;
        shell.show();
        token = next_token;
        flush_stdin_input_queue();
        link.input.attach(&shell);
        link.input.resize_to_terminal();
        // This process is running the audio the old server aligned; the new
        // pairing must never treat the boot mute as still pending.
        policy = PairRetryPolicy::after_handoff();
        info!("drain handoff: session moved to the new server");
    };

    resize_task.abort();
    exit
}

/// How one native session's turn ends: the session itself ended, or a drain
/// handoff holds a session on the new server, its output not yet shown.
enum NativeTurn {
    Ended(Result<ssh::SshExit>),
    Moved {
        token: String,
        shell: ssh::NativeShell,
    },
}

/// A drain handoff's half of a move, run while the old session stays on
/// screen and paired: dial and authenticate the new server, then open the
/// session there with its output held. A failure at any step leaves the old
/// session as it was.
async fn open_handoff_session(
    config: &Config,
    ssh_identity: &std::path::Path,
    link: &ssh::NativeLink,
    resume: &str,
) -> Result<(String, ssh::NativeShell)> {
    let connection = ssh::connect_native(config, ssh_identity).await?;
    let token = connection.token().to_string();
    let shell = connection
        .open_shell(link, ssh::ShellStart::Handoff { resume })
        .await
        .context("failed to open the session on the new server")?;
    Ok((token, shell))
}

/// The legacy subprocess (`old`) mode: one session, no handoff.
async fn run_subprocess_session(
    config: &Config,
    ssh_identity: &std::path::Path,
    audio: &AudioRuntime,
) -> Result<ssh::SshExit> {
    let (token_tx, token_rx) = oneshot::channel();
    let SshProcess {
        completion_task,
        resize_handle,
        input_gate,
    } = spawn_subprocess_ssh(config, ssh_identity, token_tx)?;
    let resize_task = tokio::spawn(forward_resize_events(resize_handle));

    let token = tokio::time::timeout(Duration::from_secs(10), token_rx)
        .await
        .context(
            "timed out waiting for SSH session token (is the server reachable? \
             try: ssh late.sh)",
        )?
        .context("ssh session token channel closed")?;
    flush_stdin_input_queue();
    input_gate.store(true, Ordering::Relaxed);
    let ssh_exit = wait_for_ssh_with_ws_pairing(completion_task, config, token, audio).await;

    resize_task.abort();
    ssh_exit
}

fn join_ssh_exit(
    result: Result<Result<ssh::SshExit>, tokio::task::JoinError>,
) -> Result<ssh::SshExit> {
    match result {
        Ok(result) => result,
        Err(err) => Err(anyhow::anyhow!("ssh session task join failed: {err}")),
    }
}

fn install_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

// Windows/macOS run the webview in-process: the system webview
// (WebView2/WKWebView) carries no load-time dependency risk, so `late`
// stays a single binary there.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn run_webview_spike_subcommand(args: &[String]) -> Result<()> {
    let video_id = args
        .first()
        .context("usage: late webview-spike <video_id>")?;
    let _ = init_logging(true)?;
    late_webview::run_spike(video_id)
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn run_webview_pair_subcommand(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        anyhow::bail!("usage: late webview-pair (token is read from stdin)");
    }
    let token = read_webview_pair_token_from_stdin()?;
    let api_base_url =
        env::var("LATE_API_BASE_URL").unwrap_or_else(|_| config::DEFAULT_API_BASE_URL.to_string());
    let _ = init_logging(true)?;
    late_webview::run_relay(None, move |proxy, ipc_rx| {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                error!(error = %err, "failed to build webview pair runtime");
                let _ = proxy.send_event(late_webview::WebviewCommand::Shutdown);
                return;
            }
        };
        rt.block_on(async move {
            if let Err(err) = late_webview::pair::run(&api_base_url, &token, proxy, ipc_rx).await {
                error!(error = %err, "webview pair task ended with error");
            }
        });
    })
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn read_webview_pair_token_from_stdin() -> Result<String> {
    use std::io::BufRead;

    let mut token = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut token)
        .context("failed to read webview pair token from stdin")?;
    let token = token.trim_end_matches(['\r', '\n']).to_string();
    if token.is_empty() {
        anyhow::bail!("webview pair token was empty");
    }
    if token.chars().any(char::is_whitespace) {
        anyhow::bail!("webview pair token was invalid");
    }
    Ok(token)
}

// Everywhere else `late` never links the webview stack; these subcommands
// forward to the standalone `late-webview` helper binary (token/stdin and
// LATE_API_BASE_URL pass through inherited stdio/env).
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn run_webview_spike_subcommand(args: &[String]) -> Result<()> {
    let video_id = args
        .first()
        .context("usage: late webview-spike <video_id>")?;
    forward_to_webview_helper(&["spike", video_id])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn run_webview_pair_subcommand(args: &[String]) -> Result<()> {
    if !args.is_empty() {
        anyhow::bail!("usage: late webview-pair (token is read from stdin)");
    }
    forward_to_webview_helper(&[])
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn forward_to_webview_helper(args: &[&str]) -> Result<()> {
    let program = ws::webview_helper_program();
    let status = std::process::Command::new(&program)
        .args(args)
        .status()
        .with_context(|| {
            format!(
                "failed to run the webview helper at {} — is late-webview installed \
                 next to late? (set LATE_WEBVIEW_BIN to override)",
                program.display()
            )
        })?;
    if !status.success() {
        anyhow::bail!("webview helper exited with {status}");
    }
    Ok(())
}

async fn run_openssh_mode(config: Config, ssh_identity: Option<std::path::PathBuf>) -> Result<()> {
    // Authenticate first, while OpenSSH still has direct access to the
    // terminal. Audio and WebSocket pairing start only after the token exec
    // succeeds, so PIN/passphrase/touch prompts are not interleaved with them.
    info!("starting OpenSSH control master");
    let session = ssh::prepare_openssh_ssh(&config, ssh_identity.as_deref()).await?;
    let token = session.token().to_string();

    info!("starting audio runtime");
    let audio = AudioRuntime::start(
        config.audio_base_url.clone(),
        config.audio_output_device.clone(),
    )
    .await
    .map_err(|err| {
        let hint = audio_startup_hint();
        anyhow::anyhow!("failed to start local audio: {err:#}\n\n{hint}")
    })?;
    if audio.enabled {
        info!(sample_rate = audio.sample_rate, "audio runtime ready");
    } else {
        info!("local audio disabled on this platform");
    }
    info!("starting OpenSSH interactive session");
    let ssh::OpenSshProcess { completion_task } = session.spawn_shell(&config)?;
    let ssh_exit = wait_for_ssh_with_ws_pairing(completion_task, &config, token, &audio).await?;

    audio.stop.store(true, Ordering::Relaxed);
    debug!(?ssh_exit, "ssh session ended");
    ssh_exit.ensure_success()?;

    Ok(())
}

/// One session with its pairing, for the modes that cannot redial (OpenSSH,
/// subprocess). They never advertise `reconnect`, so no handoff arrives, and
/// they never sample latency (OpenSSH's ssh child owns the terminal, old mode
/// is not wired): the held sink keeps the pair loop's queue open and quiet
/// (`latency.rs`).
async fn wait_for_ssh_with_ws_pairing(
    completion_task: tokio::task::JoinHandle<Result<ssh::SshExit>>,
    config: &Config,
    token: String,
    audio: &AudioRuntime,
) -> Result<ssh::SshExit> {
    let (handoff_tx, _handoff_rx) = tokio::sync::mpsc::channel(1);
    let (_latency_sink, mut latency_samples) = latency::latency_queue();
    tokio::select! {
        result = completion_task => join_ssh_exit(result),
        () = run_ws_pairing(
            config,
            token,
            audio,
            &mut latency_samples,
            PairRetryPolicy::new(),
            &handoff_tx,
        ) => unreachable!("the pair loop retries forever"),
    }
}

/// Pair with the session on `token` and keep it paired, forever. A drain
/// handoff's resume point goes out on `handoff`.
async fn run_ws_pairing(
    config: &Config,
    token: String,
    audio: &AudioRuntime,
    latency_samples: &mut tokio::sync::mpsc::Receiver<latency::LatencySample>,
    mut policy: PairRetryPolicy,
    handoff: &tokio::sync::mpsc::Sender<String>,
) {
    info!("received session token and starting websocket pairing");
    let api_base_url = config.api_base_url.clone();
    let video_probe = video::probe(config.ssh_mode, client_platform_label());
    match video_probe.blockers.as_slice() {
        [] => info!("video watch available"),
        blockers => info!(?blockers, "video watch unavailable"),
    }
    let client = PairClientInfo {
        ssh_mode: config.ssh_mode.client_state_label(),
        platform: client_platform_label(),
        video_blockers: video_probe.blockers,
        reconnect: config.ssh_mode == config::SshMode::Native,
    };
    let mut video = video::VideoPlayer::new(video_probe.tools);
    let played_samples = Arc::clone(&audio.played_samples);
    let muted = Arc::clone(&audio.muted);
    let volume_percent = Arc::clone(&audio.volume_percent);
    let source_is_icecast = Arc::clone(&audio.source_is_icecast);
    let native_source_selected = Arc::clone(&audio.native_source_selected);
    let stream_url = Arc::clone(&audio.stream_url);
    let stream_generation = Arc::clone(&audio.stream_generation);
    let stream_flushed_generation = Arc::clone(&audio.stream_flushed_generation);
    let icecast_stream_url = audio.icecast_stream_url.clone();
    let silenced_for_video = Arc::clone(&audio.silenced_for_video);
    // Copy scalar state before entering the long-lived pair loop.
    let sample_rate = audio.sample_rate;
    let mut webview = WebviewPlaybackController::new(
        api_base_url.clone(),
        token.clone(),
        audio.analyzer_tx.clone(),
    );
    let mut voice = voice::VoiceRuntimeState::default();
    let (mut desktop_media, mut desktop_commands) = match config.mpris {
        true => mpris::DesktopMedia::new(mpris::AudioControls {
            muted: Arc::clone(&muted),
            volume_percent: Arc::clone(&volume_percent),
        }),
        false => mpris::DesktopMedia::disabled(),
    };

    let playback = PlaybackState {
        played_samples: &played_samples,
        sample_rate,
        muted: &muted,
        volume_percent: &volume_percent,
        source_is_icecast: &source_is_icecast,
        native_source_selected: &native_source_selected,
        stream_url: &stream_url,
        stream_generation: &stream_generation,
        stream_flushed_generation: &stream_flushed_generation,
        icecast_stream_url: &icecast_stream_url,
        silenced_for_video: &silenced_for_video,
    };
    loop {
        let attempt = match establish_pair_session(&api_base_url, &token, &client, &playback).await
        {
            Err(err) => {
                error!(error = ?err, "pair websocket could not be established");
                PairAttempt::NotEstablished
            }
            Ok(ws) => {
                let established = Instant::now();
                // Subscribed per session so a reconnect starts from live
                // frames instead of flushing a backlog from the outage.
                let mut viz_frames = audio.analyzer_tx.subscribe();
                let session = run_pair_session(
                    ws,
                    &client,
                    &playback,
                    PairRuntime {
                        webview: &mut webview,
                        video: &mut video,
                        voice: &mut voice,
                        desktop_media: &mut desktop_media,
                        desktop_commands: &mut desktop_commands,
                        viz_frames: &mut viz_frames,
                        latency_samples: &mut *latency_samples,
                        handoff,
                    },
                )
                .await;
                end_video_without_socket(&mut video, &playback, &mut webview);
                match &session.result {
                    Err(err) => error!(error = ?err, "pair websocket session failed"),
                    Ok(()) => info!("pair websocket closed by the server"),
                }
                let attempt = session.attempt(established.elapsed());
                if attempt == PairAttempt::NotEstablished {
                    warn!(
                        "pair websocket accepted but dropped before the server registered this session"
                    );
                }
                attempt
            }
        };

        // One place to look for what a dead pair socket costs this session.
        let delay = match policy.note_attempt(attempt) {
            ReconnectPlan::Soon => PAIR_RECONNECT_DELAY,
            ReconnectPlan::Slow => {
                warn!(
                    "pair websocket unavailable after {MAX_CONSECUTIVE_FAILURES} consecutive failures; retrying slowly"
                );
                PAIR_SLOW_RECONNECT_DELAY
            }
            ReconnectPlan::ReleaseStartupMuteThenSlow => {
                // Pairing is the only way to learn the stored device mute, and
                // this session never reached the server, so it is still sitting
                // on the boot mute with no answer coming. Release it rather
                // than leave the user silently muted with no explanation. A
                // session that did pair keeps its mute; see PairRetryPolicy.
                muted.store(false, Ordering::Relaxed);
                warn!(
                    "pair websocket never established in {MAX_CONSECUTIVE_FAILURES} attempts; released startup audio mute and retrying slowly"
                );
                PAIR_SLOW_RECONNECT_DELAY
            }
        };
        tokio::time::sleep(delay).await;
    }
}
