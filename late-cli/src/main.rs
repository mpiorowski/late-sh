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
mod mpris;
mod pty;
mod raw_mode;
mod ssh;
mod update;
mod voice;
mod ws;

use audio::{AudioRuntime, audio_startup_hint};
use config::{Config, init_logging};
use identity::ensure_client_identity_at;
use raw_mode::{RawModeGuard, enable_ansi_output_if_tty};
use ssh::{SshProcess, flush_stdin_input_queue, forward_resize_events, spawn_ssh};
use ws::{
    MAX_CONSECUTIVE_FAILURES, PAIR_RECONNECT_DELAY, PAIR_SLOW_RECONNECT_DELAY, PairAttempt,
    PairClientInfo, PairRetryPolicy, PairRuntime, PlaybackState, ReconnectPlan,
    WebviewPlaybackController, client_platform_label, establish_pair_session, run_pair_session,
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
    let (token_tx, token_rx) = oneshot::channel();
    let SshProcess {
        completion_task,
        resize_handle,
        input_gate,
    } = spawn_ssh(&config, &ssh_identity, token_tx).await?;
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
    let ssh_exit = wait_for_ssh_with_ws_pairing(completion_task, &config, token, &audio).await?;

    audio.stop.store(true, Ordering::Relaxed);
    resize_task.abort();
    debug!(?ssh_exit, "ssh session ended");
    ssh_exit.ensure_success()?;

    Ok(())
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

async fn wait_for_ssh_with_ws_pairing(
    completion_task: tokio::task::JoinHandle<Result<ssh::SshExit>>,
    config: &Config,
    token: String,
    audio: &AudioRuntime,
) -> Result<ssh::SshExit> {
    tokio::select! {
        result = completion_task => {
            match result {
                Ok(result) => result,
                Err(err) => Err(anyhow::anyhow!("ssh session task join failed: {err}")),
            }
        }
        () = run_ws_pairing(config, token, audio) => {
            std::future::pending::<Result<ssh::SshExit>>().await
        }
    }
}

async fn run_ws_pairing(config: &Config, token: String, audio: &AudioRuntime) {
    info!("received session token and starting websocket pairing");
    let api_base_url = config.api_base_url.clone();
    let client = PairClientInfo {
        ssh_mode: config.ssh_mode.client_state_label(),
        platform: client_platform_label(),
    };
    let played_samples = Arc::clone(&audio.played_samples);
    let muted = Arc::clone(&audio.muted);
    let volume_percent = Arc::clone(&audio.volume_percent);
    let source_is_icecast = Arc::clone(&audio.source_is_icecast);
    let native_source_selected = Arc::clone(&audio.native_source_selected);
    let stream_url = Arc::clone(&audio.stream_url);
    let stream_generation = Arc::clone(&audio.stream_generation);
    let stream_flushed_generation = Arc::clone(&audio.stream_flushed_generation);
    let icecast_stream_url = audio.icecast_stream_url.clone();
    // Copy scalar state before entering the long-lived pair loop.
    let sample_rate = audio.sample_rate;
    let mut webview = WebviewPlaybackController::new(api_base_url.clone(), token.clone());
    let mut voice = voice::VoiceRuntimeState::default();
    let (mut desktop_media, mut desktop_commands) =
        mpris::DesktopMedia::new(mpris::AudioControls {
            muted: Arc::clone(&muted),
            volume_percent: Arc::clone(&volume_percent),
        });

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
    };
    let mut policy = PairRetryPolicy::new();
    loop {
        let attempt = match establish_pair_session(&api_base_url, &token, &client, &playback).await
        {
            Err(err) => {
                error!(error = ?err, "pair websocket could not be established");
                PairAttempt::NotEstablished
            }
            Ok(ws) => {
                let established = Instant::now();
                let session = run_pair_session(
                    ws,
                    &client,
                    &playback,
                    PairRuntime {
                        webview: &mut webview,
                        voice: &mut voice,
                        desktop_media: &mut desktop_media,
                        desktop_commands: &mut desktop_commands,
                    },
                )
                .await;
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
