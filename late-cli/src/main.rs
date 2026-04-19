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
use identity::ensure_client_identity;
use pty::forward_resize_events;
use raw_mode::RawModeGuard;
use ssh::{SshExit, SshProcess, flush_stdin_input_queue, spawn_ssh};
use ws::run_viz_ws;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_args(env::args().skip(1))?;
    init_logging(config.verbose)?;
    debug!(?config, "resolved cli config");
    let ssh_identity = ensure_client_identity()?;
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
        mut child,
        mut output_task,
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
    let played_samples = Arc::clone(&audio.played_samples);
    let sample_rate = audio.sample_rate;
    let muted = Arc::clone(&audio.muted);
    let volume_percent = Arc::clone(&audio.volume_percent);
    let mut frames = audio.analyzer_tx.subscribe();

    let ws_task = tokio::spawn(async move {
        let mut retries = 0;
        const MAX_RETRIES: usize = 10;
        loop {
            if let Err(err) = run_viz_ws(
                &api_base_url,
                &token,
                &mut frames,
                &played_samples,
                sample_rate,
                &muted,
                &volume_percent,
            )
            .await
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

    let mut stdout_result = None;
    let mut stdout_task_consumed = false;
    let status = match tokio::select! {
        status = child.wait() => {
            let status = status.context("ssh process failed to exit cleanly")?;
            SshExit::Process(status)
        }
        stdout = &mut output_task => {
            stdout_task_consumed = true;
            match stdout {
                Ok(Ok(())) => {
                    info!("ssh stdout closed; treating session as ended");
                    stdout_result = Some(Ok(Ok(())));
                }
                Ok(Err(err)) => return Err(err.context("ssh stdout forwarding failed")),
                Err(err) => return Err(anyhow::anyhow!("ssh stdout task join failed: {err}")),
            }
            SshExit::StdoutClosed
        }
    } {
        SshExit::Process(status) => {
            info!(%status, "ssh session exited");
            Some(status)
        }
        SshExit::StdoutClosed => {
            if let Err(err) = child.start_kill() {
                debug!(error = ?err, "failed to kill lingering ssh wrapper after stdout closed");
            }
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
            None
        }
    };

    audio.stop.store(true, Ordering::Relaxed);
    resize_task.abort();
    input_task.abort();
    ws_task.abort();
    if !stdout_task_consumed && output_task.is_finished() {
        stdout_result = Some(output_task.await);
    } else if !stdout_task_consumed {
        output_task.abort();
        let _ = output_task.await;
    }

    if let Some(status) = status {
        let stdout_closed_cleanly = matches!(stdout_result, Some(Ok(Ok(()))));
        if !(status.success() || status.code() == Some(255) && stdout_closed_cleanly) {
            anyhow::bail!("ssh exited with status {status}");
        }
    }

    Ok(())
}
