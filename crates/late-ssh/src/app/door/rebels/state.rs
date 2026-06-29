use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{ProxyConfig, ProxyStatus, RebelsProxy};
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
    /// Feature flag: when false the door is reachable but connecting is a no-op
    /// and the Launcher shows an "unavailable" message.
    enabled: bool,
    mode: Mode,
    proxy: Option<RebelsProxy>,
    /// Inner viewport (below the top bar) from the last render, used for PTY
    /// sizing and mouse-coordinate offsetting.
    viewport: Rect,
    term: String,
    /// Render-loop wakeup (from the transport). Passed to the proxy so new
    /// remote output repaints promptly. `None` on headless/test paths.
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
            viewport: Rect::new(0, 0, 80, 24),
            term,
            repaint,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether the door is enabled (connectable). When false the Launcher shows
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
        self.proxy = Some(RebelsProxy::connect(ProxyConfig {
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

    /// Called every app tick: if the proxy closed (clean quit, drop, or
    /// timeout), return to the Launcher. Treats all disconnects identically.
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

    pub fn proxy(&self) -> Option<&RebelsProxy> {
        self.proxy.as_ref()
    }

    /// Forward raw client bytes to rebels, rewriting SGR mouse coordinates so
    /// they are relative to the viewport. Mouse events outside the viewport
    /// content area are dropped. Non-mouse bytes pass through verbatim.
    pub fn forward_input(&self, data: &[u8]) {
        let Some(proxy) = &self.proxy else {
            return;
        };
        let out = rewrite_mouse(data, self.viewport.x, self.viewport.y);
        if !out.is_empty() {
            proxy.send_input(out);
        }
    }
}

/// Rewrite SGR mouse reports (`ESC [ < b ; x ; y (M|m)`) by subtracting
/// the viewport offsets from the 1-based column and row. Reports outside or on
/// the viewport border are dropped. All other bytes are copied through
/// unchanged.
pub fn rewrite_mouse(data: &[u8], x_offset: u16, y_offset: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && data.get(i + 1) == Some(&b'[') && data.get(i + 2) == Some(&b'<') {
            // find the terminating 'M' or 'm'
            if let Some(end_rel) = data[i + 3..].iter().position(|&b| b == b'M' || b == b'm') {
                let end = i + 3 + end_rel;
                let body = &data[i + 3..end];
                if let Some(rewritten) = rewrite_sgr_mouse_body(body, x_offset, y_offset, data[end])
                {
                    out.extend_from_slice(&rewritten);
                } // else: dropped (outside viewport or unparseable)
                i = end + 1;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

fn rewrite_sgr_mouse_body(
    body: &[u8],
    x_offset: u16,
    y_offset: u16,
    terminator: u8,
) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(body).ok()?;
    let mut parts = s.split(';');
    let b = parts.next()?;
    let x: u16 = parts.next()?.parse().ok()?;
    let y: u16 = parts.next()?.parse().ok()?;
    if x <= x_offset || y <= y_offset {
        return None; // on or outside the viewport border
    }
    let new_x = x - x_offset;
    let new_y = y - y_offset;
    Some(format!("\x1b[<{b};{new_x};{new_y}{}", terminator as char).into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_non_mouse_bytes() {
        assert_eq!(rewrite_mouse(b"hello\r", 1, 3), b"hello\r");
    }

    #[test]
    fn mouse_position_is_offset_by_viewport() {
        // click at col 5,row 10 with viewport x=1,y=3 -> rebels col 4,row 7
        let input = b"\x1b[<0;5;10M";
        assert_eq!(rewrite_mouse(input, 1, 3), b"\x1b[<0;4;7M".to_vec());
    }

    #[test]
    fn mouse_on_top_bar_is_dropped() {
        // click at row 2 (<= y_offset 3) -> dropped
        let input = b"\x1b[<0;5;2M";
        assert_eq!(rewrite_mouse(input, 1, 3), Vec::<u8>::new());
    }

    #[test]
    fn mouse_on_left_border_is_dropped() {
        // click at col 1 (<= x_offset 1) -> dropped
        let input = b"\x1b[<0;1;10M";
        assert_eq!(rewrite_mouse(input, 1, 3), Vec::<u8>::new());
    }

    #[test]
    fn mixed_stream_keeps_keys_and_rewrites_mouse() {
        let input = b"a\x1b[<0;5;10Mb";
        assert_eq!(rewrite_mouse(input, 1, 3), b"a\x1b[<0;4;7Mb".to_vec());
    }

    #[test]
    fn truncated_mouse_sequence_passes_through_verbatim() {
        // No terminating 'M'/'m' before end-of-buffer: copy bytes through
        // unchanged rather than panicking or swallowing them.
        let input = b"\x1b[<0;5;10";
        assert_eq!(rewrite_mouse(input, 1, 3), input.to_vec());
    }

    #[test]
    fn arrow_key_csi_passes_through_untouched() {
        // `ESC [ A` starts `ESC [` but is not the `ESC [ <` mouse prefix, so it
        // must be forwarded unchanged.
        let input = b"\x1b[A";
        assert_eq!(rewrite_mouse(input, 1, 3), input.to_vec());
    }

    #[test]
    fn connect_is_a_no_op_when_disabled() {
        let mut state = State::new(
            uuid::Uuid::nil(),
            "frittura.org".to_string(),
            3788,
            String::new(),
            "xterm".to_string(),
            false,
            None,
        );
        assert!(!state.is_enabled());
        state.connect();
        // No proxy spawned and we stay in the Launcher.
        assert!(state.proxy().is_none());
        assert_eq!(state.mode(), Mode::Launcher);
    }
}
