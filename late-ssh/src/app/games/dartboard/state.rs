use dartboard_core::{CanvasOp, Pos, RgbColor};
use ratatui::layout::Rect;
use tokio::sync::{
    broadcast::{self, error::TryRecvError},
    watch,
};

use super::svc::{DartboardEvent, DartboardService, DartboardSnapshot};

pub struct State {
    pub snapshot: DartboardSnapshot,
    pub private_notice: Option<String>,
    #[allow(dead_code)]
    pub(crate) svc: DartboardService,
    pub(crate) cursor: Pos,
    pub(crate) viewport_origin: Pos,
    snapshot_rx: watch::Receiver<DartboardSnapshot>,
    event_rx: broadcast::Receiver<DartboardEvent>,
}

impl State {
    pub fn new(svc: DartboardService) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let event_rx = svc.subscribe_events();
        Self {
            snapshot,
            private_notice: None,
            svc,
            cursor: Pos { x: 0, y: 0 },
            viewport_origin: Pos { x: 0, y: 0 },
            snapshot_rx,
            event_rx,
        }
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
        if let Some(reason) = self.snapshot.connect_rejected.as_ref() {
            self.private_notice = Some(reason.clone());
        }

        loop {
            match self.event_rx.try_recv() {
                Ok(DartboardEvent::Reject { reason, .. }) => self.private_notice = Some(reason),
                Ok(DartboardEvent::ConnectRejected { reason }) => {
                    self.private_notice = Some(reason);
                }
                Ok(DartboardEvent::Ack { .. })
                | Ok(DartboardEvent::PeerJoined { .. })
                | Ok(DartboardEvent::PeerLeft { .. }) => {}
                Err(TryRecvError::Empty) | Err(TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(skipped)) => {
                    self.private_notice =
                        Some(format!("Artboard updates lagged ({skipped} dropped)."));
                }
            }
        }
    }

    pub fn set_viewport_for_screen(&mut self, screen_size: (u16, u16)) {
        let viewport = super::ui::canvas_area_for_screen(screen_size);
        self.clamp_to_viewport(viewport);
    }

    pub fn move_left(&mut self, screen_size: (u16, u16)) {
        if self.cursor.x > 0 {
            self.cursor.x -= 1;
        }
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_right(&mut self, screen_size: (u16, u16)) {
        if self.cursor.x + 1 < self.snapshot.canvas.width {
            self.cursor.x += 1;
        }
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_up(&mut self, screen_size: (u16, u16)) {
        if self.cursor.y > 0 {
            self.cursor.y -= 1;
        }
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_down(&mut self, screen_size: (u16, u16)) {
        if self.cursor.y + 1 < self.snapshot.canvas.height {
            self.cursor.y += 1;
        }
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_home(&mut self, screen_size: (u16, u16)) {
        self.cursor.x = self.visible_bounds(screen_size).min_x;
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_end(&mut self, screen_size: (u16, u16)) {
        self.cursor.x = self.visible_bounds(screen_size).max_x;
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_page_up(&mut self, screen_size: (u16, u16)) {
        self.cursor.y = self.visible_bounds(screen_size).min_y;
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_page_down(&mut self, screen_size: (u16, u16)) {
        self.cursor.y = self.visible_bounds(screen_size).max_y;
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn paint_char(&mut self, ch: char) {
        if ch.is_control() {
            return;
        }
        let fg = self
            .snapshot
            .your_color
            .unwrap_or_else(|| RgbColor::new(255, 196, 64));
        let op = CanvasOp::PaintCell {
            pos: self.cursor,
            ch,
            fg,
        };
        self.snapshot.canvas.apply(&op);
        self.svc.submit_op(op);
    }

    pub fn clear_at_cursor(&mut self) {
        let op = CanvasOp::ClearCell { pos: self.cursor };
        self.snapshot.canvas.apply(&op);
        self.svc.submit_op(op);
    }

    pub fn backspace(&mut self, screen_size: (u16, u16)) {
        if self.cursor.x > 0 {
            self.cursor.x -= 1;
        }
        self.clear_at_cursor();
        self.scroll_viewport_to_cursor(screen_size);
    }

    /// Paste bracketed-paste bytes into the canvas starting at the cursor.
    /// Printable chars emit `PaintCell` ops and advance x; `\n` (and
    /// normalized `\r\n`) wrap x back to the column where the paste began
    /// and advance y by one. Stops when either axis runs off the canvas.
    pub fn paste_bytes(&mut self, bytes: &[u8], screen_size: (u16, u16)) {
        let start_x = self.cursor.x;
        let width = self.snapshot.canvas.width;
        let height = self.snapshot.canvas.height;
        let fg = self
            .snapshot
            .your_color
            .unwrap_or_else(|| RgbColor::new(255, 196, 64));

        let text = match std::str::from_utf8(bytes) {
            Ok(s) => s,
            // Bracketed paste payloads should be UTF-8; on anything else, do
            // nothing rather than inserting mojibake.
            Err(_) => return,
        };

        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if self.cursor.y >= height {
                break;
            }
            match ch {
                '\r' => {
                    if matches!(chars.peek(), Some('\n')) {
                        chars.next();
                    }
                    self.cursor.x = start_x;
                    self.cursor.y += 1;
                }
                '\n' => {
                    self.cursor.x = start_x;
                    self.cursor.y += 1;
                }
                ch if ch.is_control() => {
                    // Drop other control bytes — they have no paint semantics.
                }
                _ => {
                    if self.cursor.x >= width {
                        continue;
                    }
                    let op = CanvasOp::PaintCell {
                        pos: self.cursor,
                        ch,
                        fg,
                    };
                    self.snapshot.canvas.apply(&op);
                    self.svc.submit_op(op);
                    self.cursor.x += 1;
                }
            }
        }

        if self.cursor.y >= height {
            self.cursor.y = height.saturating_sub(1);
        }
        if self.cursor.x >= width {
            self.cursor.x = width.saturating_sub(1);
        }
        self.scroll_viewport_to_cursor(screen_size);
    }

    pub fn move_to_screen_point(&mut self, screen_size: (u16, u16), x: u16, y: u16) -> bool {
        let viewport = super::ui::canvas_area_for_screen(screen_size);
        if x < viewport.x || y < viewport.y || x >= viewport.right() || y >= viewport.bottom() {
            return false;
        }
        let next = Pos {
            x: self.viewport_origin.x + (x - viewport.x) as usize,
            y: self.viewport_origin.y + (y - viewport.y) as usize,
        };
        if next.x >= self.snapshot.canvas.width || next.y >= self.snapshot.canvas.height {
            return false;
        }
        self.cursor = next;
        true
    }

    fn scroll_viewport_to_cursor(&mut self, screen_size: (u16, u16)) {
        let viewport = super::ui::canvas_area_for_screen(screen_size);
        let width = viewport.width.max(1) as usize;
        let height = viewport.height.max(1) as usize;

        if self.cursor.x < self.viewport_origin.x {
            self.viewport_origin.x = self.cursor.x;
        } else if self.cursor.x >= self.viewport_origin.x + width {
            self.viewport_origin.x = self.cursor.x + 1 - width;
        }

        if self.cursor.y < self.viewport_origin.y {
            self.viewport_origin.y = self.cursor.y;
        } else if self.cursor.y >= self.viewport_origin.y + height {
            self.viewport_origin.y = self.cursor.y + 1 - height;
        }

        self.clamp_to_viewport(viewport);
    }

    fn clamp_to_viewport(&mut self, viewport: Rect) {
        let width = viewport.width.max(1) as usize;
        let height = viewport.height.max(1) as usize;
        let max_origin_x = self.snapshot.canvas.width.saturating_sub(width);
        let max_origin_y = self.snapshot.canvas.height.saturating_sub(height);
        self.viewport_origin.x = self.viewport_origin.x.min(max_origin_x);
        self.viewport_origin.y = self.viewport_origin.y.min(max_origin_y);
        let bounds = Bounds {
            min_x: self
                .viewport_origin
                .x
                .min(self.snapshot.canvas.width.saturating_sub(1)),
            max_x: (self.viewport_origin.x + width.saturating_sub(1))
                .min(self.snapshot.canvas.width.saturating_sub(1)),
            min_y: self
                .viewport_origin
                .y
                .min(self.snapshot.canvas.height.saturating_sub(1)),
            max_y: (self.viewport_origin.y + height.saturating_sub(1))
                .min(self.snapshot.canvas.height.saturating_sub(1)),
        };
        self.cursor.x = self.cursor.x.clamp(bounds.min_x, bounds.max_x);
        self.cursor.y = self.cursor.y.clamp(bounds.min_y, bounds.max_y);
    }

    pub(crate) fn visible_bounds(&self, screen_size: (u16, u16)) -> Bounds {
        let viewport = super::ui::canvas_area_for_screen(screen_size);
        let width = viewport.width.max(1) as usize;
        let height = viewport.height.max(1) as usize;
        let min_x = self
            .viewport_origin
            .x
            .min(self.snapshot.canvas.width.saturating_sub(1));
        let min_y = self
            .viewport_origin
            .y
            .min(self.snapshot.canvas.height.saturating_sub(1));
        let max_x = (self.viewport_origin.x + width.saturating_sub(1))
            .min(self.snapshot.canvas.width.saturating_sub(1));
        let max_y = (self.viewport_origin.y + height.saturating_sub(1))
            .min(self.snapshot.canvas.height.saturating_sub(1));
        Bounds {
            min_x,
            max_x,
            min_y,
            max_y,
        }
    }
}

pub(crate) struct Bounds {
    pub min_x: usize,
    pub max_x: usize,
    pub min_y: usize,
    pub max_y: usize,
}
