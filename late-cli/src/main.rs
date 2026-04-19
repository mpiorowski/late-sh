use anyhow::{Context, Result};
use std::{
    env,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};
use tokio::sync::oneshot;
use tracing::{debug, error, info};

mod audio;

mod config;
mod identity;
mod pty;
mod raw_mode;
mod ssh;
mod ws;

use audio::{AudioRuntime, audio_startup_hint};
use config::{Config, init_logging};
use identity::ensure_client_identity_at;
use raw_mode::RawModeGuard;
use ssh::{SshProcess, flush_stdin_input_queue, forward_resize_events, spawn_ssh};
use ws::{PairClientInfo, PlaybackState, client_platform_label, run_viz_ws};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_args(env::args().skip(1))?;
    init_logging(config.verbose)?;
    debug!(?config, "resolved cli config");
    let ssh_identity = ensure_client_identity_at(config.key_file.as_deref())?;
    let _raw_mode = RawModeGuard::enable_if_tty();

    info!("starting audio runtime");
    let audio = AudioRuntime::start(config.audio_base_url.clone())
        .await
        .map_err(|err| {
            let hint = audio_startup_hint();
            anyhow::anyhow!("failed to start local audio: {err:#}\n\n{hint}")
        })?;
    info!(sample_rate = audio.sample_rate, "audio runtime ready");
    info!("starting ssh session");
    let (token_tx, token_rx) = oneshot::channel();
    let SshProcess {
        completion_task,
        input_task,
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
    info!("received session token and starting websocket pairing");

    let api_base_url = config.api_base_url.clone();
    let client = PairClientInfo {
        ssh_mode: config.ssh_mode.client_state_label(),
        platform: client_platform_label(),
    };
    let played_samples = Arc::clone(&audio.played_samples);
    let muted = Arc::clone(&audio.muted);
    let volume_percent = Arc::clone(&audio.volume_percent);
    let mut frames = audio.analyzer_tx.subscribe();

    let ws_task = tokio::spawn(async move {
        let playback = PlaybackState {
            played_samples: &played_samples,
            sample_rate: audio.sample_rate,
            muted: &muted,
            volume_percent: &volume_percent,
        };
        let mut retries = 0;
        const MAX_RETRIES: usize = 10;
        loop {
            if let Err(err) =
                run_viz_ws(&api_base_url, &token, &client, &mut frames, &playback).await
            {
                retries += 1;
                if retries > MAX_RETRIES {
                    error!(error = ?err, "visualizer websocket task failed {MAX_RETRIES} times consecutively; giving up");
                    break;
                }
                error!(error = ?err, attempt = retries, "visualizer websocket task failed; reconnecting in 2s...");
            } else {
                retries = 0;
                info!("visualizer websocket closed cleanly; reconnecting in 2s...");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    });

    let ssh_exit = match completion_task.await {
        Ok(result) => result?,
        Err(err) => return Err(anyhow::anyhow!("ssh session task join failed: {err}")),
    };

    audio.stop.store(true, Ordering::Relaxed);
    resize_task.abort();
    input_task.abort();
    ws_task.abort();
    debug!(?ssh_exit, "ssh session ended");
    ssh_exit.ensure_success()?;

    Ok(())
}
