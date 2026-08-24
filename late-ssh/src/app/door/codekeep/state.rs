use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{CodekeepProcess, ProcessConfig, ProxyStatus};
use crate::app::door::keys;
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Launcher,
    Running,
}

pub struct State {
    user_id: uuid::Uuid,
    host: String,
    port: u16,
    secret: String,
    enabled: bool,
    mode: Mode,
    proxy: Option<CodekeepProcess>,
    viewport: Rect,
    term: String,
    repaint: Option<Arc<RenderSignal>>,
}

impl State {
    pub fn new(
        user_id: uuid::Uuid,
        host: String,
        port: u16,
        secret: String,
        term: String,
        enabled: bool,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        Self {
            user_id,
            host,
            port,
            secret,
            enabled,
            mode: Mode::Launcher,
            proxy: None,
            viewport: Rect::new(0, 0, 108, 24),
            term,
            repaint,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_running(&self) -> bool {
        matches!(self.mode, Mode::Running)
    }

    pub fn set_viewport(&mut self, area: Rect) {
        let resized = self.viewport.width != area.width || self.viewport.height != area.height;
        self.viewport = area;
        if resized && let Some(proxy) = &self.proxy {
            proxy.resize(area.width, area.height);
        }
    }

    pub fn connect(&mut self) {
        if !self.enabled || self.proxy.is_some() {
            return;
        }
        self.proxy = Some(CodekeepProcess::spawn(ProcessConfig {
            host: self.host.clone(),
            port: self.port,
            secret: self.secret.clone(),
            user_id: self.user_id,
            cols: self.viewport.width.max(1),
            rows: self.viewport.height.max(1),
            term: self.term.clone(),
            repaint: self.repaint.clone(),
        }));
        self.mode = Mode::Running;
    }

    pub fn tick(&mut self) {
        if self.mode == Mode::Running {
            let closed = self
                .proxy
                .as_ref()
                .is_none_or(|proxy| proxy.status() == ProxyStatus::Closed);
            if closed {
                self.proxy = None;
                self.mode = Mode::Launcher;
            }
        }
    }

    pub fn proxy(&self) -> Option<&CodekeepProcess> {
        self.proxy.as_ref()
    }

    /// Keep late.sh's any-event mouse tracking and bracketed-paste control
    /// sequences out of Ink. Ordinary keys and arrow escapes pass through.
    pub fn forward_input(&self, data: &[u8]) {
        if let Some(proxy) = &self.proxy {
            let keys = keys_for_game(proxy, data);
            if !keys.is_empty() {
                proxy.send_input(keys);
            }
        }
    }
}

/// The exact bytes a client chunk becomes for the running game: noise
/// stripped, then cursor keys retyped to the mode the guest holds
/// (`app/door/keys.rs`). A guest that never requests application cursor mode
/// (the expected case for Ink, which decodes CSI itself) gets its input
/// untouched.
fn keys_for_game(proxy: &CodekeepProcess, data: &[u8]) -> Vec<u8> {
    let filtered = strip_input_noise(data);
    match proxy.with_screen(|screen| screen.application_cursor()) {
        true => keys::to_application_cursor(&filtered),
        false => filtered,
    }
}

fn strip_input_noise(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
            let rest = &data[i + 2..];
            if rest.first() == Some(&b'<')
                && let Some(end) = rest.iter().position(|&b| b == b'M' || b == b'm')
            {
                i += 2 + end + 1;
                continue;
            }
            if rest.first() == Some(&b'M') && rest.len() >= 4 {
                i += 6;
                continue;
            }
            if rest.starts_with(b"200~") || rest.starts_with(b"201~") {
                i += 6;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
