//! Embedded webview for CLI-side YouTube playback.
//!
//! See late-ssh/src/app/audio/CONTEXT.md §17.
//!
//! The webview is owned by the CLI: it never opens its own WebSocket.
//! Rust pushes commands (LoadVideo, SourceChanged, Shutdown) into the JS
//! bridge via tao's user-event mechanism; JS posts player state back through
//! wry's IPC handler. See `commands.rs` for the payload shapes and
//! `pair.rs` for the WS-relay task used by `late webview-pair`, which reads
//! the session token from stdin.

use anyhow::{Context, Result};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};
use tao::{
    dpi::{LogicalSize, PhysicalPosition},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy},
    window::WindowBuilder,
};
use tokio::sync::mpsc;
use tracing::{info, warn};
use wry::{WebView, WebViewBuilder};

#[cfg(target_os = "linux")]
use tao::platform::unix::{EventLoopBuilderExtUnix, WindowBuilderExtUnix, WindowExtUnix};
#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;

pub mod commands;
pub mod pair;

pub use commands::{WebviewCommand, WebviewEvent};

const PAGE_HTML: &str = include_str!("page.html");
const WEBVIEW_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";
const WEBVIEW_WINDOW_WIDTH: f64 = 200.0;
const WEBVIEW_WINDOW_HEIGHT: f64 = 200.0;

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
    let mut event_loop_builder = EventLoopBuilder::<WebviewCommand>::with_user_event();
    #[cfg(target_os = "linux")]
    event_loop_builder.with_app_id("sh.late.youtube");
    let event_loop: EventLoop<WebviewCommand> = event_loop_builder.build();
    let proxy = event_loop.create_proxy();
    let (ipc_tx, ipc_rx) = mpsc::unbounded_channel::<WebviewEvent>();

    let window_size = LogicalSize::new(WEBVIEW_WINDOW_WIDTH, WEBVIEW_WINDOW_HEIGHT);
    let mut window_builder = WindowBuilder::new()
        .with_title("late.sh — YouTube")
        .with_inner_size(window_size)
        .with_resizable(false)
        .with_decorations(false)
        .with_focused(false)
        .with_always_on_bottom(true);
    if let Some(position) = top_right_webview_position(&event_loop) {
        window_builder = window_builder.with_position(position);
    }
    #[cfg(target_os = "linux")]
    let window_builder = window_builder.with_skip_taskbar(true);
    let window = window_builder
        .build(&event_loop)
        .context("failed to build webview window")?;

    #[cfg(target_os = "linux")]
    expose_gstreamer_paths_to_webkit_sandbox();

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
    let page_server = PageServer::spawn(html).context("failed to start webview page server")?;
    info!(
        target: "late_cli::webview",
        url = %page_server.url,
        "serving embedded webview page"
    );

    let ipc_tx_handler = ipc_tx.clone();
    let webview = WebViewBuilder::new()
        .with_user_agent(WEBVIEW_USER_AGENT)
        .with_url(page_server.url.clone())
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
        });

    #[cfg(target_os = "linux")]
    let webview = webview
        .build_gtk(
            window
                .default_vbox()
                .context("tao window did not provide a GTK vbox")?,
        )
        .context("failed to build webview")?;
    #[cfg(not(target_os = "linux"))]
    let webview = webview.build(&window).context("failed to build webview")?;

    std::thread::Builder::new()
        .name("late-webview-bridge".into())
        .spawn(move || on_setup(proxy, ipc_rx))
        .context("failed to spawn webview bridge thread")?;

    info!(target: "late_cli::webview", "webview runtime ready");

    event_loop.run(move |event, _, control_flow| {
        // `build_gtk` mounts into Tao's GTK container; keep the Tao window
        // owned by the event-loop closure for the lifetime of the webview.
        let _keep_window_alive = &window;
        let _keep_page_server = &page_server;

        *control_flow = ControlFlow::Wait;
        match event {
            Event::UserEvent(cmd) => {
                let should_exit = matches!(cmd, WebviewCommand::Shutdown);
                if let Err(err) = apply_command(&webview, cmd) {
                    warn!(error = %err, "failed to apply webview command");
                }
                if should_exit {
                    *control_flow = ControlFlow::Exit;
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

fn top_right_webview_position<T>(event_loop: &EventLoop<T>) -> Option<PhysicalPosition<i32>> {
    let monitor = event_loop
        .primary_monitor()
        .or_else(|| event_loop.available_monitors().next())?;
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let window_size = LogicalSize::new(WEBVIEW_WINDOW_WIDTH, WEBVIEW_WINDOW_HEIGHT)
        .to_physical::<i32>(monitor.scale_factor());
    let monitor_width = monitor_size.width.min(i32::MAX as u32) as i32;
    let x = monitor_position.x + (monitor_width - window_size.width).max(0);

    Some(PhysicalPosition::new(x, monitor_position.y))
}

#[cfg(target_os = "linux")]
fn expose_gstreamer_paths_to_webkit_sandbox() {
    use std::{collections::BTreeSet, path::PathBuf};
    use webkit2gtk::{WebContext, WebContextExt};

    // WebKitWebProcess runs inside WebKitGTK's sandbox, so Nix store plugin
    // paths may need to be mounted even when GStreamer env vars are inherited.
    let mut paths = BTreeSet::<PathBuf>::new();
    collect_env_paths("LATE_WEBKIT_GSTREAMER_SANDBOX_PATHS", &mut paths);
    collect_env_paths("GST_PLUGIN_SYSTEM_PATH_1_0", &mut paths);
    collect_env_parent_paths("GST_PLUGIN_SCANNER", &mut paths);

    if paths.is_empty() {
        return;
    }

    let Some(context) = WebContext::default() else {
        warn!("WebKitGTK default context unavailable; cannot expose GStreamer paths to sandbox");
        return;
    };

    for path in paths {
        if !path.is_dir() {
            warn!(
                path = %path.display(),
                "skipping missing GStreamer path for WebKit sandbox"
            );
            continue;
        }

        context.add_path_to_sandbox(&path, true);
        info!(
            target: "late_cli::webview",
            path = %path.display(),
            "exposed GStreamer path to WebKit sandbox"
        );
    }
}

#[cfg(target_os = "linux")]
fn collect_env_paths(name: &str, paths: &mut std::collections::BTreeSet<std::path::PathBuf>) {
    let Some(value) = std::env::var_os(name) else {
        return;
    };
    if value.as_os_str().is_empty() {
        return;
    }

    paths.extend(std::env::split_paths(&value).filter(|path| !path.as_os_str().is_empty()));
}

#[cfg(target_os = "linux")]
fn collect_env_parent_paths(
    name: &str,
    paths: &mut std::collections::BTreeSet<std::path::PathBuf>,
) {
    let Some(value) = std::env::var_os(name) else {
        return;
    };
    if value.as_os_str().is_empty() {
        return;
    }

    for path in std::env::split_paths(&value).filter(|path| !path.as_os_str().is_empty()) {
        if let Some(parent) = path.parent() {
            paths.insert(parent.to_path_buf());
        }
    }
}

struct PageServer {
    url: String,
    _thread: std::thread::JoinHandle<()>,
}

impl PageServer {
    fn spawn(html: String) -> Result<Self> {
        let listener =
            TcpListener::bind(("localhost", 0)).context("failed to bind local page server")?;
        let addr = listener
            .local_addr()
            .context("failed to resolve local page server address")?;
        let port = addr.port();
        let html = Arc::new(html.into_bytes());
        let server_html = Arc::clone(&html);
        let thread = std::thread::Builder::new()
            .name("late-webview-page".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            if let Err(err) = serve_page_request(stream, server_html.as_slice()) {
                                warn!(error = %err, "failed to serve embedded webview page");
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "local webview page server stopped accepting");
                            break;
                        }
                    }
                }
            })
            .context("failed to spawn local page server")?;

        Ok(Self {
            url: format!("http://localhost:{port}/"),
            _thread: thread,
        })
    }
}

fn serve_page_request(mut stream: TcpStream, html: &[u8]) -> Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut request = Vec::with_capacity(1024);
    let mut buf = [0_u8; 1024];
    loop {
        let len = stream
            .read(&mut buf)
            .context("failed to read local page request")?;
        if len == 0 {
            break;
        }
        request.extend_from_slice(&buf[..len]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") || request.len() > 8192 {
            break;
        }
    }

    let request = String::from_utf8_lossy(&request);
    let Some((method, path)) = request.lines().next().and_then(parse_request_line) else {
        return write_http_response(
            stream,
            "400 Bad Request",
            "text/plain; charset=utf-8",
            b"bad request",
            true,
        );
    };
    let include_body = method != "HEAD";
    match (method, path) {
        ("GET" | "HEAD", "/" | "/index.html") => write_http_response(
            stream,
            "200 OK",
            "text/html; charset=utf-8",
            html,
            include_body,
        ),
        ("GET" | "HEAD", "/favicon.ico") => {
            write_http_response(stream, "204 No Content", "text/plain", b"", false)
        }
        _ => write_http_response(
            stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"not found",
            include_body,
        ),
    }
}

