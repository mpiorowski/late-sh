//! Standalone webview helper binary.
//!
//! Spawned by `late` on Linux so the main CLI binary never links
//! WebKitGTK/GTK. If this binary is missing or its libraries fail to load,
//! the parent's crash backoff disables embedded YouTube playback while the
//! CLI session keeps running.
//!
//! Modes:
//! - `late-webview` — pair relay: reads the session token from stdin and
//!   `LATE_API_BASE_URL` from the environment (default `https://api.late.sh`).
//! - `late-webview spike <video_id>` — debugging: autoload one video, no WS.

use anyhow::{Context, Result};
use std::io::BufRead;
use tracing::error;

const DEFAULT_API_BASE_URL: &str = "https://api.late.sh";

fn main() -> Result<()> {
    // The dependency graph can contain both Rustls providers; select one
    // explicitly before any WebSocket/TLS setup, same as the parent CLI.
    let _ = rustls::crypto::ring::default_provider().install_default();
    init_logging();

    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("spike") => {
            let video_id = args
                .get(1)
                .context("usage: late-webview spike <video_id>")?;
            late_webview::run_spike(video_id)
        }
        Some(other) => anyhow::bail!("unknown late-webview mode: {other}"),
        None => run_pair(),
    }
}

/// Tracing goes to stderr: the parent redirects helper stderr to the webview
/// log file (or inherits it under `LATE_WEBVIEW_DEBUG_STDERR=1`).
fn init_logging() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,late_webview=debug,late_cli=debug"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

fn run_pair() -> Result<()> {
    let token = read_pair_token_from_stdin()?;
    let api_base_url =
        std::env::var("LATE_API_BASE_URL").unwrap_or_else(|_| DEFAULT_API_BASE_URL.to_string());
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

fn read_pair_token_from_stdin() -> Result<String> {
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
