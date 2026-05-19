//! Embedded webview for CLI-side YouTube playback.
//!
//! Feature-gated (`--features webview`); see late-ssh/src/app/audio/CONTEXT.md §17.
//!
//! The webview is owned by the CLI: it never opens its own WebSocket.
//! Rust pushes commands (LoadVideo, SourceChanged, Shutdown) into the JS
//! bridge via tao's user-event mechanism; JS posts player state back through
//! wry's IPC handler. See `commands.rs` for the payload shapes and
//! `pair.rs` for the WS-relay task used by `late webview-pair <token>`.

use anyhow::{Context, Result};
use serde_json::json;
use tao::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use tokio::sync::mpsc;
use tracing::{info, warn};
use wry::{WebView, WebViewBuilder};

pub mod commands;
pub mod pair;

pub use commands::{WebviewCommand, WebviewEvent};

const PAGE_HTML: &str = include_str!("page.html");

/// Legacy spike entry point. Opens the webview and autoloads a single
/// hard-coded `video_id`. No WS connection.
pub fn run_spike(video_id: &str) -> Result<()> {
    validate_video_id(video_id)?;
    let video_id = video_id.to_string();
    run_relay(Some(video_id), |_proxy, _ipc_rx| {
        // No bridge work — the autoload script in the HTML handles startup.
    })
}

/// Open the webview and run the tao event loop on the calling thread (which
/// must be the OS main thread on macOS).
///
/// `on_setup` is invoked on a dedicated OS thread before the event loop
/// starts. It receives the proxy used to push `WebviewCommand`s into JS
/// and the receiver end for `WebviewEvent`s posted back from the page.
pub fn run_relay<F>(initial_video_id: Option<String>, on_setup: F) -> Result<()>
where
    F: FnOnce(EventLoopProxy<WebviewCommand>, mpsc::UnboundedReceiver<WebviewEvent>)
        + Send
        + 'static,
{
    let event_loop: EventLoop<WebviewCommand> = EventLoopBuilder::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let (ipc_tx, ipc_rx) = mpsc::unbounded_channel::<WebviewEvent>();

    std::thread::Builder::new()
        .name("late-webview-bridge".into())
        .spawn(move || on_setup(proxy, ipc_rx))
        .context("failed to spawn webview bridge thread")?;

    let window = WindowBuilder::new()
        .with_title("late.sh — YouTube")
        .with_inner_size(LogicalSize::new(480.0, 320.0))
        .build(&event_loop)
        .context("failed to build webview window")?;

    let mut html = PAGE_HTML.to_string();
    if let Some(video_id) = initial_video_id {
        let payload = json!({
            "item_id": "spike",
            "video_id": video_id,
            "is_stream": false,
        });
        html.push_str(&format!(
            "\n<script>window.lateBridge.loadVideo({});</script>\n",
            payload
        ));
    }

    let ipc_tx_handler = ipc_tx.clone();
    let webview = WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(move |req| {
            let body = req.body();
            match serde_json::from_str::<WebviewEvent>(body) {
                Ok(event) => {
                    let _ = ipc_tx_handler.send(event);
                }
                Err(err) => {
                    warn!(payload = %body, error = %err, "failed to parse webview event");
                }
            }
        })
        .build(&window)
        .context("failed to build webview")?;

    info!(target: "late_cli::webview", "webview runtime ready");

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(cmd) => {
                if let Err(err) = apply_command(&webview, cmd) {
                    warn!(error = %err, "failed to apply webview command");
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                warn!(target: "late_cli::webview", "window close requested; exiting");
                *control_flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}

fn apply_command(webview: &WebView, cmd: WebviewCommand) -> Result<()> {
    let js = match cmd {
        WebviewCommand::LoadVideo {
            item_id,
            video_id,
            is_stream,
        } => format!(
            "window.lateBridge.loadVideo({});",
            json!({
                "item_id": item_id,
                "video_id": video_id,
                "is_stream": is_stream,
            })
        ),
        WebviewCommand::SourceChanged { audio_mode } => format!(
            "window.lateBridge.sourceChanged({});",
            json!({ "audio_mode": audio_mode })
        ),
        WebviewCommand::Shutdown => "window.lateBridge.shutdown();".to_string(),
    };
    webview
        .evaluate_script(&js)
        .map_err(|err| anyhow::anyhow!("evaluate_script failed: {err}"))
}

fn validate_video_id(video_id: &str) -> Result<()> {
    if video_id.is_empty() || video_id.len() > 32 {
        anyhow::bail!("invalid video id");
    }
    if !video_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!("invalid video id");
    }
    Ok(())
}
