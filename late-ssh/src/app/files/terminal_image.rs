use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ratatui::layout::Rect;
use uuid::Uuid;

const KITTY_CHUNK_BYTES: usize = 4096;
const KITTY_LATE_IMAGE_ID_MIN: u32 = 0x4C00_0000;
const KITTY_LATE_IMAGE_ID_MAX: u32 = 0x4CFF_FFFF;
const KITTY_LATE_Z_INDEX: i32 = -1_024_076_853;
const KITTY_PROTOCOL_IDENTITIES: &[&str] =
    &["kitty", "ghostty", "wezterm", "rio", "warp", "konsole"];
const ITERM2_PROTOCOL_IDENTITIES: &[&str] = &["iterm", "mintty", "hterm"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TerminalImageProtocol {
    Kitty,
    Iterm2,
}

#[derive(Clone, Debug)]
pub struct TerminalImageData {
    pub png_bytes: Arc<Vec<u8>>,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub display_cols: u16,
    pub display_rows: u16,
}

impl TerminalImageData {
    pub(crate) fn image_hash(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.png_bytes.len().hash(&mut hasher);
        self.pixel_width.hash(&mut hasher);
        self.pixel_height.hash(&mut hasher);
        self.display_cols.hash(&mut hasher);
        self.display_rows.hash(&mut hasher);
        self.png_bytes.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Clone, Debug)]
pub struct TerminalImagePlacement {
    pub message_id: Uuid,
    pub area: Rect,
    pub data: TerminalImageData,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalImagePlacementKey {
    message_id: Uuid,
    x: u16,
    y: u16,
    cols: u16,
    rows: u16,
    image_hash: u64,
}

#[derive(Default)]
pub struct TerminalImageFrame {
    placements: Vec<TerminalImagePlacement>,
}

impl TerminalImageFrame {
    pub(crate) fn push(&mut self, placement: TerminalImagePlacement) {
        self.placements.push(placement);
    }

    #[cfg(test)]
    pub(crate) fn placements(&self) -> &[TerminalImagePlacement] {
        &self.placements
    }

    fn keys(&self) -> Vec<TerminalImagePlacementKey> {
        self.placements
            .iter()
            .map(|placement| TerminalImagePlacementKey {
                message_id: placement.message_id,
                x: placement.area.x,
                y: placement.area.y,
                cols: placement.area.width,
                rows: placement.area.height,
                image_hash: placement.data.image_hash(),
            })
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct TerminalImageRenderState {
    protocol: Option<TerminalImageProtocol>,
    placements: Vec<TerminalImagePlacementKey>,
}

impl TerminalImageRenderState {
    pub(crate) fn build_commands(
        &mut self,
        protocol: Option<TerminalImageProtocol>,
        frame: &TerminalImageFrame,
    ) -> Vec<Vec<u8>> {
        if protocol.is_none() {
            let previous_had_kitty =
                self.protocol == Some(TerminalImageProtocol::Kitty) && !self.placements.is_empty();
            self.protocol = None;
            self.placements.clear();
            if previous_had_kitty {
                return kitty_cleanup_commands();
            }
            return Vec::new();
        }

        let keys = frame.keys();
        if self.protocol == protocol && self.placements == keys {
            return Vec::new();
        }

        let previous_had_kitty =
            self.protocol == Some(TerminalImageProtocol::Kitty) && !self.placements.is_empty();
        self.protocol = protocol;
        self.placements = keys;

        let Some(protocol) = protocol else {
            return Vec::new();
        };

        let mut commands = Vec::new();
        if previous_had_kitty || protocol == TerminalImageProtocol::Kitty {
            commands.extend(kitty_cleanup_commands());
        }

        for placement in &frame.placements {
            match protocol {
                TerminalImageProtocol::Kitty => {
                    commands.extend(kitty_image_commands(placement));
                }
                TerminalImageProtocol::Iterm2 => {
                    commands.extend(iterm2_image_commands(placement));
                }
            }
        }
        commands
    }
}

pub(crate) fn protocol_from_term(term: &str) -> Option<TerminalImageProtocol> {
    protocol_from_identity(term)
}

pub(crate) fn protocol_from_terminal_program(program: &str) -> Option<TerminalImageProtocol> {
    protocol_from_identity(program)
}

pub(crate) fn protocol_from_xtversion(version: &str) -> Option<TerminalImageProtocol> {
    protocol_from_identity(version)
}

pub(crate) fn protocol_from_env_hint(name: &str, value: &str) -> Option<TerminalImageProtocol> {
    match name.trim() {
        "TERM_PROGRAM" | "LC_TERMINAL" => protocol_from_terminal_program(value),
        "TERM_FEATURES" => protocol_from_terminal_features(value),
        "KITTY_WINDOW_ID" | "KITTY_PID" | "KITTY_PUBLIC_KEY" => {
            non_empty_protocol(value, TerminalImageProtocol::Kitty)
        }
        "WEZTERM_PANE" | "WEZTERM_EXECUTABLE" => {
            non_empty_protocol(value, TerminalImageProtocol::Kitty)
        }
        "KONSOLE_VERSION" | "GHOSTTY_RESOURCES_DIR" | "GHOSTTY_BIN_DIR" => {
            non_empty_protocol(value, TerminalImageProtocol::Kitty)
        }
        _ => None,
    }
}

pub(crate) fn protocol_from_terminal_features(features: &str) -> Option<TerminalImageProtocol> {
    if terminal_features_include_file(features) {
        Some(TerminalImageProtocol::Iterm2)
    } else {
        None
    }
}

fn protocol_from_identity(value: &str) -> Option<TerminalImageProtocol> {
    let value = value.trim().to_ascii_lowercase();
    if ITERM2_PROTOCOL_IDENTITIES
        .iter()
        .any(|identity| value.contains(identity))
    {
        Some(TerminalImageProtocol::Iterm2)
    } else if KITTY_PROTOCOL_IDENTITIES
        .iter()
        .any(|identity| value.contains(identity))
    {
        Some(TerminalImageProtocol::Kitty)
    } else {
        None
    }
}

fn non_empty_protocol(
    value: &str,
    protocol: TerminalImageProtocol,
) -> Option<TerminalImageProtocol> {
    if value.trim().is_empty() {
        None
    } else {
        Some(protocol)
    }
}

fn terminal_features_include_file(features: &str) -> bool {
    let mut chars = features.chars().peekable();
    while let Some(ch) = chars.next() {
        if !ch.is_ascii_alphanumeric() {
            break;
        }
        if !ch.is_ascii_uppercase() {
            continue;
        }

        let mut code = String::from(ch);
        while let Some(next) = chars.peek().copied() {
            if next.is_ascii_lowercase() {
                code.push(next);
                chars.next();
            } else {
                break;
            }
        }
        while chars.peek().is_some_and(|next| next.is_ascii_digit()) {
            chars.next();
        }

        if code == "F" {
            return true;
        }
    }

    false
}

pub(crate) fn term_disables_terminal_images(term: &str) -> bool {
    let term = term.trim().to_ascii_lowercase();
    term.contains("tmux")
}

pub(crate) fn xtversion_probe() -> Vec<u8> {
    b"\x1b[>q".to_vec()
}

pub(crate) fn iterm2_capabilities_probe() -> Vec<u8> {
    b"\x1b]1337;Capabilities\x1b\\".to_vec()
}

fn kitty_clear_visible_images() -> Vec<u8> {
    b"\x1b_Ga=d,q=2\x1b\\".to_vec()
}

fn kitty_delete_command(control: impl AsRef<str>) -> Vec<u8> {
    format!("\x1b_G{}\x1b\\", control.as_ref()).into_bytes()
}

pub(crate) fn kitty_cleanup_commands() -> Vec<Vec<u8>> {
    kitty_cleanup_base_commands()
}

pub(crate) fn terminal_image_cleanup_commands() -> Vec<Vec<u8>> {
    kitty_cleanup_base_commands()
}

fn cursor_to(area: Rect) -> Vec<u8> {
    format!(
        "\x1b[{};{}H",
        area.y.saturating_add(1),
        area.x.saturating_add(1)
    )
    .into_bytes()
}

fn kitty_image_commands(placement: &TerminalImagePlacement) -> Vec<Vec<u8>> {
    let encoded = STANDARD.encode(placement.data.png_bytes.as_slice());
    let image_id = kitty_image_id(placement.message_id);
    let mut commands = Vec::new();
    commands.push(cursor_to(placement.area));

    let mut chunks = encoded.as_bytes().chunks(KITTY_CHUNK_BYTES).peekable();
    let mut first = true;
    while let Some(chunk) = chunks.next() {
        let more = if chunks.peek().is_some() { 1 } else { 0 };
        let control = if first {
            first = false;
            format!(
                "a=T,f=100,q=2,i={image_id},p=1,z={},c={},r={},C=1,m={more}",
                KITTY_LATE_Z_INDEX, placement.area.width, placement.area.height
            )
        } else {
            format!("q=2,m={more}")
        };
        let mut command = format!("\x1b_G{control};").into_bytes();
        command.extend_from_slice(chunk);
        command.extend_from_slice(b"\x1b\\");
        commands.push(command);
    }

    commands
}

fn kitty_image_id(message_id: Uuid) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    message_id.hash(&mut hasher);
    KITTY_LATE_IMAGE_ID_MIN | ((hasher.finish() as u32) & 0x00FF_FFFF)
}

fn kitty_cleanup_base_commands() -> Vec<Vec<u8>> {
    vec![
        kitty_clear_visible_images(),
        kitty_delete_command("a=d,d=A,q=2"),
        kitty_delete_command(format!("a=d,d=Z,z={KITTY_LATE_Z_INDEX},q=2")),
        kitty_delete_command(format!(
            "a=d,d=R,x={KITTY_LATE_IMAGE_ID_MIN},y={KITTY_LATE_IMAGE_ID_MAX},q=2"
        )),
    ]
}

fn iterm2_image_commands(placement: &TerminalImagePlacement) -> Vec<Vec<u8>> {
    let mut commands = vec![cursor_to(placement.area)];
    let encoded = STANDARD.encode(placement.data.png_bytes.as_slice());
    commands.push(
        format!(
            "\x1b]1337;File=inline=1;width={};height={};preserveAspectRatio=1;size={}:{}\x07",
            placement.area.width,
            placement.area.height,
            placement.data.png_bytes.len(),
            encoded
        )
        .into_bytes(),
    );
    commands
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kitty_family_identities_use_kitty_protocol() {
        for value in [
            "kitty",
            "xterm-kitty",
            "ghostty",
            "xterm-ghostty",
            "WezTerm 20240203",
            "rio",
            "WarpTerminal",
            "konsole",
        ] {
            assert_eq!(
                protocol_from_identity(value),
                Some(TerminalImageProtocol::Kitty)
            );
        }
    }

    #[test]
    fn iterm_family_identities_use_iterm2_protocol() {
        for value in ["iTerm.app", "iTerm2", "mintty", "hterm"] {
            assert_eq!(
                protocol_from_identity(value),
                Some(TerminalImageProtocol::Iterm2)
            );
        }
    }

    #[test]
    fn terminal_env_hints_enable_image_protocols() {
        assert_eq!(
            protocol_from_env_hint("LC_TERMINAL", "iTerm2"),
            Some(TerminalImageProtocol::Iterm2)
        );
        assert_eq!(
            protocol_from_env_hint("WEZTERM_PANE", "3"),
            Some(TerminalImageProtocol::Kitty)
        );
        assert_eq!(protocol_from_env_hint("WEZTERM_PANE", ""), None);
    }

    #[test]
    fn terminal_features_enable_iterm2_file_protocol() {
        assert_eq!(
            protocol_from_terminal_features("T1CwMUBSxF"),
            Some(TerminalImageProtocol::Iterm2)
        );
        assert_eq!(protocol_from_terminal_features("T1CwMUBSx"), None);
    }

    #[test]
    fn tmux_term_disables_terminal_images() {
        assert!(term_disables_terminal_images("tmux-256color"));
        assert!(!term_disables_terminal_images("xterm-kitty"));
    }
}
