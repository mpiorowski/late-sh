use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{NethackProcess, ProcessConfig, ProxyStatus, sanitize_playname};
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Launcher,
    Running,
}

pub struct State {
    user_id: uuid::Uuid,
    username: String,
    bin: String,
    data_dir: String,
    /// Feature flag: when false the door is reachable but launching is a no-op
    /// and the Launcher shows an "unavailable" message.
    enabled: bool,
    mode: Mode,
    proxy: Option<NethackProcess>,
    /// Inner viewport (below the top bar) from the last render, used for PTY
    /// sizing.
    viewport: Rect,
    term: String,
    /// Render-loop wakeup (from the transport). Passed to the proxy so new
    /// output repaints promptly. `None` on headless/test paths.
    repaint: Option<Arc<RenderSignal>>,
}

impl State {
    pub fn new(
        user_id: uuid::Uuid,
        username: String,
        bin: String,
        data_dir: String,
        term: String,
        enabled: bool,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        Self {
            user_id,
            username,
            bin,
            data_dir,
            enabled,
            mode: Mode::Launcher,
            proxy: None,
            viewport: Rect::new(0, 0, 80, 24),
            term,
            repaint,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether the door is enabled (launchable). When false the Launcher shows
    /// an "unavailable" message and `connect` is a no-op.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_running(&self) -> bool {
        matches!(self.mode, Mode::Running)
    }

    pub fn set_viewport(&mut self, area: Rect) {
        let resized = self.viewport.width != area.width || self.viewport.height != area.height;
        self.viewport = area;
        if resized && let Some(p) = &self.proxy {
            p.resize(area.width, area.height);
        }
    }

    pub fn connect(&mut self) {
        if !self.enabled || self.proxy.is_some() {
            return;
        }
        self.proxy = Some(NethackProcess::spawn(ProcessConfig {
            bin: self.bin.clone(),
            data_dir: self.data_dir.clone(),
            playname: sanitize_playname(&self.username, self.user_id),
            cols: self.viewport.width.max(1),
            rows: self.viewport.height.max(1),
            term: self.term.clone(),
            repaint: self.repaint.clone(),
        }));
        self.mode = Mode::Running;
    }

    /// Called every app tick: if the process closed (clean quit, death, or
    /// crash), return to the Launcher. Treats all exits identically.
    pub fn tick(&mut self) {
        if self.mode == Mode::Running {
            let closed = self
                .proxy
                .as_ref()
                .is_none_or(|p| p.status() == ProxyStatus::Closed);
            if closed {
                self.proxy = None;
                self.mode = Mode::Launcher;
            }
        }
    }

    pub fn proxy(&self) -> Option<&NethackProcess> {
        self.proxy.as_ref()
    }

    /// Forward raw client bytes to nethack verbatim. NetHack is keyboard-driven,
    /// so unlike rebels there is no mouse-coordinate rewriting.
    pub fn forward_input(&self, data: &[u8]) {
        if let Some(proxy) = &self.proxy {
            proxy.send_input(data.to_vec());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_state() -> State {
        State::new(
            uuid::Uuid::nil(),
            "tester".to_string(),
            "/usr/games/nethack".to_string(),
            "/var/lib/late-nethack".to_string(),
            "xterm".to_string(),
            false,
            None,
        )
    }

    #[test]
    fn connect_is_a_no_op_when_disabled() {
        let mut state = disabled_state();
        assert!(!state.is_enabled());
        state.connect();
        assert!(state.proxy().is_none());
        assert_eq!(state.mode(), Mode::Launcher);
    }

    #[test]
    fn forward_input_without_proxy_is_a_no_op() {
        let state = disabled_state();
        // Must not panic when nothing is running.
        state.forward_input(b"hjkl");
    }
}
