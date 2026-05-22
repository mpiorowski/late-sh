//! Typed commands flowing between the CLI and the embedded webview.
//!
//! `WebviewCommand` is pushed from Rust into the webview's JS bridge via
//! tao's `EventLoopProxy::send_event`. `WebviewEvent` is parsed from JSON
//! that the page posts back through wry's IPC bridge.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub enum WebviewCommand {
    LoadVideo {
        item_id: String,
        video_id: String,
        is_stream: bool,
        start_seconds: Option<u64>,
    },
    SourceChanged {
        audio_mode: String,
    },
    AudioSettings {
        muted: bool,
        volume_percent: u8,
    },
    Shutdown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WebviewEvent {
    Ready,
    ApiLoadFailed,
    State {
        item_id: Option<String>,
        state: String,
        #[serde(default)]
        position_ms: u64,
        #[serde(default)]
        duration_ms: Option<u64>,
        #[serde(default)]
        autoplay_blocked: bool,
    },
    AutoplayBlocked {
        item_id: Option<String>,
    },
    Error {
        item_id: Option<String>,
        #[serde(default)]
        video_id: Option<String>,
        code: String,
    },
    SourceAck {
        audio_mode: String,
    },
    ShutdownAck,
}
