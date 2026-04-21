use dartboard_core::{Canvas, Pos, RgbColor};
use dartboard_editor::{
    AppKey, AppModifiers, AppPointerEvent, Bounds, EditorAction, EditorContext, EditorKeyDispatch,
    EditorPointerDispatch, EditorSession, FloatingSelection as EditorFloatingSelection, KeyMap,
    Mode as EditorMode, MoveDir, SWATCH_CAPACITY, Selection as EditorSelection,
    SelectionShape as EditorSelectionShape, Swatch, SwatchActivation, Viewport,
    backspace as editor_backspace, capture_bounds, capture_selection, delete_at_cursor,
    diff_canvas_op, dismiss_floating as editor_dismiss_floating,
    export_system_clipboard_text as editor_export_system_clipboard_text,
    handle_editor_action as editor_handle_action, handle_editor_pointer as editor_handle_pointer,
    insert_char as editor_insert_char, paste_text_block, stamp_floating as editor_stamp_floating,
};
use dartboard_tui::{FloatingView, SelectionShape as TuiSelectionShape, SelectionView};
use ratatui::layout::Rect;
use std::time::Instant;
use tokio::sync::{
    broadcast::{self, error::TryRecvError},
    watch,
};

use dartboard_picker_core::IconCatalogData;

use super::glyph_picker;
use super::svc::{DartboardEvent, DartboardService, DartboardSnapshot};

const DOUBLE_CLICK_WINDOW_MS: u128 = 400;

