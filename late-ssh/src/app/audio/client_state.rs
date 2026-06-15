use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Browser,
    Cli,
    #[default]
    Unknown,
}

impl ClientKind {
    pub fn label(self) -> &'static str {
        match self {
            ClientKind::Browser => "Browser",
            ClientKind::Cli => "CLI",
            ClientKind::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClientSshMode {
    Native,
    #[serde(rename = "openssh")]
    OpenSsh,
    Old,
    Webview,
    #[default]
    Unknown,
}

impl ClientSshMode {
    pub(crate) fn metric_label(self) -> Option<&'static str> {
        match self {
            Self::Native => Some("native"),
            Self::OpenSsh => Some("openssh"),
            Self::Old => Some("old"),
            Self::Webview => None,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientAudioState {
    pub client_kind: ClientKind,
    #[serde(default)]
    pub ssh_mode: ClientSshMode,
    #[serde(default)]
    pub platform: ClientPlatform,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub muted: bool,
    pub volume_percent: u8,
    #[serde(default = "default_icecast_output_available")]
    pub icecast_output_available: bool,
}

impl Default for ClientAudioState {
    fn default() -> Self {
        Self {
            client_kind: ClientKind::Unknown,
            ssh_mode: ClientSshMode::Unknown,
            platform: ClientPlatform::Unknown,
            capabilities: Vec::new(),
            muted: false,
            volume_percent: 30,
            icecast_output_available: true,
        }
    }
}

const fn default_icecast_output_available() -> bool {
    true
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

    pub(crate) fn cli_usage_labels(&self) -> Option<(&'static str, &'static str)> {
        if self.client_kind != ClientKind::Cli {
            return None;
        }

        Some((self.ssh_mode.metric_label()?, self.platform.metric_label()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ssh_mode_parses_openssh() {
        let mode: ClientSshMode = serde_json::from_str(r#""openssh""#).unwrap();
        assert_eq!(mode, ClientSshMode::OpenSsh);
        assert_eq!(mode.metric_label(), Some("openssh"));
    }

    #[test]
    fn client_ssh_mode_parses_webview() {
        let mode: ClientSshMode = serde_json::from_str(r#""webview""#).unwrap();
        assert_eq!(mode, ClientSshMode::Webview);
        assert_eq!(mode.metric_label(), None);
    }
}
