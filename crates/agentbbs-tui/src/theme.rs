//! Retro Wildcat!-era colour palette and ANSI furniture.
//!
//! The look is deliberately 1990s dial-up BBS: high-cyan and magenta on
//! black, double-line box drawing, a blinking-style lightbar menu, and a
//! chunky banner. It is cosmetic only — every screen is driven by the same
//! [`agentbbs_core`] domain underneath.

use ratatui::style::{Color, Modifier, Style};

/// Classic BBS cyan (the Wildcat! menu colour).
pub const CYAN: Color = Color::Cyan;
/// Hot magenta used for highlights and the lightbar.
pub const MAGENTA: Color = Color::Magenta;
/// Bright yellow for hotkeys.
pub const YELLOW: Color = Color::Yellow;
/// Muted gray for chrome and timestamps.
pub const GRAY: Color = Color::DarkGray;
/// Green for "online"/success.
pub const GREEN: Color = Color::Green;
/// Red for warnings.
pub const RED: Color = Color::Red;

/// Style for a screen title bar.
pub fn title() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(CYAN)
        .add_modifier(Modifier::BOLD)
}

/// Style for the selected lightbar row.
pub fn lightbar() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(MAGENTA)
        .add_modifier(Modifier::BOLD)
}

/// Style for a hotkey letter.
pub fn hotkey() -> Style {
    Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)
}

/// Style for chrome/borders.
pub fn chrome() -> Style {
    Style::default().fg(CYAN)
}

/// Style for dim secondary text.
pub fn dim() -> Style {
    Style::default().fg(GRAY)
}

/// The AgentBBS banner, rendered at the top of the splash and main menu.
pub const BANNER: &[&str] = &[
    "  ▄▀█ █▀▀ █▀▀ █▄░█ ▀█▀ █▄▄ █▄▄ █▀   ",
    "  █▀█ █▄█ ██▄ █░▀█ ░█░ █▄█ █▄█ ▄█   ",
    "   t h e   b b s   f o r   a g e n t s   &   h u m a n s ",
];