pub struct State {
    pub snapshot: DartboardSnapshot,
    pub private_notice: Option<String>,
    #[allow(dead_code)]
    pub(crate) svc: DartboardService,
    pub(crate) editor: EditorSession,
    active_brush: Option<Brush>,
    drag_brush: Option<Brush>,
    floating_source_selection: Option<EditorSelection>,
    suppress_swatch_preview: bool,
    last_canvas_click: Option<(Instant, Pos)>,
    help_open: bool,
    help_tab: HelpTab,
    help_scroll: u16,
    glyph_picker: glyph_picker::State,
    glyph_picker_open: bool,
    glyph_catalog: Option<IconCatalogData>,
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
            editor: EditorSession::default(),
            active_brush: None,
            drag_brush: None,
            floating_source_selection: None,
            suppress_swatch_preview: false,
            last_canvas_click: None,
            help_open: false,
            help_tab: HelpTab::default(),
            help_scroll: 0,
            glyph_picker: glyph_picker::State::default(),
            glyph_picker_open: false,
            glyph_catalog: None,
            snapshot_rx,
            event_rx,
        }
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.editor.clamp_cursor(&self.snapshot.canvas);
            self.editor.clamp_viewport_origin(&self.snapshot.canvas);
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

    pub fn cursor(&self) -> Pos {
        self.editor.cursor
    }

    pub fn viewport_origin(&self) -> Pos {
        self.editor.viewport_origin
    }

    pub fn set_viewport_for_screen(&mut self, screen_size: (u16, u16)) {
        let viewport = super::ui::canvas_area_for_screen(screen_size);
        self.editor
            .set_viewport(viewport_to_editor(viewport), &self.snapshot.canvas);
    }

    pub fn move_left(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor.move_left(&self.snapshot.canvas);
    }

    pub fn move_right(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor.move_right(&self.snapshot.canvas);
    }

    pub fn move_up(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor.move_up(&self.snapshot.canvas);
    }

    pub fn move_down(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor.move_down(&self.snapshot.canvas);
    }

    pub fn move_home(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor
            .move_dir(&self.snapshot.canvas, MoveDir::LineStart);
    }

    pub fn move_end(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor
            .move_dir(&self.snapshot.canvas, MoveDir::LineEnd);
    }

    pub fn move_page_up(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor.move_dir(&self.snapshot.canvas, MoveDir::PageUp);
    }

    pub fn move_page_down(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        self.editor
            .move_dir(&self.snapshot.canvas, MoveDir::PageDown);
    }

    pub fn paint_char(&mut self, ch: char) {
        self.apply_brush(Brush::for_typed_char(ch));
    }

    pub fn type_char(&mut self, ch: char, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        let brush = Brush::for_typed_char(ch);
        match brush {
            Brush::Glyph(ch) => {
                let _ = self.edit_canvas(|editor, canvas, color| {
                    editor_insert_char(editor, canvas, ch, color)
                });
            }
            Brush::Erase => {
                let _ = self.edit_canvas(|editor, canvas, _| delete_at_cursor(editor, canvas));
                self.editor.move_right(&self.snapshot.canvas);
            }
        }
    }

    pub fn clear_at_cursor(&mut self) {
        let _ = self.edit_canvas(|editor, canvas, _| delete_at_cursor(editor, canvas));
    }

    pub fn handle_app_key(&mut self, key: AppKey) -> EditorKeyDispatch {
        if key.code == dartboard_editor::AppKeyCode::Esc
            && key.modifiers == AppModifiers::default()
            && self.dismiss_active_brush()
        {
            return EditorKeyDispatch {
                handled: true,
                effects: Vec::new(),
            };
        }
        let action = KeyMap::default_standalone().resolve(
            key,
            EditorContext {
                mode: self.editor.mode,
                has_selection_anchor: self.editor.selection_anchor.is_some(),
                is_floating: self.editor.floating.is_some(),
            },
        );
        if self.editor.floating.is_some() {
            match self.apply_floating_override(action) {
                FloatingOverride::Consumed(dispatch) => return dispatch,
                FloatingOverride::PassThrough => {}
                FloatingOverride::DismissAndContinue => {
                    let _ = self.dismiss_floating();
                }
            }
        }

        let Some(action) = action else {
            return EditorKeyDispatch::default();
        };
        self.handle_editor_action(action)
    }

    pub fn handle_editor_action(&mut self, action: EditorAction) -> EditorKeyDispatch {
        let before = self.snapshot.canvas.clone();
        let color = self.active_user_color();
        let dispatch =
            editor_handle_action(&mut self.editor, &mut self.snapshot.canvas, action, color);
        self.sync_floating_source_selection();

        if self.snapshot.canvas != before {
            let _ = self.submit_canvas_diff(before);
        }

        dispatch
    }

    pub fn handle_pointer_event(&mut self, pointer: AppPointerEvent) -> EditorPointerDispatch {
        let before = self.snapshot.canvas.clone();
        let had_floating = self.editor.floating.is_some();
        let had_local_floating = self.floating_source_selection.is_some();
        let pointer_over_canvas = self
            .editor
            .canvas_pos_for_pointer(pointer.column, pointer.row, &self.snapshot.canvas)
            .is_some();
        let color = self.active_user_color();
        let dispatch =
            editor_handle_pointer(&mut self.editor, &mut self.snapshot.canvas, pointer, color);
        self.sync_floating_source_selection();
        if had_floating && had_local_floating && self.editor.floating.is_none() {
            self.restore_floating_source_selection();
        }
        if self.editor.floating.is_none() || pointer_over_canvas {
            self.suppress_swatch_preview = false;
        }
        if self.snapshot.canvas != before {
            let _ = self.submit_canvas_diff(before);
        }
        dispatch
    }

    pub fn backspace(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        let _ = self.edit_canvas(|editor, canvas, _| editor_backspace(editor, canvas));
    }

    pub fn paste_bytes(&mut self, bytes: &[u8], screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => return,
        };

        let start = self.editor.cursor;
        let _ =
            self.edit_canvas(|editor, canvas, color| paste_text_block(editor, canvas, text, color));
        self.editor.cursor = paste_cursor_end(
            start,
            text,
            self.snapshot.canvas.width,
            self.snapshot.canvas.height,
        );
        self.editor.scroll_viewport_to_cursor(&self.snapshot.canvas);
    }

    pub fn move_to_screen_point(&mut self, screen_size: (u16, u16), x: u16, y: u16) -> bool {
        self.set_viewport_for_screen(screen_size);
        let Some(next) = self.canvas_pos_for_screen_point(screen_size, x, y) else {
            return false;
        };
        if next.x >= self.snapshot.canvas.width || next.y >= self.snapshot.canvas.height {
            return false;
        }
        self.editor.cursor = next;
        true
    }

    pub fn canvas_pos_for_screen_point(
        &self,
        screen_size: (u16, u16),
        x: u16,
        y: u16,
    ) -> Option<Pos> {
        let viewport = super::ui::canvas_area_for_screen(screen_size);
        canvas_pos_for_screen_point(
            viewport,
            self.editor.viewport_origin,
            self.snapshot.canvas.width,
            self.snapshot.canvas.height,
            x,
            y,
        )
    }

    pub fn begin_drag_brush_from_cursor(&mut self) {
        self.drag_brush = self.active_brush;
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
        self.editor.begin_selection();
    }

    pub fn update_selection_to_cursor(&mut self) -> bool {
        self.editor.selection_anchor.is_some()
    }

    pub fn selection_view(&self) -> Option<SelectionView> {
        self.editor.selection().map(|selection| SelectionView {
            anchor: selection.anchor,
            cursor: selection.cursor,
            shape: selection_shape_to_tui(selection.shape),
        })
    }

    pub fn floating_view(&self) -> Option<FloatingView<'_>> {
        if self.swatch_preview_suppressed() {
            return None;
        }
        self.editor.floating.as_ref().map(|floating| FloatingView {
            width: floating.clipboard.width,
            height: floating.clipboard.height,
            cells: floating.clipboard.cells(),
            anchor: self.editor.cursor,
            transparent: floating.transparent,
            active_color: self.active_user_color(),
        })
    }

    pub fn canvas_for_render(&self) -> Option<Canvas> {
        let floating = self.editor.floating.as_ref()?;
        let mut canvas = self.snapshot.canvas.clone();
        if !floating.transparent
            && let Some(selection) = self.floating_source_selection
        {
            clear_bounds_on(
                &mut canvas,
                selection
                    .bounds()
                    .normalized_for_canvas(&self.snapshot.canvas),
            );
        }
        Some(canvas)
    }

    pub fn should_show_canvas_cursor(&self) -> bool {
        !self.help_open && !self.swatch_preview_suppressed()
    }

    pub fn export_system_clipboard_text(&self) -> String {
        editor_export_system_clipboard_text(&self.editor, &self.snapshot.canvas)
    }

    pub fn lift_selection_to_floating(&mut self) -> bool {
        let Some(selection) = self.editor.selection() else {
            return false;
        };
        let clipboard = capture_selection(&self.snapshot.canvas, selection);
        let bounds = selection
            .bounds()
            .normalized_for_canvas(&self.snapshot.canvas);
        self.editor.cursor = Pos {
            x: bounds.min_x,
            y: bounds.min_y,
        };
        self.drag_brush = None;
        self.editor.clear_selection();
        self.editor.floating = Some(EditorFloatingSelection {
            clipboard,
            transparent: false,
            source_index: None,
        });
        self.floating_source_selection = Some(selection);
        self.suppress_swatch_preview = false;
        true
    }

    pub fn commit_floating(&mut self) -> bool {
        let Some(floating) = self.editor.floating.clone() else {
            return false;
        };
        let was_temp_brush =
            self.active_brush.is_some() && self.floating_source_selection.is_none();

        let before = self.snapshot.canvas.clone();
        if let Some(selection) = self.floating_source_selection {
            clear_bounds_on(
                &mut self.snapshot.canvas,
                selection.bounds().normalized_for_canvas(&before),
            );
        }
        let color = self.active_user_color();
        dartboard_editor::stamp_clipboard(
            &mut self.snapshot.canvas,
            &floating.clipboard,
            self.editor.cursor,
            color,
            floating.transparent,
        );
        let _ = self.submit_canvas_diff(before);
        editor_dismiss_floating(&mut self.editor);
        self.floating_source_selection = None;
        if was_temp_brush {
            self.active_brush = None;
        }
        self.suppress_swatch_preview = false;
        true
    }

    pub fn dismiss_floating(&mut self) -> bool {
        if self.editor.floating.is_none() {
            return false;
        }

        let was_temp_brush =
            self.active_brush.is_some() && self.floating_source_selection.is_none();
        editor_dismiss_floating(&mut self.editor);
        if let Some(selection) = self.floating_source_selection.take() {
            self.editor.selection_anchor = Some(selection.anchor);
            self.editor.selection_shape = selection.shape;
            self.editor.mode = EditorMode::Select;
            self.editor.cursor = selection.cursor;
        }
        if was_temp_brush {
            self.active_brush = None;
        }
        self.suppress_swatch_preview = false;
        true
    }

    pub fn has_floating(&self) -> bool {
        self.editor.floating.is_some()
    }

    pub fn clear_local_state(&mut self) {
        self.active_brush = None;
        self.drag_brush = None;
        self.editor.clear_selection();
        editor_dismiss_floating(&mut self.editor);
        self.floating_source_selection = None;
        self.suppress_swatch_preview = false;
        self.last_canvas_click = None;
    }

    pub fn active_brush(&self) -> Option<Brush> {
        self.active_brush
    }

    pub fn brush_mode(&self) -> BrushMode {
        if self.active_swatch_index().is_some() {
            BrushMode::Swatch
        } else if let Some(Brush::Glyph(ch)) = self.active_brush {
            BrushMode::Glyph(ch)
        } else {
            BrushMode::None
        }
    }

    pub fn swatches(&self) -> &[Option<Swatch>; SWATCH_CAPACITY] {
        &self.editor.swatches
    }

    pub fn active_swatch_index(&self) -> Option<usize> {
        self.editor
            .floating
            .as_ref()
            .and_then(|floating| floating.source_index)
    }

    pub fn floating_is_transparent(&self) -> bool {
        self.editor
            .floating
            .as_ref()
            .map(|floating| floating.transparent)
            .unwrap_or(false)
    }

    pub fn activate_swatch(&mut self, idx: usize) {
        let activation = self.editor.activate_swatch(idx);
        self.active_brush = None;
        if activation == SwatchActivation::ActivatedFloating {
            self.suppress_swatch_preview = true;
        }
        self.sync_floating_source_selection();
    }

    pub fn toggle_swatch_pin(&mut self, idx: usize) {
        self.editor.toggle_pin(idx);
    }

    pub fn clear_swatch(&mut self, idx: usize) {
        self.editor.clear_swatch(idx);
        self.suppress_swatch_preview = false;
    }

    pub fn is_in_normal_brush_mode(&self) -> bool {
        self.editor.floating.is_none() && self.active_brush.is_none()
    }

    pub fn register_canvas_click(&mut self, pos: Pos) -> bool {
        let now = Instant::now();
        let is_double = match self.last_canvas_click {
            Some((prev, prev_pos)) => {
                prev_pos == pos && now.duration_since(prev).as_millis() <= DOUBLE_CLICK_WINDOW_MS
            }
            None => false,
        };
        self.last_canvas_click = if is_double { None } else { Some((now, pos)) };
        is_double
    }

    pub fn clear_pending_canvas_click(&mut self) {
        self.last_canvas_click = None;
    }

    pub fn is_help_open(&self) -> bool {
        self.help_open
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }

    pub fn help_tab(&self) -> HelpTab {
        self.help_tab
    }

    pub fn help_scroll(&self) -> u16 {
        self.help_scroll
    }

    pub fn select_next_help_tab(&mut self) {
        self.help_tab = self.help_tab.next();
        self.help_scroll = 0;
    }

    pub fn select_prev_help_tab(&mut self) {
        self.help_tab = self.help_tab.prev();
        self.help_scroll = 0;
    }

    pub fn select_help_tab(&mut self, tab: HelpTab) {
        if self.help_tab != tab {
            self.help_tab = tab;
            self.help_scroll = 0;
        }
    }

    pub fn scroll_help(&mut self, delta: i16) {
        let current = self.help_scroll as i32;
        self.help_scroll = (current + delta as i32).max(0) as u16;
    }

    pub fn reset_help_scroll(&mut self) {
        self.help_scroll = 0;
    }

    pub fn is_glyph_picker_open(&self) -> bool {
        self.glyph_picker_open
    }

    pub fn glyph_picker_state(&self) -> &glyph_picker::State {
        &self.glyph_picker
    }

    pub fn glyph_picker_state_mut(&mut self) -> &mut glyph_picker::State {
        &mut self.glyph_picker
    }

    pub fn glyph_catalog(&self) -> Option<&IconCatalogData> {
        self.glyph_catalog.as_ref()
    }

    pub fn open_glyph_picker(&mut self) {
        // Enforce the "at most one of {selection, floating, picker}" invariant:
        // opening dismisses any floating preview and clears any selection.
        let _ = self.dismiss_floating();
        self.editor.clear_selection();
        self.active_brush = None;
        self.drag_brush = None;
        self.last_canvas_click = None;
        self.suppress_swatch_preview = false;

        if self.glyph_catalog.is_none() {
            self.glyph_catalog = Some(glyph_picker::load_catalog());
        }
        self.glyph_picker = glyph_picker::State::default();
        self.glyph_picker_open = true;
    }

    pub fn close_glyph_picker(&mut self) {
        self.glyph_picker_open = false;
    }

    pub fn glyph_picker_next_tab(&mut self) {
        self.glyph_picker.next_tab();
    }

    pub fn glyph_picker_prev_tab(&mut self) {
        self.glyph_picker.prev_tab();
    }

    pub fn glyph_picker_move_selection(&mut self, delta: isize) {
        let Some(catalog) = self.glyph_catalog.as_ref() else {
            return;
        };
        glyph_picker::move_selection(&mut self.glyph_picker, catalog, delta);
    }

    /// Handle a left-down in the picker list at screen coords (column, row),
    /// 0-based. Returns `true` if this was a double-click (caller should
    /// treat as confirm).
    pub fn glyph_picker_click_list(&mut self, column: u16, row: u16) -> bool {
        let Some(catalog) = self.glyph_catalog.as_ref() else {
            return false;
        };
        glyph_picker::click_list(&mut self.glyph_picker, catalog, column, row)
    }

    /// Handle a left-down in the tab strip at screen column `column`.
    /// Returns `true` if a tab was hit.
    pub fn glyph_picker_click_tab(&mut self, column: u16, row: u16) -> bool {
        let tabs = self.glyph_picker.tabs_inner.get();
        if tabs.height == 0 || row < tabs.y || row >= tabs.y + tabs.height {
            return false;
        }
        if let Some(tab) = glyph_picker::tab_at_x(tabs, column) {
            self.glyph_picker.set_tab(tab);
            true
        } else {
            false
        }
    }

    /// Confirm the selection: paint the leading scalar of the selected glyph
    /// at the cursor using `insert_char` semantics, and close the picker
    /// unless `keep_open` is set. Returns `true` if a glyph was inserted.
    pub fn glyph_picker_insert(&mut self, keep_open: bool, screen_size: (u16, u16)) -> bool {
        let Some(catalog) = self.glyph_catalog.as_ref() else {
            self.glyph_picker_open = false;
            return false;
        };
        let Some(icon) = glyph_picker::selected_glyph(&self.glyph_picker, catalog) else {
            if !keep_open {
                self.glyph_picker_open = false;
            }
            return false;
        };
        let Some(ch) = icon.chars().next() else {
            if !keep_open {
                self.glyph_picker_open = false;
            }
            return false;
        };
        if !keep_open {
            self.glyph_picker_open = false;
        }
        self.set_viewport_for_screen(screen_size);
        if ch.is_control() {
            return false;
        }
        let _ =
            self.edit_canvas(|editor, canvas, color| editor_insert_char(editor, canvas, ch, color));
        true
    }

    pub fn activate_temp_glyph_brush_at(&mut self, pos: Pos) -> bool {
        let Some(glyph) = self.snapshot.canvas.glyph_at(pos) else {
            return false;
        };
        if glyph.ch == ' ' {
            return false;
        }
        self.editor.cursor = pos;
        self.editor.clear_selection();
        self.editor.floating = Some(EditorFloatingSelection {
            clipboard: capture_bounds(&self.snapshot.canvas, Bounds::single(pos)),
            transparent: false,
            source_index: None,
        });
        self.floating_source_selection = None;
        self.active_brush = Some(Brush::Glyph(glyph.ch));
        self.suppress_swatch_preview = false;
        true
    }

    fn active_user_color(&self) -> RgbColor {
        self.snapshot
            .your_color
            .unwrap_or_else(|| RgbColor::new(255, 196, 64))
    }

    fn swatch_preview_suppressed(&self) -> bool {
        self.suppress_swatch_preview
            && self
                .editor
                .floating
                .as_ref()
                .and_then(|floating| floating.source_index)
                .is_some()
    }

    fn edit_canvas(
        &mut self,
        edit: impl FnOnce(&mut EditorSession, &mut Canvas, RgbColor) -> bool,
    ) -> bool {
        let before = self.snapshot.canvas.clone();
        let color = self.active_user_color();
        let changed = edit(&mut self.editor, &mut self.snapshot.canvas, color);
        if !changed {
            return false;
        }
        self.submit_canvas_diff(before)
    }

    fn submit_canvas_diff(&mut self, before: Canvas) -> bool {
        let Some(op) = diff_canvas_op(&before, &self.snapshot.canvas, self.active_user_color())
        else {
            return false;
        };
        self.svc.submit_op(op);
        true
    }

    fn stamp_floating(&mut self) -> bool {
        let _ =
            self.edit_canvas(|editor, canvas, color| editor_stamp_floating(editor, canvas, color));
        true
    }

    fn apply_brush(&mut self, brush: Brush) {
        match brush {
            Brush::Glyph(ch) => {
                if ch.is_control() {
                    return;
                }
                let before = self.snapshot.canvas.clone();
                let _ = self.snapshot.canvas.put_glyph_colored(
                    self.editor.cursor,
                    ch,
                    self.active_user_color(),
                );
                let _ = self.submit_canvas_diff(before);
            }
            Brush::Erase => self.clear_at_cursor(),
        }
    }

    fn dismiss_active_brush(&mut self) -> bool {
        if self.editor.floating.is_some() {
            return self.dismiss_floating();
        }
        if self.active_brush.is_some() {
            self.active_brush = None;
            self.drag_brush = None;
            return true;
        }
        false
    }

    fn apply_floating_override(&mut self, action: Option<EditorAction>) -> FloatingOverride {
        match action {
            Some(EditorAction::PastePrimarySwatch) => {
                self.stamp_floating();
                FloatingOverride::Consumed(EditorKeyDispatch {
                    handled: true,
                    effects: Vec::new(),
                })
            }
            Some(EditorAction::CopySelection) | Some(EditorAction::CutSelection) => {
                FloatingOverride::Consumed(EditorKeyDispatch {
                    handled: true,
                    effects: Vec::new(),
                })
            }
            Some(EditorAction::ClearSelection) => {
                let _ = self.dismiss_floating();
                FloatingOverride::Consumed(EditorKeyDispatch {
                    handled: true,
                    effects: Vec::new(),
                })
            }
            Some(EditorAction::StrokeFloating { .. })
            | Some(EditorAction::Pan { .. })
            | Some(EditorAction::ExportSystemClipboard) => FloatingOverride::PassThrough,
            Some(EditorAction::Move { .. }) => FloatingOverride::PassThrough,
            Some(EditorAction::ActivateSwatch(_)) => FloatingOverride::PassThrough,
            _ => FloatingOverride::DismissAndContinue,
        }
    }

    fn sync_floating_source_selection(&mut self) {
        if self
            .editor
            .floating
            .as_ref()
            .and_then(|floating| floating.source_index)
            .is_some()
        {
            self.floating_source_selection = None;
        }
    }

    fn restore_floating_source_selection(&mut self) {
        if let Some(selection) = self.floating_source_selection.take() {
            self.editor.selection_anchor = Some(selection.anchor);
            self.editor.selection_shape = selection.shape;
            self.editor.mode = EditorMode::Select;
            self.editor.cursor = selection.cursor;
        }
        self.suppress_swatch_preview = false;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Brush {
    Glyph(char),
    Erase,
}

impl Brush {
    fn for_typed_char(ch: char) -> Self {
        if ch == ' ' {
            Self::Erase
        } else {
            Self::Glyph(ch)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrushMode {
    None,
    Swatch,
    Glyph(char),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HelpTab {
    #[default]
    Guide,
    Drawing,
    Selection,
    Clipboard,
    Transform,
    Session,
}

impl HelpTab {
    pub const ALL: [HelpTab; 6] = [
        HelpTab::Guide,
        HelpTab::Drawing,
        HelpTab::Selection,
        HelpTab::Clipboard,
        HelpTab::Transform,
        HelpTab::Session,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HelpTab::Guide => "guide",
            HelpTab::Drawing => "drawing",
            HelpTab::Selection => "selection",
            HelpTab::Clipboard => "clipboard",
            HelpTab::Transform => "transform",
            HelpTab::Session => "session",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }

    pub fn next(self) -> Self {
        let next = (self.index() + 1) % Self::ALL.len();
        Self::ALL[next]
    }

    pub fn prev(self) -> Self {
        let len = Self::ALL.len();
        let prev = (self.index() + len - 1) % len;
        Self::ALL[prev]
    }
}

enum FloatingOverride {
    Consumed(EditorKeyDispatch),
    PassThrough,
    DismissAndContinue,
}

fn viewport_to_editor(viewport: Rect) -> Viewport {
    Viewport {
        x: viewport.x,
        y: viewport.y,
        width: viewport.width,
        height: viewport.height,
    }
}

fn selection_shape_to_tui(shape: EditorSelectionShape) -> TuiSelectionShape {
    match shape {
        EditorSelectionShape::Rect => TuiSelectionShape::Rect,
        EditorSelectionShape::Ellipse => TuiSelectionShape::Ellipse,
    }
}

fn clear_bounds_on(canvas: &mut Canvas, bounds: Bounds) {
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            canvas.clear(Pos { x, y });
        }
    }
}

fn paste_cursor_end(start: Pos, text: &str, width: usize, height: usize) -> Pos {
    let mut cursor = start;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if cursor.y >= height {
            break;
        }
        match ch {
            '\r' => {
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
                cursor.x = start.x;
                cursor.y += 1;
            }
            '\n' => {
                cursor.x = start.x;
                cursor.y += 1;
            }
            ch if ch.is_control() => {}
            ch => {
                if cursor.x < width {
                    cursor.x = cursor.x.saturating_add(Canvas::display_width(ch));
                }
            }
        }
    }

    Pos {
        x: cursor.x.min(width.saturating_sub(1)),
        y: cursor.y.min(height.saturating_sub(1)),
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
    use dartboard_core::CellValue;
    use dartboard_editor::Clipboard;
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
        let server = crate::dartboard::spawn_server();
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
        assert_eq!(state.cursor(), Pos { x: 1, y: 0 });
    }

    #[test]
    fn drag_brush_requires_temp_brush_and_paints_without_advancing() {
        let mut state = test_state();
        state.paint_char('B');
        assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));
        state.begin_drag_brush_from_cursor();
        state.move_right((80, 24));
        assert!(state.paint_drag_brush());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), 'B');
        assert_eq!(state.cursor(), Pos { x: 1, y: 0 });
        state.clear_drag_brush();
        state.move_right((80, 24));
        assert!(!state.paint_drag_brush());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 0 }), ' ');
    }

    #[test]
    fn drag_brush_no_longer_samples_canvas_without_temp_brush() {
        let mut state = test_state();
        state.paint_char('Z');
        state.begin_drag_brush_from_cursor();
        state.move_right((80, 24));
        assert!(!state.paint_drag_brush());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), ' ');
    }

    #[test]
    fn escape_clears_active_and_drag_brushes() {
        let mut state = test_state();
        state.type_char('Q', (80, 24));
        assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));
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
        assert!(matches!(selection.shape, TuiSelectionShape::Rect));
    }

    #[test]
    fn app_key_char_fills_active_selection_via_shared_executor() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(3, 2);
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.move_down((80, 24));

        let dispatch = state.handle_app_key(AppKey {
            code: dartboard_editor::AppKeyCode::Char('x'),
            modifiers: Default::default(),
        });

        assert!(dispatch.handled);
        assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'x');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 1 }), 'x');
        assert_eq!(state.brush_mode(), BrushMode::None);
    }

    #[test]
    fn app_key_alt_c_returns_copy_effect() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(2, 1);
        state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');

        let dispatch = state.handle_app_key(AppKey {
            code: dartboard_editor::AppKeyCode::Char('c'),
            modifiers: dartboard_editor::AppModifiers {
                alt: true,
                ..Default::default()
            },
        });

        assert_eq!(
            dispatch.effects,
            vec![dartboard_editor::HostEffect::CopyToClipboard(
                "A ".to_string()
            )]
        );
    }

    #[test]
    fn app_key_escape_dismisses_temp_brush_back_to_none() {
        let mut state = test_state();
        state.type_char('Q', (80, 24));
        assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));

        let dispatch = state.handle_app_key(AppKey {
            code: dartboard_editor::AppKeyCode::Esc,
            modifiers: Default::default(),
        });

        assert!(dispatch.handled);
        assert!(!state.has_floating());
        assert_eq!(state.brush_mode(), BrushMode::None);
    }

    #[test]
    fn swatch_brush_mode_reports_swatch() {
        let mut state = test_state();
        state.editor.swatches[0] = Some(Swatch {
            clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
            pinned: false,
        });

        state.activate_swatch(0);

        assert_eq!(state.brush_mode(), BrushMode::Swatch);
    }

    #[test]
    fn temp_glyph_brush_mode_reports_canvas_glyph() {
        let mut state = test_state();
        state.type_char('🔥', (80, 24));

        assert!(state.activate_temp_glyph_brush_at(Pos { x: 0, y: 0 }));

        assert_eq!(state.brush_mode(), BrushMode::Glyph('🔥'));
        assert!(state.has_floating());
    }

    #[test]
    fn app_key_ctrl_v_stamps_floating_like_reference_client() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(5, 3);
        state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'A');
        state.editor.cursor = Pos { x: 1, y: 1 };
        state.begin_selection_from_cursor();
        assert!(state.lift_selection_to_floating());
        state.editor.cursor = Pos { x: 3, y: 0 };

        let dispatch = state.handle_app_key(AppKey {
            code: dartboard_editor::AppKeyCode::Char('v'),
            modifiers: dartboard_editor::AppModifiers {
                ctrl: true,
                ..Default::default()
            },
        });

        assert!(dispatch.handled);
        assert_eq!(state.snapshot.canvas.get(Pos { x: 3, y: 0 }), 'A');
        assert!(state.has_floating());
    }

    #[test]
    fn swatch_preview_stays_hidden_until_pointer_reenters_canvas() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(40, 20);
        state.editor.swatches[0] = Some(Swatch {
            clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
            pinned: false,
        });
        state.editor.cursor = Pos { x: 12, y: 7 };

        state.activate_swatch(0);

        assert!(state.has_floating());
        assert!(state.floating_view().is_none());

        let dispatch = state.handle_pointer_event(AppPointerEvent {
            column: 4,
            row: 3,
            kind: dartboard_editor::AppPointerKind::Moved,
            modifiers: Default::default(),
        });

        assert!(dispatch.outcome.is_consumed());
        let floating = state.floating_view().expect("floating preview shown");
        assert_eq!(floating.anchor, Pos { x: 3, y: 2 });
    }

    #[test]
    fn swatch_preview_suppression_hides_canvas_cursor() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(40, 20);
        state.editor.swatches[0] = Some(Swatch {
            clipboard: Clipboard::new(3, 3, vec![Some(CellValue::Narrow('A')); 9]),
            pinned: false,
        });

        state.activate_swatch(0);

        assert!(state.has_floating());
        assert!(!state.should_show_canvas_cursor());
    }

    #[test]
    fn system_clipboard_export_uses_selection_when_present() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(3, 2);
        state.snapshot.canvas.set(Pos { x: 0, y: 0 }, 'A');
        state.snapshot.canvas.set(Pos { x: 1, y: 0 }, 'B');
        state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'D');
        state.editor.cursor = Pos { x: 1, y: 0 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.move_down((80, 24));

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
        state.editor.cursor = Pos { x: 1, y: 0 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        assert!(state.lift_selection_to_floating());
        state.editor.cursor = Pos { x: 0, y: 1 };

        assert!(state.dismiss_floating());

        let selection = state.selection_view().expect("selection restored");
        assert_eq!(selection.anchor, Pos { x: 1, y: 0 });
        assert_eq!(selection.cursor, Pos { x: 2, y: 0 });
        assert_eq!(state.cursor(), Pos { x: 2, y: 0 });
    }

    #[test]
    fn pointer_dismiss_floating_restores_original_selection() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(4, 2);
        state.editor.cursor = Pos { x: 1, y: 0 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        assert!(state.lift_selection_to_floating());
        state.editor.cursor = Pos { x: 0, y: 1 };

        let dispatch = state.handle_pointer_event(AppPointerEvent {
            column: 1,
            row: 2,
            kind: dartboard_editor::AppPointerKind::Down(dartboard_editor::AppPointerButton::Right),
            modifiers: Default::default(),
        });

        assert!(dispatch.outcome.is_consumed());
        assert_eq!(
            dispatch.stroke_hint,
            Some(dartboard_editor::PointerStrokeHint::End)
        );
        assert!(!state.has_floating());
        let selection = state.selection_view().expect("selection restored");
        assert_eq!(selection.anchor, Pos { x: 1, y: 0 });
        assert_eq!(selection.cursor, Pos { x: 2, y: 0 });
        assert_eq!(state.cursor(), Pos { x: 2, y: 0 });
    }

    #[test]
    fn glyph_picker_opens_closes_and_inserts_selected_glyph() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(10, 3);
        state.editor.cursor = Pos { x: 0, y: 0 };

        state.open_glyph_picker();
        assert!(state.is_glyph_picker_open());
        assert!(state.glyph_catalog().is_some());

        // First selectable entry on the emoji tab is the first COMMON_EMOJI
        // ("👍" thumbs up). Confirm insertion paints its leading scalar at
        // the cursor and closes the picker.
        assert!(state.glyph_picker_insert(false, (80, 24)));
        assert!(!state.is_glyph_picker_open());
        assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), '👍');
    }

    #[test]
    fn glyph_picker_keep_open_leaves_picker_visible_after_insert() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(10, 3);
        state.editor.cursor = Pos { x: 0, y: 0 };
        state.open_glyph_picker();
        assert!(state.glyph_picker_insert(true, (80, 24)));
        assert!(state.is_glyph_picker_open());
    }

    #[test]
    fn glyph_picker_open_dismisses_floating_and_selection() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(4, 2);
        state.editor.cursor = Pos { x: 0, y: 0 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        assert!(state.lift_selection_to_floating());
        assert!(state.has_floating());

        state.open_glyph_picker();

        assert!(state.is_glyph_picker_open());
        assert!(!state.has_floating());
        assert!(state.selection_view().is_none());
    }

    #[test]
    fn commit_floating_moves_selected_region() {
        let mut state = test_state();
        state.snapshot.canvas = Canvas::with_size(5, 3);
        state.snapshot.canvas.set(Pos { x: 1, y: 1 }, 'A');
        state.snapshot.canvas.set(Pos { x: 2, y: 1 }, 'B');
        state.editor.cursor = Pos { x: 1, y: 1 };
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        assert!(state.lift_selection_to_floating());

        state.editor.cursor = Pos { x: 0, y: 0 };
        assert!(state.commit_floating());

        assert_eq!(state.snapshot.canvas.get(Pos { x: 0, y: 0 }), 'A');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 0 }), 'B');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 1, y: 1 }), ' ');
        assert_eq!(state.snapshot.canvas.get(Pos { x: 2, y: 1 }), ' ');
        assert!(!state.has_floating());
    }
}
