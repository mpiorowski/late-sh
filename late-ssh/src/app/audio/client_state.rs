use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    /// The CLI's embedded YouTube webview helper. It is a real browser engine,
    /// which is why it used to register as `browser`, but since browser
    /// pairing was removed it is the only one left, so the alias keeps helpers
    /// from older `late` releases registering correctly.
    #[serde(alias = "browser")]
    Webview,
    Cli,
    #[default]
    Unknown,
}

impl ClientKind {
    pub fn label(self) -> &'static str {
        match self {
            ClientKind::Webview => "Webview",
            ClientKind::Cli => "CLI",
            ClientKind::Unknown => "Unknown",
        }
    }
}

/// How the user reached the SSH session. The webview helper is a sidecar, not
/// an SSH mode, so it has no variant here: `ClientKind::Webview` identifies it.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientSshMode {
    Native,
    #[serde(rename = "openssh")]
    OpenSsh,
    Old,
    /// Also the landing spot for any mode string we don't recognize, including
    /// the `webview` that helpers from older `late` releases still send. An
    /// unknown value must degrade, not fail the whole `client_state` message
    /// and leave that client without mute and volume.
    #[default]
    #[serde(other)]
    Unknown,
}

impl ClientSshMode {
    pub(crate) fn metric_label(self) -> Option<&'static str> {
        match self {
            Self::Native => Some("native"),
            Self::OpenSsh => Some("openssh"),
            Self::Old => Some("old"),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientPlatform {
    Android,
    Linux,
    Macos,
    Windows,
    #[default]
    Unknown,
}

impl ClientPlatform {
    pub(crate) fn metric_label(self) -> Option<&'static str> {
        match self {
            Self::Android => Some("android"),
            Self::Linux => Some("linux"),
            Self::Macos => Some("macos"),
            Self::Windows => Some("windows"),
            Self::Unknown => None,
        }
    }
}

/// One reason a paired CLI cannot play a News video, as it reports them in
/// `client_state.video_blockers` (`late-cli/src/video.rs` owns the checks).
/// The watch setup box marks its rows from these.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VideoBlocker {
    /// OpenSSH and old modes: the CLI does not own the tty.
    NotNative,
    /// Only Linux and macOS play video.
    Platform,
    /// tmux or screen sits between mpv and the terminal.
    Multiplexer,
    /// The CLI's env does not look like a Kitty-graphics terminal.
    NotKitty,
    /// The tty reports no pixel size, so mpv cannot size its placement.
    NoPixelSize,
    NoMpv,
    /// Older than 0.36, or a version string that does not parse.
    MpvTooOld,
    #[serde(rename = "no_ytdlp")]
    NoYtDlp,
}

impl VideoBlocker {
    /// The wire string the CLI sends. `None` for a string this server does
    /// not know: a newer CLI than the server, which releases on its own
    /// schedule, so the caller drops it with a warn instead of failing.
    pub fn parse(wire: &str) -> Option<Self> {
        match wire {
            "not_native" => Some(Self::NotNative),
            "platform" => Some(Self::Platform),
            "multiplexer" => Some(Self::Multiplexer),
            "not_kitty" => Some(Self::NotKitty),
            "no_pixel_size" => Some(Self::NoPixelSize),
            "no_mpv" => Some(Self::NoMpv),
            "mpv_too_old" => Some(Self::MpvTooOld),
            "no_ytdlp" => Some(Self::NoYtDlp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientAudioState {
    pub client_kind: ClientKind,
    #[serde(default)]
    pub ssh_mode: ClientSshMode,
    #[serde(default)]
    pub platform: ClientPlatform,
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Present only when `video` is not in `capabilities`.
    #[serde(default)]
    pub video_blockers: Vec<VideoBlocker>,
    pub muted: bool,
    pub volume_percent: u8,
}

impl Default for ClientAudioState {
    fn default() -> Self {
        Self {
            client_kind: ClientKind::Unknown,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            video_blockers: Vec::new(),
            muted: false,
            volume_percent: 30,
        }
    }
}

impl ClientAudioState {
    pub fn supports_clipboard_image(&self) -> bool {
        self.client_kind == ClientKind::Cli
            && self
                .capabilities
                .iter()
                .any(|capability| capability == "clipboard_image")
    }

    pub fn supports_youtube_playback(&self) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability == "youtube")
    }

    pub fn supports_voice(&self) -> bool {
        self.client_kind == ClientKind::Cli
            && self
                .capabilities
                .iter()
                .any(|capability| capability == "voice")
    }

    pub fn supports_open_url(&self) -> bool {
        self.client_kind == ClientKind::Cli
            && self
                .capabilities
                .iter()
                .any(|capability| capability == "open_url")
    }

    /// The `video` capability: every precondition for playing a News video
    /// in the terminal held when the CLI started.
    pub fn supports_video(&self) -> bool {
        self.client_kind == ClientKind::Cli
            && self
                .capabilities
                .iter()
                .any(|capability| capability == "video")
    }

    pub(crate) fn cli_usage_labels(&self) -> Option<(&'static str, &'static str)> {
        if self.client_kind != ClientKind::Cli {
            return None;
        }

        Some((self.ssh_mode.metric_label()?, self.platform.metric_label()?))
    }
}

#[cfg(test)]
#[path = "client_state_test.rs"]
mod client_state_test;
