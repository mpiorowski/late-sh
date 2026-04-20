use dartboard_core::{Canvas, CanvasOp, CellValue, CellWrite, Pos, RgbColor};
use dartboard_tui::{FloatingView, SelectionShape, SelectionView};
use ratatui::layout::Rect;
use std::collections::VecDeque;
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
    active_brush: Option<Brush>,
    recent_brushes: VecDeque<Brush>,
    drag_brush: Option<Brush>,
    selection: Option<LocalSelection>,
    floating: Option<FloatingSelection>,
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
            active_brush: None,
            recent_brushes: VecDeque::new(),
            drag_brush: None,
            selection: None,
            floating: None,
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
        self.apply_brush(Brush::for_typed_char(ch));
    }

    pub fn type_char(&mut self, ch: char, screen_size: (u16, u16)) {
        let brush = Brush::for_typed_char(ch);
        self.apply_brush(brush);
        self.set_active_brush(brush);
        if self.cursor.x + 1 < self.snapshot.canvas.width {
            self.cursor.x += 1;
        }
        self.scroll_viewport_to_cursor(screen_size);
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
        let Some(next) = canvas_pos_for_screen_point(
            viewport,
            self.viewport_origin,
            self.snapshot.canvas.width,
            self.snapshot.canvas.height,
            x,
            y,
        ) else {
            return false;
        };
        if next.x >= self.snapshot.canvas.width || next.y >= self.snapshot.canvas.height {
            return false;
        }
        self.cursor = next;
        true
    }

    pub fn begin_drag_brush_from_cursor(&mut self) {
        self.drag_brush = self.active_brush.or_else(|| {
            self.snapshot
                .canvas
                .glyph_at(self.cursor)
                .map(|glyph| Brush::Glyph(glyph.ch))
        });
    }

    pub fn paint_drag_brush(&mut self) -> bool {
        let Some(brush) = self.drag_brush else {
            return false;
        };
        self.apply_brush(brush);
        true
    }

    pub fn clear_drag_brush(&mut self) {
        self.drag_brush = None;
    }

    pub fn begin_selection_from_cursor(&mut self) {
        self.selection = Some(LocalSelection {
            anchor: self.cursor,
            cursor: self.cursor,
        });
    }

    pub fn update_selection_to_cursor(&mut self) -> bool {
        let Some(selection) = &mut self.selection else {
            return false;
        };
        selection.cursor = self.cursor;
        true
    }

    pub fn selection_view(&self) -> Option<SelectionView> {
        self.selection.map(|selection| SelectionView {
            anchor: selection.anchor,
            cursor: selection.cursor,
            shape: SelectionShape::Rect,
        })
    }

    pub fn floating_view(&self) -> Option<FloatingView<'_>> {
        self.floating.as_ref().map(|floating| FloatingView {
            width: floating.width,
            height: floating.height,
            cells: &floating.cells,
            anchor: self.cursor,
            transparent: false,
            active_color: self.active_user_color(),
        })
    }

    pub fn canvas_for_render(&self) -> Option<Canvas> {
        let floating = self.floating.as_ref()?;
        let mut canvas = self.snapshot.canvas.clone();
        clear_bounds_on(&mut canvas, floating.source_bounds());
        Some(canvas)
    }

    pub fn export_system_clipboard_text(&self) -> String {
        match self.selection {
            Some(selection) => self.export_selection_as_text(selection),
            None => self.export_bounds_as_text(self.full_canvas_bounds()),
        }
    }

    pub fn lift_selection_to_floating(&mut self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        let floating = self.capture_selection(selection);
        self.cursor = Pos {
            x: floating.source_bounds().min_x,
            y: floating.source_bounds().min_y,
        };
        self.drag_brush = None;
        self.selection = None;
        self.floating = Some(floating);
        true
    }

    pub fn commit_floating(&mut self) -> bool {
        let Some(floating) = self.floating.take() else {
            return false;
        };

        let mut writes = Vec::with_capacity(
            floating.width * floating.height
                + floating.source_bounds().width() * floating.source_bounds().height(),
        );
        for y in floating.source_bounds().min_y..=floating.source_bounds().max_y {
            for x in floating.source_bounds().min_x..=floating.source_bounds().max_x {
                writes.push(CellWrite::Clear { pos: Pos { x, y } });
            }
        }

        let color = self.active_user_color();
        for cy in 0..floating.height {
            for cx in 0..floating.width {
                let target = Pos {
                    x: self.cursor.x + cx,
                    y: self.cursor.y + cy,
                };
                if target.x >= self.snapshot.canvas.width || target.y >= self.snapshot.canvas.height
                {
                    continue;
                }
                match floating.cells[cy * floating.width + cx] {
                    Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) => {
                        writes.push(CellWrite::Paint {
                            pos: target,
                            ch,
                            fg: color,
                        });
                    }
                    Some(CellValue::WideCont) => {}
                    None => writes.push(CellWrite::Clear { pos: target }),
                }
            }
        }

        let op = CanvasOp::PaintRegion { cells: writes };
        self.snapshot.canvas.apply(&op);
        self.svc.submit_op(op);
        true
    }

    pub fn dismiss_floating(&mut self) -> bool {
        let Some(floating) = self.floating.take() else {
            return false;
        };
        self.selection = Some(floating.source_selection);
        self.cursor = floating.source_selection.cursor;
        true
    }

    pub fn has_floating(&self) -> bool {
        self.floating.is_some()
    }

    pub fn clear_local_state(&mut self) {
        self.active_brush = None;
        self.drag_brush = None;
        self.selection = None;
        self.floating = None;
    }

    pub fn active_brush(&self) -> Option<Brush> {
        self.active_brush
    }

    pub fn recent_brushes(&self) -> impl Iterator<Item = Brush> + '_ {
        self.recent_brushes.iter().copied()
    }

    fn active_user_color(&self) -> RgbColor {
        self.snapshot
            .your_color
            .unwrap_or_else(|| RgbColor::new(255, 196, 64))
    }

    fn apply_brush(&mut self, brush: Brush) {
        match brush {
            Brush::Glyph(ch) => {
                if ch.is_control() {
                    return;
                }
                let fg = self.active_user_color();
                let op = CanvasOp::PaintCell {
                    pos: self.cursor,
                    ch,
                    fg,
                };
                self.snapshot.canvas.apply(&op);
                self.svc.submit_op(op);
            }
            Brush::Erase => self.clear_at_cursor(),
        }
    }

    fn set_active_brush(&mut self, brush: Brush) {
        self.active_brush = Some(brush);
        self.remember_brush(brush);
    }

    fn remember_brush(&mut self, brush: Brush) {
        if let Some(idx) = self
            .recent_brushes
            .iter()
            .position(|existing| *existing == brush)
        {
            self.recent_brushes.remove(idx);
        }
        self.recent_brushes.push_front(brush);
        while self.recent_brushes.len() > 6 {
            self.recent_brushes.pop_back();
        }
    }

    fn full_canvas_bounds(&self) -> Bounds {
        Bounds {
            min_x: 0,
            max_x: self.snapshot.canvas.width.saturating_sub(1),
            min_y: 0,
            max_y: self.snapshot.canvas.height.saturating_sub(1),
        }
    }

    fn capture_selection(&self, selection: LocalSelection) -> FloatingSelection {
        let bounds = selection
            .bounds()
            .normalized_for_canvas(&self.snapshot.canvas);
        let mut cells = Vec::with_capacity(bounds.width() * bounds.height());
        for y in bounds.min_y..=bounds.max_y {
            for x in bounds.min_x..=bounds.max_x {
                cells.push(self.snapshot.canvas.cell(Pos { x, y }));
            }
        }
        FloatingSelection {
            width: bounds.width(),
            height: bounds.height(),
            cells,
            source_selection: selection,
        }
    }

    fn export_bounds_as_text(&self, bounds: Bounds) -> String {
        let mut text = String::with_capacity(bounds.width() * bounds.height() + bounds.height());
        for y in bounds.min_y..=bounds.max_y {
            for x in bounds.min_x..=bounds.max_x {
                match self.snapshot.canvas.cell(Pos { x, y }) {
                    Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) => text.push(ch),
                    Some(CellValue::WideCont) => {}
                    None => text.push(' '),
                }
            }
            if y != bounds.max_y {
                text.push('\n');
            }
        }
        text
    }

    fn export_selection_as_text(&self, selection: LocalSelection) -> String {
        self.export_bounds_as_text(
            selection
                .bounds()
                .normalized_for_canvas(&self.snapshot.canvas),
        )
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Bounds {
    pub min_x: usize,
    pub max_x: usize,
    pub min_y: usize,
    pub max_y: usize,
}

impl Bounds {
    fn width(self) -> usize {
        self.max_x.saturating_sub(self.min_x).saturating_add(1)
    }

    fn height(self) -> usize {
        self.max_y.saturating_sub(self.min_y).saturating_add(1)
    }

    fn normalized_for_canvas(self, canvas: &Canvas) -> Self {
        Self {
            min_x: self.min_x.min(canvas.width.saturating_sub(1)),
            max_x: self.max_x.min(canvas.width.saturating_sub(1)),
            min_y: self.min_y.min(canvas.height.saturating_sub(1)),
            max_y: self.max_y.min(canvas.height.saturating_sub(1)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Brush {
    Glyph(char),
    Erase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocalSelection {
    anchor: Pos,
    cursor: Pos,
}

impl LocalSelection {
    fn bounds(self) -> Bounds {
        Bounds {
            min_x: self.anchor.x.min(self.cursor.x),
            max_x: self.anchor.x.max(self.cursor.x),
            min_y: self.anchor.y.min(self.cursor.y),
            max_y: self.anchor.y.max(self.cursor.y),
        }
    }
}

struct FloatingSelection {
    width: usize,
    height: usize,
    cells: Vec<Option<CellValue>>,
    source_selection: LocalSelection,
}

impl FloatingSelection {
    fn source_bounds(&self) -> Bounds {
        self.source_selection.bounds()
    }
}

fn clear_bounds_on(canvas: &mut Canvas, bounds: Bounds) {
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            canvas.clear(Pos { x, y });
        }
    }
}

impl Brush {
    fn for_typed_char(ch: char) -> Self {
        if ch == ' ' {
            Self::Erase
        } else {
            Self::Glyph(ch)
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Glyph(ch) => format!("'{ch}'"),
            Self::Erase => "erase".to_string(),
        }
    }
}

fn canvas_pos_for_screen_point(
    viewport: Rect,
    viewport_origin: Pos,
    canvas_width: usize,
    canvas_height: usize,
    sgr_x: u16,
    sgr_y: u16,
) -> Option<Pos> {
    let screen_x = sgr_x.checked_sub(1)?;
    let screen_y = sgr_y.checked_sub(1)?;
    if screen_x < viewport.x
        || screen_y < viewport.y
        || screen_x >= viewport.right()
        || screen_y >= viewport.bottom()
    {
        return None;
    }
    let next = Pos {
        x: viewport_origin.x + (screen_x - viewport.x) as usize,
        y: viewport_origin.y + (screen_y - viewport.y) as usize,
    };
    if next.x >= canvas_width || next.y >= canvas_height {
        return None;
    }
    Some(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dartboard_local::InMemStore;
    use std::{
        thread,
        time::{Duration, Instant},
    };
    use uuid::Uuid;

    fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(value) = check() {
                return value;
            }
            assert!(
                Instant::now() < deadline,
                "condition not met before timeout"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn test_state() -> State {
        let server = dartboard_local::ServerHandle::spawn_local(InMemStore);
        let svc = DartboardService::new(server, Uuid::now_v7(), "painter");
        let rx = svc.subscribe_state();
        wait_for(|| rx.borrow().your_user_id.is_some().then_some(()));
        let mut state = State::new(svc);
        state.tick();
        state.set_viewport_for_screen((80, 24));
        state
    }

    #[test]
    fn screen_point_conversion_uses_sgr_one_based_coords() {
        let viewport = Rect::new(1, 1, 50, 22);
        let pos = canvas_pos_for_screen_point(viewport, Pos { x: 0, y: 0 }, 120, 60, 2, 2);
        assert_eq!(pos, Some(Pos { x: 0, y: 0 }));
    }

    #[test]
    fn screen_point_conversion_respects_viewport_origin() {
        let viewport = Rect::new(1, 1, 50, 22);
        let pos = canvas_pos_for_screen_point(viewport, Pos { x: 10, y: 5 }, 120, 60, 12, 8);
        assert_eq!(pos, Some(Pos { x: 20, y: 11 }));
    }

    #[test]
    fn screen_point_conversion_rejects_points_outside_canvas() {
        let viewport = Rect::new(1, 1, 50, 22);
        assert_eq!(
            canvas_pos_for_screen_point(viewport, Pos { x: 0, y: 0 }, 4, 4, 10, 10),
            None
        );
    }

    #[test]
    fn type_char_advances_cursor_right() {
        let mut state = test_state();
        state.type_char('A', (80, 24));
        assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'A');
        assert_eq!(state.cursor, Pos { x: 1, y: 0 });
    }

    #[test]
    fn drag_brush_samples_clicked_glyph_and_paints_without_advancing() {
        let mut state = test_state();
        state.paint_char('B');
        state.begin_drag_brush_from_cursor();
        state.move_right((80, 24));
        assert!(state.paint_drag_brush());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), 'B');
        assert_eq!(state.cursor, Pos { x: 1, y: 0 });
        state.clear_drag_brush();
        state.move_right((80, 24));
        assert!(!state.paint_drag_brush());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 0 }), ' ');
    }

    #[test]
    fn active_brush_overrides_sampled_drag_brush() {
        let mut state = test_state();
        state.type_char('A', (80, 24));
        state.move_right((80, 24));
        state.paint_char('Z');
        state.begin_drag_brush_from_cursor();
        assert!(state.paint_drag_brush());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 0 }), 'A');
    }

    #[test]
    fn escape_clears_active_and_drag_brushes() {
        let mut state = test_state();
        state.type_char('Q', (80, 24));
        state.begin_drag_brush_from_cursor();
        state.begin_selection_from_cursor();
        state.clear_local_state();
        assert_eq!(state.active_brush(), None);
        state.move_right((80, 24));
        assert!(!state.paint_drag_brush());
        assert!(state.selection_view().is_none());
    }

    #[test]
    fn selection_tracks_anchor_and_drag_cursor() {
        let mut state = test_state();
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.move_down((80, 24));
        assert!(state.update_selection_to_cursor());
        let selection = state.selection_view().expect("selection should exist");
        assert_eq!(selection.anchor, Pos { x: 0, y: 0 });
        assert_eq!(selection.cursor, Pos { x: 1, y: 1 });
        assert!(matches!(selection.shape, SelectionShape::Rect));
    }

    #[test]
    fn system_clipboard_export_uses_selection_when_present() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(3, 2);
        state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');
        state.snapshot.canvas.set(Pos { x: 1, y: 0 }, 'B');
        state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'D');
        state.cursor = Pos { x: 1, y: 0 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.move_down((80, 24));
        state.update_selection_to_cursor();

        assert_eq!(state.export_system_clipboard_text(), "B \nD ");
    }

    #[test]
    fn system_clipboard_export_uses_full_canvas_without_selection() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(3, 2);
        state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');
        state.snapshot.canvas.set(Pos { x: 1, y: 0 }, 'B');
        state.snapshot.canvas.set(Pos { x: 0, y: 1 }, 'C');
        state.snapshot.canvas.set(Pos { x: 2, y: 1 }, 'D');

        assert_eq!(state.export_system_clipboard_text(), "AB \nC D");
    }

    #[test]
    fn dismissing_floating_restores_original_selection() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(4, 2);
        state.cursor = Pos { x: 1, y: 0 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.update_selection_to_cursor();
        assert!(state.lift_selection_to_floating());
        state.cursor = Pos { x: 0, y: 1 };

        assert!(state.dismiss_floating());

        let selection = state.selection_view().expect("selection restored");
        assert_eq!(selection.anchor, Pos { x: 1, y: 0 });
        assert_eq!(selection.cursor, Pos { x: 2, y: 0 });
        assert_eq!(state.cursor, Pos { x: 2, y: 0 });
    }

    #[test]
    fn commit_floating_moves_selected_region() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(5, 3);
        state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'A');
        state.snapshot.canvas.set(Pos { x: 2, y: 1 }, 'B');
        state.cursor = Pos { x: 1, y: 1 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.update_selection_to_cursor();
        assert!(state.lift_selection_to_floating());

        state.cursor = Pos { x: 0, y: 0 };
        assert!(state.commit_floating());

        assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'A');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), 'B');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 1 }), ' ');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 1 }), ' ');
        assert!(!state.has_floating());
    }
}