fn parse_request_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let path = parts.next()?;
    Some((method, path))
}

fn write_http_response(
    mut stream: TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
    include_body: bool,
) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\n\
         Content-Type: {content_type}\r\n\
         Content-Length: {}\r\n\
         Cache-Control: no-store\r\n\
         Referrer-Policy: strict-origin-when-cross-origin\r\n\
         Connection: close\r\n\
         \r\n",
        body.len()
    )
    .context("failed to write local page response headers")?;
    if include_body {
        stream
            .write_all(body)
            .context("failed to write local page response body")?;
    }
    stream
        .flush()
        .context("failed to flush local page response")
}

fn apply_command(webview: &WebView, cmd: WebviewCommand) -> Result<()> {
    let js = match cmd {
        WebviewCommand::LoadVideo {
            item_id,
            video_id,
            is_stream,
            start_seconds,
        } => format!(
            "window.lateBridge.loadVideo({});",
            json!({
                "item_id": item_id,
                "video_id": video_id,
                "is_stream": is_stream,
                "start_seconds": start_seconds,
            })
        ),
        WebviewCommand::SourceChanged { audio_mode } => format!(
            "window.lateBridge.sourceChanged({});",
            json!({ "audio_mode": audio_mode })
        ),
        WebviewCommand::AudioSettings {
            muted,
            volume_percent,
        } => format!(
            "window.lateBridge.audioSettings({});",
            json!({
                "muted": muted,
                "volume_percent": volume_percent,
            })
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
