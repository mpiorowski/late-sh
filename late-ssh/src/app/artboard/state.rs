use dartboard_core::{Canvas, CanvasOp, CellValue, Pos, RgbColor};
use dartboard_editor::{
    AppKey, AppModifiers, AppPointerButton, AppPointerEvent, AppPointerKind, Bounds, Clipboard,
    EditorAction, EditorContext, EditorKeyDispatch, EditorPointerDispatch, EditorSession,
    FloatingSelection as EditorFloatingSelection, KeyMap, Mode as EditorMode, MoveDir,
    PointerOutcome, PointerStrokeHint, SWATCH_CAPACITY, Selection as EditorSelection,
    SelectionShape as EditorSelectionShape, Swatch, SwatchActivation, Viewport,
    backspace as editor_backspace, begin_paint_stroke as editor_begin_paint_stroke, capture_bounds,
    capture_selection, diff_canvas_op, dismiss_floating as editor_dismiss_floating,
    export_system_clipboard_text as editor_export_system_clipboard_text,
    handle_editor_action as editor_handle_action, handle_editor_pointer as editor_handle_pointer,
    paste_text_block,
};
use dartboard_tui::{FloatingView, SelectionShape as TuiSelectionShape, SelectionView};
use ratatui::layout::Rect;
use std::{cell::Cell, time::Instant};
use tokio::sync::{
    broadcast::{self, error::TryRecvError},
    watch,
};
use uuid::Uuid;

use super::gallery::{
    frame::frame_piece,
    state::{Focus as GalleryFocus, GalleryState, HangFlow, RailRow},
    svc::GalleryService,
};
use super::provenance::{SharedArtboardProvenance, apply_shared_op};
use super::svc::{
    ArtboardArchiveEntry, ArtboardArchiveLoader, ArtboardArchiveResult, ArtboardArchiveSnapshot,
    ArtboardSnapshotKind, ArtboardSnapshotService, DartboardEvent, DartboardService,
    DartboardSnapshot,
};
use crate::app::common::color_picker::{ColorPicker, PAINT_PALETTE};
use crate::app::icon_picker::{self, catalog::IconCatalogData};

const DOUBLE_CLICK_WINDOW_MS: u128 = 400;
pub(crate) const PRIMARY_SWATCH_IDX: usize = 0;

pub struct State {
    pub snapshot: DartboardSnapshot,
    pub private_notice: Option<String>,
    #[allow(dead_code)]
    pub(crate) svc: DartboardService,
    pub(crate) editor: EditorSession,
    active_brush: Option<Brush>,
    drag_brush: Option<Brush>,
    /// The local paint colour; `None` paints in the peer colour.
    paint_color: Option<RgbColor>,
    color_picker: Option<ColorPicker>,
    swatch_colors: [Option<Vec<Option<RgbColor>>>; SWATCH_CAPACITY],
    floating_colors: Option<Vec<Option<RgbColor>>>,
    floating_source_selection: Option<EditorSelection>,
    floating_source_bounds: Option<Bounds>,
    suppress_swatch_preview: bool,
    last_canvas_click: Option<(Instant, Pos)>,
    help_open: bool,
    help_tab: HelpTab,
    help_scroll_offsets: [u16; HelpTab::ALL.len()],
    glyph_picker: icon_picker::IconPickerState,
    glyph_picker_open: bool,
    glyph_catalog: Option<IconCatalogData>,
    username: String,
    shared_provenance: SharedArtboardProvenance,
    ownership_overlay: bool,
    hover_pos: Option<Pos>,
    snapshot_rx: watch::Receiver<DartboardSnapshot>,
    event_rx: broadcast::Receiver<DartboardEvent>,
    archive_loader: ArtboardArchiveLoader,
    archives: ArchiveBrowser,
    gallery: GalleryState,
    owner_overlay_cache: std::cell::RefCell<Option<(Pos, u16, u16, Canvas)>>,
}

impl State {
    pub fn new(
        svc: DartboardService,
        snapshot_service: ArtboardSnapshotService,
        gallery_service: GalleryService,
        viewer_id: Uuid,
        username: String,
        shared_provenance: SharedArtboardProvenance,
    ) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let event_rx = svc.subscribe_events();
        let archive_loader = ArtboardArchiveLoader::new(snapshot_service);
        let mut state = Self {
            snapshot,
            private_notice: None,
            svc,
            editor: EditorSession::default(),
            active_brush: None,
            drag_brush: None,
            paint_color: None,
            color_picker: None,
            swatch_colors: Default::default(),
            floating_colors: None,
            floating_source_selection: None,
            floating_source_bounds: None,
            suppress_swatch_preview: false,
            last_canvas_click: None,
            help_open: false,
            help_tab: HelpTab::default(),
            help_scroll_offsets: [0; HelpTab::ALL.len()],
            glyph_picker: icon_picker::IconPickerState::default(),
            glyph_picker_open: false,
            glyph_catalog: None,
            username,
            shared_provenance,
            ownership_overlay: false,
            hover_pos: None,
            snapshot_rx,
            event_rx,
            archive_loader,
            archives: ArchiveBrowser::default(),
            gallery: GalleryState::new(gallery_service, viewer_id),
            owner_overlay_cache: std::cell::RefCell::new(None),
        };
        // The archive keys are cheap (no canvases): list them on entry so
        // the rail's numbers are there before anyone opens a list.
        for kind in ArtboardSnapshotKind::ALL {
            state.ensure_archive_listed(kind);
        }
        state
    }

    /// Returns true when this tick changed anything render-visible: a canvas
    /// snapshot swap, a drained dartboard event, or an archive load result.
    /// In archive view the snapshot peek stays out of the report (the drain
    /// is deliberately skipped there, so the latched watch flag must not
    /// dirty every tick); leaving archive view is input-driven and the next
    /// tick drains and reports it.
    pub fn tick(&mut self) -> bool {
        let mut changed = self.drain_archive_results();
        changed |= self.gallery.tick();
        changed |= !self.event_rx.is_empty();

        if !self.is_archive_view_active() && self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.invalidate_owner_overlay_cache();
            self.editor.clamp_cursor(&self.snapshot.canvas);
            self.editor.clamp_viewport_origin(&self.snapshot.canvas);
            changed = true;
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
        changed
    }

    pub fn cursor(&self) -> Pos {
        self.editor.cursor
    }

    pub fn viewport_origin(&self) -> Pos {
        self.editor.viewport_origin
    }

    pub fn hover_pos(&self) -> Option<Pos> {
        self.hover_pos
    }

    pub fn owner_subject_pos(&self) -> Pos {
        self.hover_pos.unwrap_or(self.cursor())
    }

    pub fn owner_username(&self) -> Option<&str> {
        self.snapshot
            .provenance
            .username_at(&self.snapshot.canvas, self.owner_subject_pos())
    }

    pub fn ownership_overlay_enabled(&self) -> bool {
        self.ownership_overlay
    }

    pub fn toggle_ownership_overlay(&mut self) {
        self.ownership_overlay = !self.ownership_overlay;
    }

    pub fn set_hover_screen_point(&mut self, screen_size: (u16, u16), x: u16, y: u16) {
        self.hover_pos = self.canvas_pos_for_screen_point(screen_size, x, y);
    }

    pub fn clear_hover(&mut self) {
        self.hover_pos = None;
    }

    pub fn set_viewport_for_screen(&mut self, screen_size: (u16, u16)) {
        let viewport = super::ui::canvas_area_for_state(screen_size, self.gallery.rail_visible());
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

    pub fn pan_viewport_by(&mut self, screen_size: (u16, u16), dx: isize, dy: isize) {
        self.set_viewport_for_screen(screen_size);
        self.editor.pan_by(&self.snapshot.canvas, dx, dy);
    }

    pub fn paint_char(&mut self, ch: char) {
        self.apply_brush(Brush::for_typed_char(ch));
    }

    pub fn type_char(&mut self, ch: char, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        let brush = Brush::for_typed_char(ch);
        match brush {
            Brush::Glyph(ch) => {
                let pos = self.editor.cursor;
                let fg = self.active_user_color();
                let op = CanvasOp::PaintCell { pos, ch, fg };
                let _ = self.submit_single_op(op);
                self.editor.move_right(&self.snapshot.canvas);
            }
            Brush::Erase => {
                let pos = self.editor.cursor;
                let op = CanvasOp::ClearCell { pos };
                let _ = self.submit_single_op(op);
                self.editor.move_right(&self.snapshot.canvas);
            }
        }
    }

    pub fn clear_at_cursor(&mut self) {
        let pos = self.editor.cursor;
        let op = CanvasOp::ClearCell { pos };
        let _ = self.submit_single_op(op);
    }

    pub fn handle_app_key(&mut self, key: AppKey) -> EditorKeyDispatch {
        if key.code == dartboard_editor::AppKeyCode::Esc && key.modifiers == AppModifiers::default()
        {
            if self.dismiss_active_brush() {
                return EditorKeyDispatch {
                    handled: true,
                    effects: Vec::new(),
                };
            }
            if self.editor.selection_anchor.is_none() {
                return EditorKeyDispatch::default();
            }
        }
        if key.code == dartboard_editor::AppKeyCode::Char(' ')
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
        if self.is_archive_view_active() && matches!(action, EditorAction::CutSelection) {
            return EditorKeyDispatch::default();
        }
        let copied_to_slot = (self.editor.floating.is_none()
            && matches!(
                action,
                EditorAction::CopySelection | EditorAction::CutSelection
            ))
        .then_some(PRIMARY_SWATCH_IDX);
        if copied_to_slot.is_some() {
            self.prepare_primary_clipboard_slot();
            let colors = match self.editor.selection() {
                Some(selection) => capture_selection_colors(&self.snapshot.canvas, selection),
                None => capture_bounds_colors(
                    &self.snapshot.canvas,
                    self.editor
                        .selection_or_cursor_bounds()
                        .normalized_for_canvas(&self.snapshot.canvas),
                ),
            };
            push_swatch_colors(&mut self.swatch_colors, &self.editor.swatches, colors);
        }
        let primary_colors = matches!(action, EditorAction::PastePrimarySwatch)
            .then(|| self.swatch_colors[PRIMARY_SWATCH_IDX].clone())
            .flatten();
        let floating_colors = matches!(action, EditorAction::StrokeFloating { .. })
            .then(|| self.floating_colors.clone())
            .flatten();
        let reach = self.action_reach(action);
        let (dispatch, _) = self.edit_board(reach, |editor, canvas, color| match action {
            EditorAction::PastePrimarySwatch => {
                if let Some(swatch) = editor.swatches[PRIMARY_SWATCH_IDX].as_ref() {
                    stamp_clipboard_colored(
                        canvas,
                        &swatch.clipboard,
                        primary_colors.as_deref(),
                        editor.cursor,
                        color,
                        false,
                    );
                }
                EditorKeyDispatch {
                    handled: true,
                    effects: Vec::new(),
                }
            }
            EditorAction::StrokeFloating { dir } => {
                if editor.floating.is_some() {
                    let start = editor.cursor;
                    let _ = paint_floating_at_cursor_colored(
                        editor,
                        canvas,
                        floating_colors.as_deref(),
                        color,
                    );
                    editor.move_dir(canvas, dir);
                    if editor.cursor != start {
                        let _ = paint_floating_at_cursor_colored(
                            editor,
                            canvas,
                            floating_colors.as_deref(),
                            color,
                        );
                    }
                }
                EditorKeyDispatch {
                    handled: true,
                    effects: Vec::new(),
                }
            }
            _ => editor_handle_action(editor, canvas, action, color),
        });
        if let EditorAction::ActivateSwatch(idx) = action {
            self.floating_colors = if self.active_swatch_index() == Some(idx) {
                self.swatch_colors.get(idx).and_then(Clone::clone)
            } else {
                None
            };
        }
        self.sync_floating_source_selection();

        if dispatch.handled
            && let Some(idx) = copied_to_slot
        {
            self.arm_swatch_brush(idx);
        }

        dispatch
    }

    pub fn handle_pointer_event(&mut self, pointer: AppPointerEvent) -> EditorPointerDispatch {
        let had_floating = self.editor.floating.is_some();
        let had_local_floating = self.floating_source_selection.is_some();
        let pointer_over_canvas = self
            .editor
            .canvas_pos_for_pointer(pointer.column, pointer.row, &self.snapshot.canvas)
            .is_some();
        let floating_colors = (had_floating
            && matches!(
                pointer.kind,
                AppPointerKind::Down(AppPointerButton::Left)
                    | AppPointerKind::Drag(AppPointerButton::Left)
            ))
        .then(|| self.floating_colors.clone())
        .flatten();
        let reach = self.pointer_reach(pointer);
        let (dispatch, _) = self.edit_board(reach, |editor, canvas, color| {
            if floating_colors.is_some() {
                paint_floating_pointer_colored(
                    editor,
                    canvas,
                    pointer,
                    floating_colors.as_deref(),
                    color,
                )
            } else {
                editor_handle_pointer(editor, canvas, pointer, color)
            }
        });
        if self.editor.floating.is_none() {
            self.floating_colors = None;
        }
        self.sync_floating_source_selection();
        if had_floating && had_local_floating && self.editor.floating.is_none() {
            self.restore_floating_source_selection();
        }
        if self.editor.floating.is_none() || pointer_over_canvas {
            self.suppress_swatch_preview = false;
        }
        dispatch
    }

    pub fn backspace(&mut self, screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        let reach = EditReach::Within(backspace_bounds(self.editor.cursor));
        let _ = self.edit_canvas(reach, |editor, canvas, _| editor_backspace(editor, canvas));
    }

    pub fn paste_bytes(&mut self, bytes: &[u8], screen_size: (u16, u16)) {
        self.set_viewport_for_screen(screen_size);
        if self.is_archive_view_active() {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return;
        };
        let Some(clipboard) = clipboard_from_text(
            text,
            self.snapshot.canvas.width,
            self.snapshot.canvas.height,
        ) else {
            return;
        };

        self.drag_brush = None;
        self.arm_transparent_floating(clipboard, None, None, None);
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
        let viewport = super::ui::canvas_area_for_state(screen_size, self.gallery.rail_visible());
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

    pub fn canvas_for_render(&self, width: u16, height: u16) -> Option<Canvas> {
        let mut canvas = if self.ownership_overlay {
            self.owner_overlay_canvas(width, height)
        } else {
            let floating = self.editor.floating.as_ref()?;
            let mut canvas = self.snapshot.canvas.clone();
            if !floating.transparent {
                if let Some(bounds) = self.floating_source_bounds {
                    clear_bounds_on(&mut canvas, bounds);
                } else if let Some(selection) = self.floating_source_selection {
                    clear_bounds_on(
                        &mut canvas,
                        selection
                            .bounds()
                            .normalized_for_canvas(&self.snapshot.canvas),
                    );
                }
            }
            canvas
        };

        if self.ownership_overlay
            && let Some(floating) = self.editor.floating.as_ref()
            && !floating.transparent
        {
            if let Some(bounds) = self.floating_source_bounds {
                clear_bounds_on(&mut canvas, bounds);
            } else if let Some(selection) = self.floating_source_selection {
                clear_bounds_on(
                    &mut canvas,
                    selection
                        .bounds()
                        .normalized_for_canvas(&self.snapshot.canvas),
                );
            }
        }

        Some(canvas)
    }

    pub fn should_show_canvas_cursor(&self) -> bool {
        !self.help_open && !self.swatch_preview_suppressed()
    }

    fn owner_overlay_canvas(&self, width: u16, height: u16) -> Canvas {
        let origin = self.viewport_origin();
        if let Some((cached_origin, cached_w, cached_h, cached_canvas)) =
            &*self.owner_overlay_cache.borrow()
            && *cached_origin == origin
            && *cached_w == width
            && *cached_h == height
        {
            return cached_canvas.clone();
        }

        let mut canvas = self.snapshot.canvas.clone();
        let max_x = (origin.x + width as usize).min(canvas.width);
        let max_y = (origin.y + height as usize).min(canvas.height);

        for y in origin.y..max_y {
            for x in origin.x..max_x {
                let pos = Pos { x, y };
                if self.snapshot.canvas.glyph_origin(pos).is_none() {
                    continue;
                }
                let Some(username) = self
                    .snapshot
                    .provenance
                    .username_at(&self.snapshot.canvas, pos)
                else {
                    let _ = canvas.put_glyph(pos, '?');
                    continue;
                };
                let _ =
                    canvas.put_glyph_colored(pos, owner_initial(username), owner_color(username));
            }
        }
        *self.owner_overlay_cache.borrow_mut() = Some((origin, width, height, canvas.clone()));
        canvas
    }

    pub fn export_system_clipboard_text(&self) -> String {
        editor_export_system_clipboard_text(&self.editor, &self.snapshot.canvas)
    }

    pub fn lift_selection_to_floating(&mut self) -> bool {
        let Some(selection) = self.editor.selection() else {
            return false;
        };
        let clipboard = capture_selection(&self.snapshot.canvas, selection);
        let colors = capture_selection_colors(&self.snapshot.canvas, selection);
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
        self.floating_colors = Some(colors);
        self.floating_source_selection = Some(selection);
        self.floating_source_bounds = Some(bounds);
        self.suppress_swatch_preview = false;
        true
    }

    pub fn commit_floating(&mut self) -> bool {
        let Some(floating) = self.editor.floating.clone() else {
            return false;
        };
        let was_temp_brush =
            self.active_brush.is_some() && self.floating_source_selection.is_none();

        // An opaque lift leaves its source empty; the stamp lands at the
        // cursor.
        let source = match (
            floating.transparent,
            self.floating_source_bounds,
            self.floating_source_selection,
        ) {
            (true, _, _) | (false, None, None) => None,
            (false, Some(bounds), _) => Some(bounds),
            (false, None, Some(selection)) => Some(
                selection
                    .bounds()
                    .normalized_for_canvas(&self.snapshot.canvas),
            ),
        };
        let cursor = self.editor.cursor;
        let stamp = stamp_bounds(cursor, &floating.clipboard);
        let reach = match source {
            Some(source) => union_bounds(source, stamp),
            None => stamp,
        };
        let floating_colors = std::mem::take(&mut self.floating_colors);
        let _ = self.edit_board(EditReach::Within(reach), |_, canvas, color| {
            if let Some(source) = source {
                clear_bounds_on(canvas, source);
            }
            stamp_clipboard_colored(
                canvas,
                &floating.clipboard,
                floating_colors.as_deref(),
                cursor,
                color,
                floating.transparent,
            );
        });
        editor_dismiss_floating(&mut self.editor);
        self.floating_source_selection = None;
        self.floating_source_bounds = None;
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
        self.floating_colors = None;
        self.floating_source_bounds = None;
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
        self.floating_colors = None;
        self.floating_source_selection = None;
        self.floating_source_bounds = None;
        self.suppress_swatch_preview = false;
        self.last_canvas_click = None;
        self.hover_pos = None;
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

    pub fn active_paint_color(&self) -> RgbColor {
        self.active_user_color()
    }

    /// The preset the paint colour sits on, or the cycle's starting point
    /// when it is a custom or peer colour outside the presets.
    pub fn active_paint_color_index(&self) -> usize {
        palette_index(self.active_user_color()).unwrap_or(1)
    }

    /// The preset the paint colour sits on; `None` for a custom colour.
    pub fn active_paint_palette_index(&self) -> Option<usize> {
        palette_index(self.active_user_color())
    }

    pub fn cycle_paint_color(&mut self, delta: isize) {
        let len = PAINT_PALETTE.len() as isize;
        let current = self.active_paint_color_index() as isize;
        let next = (current + delta).rem_euclid(len) as usize;
        self.select_palette_color(next);
    }

    pub fn select_palette_color(&mut self, index: usize) {
        self.paint_color = Some(PAINT_PALETTE[index]);
        self.suppress_swatch_preview = false;
    }

    pub fn is_color_picker_open(&self) -> bool {
        self.color_picker.is_some()
    }

    pub fn color_picker(&self) -> Option<&ColorPicker> {
        self.color_picker.as_ref()
    }

    pub fn color_picker_mut(&mut self) -> Option<&mut ColorPicker> {
        self.color_picker.as_mut()
    }

    /// Open the picker on the current paint colour. The colour is local,
    /// so an archive view may pick one too.
    pub fn open_color_picker(&mut self) {
        self.last_canvas_click = None;
        self.color_picker = Some(ColorPicker::open(self.active_user_color()));
    }

    pub fn close_color_picker(&mut self) {
        self.color_picker = None;
    }

    /// Enter in the picker: the working colour becomes the paint colour.
    pub fn apply_color_picker(&mut self) {
        let Some(picker) = self.color_picker.take() else {
            return;
        };
        self.paint_color = Some(picker.color);
        self.suppress_swatch_preview = false;
    }

    pub fn swatches(&self) -> &[Option<Swatch>; SWATCH_CAPACITY] {
        &self.editor.swatches
    }

    pub fn swatch_colors(&self, idx: usize) -> Option<&[Option<RgbColor>]> {
        self.swatch_colors.get(idx).and_then(Option::as_deref)
    }

    pub fn floating_colors(&self) -> Option<&[Option<RgbColor>]> {
        self.floating_colors.as_deref()
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
            if let Some(floating) = self.editor.floating.as_mut() {
                floating.transparent = true;
            }
            self.floating_colors = self.swatch_colors.get(idx).and_then(Clone::clone);
            self.suppress_swatch_preview = false;
        } else if self.editor.floating.is_none() {
            self.floating_colors = None;
        }
        self.sync_floating_source_selection();
    }

    pub fn toggle_swatch_pin(&mut self, idx: usize) {
        if idx >= SWATCH_CAPACITY {
            return;
        }
        if !self.swatch_is_pinnable(idx) {
            if let Some(swatch) = self.editor.swatches.get_mut(idx).and_then(Option::as_mut) {
                swatch.pinned = false;
            }
            return;
        }
        self.editor.toggle_pin(idx);
    }

    pub fn clear_swatch(&mut self, idx: usize) {
        self.editor.clear_swatch(idx);
        if let Some(slot) = self.swatch_colors.get_mut(idx) {
            *slot = None;
        }
        if self.editor.floating.is_none() {
            self.floating_colors = None;
        }
        self.suppress_swatch_preview = false;
    }
    pub fn is_in_normal_brush_mode(&self) -> bool {
        self.editor.floating.is_none() && self.active_brush.is_none()
    }

    pub fn register_canvas_click(&mut self, pos: Pos) -> bool {
        let pos = self.snapshot.canvas.glyph_origin(pos).unwrap_or(pos);
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

    // ----- archives -----

    pub fn is_archive_view_active(&self) -> bool {
        self.archives.active.is_some()
    }

    pub fn active_archive_snapshot(&self) -> Option<&ArtboardArchiveSnapshot> {
        self.archives.active.as_ref()
    }

    /// The archive kind whose list the rail is showing, while it is.
    pub fn browsed_archive_kind(&self) -> Option<ArtboardSnapshotKind> {
        if self.gallery.focus() != GalleryFocus::Archive {
            return None;
        }
        match self.gallery.selected_row() {
            RailRow::Archive(kind) => Some(kind),
            RailRow::Board | RailRow::Gallery(_) | RailRow::Hang => None,
        }
    }

    pub fn archive_entries(&self, kind: ArtboardSnapshotKind) -> &[ArtboardArchiveEntry] {
        &self.archives.lists[kind.index()].entries
    }

    pub fn archive_selected(&self, kind: ArtboardSnapshotKind) -> usize {
        self.archives.lists[kind.index()].selected
    }

    pub fn archive_scroll(&self, kind: ArtboardSnapshotKind) -> usize {
        self.archives.lists[kind.index()].scroll
    }

    pub fn archive_loading(&self, kind: ArtboardSnapshotKind) -> bool {
        self.archives.lists[kind.index()].loading
    }

    pub fn archive_error(&self, kind: ArtboardSnapshotKind) -> Option<&str> {
        self.archives.lists[kind.index()].error.as_deref()
    }

    /// The count the rail shows next to an archive row, once listed.
    pub fn archive_count(&self, kind: ArtboardSnapshotKind) -> Option<usize> {
        let list = &self.archives.lists[kind.index()];
        list.loaded.then_some(list.entries.len())
    }

    /// The key being fetched right now, so the list can mark it.
    pub fn archive_loading_key(&self) -> Option<&str> {
        self.archives.inflight.as_deref()
    }

    pub fn archive_load_error(&self) -> Option<&str> {
        self.archives.load_error.as_deref()
    }

    pub fn set_archive_visible_height(&self, height: usize) {
        self.archives.visible_height.set(height.max(1));
    }

    /// Turn the rail into `kind`'s list. The keys load once per session;
    /// the cursor's snapshot loads as it moves.
    pub fn open_archive_list(&mut self, kind: ArtboardSnapshotKind) {
        self.close_help();
        self.close_glyph_picker();
        self.close_color_picker();
        self.clear_local_state();
        self.gallery.rail_select(RailRow::Archive(kind));
        self.gallery.focus_archive();
        self.ensure_archive_listed(kind);
        self.want_selected_archive(kind);
    }

    /// Back to the rail rows. Whatever archive is on the board stays there
    /// until the Board row (or `exit_archive_view`) brings the live board
    /// back.
    pub fn close_archive_list(&mut self) {
        self.gallery.focus_rail();
    }

    pub fn archive_move(&mut self, delta: isize) {
        let Some(kind) = self.browsed_archive_kind() else {
            return;
        };
        let visible = self.archives.visible_height.get().max(1);
        let list = &mut self.archives.lists[kind.index()];
        if list.entries.is_empty() {
            list.selected = 0;
            list.scroll = 0;
            return;
        }
        let last = list.entries.len() as isize - 1;
        list.selected = (list.selected as isize + delta).clamp(0, last) as usize;
        if list.selected < list.scroll {
            list.scroll = list.selected;
        } else if list.selected >= list.scroll + visible {
            list.scroll = list.selected + 1 - visible;
        }
        self.want_selected_archive(kind);
    }

    pub fn archive_page(&mut self, pages: isize) {
        let visible = self.archives.visible_height.get().max(1) as isize;
        self.archive_move(pages.saturating_mul(visible));
    }

    pub fn archive_select(&mut self, index: usize) {
        let Some(kind) = self.browsed_archive_kind() else {
            return;
        };
        let list = &mut self.archives.lists[kind.index()];
        if index < list.entries.len() {
            list.selected = index;
            self.want_selected_archive(kind);
        }
    }

    /// The archive list index under a screen point, from the last draw of
    /// the rail.
    pub fn archive_index_at(&self, x: u16, y: u16) -> Option<usize> {
        let kind = self.browsed_archive_kind()?;
        let line = self.gallery.rail_line_at(x, y)?;
        let line = line.checked_sub(super::gallery::ui::ARCHIVE_LIST_HEADER_LINES)?;
        let list = &self.archives.lists[kind.index()];
        let index = list.scroll + line;
        (index < list.entries.len()).then_some(index)
    }

    pub fn exit_archive_view(&mut self) {
        self.archives.active = None;
        self.archives.wanted = None;
        self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        self.invalidate_owner_overlay_cache();
        self.editor.clamp_cursor(&self.snapshot.canvas);
        self.editor.clamp_viewport_origin(&self.snapshot.canvas);
        self.clear_local_state();
    }

    fn ensure_archive_listed(&mut self, kind: ArtboardSnapshotKind) {
        let list = &mut self.archives.lists[kind.index()];
        if list.loaded || list.loading {
            return;
        }
        list.loading = true;
        list.error = None;
        self.archive_loader.request_list(kind);
    }

    /// Point the board at the entry under the cursor.
    fn want_selected_archive(&mut self, kind: ArtboardSnapshotKind) {
        let list = &self.archives.lists[kind.index()];
        let Some(entry) = list.entries.get(list.selected) else {
            return;
        };
        self.archives.wanted = Some((kind, entry.board_key.clone()));
        self.request_wanted_archive();
    }

    /// Show the wanted archive: from the board if it is already up, from
    /// the cache if it has been seen, otherwise from the database, one
    /// fetch at a time. A cursor that moves on while a fetch is out is
    /// caught up with when the fetch lands.
    fn request_wanted_archive(&mut self) {
        let Some((kind, board_key)) = self.archives.wanted.clone() else {
            return;
        };
        if self
            .archives
            .active
            .as_ref()
            .is_some_and(|active| active.board_key == board_key)
        {
            return;
        }
        if let Some(cached) = self
            .archives
            .cache
            .iter()
            .find(|snapshot| snapshot.board_key == board_key)
            .cloned()
        {
            self.activate_archive_snapshot(cached);
            return;
        }
        if self.archives.inflight.is_some() {
            return;
        }
        self.archives.inflight = Some(board_key.clone());
        self.archive_loader.request_load(kind, board_key);
    }

    // ----- gallery -----

    pub fn gallery(&self) -> &GalleryState {
        &self.gallery
    }

    pub fn gallery_mut(&mut self) -> &mut GalleryState {
        &mut self.gallery
    }

    /// True while the gallery, not the board, owns view-mode input: the
    /// rail, a listing, a piece, or the hang flow has focus.
    pub fn gallery_claims_input(&self) -> bool {
        !matches!(self.gallery.hang(), HangFlow::Idle)
            || self.gallery.focus() != GalleryFocus::Canvas
    }

    /// True when Esc has an Artboard overlay to close before it can mean
    /// anything global.
    pub fn claims_escape(&self) -> bool {
        self.help_open
            || self.glyph_picker_open
            || self.color_picker.is_some()
            || self.gallery.claims_escape()
    }

    /// Start framing a piece on the live board. Every overlay closes and
    /// the local selection is cleared so the frame starts empty.
    pub fn begin_framing(&mut self) {
        self.close_help();
        self.close_glyph_picker();
        self.close_color_picker();
        self.clear_local_state();
        self.gallery.begin_framing();
    }

    pub fn cancel_framing(&mut self) {
        self.clear_local_state();
        self.gallery.cancel_hang();
    }

    /// Crop the current selection into a piece and move on to naming it,
    /// or say on the framing bar why it cannot hang.
    pub fn frame_selection_for_hang(&mut self) {
        let Some(selection) = self.editor.selection() else {
            self.gallery
                .set_framing_notice("Select a frame first (Shift+arrows or drag).".to_string());
            return;
        };
        let bounds = selection
            .bounds()
            .normalized_for_canvas(&self.snapshot.canvas);
        match frame_piece(
            &self.snapshot.canvas,
            &self.snapshot.provenance,
            bounds,
            &self.username,
        ) {
            Ok(framed) => {
                self.clear_local_state();
                self.gallery.set_confirm(framed);
            }
            Err(error) => self.gallery.set_framing_notice(error.notice()),
        }
    }

    pub fn is_help_open(&self) -> bool {
        self.help_open
    }

    pub fn toggle_help(&mut self) {
        if self.is_archive_view_active() {
            return;
        }
        self.help_open = !self.help_open;
    }

    pub fn close_help(&mut self) {
        self.help_open = false;
    }

    pub fn help_tab(&self) -> HelpTab {
        self.help_tab
    }

    pub fn help_scroll(&self) -> u16 {
        self.help_scroll_offsets[self.help_tab.index()]
    }

    pub fn select_next_help_tab(&mut self) {
        self.help_tab = self.help_tab.next();
    }

    pub fn select_prev_help_tab(&mut self) {
        self.help_tab = self.help_tab.prev();
    }

    pub fn select_help_tab(&mut self, tab: HelpTab) {
        self.help_tab = tab;
    }

    pub fn scroll_help(&mut self, delta: i16) {
        let idx = self.help_tab.index();
        let current = self.help_scroll_offsets[idx] as i32;
        self.help_scroll_offsets[idx] = (current + delta as i32).max(0) as u16;
    }

    pub fn reset_help_scroll(&mut self) {
        self.help_scroll_offsets[self.help_tab.index()] = 0;
    }

    pub fn is_glyph_picker_open(&self) -> bool {
        self.glyph_picker_open
    }

    pub fn glyph_picker_state(&self) -> &icon_picker::IconPickerState {
        &self.glyph_picker
    }

    pub fn glyph_picker_state_mut(&mut self) -> &mut icon_picker::IconPickerState {
        &mut self.glyph_picker
    }

    pub fn glyph_catalog(&self) -> Option<&IconCatalogData> {
        self.glyph_catalog.as_ref()
    }

    pub fn open_glyph_picker(&mut self) {
        if self.is_archive_view_active() {
            return;
        }
        // Enforce the "at most one of {selection, floating, picker}" invariant:
        // opening dismisses any floating preview and clears any selection.
        let _ = self.dismiss_floating();
        self.editor.clear_selection();
        self.active_brush = None;
        self.drag_brush = None;
        self.last_canvas_click = None;
        self.suppress_swatch_preview = false;

        if self.glyph_catalog.is_none() {
            self.glyph_catalog = Some(IconCatalogData::load());
        }
        self.glyph_picker = icon_picker::IconPickerState::default();
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
        icon_picker::picker::move_selection(&mut self.glyph_picker, catalog, delta);
    }

    /// Handle a left-down in the picker list at screen coords (column, row),
    /// 0-based. Returns `true` if this was a double-click (caller should
    /// treat as confirm).
    pub fn glyph_picker_click_list(&mut self, column: u16, row: u16) -> bool {
        let Some(catalog) = self.glyph_catalog.as_ref() else {
            return false;
        };
        icon_picker::picker::click_list(&mut self.glyph_picker, catalog, column, row)
    }

    /// Handle a left-down in the tab strip at screen column `column`.
    /// Returns `true` if a tab was hit.
    pub fn glyph_picker_click_tab(&mut self, column: u16, row: u16) -> bool {
        icon_picker::picker::click_tab(&mut self.glyph_picker, column, row)
    }

    /// Confirm the selection: paint the selected glyph/string at the cursor,
    /// and close the picker unless `keep_open` is set. Returns `true` if
    /// anything was inserted.
    pub fn glyph_picker_insert(&mut self, keep_open: bool, screen_size: (u16, u16)) -> bool {
        let Some(catalog) = self.glyph_catalog.as_ref() else {
            self.glyph_picker_open = false;
            return false;
        };
        let Some(icon) = icon_picker::picker::selected_icon(&self.glyph_picker, catalog) else {
            if !keep_open {
                self.glyph_picker_open = false;
            }
            return false;
        };
        if icon.is_empty() {
            if !keep_open {
                self.glyph_picker_open = false;
            }
            return false;
        }
        if !keep_open {
            self.glyph_picker_open = false;
        }
        self.set_viewport_for_screen(screen_size);
        let start = self.editor.cursor;
        let reach = EditReach::Within(text_block_bounds(start, &icon));
        let changed = self.edit_canvas(reach, |editor, canvas, color| {
            paste_text_block(editor, canvas, &icon, color)
        });
        if changed {
            self.editor.cursor = paste_cursor_end(
                start,
                &icon,
                self.snapshot.canvas.width,
                self.snapshot.canvas.height,
            );
            self.editor.scroll_viewport_to_cursor(&self.snapshot.canvas);
        }
        true
    }

    pub fn activate_temp_glyph_brush_at(&mut self, pos: Pos) -> bool {
        let Some(glyph) = self.snapshot.canvas.glyph_at(pos) else {
            return false;
        };
        if glyph.ch == ' ' {
            return false;
        }
        self.editor.cursor = glyph.pos;
        let clipboard = capture_bounds(
            &self.snapshot.canvas,
            Bounds::single(glyph.pos).normalized_for_canvas(&self.snapshot.canvas),
        );
        self.arm_transparent_floating(clipboard, None, None, Some(Brush::Glyph(glyph.ch)));
        // The sampler is the eyedropper too: the glyph's colour becomes the
        // paint colour, so Esc and typing carry it on.
        if let Some(fg) = self.snapshot.canvas.fg(glyph.pos) {
            self.paint_color = Some(fg);
        }
        true
    }

    /// Alt+K: the colour under the cursor becomes the paint colour. A blank
    /// or uncoloured cell has none to give, and says so.
    pub fn sample_color_at_cursor(&mut self) -> bool {
        match self.snapshot.canvas.fg(self.editor.cursor) {
            Some(fg) => {
                self.paint_color = Some(fg);
                self.suppress_swatch_preview = false;
                true
            }
            None => {
                self.private_notice = Some("No color under the cursor.".to_string());
                false
            }
        }
    }

    fn drain_archive_results(&mut self) -> bool {
        let mut changed = false;
        while let Some(result) = self.archive_loader.try_recv() {
            changed = true;
            match result {
                ArtboardArchiveResult::Listed { kind, entries } => {
                    let list = &mut self.archives.lists[kind.index()];
                    list.entries = entries;
                    list.loaded = true;
                    list.loading = false;
                    list.error = None;
                    list.selected = list.selected.min(list.entries.len().saturating_sub(1));
                    list.scroll = list.scroll.min(list.selected);
                    if self.browsed_archive_kind() == Some(kind) {
                        self.want_selected_archive(kind);
                    }
                }
                ArtboardArchiveResult::ListFailed { kind, error } => {
                    let list = &mut self.archives.lists[kind.index()];
                    list.loading = false;
                    list.error = Some(error);
                }
                ArtboardArchiveResult::Loaded(snapshot) => {
                    self.archives.inflight = None;
                    self.archives.load_error = None;
                    self.archives
                        .cache
                        .retain(|cached| cached.board_key != snapshot.board_key);
                    if self.archives.cache.len() >= ARCHIVE_CACHE_SIZE {
                        self.archives.cache.remove(0);
                    }
                    self.archives.cache.push(snapshot);
                    self.request_wanted_archive();
                }
                ArtboardArchiveResult::LoadFailed { board_key, error } => {
                    self.archives.inflight = None;
                    self.archives.load_error = Some(error);
                    // Do not chase the same key again; the cursor moving
                    // on sets a new one.
                    if self
                        .archives
                        .wanted
                        .as_ref()
                        .is_some_and(|(_, wanted)| *wanted == board_key)
                    {
                        self.archives.wanted = None;
                    }
                    self.request_wanted_archive();
                }
            }
        }
        changed
    }

    fn activate_archive_snapshot(&mut self, item: ArtboardArchiveSnapshot) {
        self.clear_local_state();
        self.snapshot.canvas = item.canvas.clone();
        self.snapshot.provenance = item.provenance.clone();
        self.snapshot.peers.clear();
        self.snapshot.connect_rejected = None;
        self.invalidate_owner_overlay_cache();
        self.editor.clamp_cursor(&self.snapshot.canvas);
        self.editor.clamp_viewport_origin(&self.snapshot.canvas);
        self.archives.active = Some(item);
    }

    fn active_user_color(&self) -> RgbColor {
        self.paint_color
            .or(self.snapshot.your_color)
            .unwrap_or(PAINT_PALETTE[1])
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

    pub fn swatch_is_pinnable(&self, idx: usize) -> bool {
        idx != PRIMARY_SWATCH_IDX && idx < SWATCH_CAPACITY
    }

    fn edit_canvas(
        &mut self,
        reach: EditReach,
        edit: impl FnOnce(&mut EditorSession, &mut Canvas, RgbColor) -> bool,
    ) -> bool {
        if self.is_archive_view_active() {
            return false;
        }
        let (_, submitted) = self.edit_board(reach, edit);
        submitted
    }

    /// Runs `edit` on the local board and submits what it changed as one op.
    /// The diff reads only the edit's `reach`, so a keystroke costs a few
    /// cells, not two copies and a diff of the whole board. Debug builds
    /// check every edit, a `Nothing` one included, against the whole-board
    /// diff.
    fn edit_board<R>(
        &mut self,
        reach: EditReach,
        edit: impl FnOnce(&mut EditorSession, &mut Canvas, RgbColor) -> R,
    ) -> (R, bool) {
        let color = self.active_user_color();
        let area = match reach {
            EditReach::Nothing => {
                #[cfg(debug_assertions)]
                let whole_before = self.snapshot.canvas.clone();
                let result = edit(&mut self.editor, &mut self.snapshot.canvas, color);
                #[cfg(debug_assertions)]
                debug_assert_eq!(
                    diff_canvas_op(&whole_before, &self.snapshot.canvas, color),
                    None,
                    "edit wrote outside its reach {reach:?}"
                );
                return (result, false);
            }
            EditReach::Within(bounds) => Some(widen_for_wide_glyphs(bounds, &self.snapshot.canvas)),
            EditReach::Anywhere => None,
        };
        #[cfg(debug_assertions)]
        let whole_before = self.snapshot.canvas.clone();
        let before = match area {
            Some(area) => crop_canvas(&self.snapshot.canvas, area),
            None => self.snapshot.canvas.clone(),
        };
        let result = edit(&mut self.editor, &mut self.snapshot.canvas, color);
        let op = match area {
            Some(area) => diff_canvas_op(&before, &crop_canvas(&self.snapshot.canvas, area), color),
            None => diff_canvas_op(&before, &self.snapshot.canvas, color),
        };
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            op,
            diff_canvas_op(&whole_before, &self.snapshot.canvas, color),
            "edit wrote outside its reach {reach:?}"
        );
        let submitted = self.submit_edit_op(&before, op);
        (result, submitted)
    }

    /// Where an editor action can write, read off the editor before it runs.
    fn action_reach(&self, action: EditorAction) -> EditReach {
        let cursor = self.editor.cursor;
        match action {
            EditorAction::Move { .. }
            | EditorAction::MoveDownLine
            | EditorAction::Pan { .. }
            | EditorAction::ClearSelection
            | EditorAction::TransposeSelectionCorner
            | EditorAction::CopySelection
            | EditorAction::ExportSystemClipboard
            | EditorAction::ActivateSwatch(_)
            | EditorAction::ToggleFloatingTransparency => EditReach::Nothing,
            EditorAction::InsertChar(ch) => EditReach::Within(Bounds {
                min_x: cursor.x,
                max_x: cursor.x + Canvas::display_width(ch) - 1,
                min_y: cursor.y,
                max_y: cursor.y,
            }),
            EditorAction::Backspace => EditReach::Within(backspace_bounds(cursor)),
            EditorAction::Delete => EditReach::Within(Bounds::single(cursor)),
            EditorAction::CutSelection
            | EditorAction::FillSelectionOrCell(_)
            | EditorAction::SmartFill
            | EditorAction::DrawBorder => EditReach::Within(
                self.editor
                    .selection_or_cursor_bounds()
                    .normalized_for_canvas(&self.snapshot.canvas),
            ),
            EditorAction::PastePrimarySwatch => match &self.editor.swatches[PRIMARY_SWATCH_IDX] {
                Some(swatch) => EditReach::Within(stamp_bounds(cursor, &swatch.clipboard)),
                None => EditReach::Nothing,
            },
            // A stamp where the cursor is, then one a step away; a jump to
            // a viewport edge has no small box.
            EditorAction::StrokeFloating { dir } => match (&self.editor.floating, dir) {
                (None, _) => EditReach::Nothing,
                (Some(floating), MoveDir::Left | MoveDir::Right | MoveDir::Up | MoveDir::Down) => {
                    let stamp = stamp_bounds(cursor, &floating.clipboard);
                    EditReach::Within(Bounds {
                        min_x: stamp.min_x.saturating_sub(1),
                        max_x: stamp.max_x + 1,
                        min_y: stamp.min_y.saturating_sub(1),
                        max_y: stamp.max_y + 1,
                    })
                }
                (
                    Some(_),
                    MoveDir::LineStart | MoveDir::LineEnd | MoveDir::PageUp | MoveDir::PageDown,
                ) => EditReach::Anywhere,
            },
            // The editor's row and column shifts rebuild the whole board.
            EditorAction::PushLeft
            | EditorAction::PushRight
            | EditorAction::PushUp
            | EditorAction::PushDown
            | EditorAction::PullFromLeft
            | EditorAction::PullFromRight
            | EditorAction::PullFromUp
            | EditorAction::PullFromDown => EditReach::Anywhere,
        }
    }

    /// Where a pointer event can write: only a left press or drag over the
    /// board with a brush up stamps, along the stroke from its anchor and
    /// last stamp to the pointer.
    fn pointer_reach(&self, pointer: AppPointerEvent) -> EditReach {
        let Some(floating) = self.editor.floating.as_ref() else {
            return EditReach::Nothing;
        };
        let paints = matches!(
            pointer.kind,
            AppPointerKind::Down(AppPointerButton::Left)
                | AppPointerKind::Drag(AppPointerButton::Left)
        );
        if !paints {
            return EditReach::Nothing;
        }
        let Some(pos) =
            self.editor
                .canvas_pos_for_pointer(pointer.column, pointer.row, &self.snapshot.canvas)
        else {
            return EditReach::Nothing;
        };
        let mut reach = Bounds::single(pos);
        for point in [
            self.editor.paint_stroke_anchor,
            self.editor.paint_stroke_last,
        ]
        .into_iter()
        .flatten()
        {
            reach = union_bounds(reach, Bounds::single(point));
        }
        reach.max_x += floating.clipboard.width.saturating_sub(1);
        reach.max_y += floating.clipboard.height.saturating_sub(1);
        EditReach::Within(reach)
    }

    fn invalidate_owner_overlay_cache(&self) {
        *self.owner_overlay_cache.borrow_mut() = None;
    }

    /// Records who painted what `op` changed, against `before` (the board,
    /// or the part of it the edit could reach, as it was before the edit),
    /// and sends the op.
    fn submit_edit_op(&mut self, before: &Canvas, op: Option<CanvasOp>) -> bool {
        if self.is_archive_view_active() {
            return false;
        }
        let Some(op) = op else {
            return false;
        };
        self.invalidate_owner_overlay_cache();
        self.snapshot
            .provenance
            .apply_op(before, &op, &self.username);
        apply_shared_op(&self.shared_provenance, before, &op, &self.username);
        self.svc.submit_op(op);
        true
    }

    fn submit_single_op(&mut self, op: CanvasOp) -> bool {
        if self.is_archive_view_active() {
            return false;
        }

        let changed = match &op {
            CanvasOp::PaintCell { pos, ch, fg } => {
                let cell_match = match self.snapshot.canvas.cell(*pos) {
                    Some(CellValue::Narrow(old_ch)) => old_ch == *ch,
                    Some(CellValue::Wide(old_ch)) => old_ch == *ch,
                    _ => false,
                };
                let fg_match = self.snapshot.canvas.fg(*pos) == Some(*fg);
                !(cell_match && fg_match)
            }
            CanvasOp::ClearCell { pos } => self.snapshot.canvas.cell(*pos).is_some(),
            _ => true,
        };

        if !changed {
            return false;
        }

        self.snapshot
            .provenance
            .apply_op(&self.snapshot.canvas, &op, &self.username);

        apply_shared_op(
            &self.shared_provenance,
            &self.snapshot.canvas,
            &op,
            &self.username,
        );

        self.invalidate_owner_overlay_cache();
        self.snapshot.canvas.apply(&op);
        self.svc.submit_op(op);
        true
    }

    fn stamp_floating(&mut self) -> bool {
        let Some(floating) = self.editor.floating.clone() else {
            return true;
        };
        let floating_colors = self.floating_colors.clone();
        let reach = EditReach::Within(stamp_bounds(self.editor.cursor, &floating.clipboard));
        let _ = self.edit_canvas(reach, |editor, canvas, color| {
            stamp_clipboard_colored(
                canvas,
                &floating.clipboard,
                floating_colors.as_deref(),
                editor.cursor,
                color,
                floating.transparent,
            );
            true
        });
        true
    }

    fn prepare_primary_clipboard_slot(&mut self) {
        if let Some(swatch) = self.editor.swatches[PRIMARY_SWATCH_IDX].as_mut() {
            swatch.pinned = false;
        }
    }

    fn arm_swatch_brush(&mut self, idx: usize) -> bool {
        let Some(clipboard) = self
            .editor
            .swatches
            .get(idx)
            .and_then(|swatch| swatch.as_ref())
            .map(|swatch| swatch.clipboard.clone())
        else {
            return false;
        };
        let colors = self.swatch_colors.get(idx).and_then(Clone::clone);
        self.arm_transparent_floating(clipboard, colors, Some(idx), None);
        true
    }

    /// Swatch, sampled-glyph and pasted brushes all float transparent with no
    /// lifted source; only an opaque lift keeps its source selection.
    fn arm_transparent_floating(
        &mut self,
        clipboard: Clipboard,
        colors: Option<Vec<Option<RgbColor>>>,
        source_index: Option<usize>,
        active_brush: Option<Brush>,
    ) {
        self.editor.clear_selection();
        self.editor.floating = Some(EditorFloatingSelection {
            clipboard,
            transparent: true,
            source_index,
        });
        self.floating_colors = colors;
        self.floating_source_selection = None;
        self.floating_source_bounds = None;
        self.active_brush = active_brush;
        self.suppress_swatch_preview = false;
    }

    fn apply_brush(&mut self, brush: Brush) {
        match brush {
            Brush::Glyph(ch) => {
                if ch.is_control() {
                    return;
                }
                let pos = self.editor.cursor;
                let fg = self.active_user_color();
                let op = CanvasOp::PaintCell { pos, ch, fg };
                let _ = self.submit_single_op(op);
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
            self.floating_source_bounds = None;
        }
    }

    fn restore_floating_source_selection(&mut self) {
        if let Some(selection) = self.floating_source_selection.take() {
            self.editor.selection_anchor = Some(selection.anchor);
            self.editor.selection_shape = selection.shape;
            self.editor.mode = EditorMode::Select;
            self.editor.cursor = selection.cursor;
        }
        self.floating_source_bounds = None;
        self.suppress_swatch_preview = false;
    }
}

fn owner_initial(username: &str) -> char {
    username
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .unwrap_or('?')
}

fn owner_color(username: &str) -> RgbColor {
    const OWNER_PALETTE: [RgbColor; 8] = [
        RgbColor::new(255, 110, 64),
        RgbColor::new(255, 196, 64),
        RgbColor::new(145, 226, 88),
        RgbColor::new(72, 220, 170),
        RgbColor::new(84, 196, 255),
        RgbColor::new(128, 163, 255),
        RgbColor::new(192, 132, 255),
        RgbColor::new(255, 124, 196),
    ];

    let idx = username
        .bytes()
        .fold(0usize, |acc, byte| acc.wrapping_add(byte as usize))
        % OWNER_PALETTE.len();
    OWNER_PALETTE[idx]
}

fn palette_index(color: RgbColor) -> Option<usize> {
    PAINT_PALETTE
        .iter()
        .position(|candidate| *candidate == color)
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

/// How many decoded archives a session keeps so stepping back through
/// the list is free.
const ARCHIVE_CACHE_SIZE: usize = 8;

#[derive(Default)]
struct ArchiveList {
    entries: Vec<ArtboardArchiveEntry>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    selected: usize,
    scroll: usize,
}

/// The archive side of the rail: one key list per kind, the snapshot the
/// cursor asks for, the one on the board, and the few seen lately.
#[derive(Default)]
struct ArchiveBrowser {
    lists: [ArchiveList; 3],
    /// The key under the cursor, the one the board should show.
    wanted: Option<(ArtboardSnapshotKind, String)>,
    /// The key a fetch is out for. One at a time.
    inflight: Option<String>,
    cache: Vec<ArtboardArchiveSnapshot>,
    /// The archive on the board, replacing the live snapshot.
    active: Option<ArtboardArchiveSnapshot>,
    load_error: Option<String>,
    visible_height: Cell<usize>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HelpTab {
    #[default]
    Overview,
    Drawing,
    Brushes,
    Session,
}

impl HelpTab {
    pub const ALL: [HelpTab; 4] = [
        HelpTab::Overview,
        HelpTab::Drawing,
        HelpTab::Brushes,
        HelpTab::Session,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HelpTab::Overview => "Overview",
            HelpTab::Drawing => "Drawing",
            HelpTab::Brushes => "Brushes",
            HelpTab::Session => "Session",
        }
    }

    pub fn index(self) -> usize {
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

/// Where an edit can write on the board, so its diff reads that area and
/// not the whole board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditReach {
    /// The edit never writes to the board.
    Nothing,
    /// Only cells inside these bounds can change, give or take the other
    /// half of a wide glyph the edit overlaps.
    Within(Bounds),
    /// The edit can move glyphs anywhere on the board.
    Anywhere,
}

/// `bounds` plus a column either side, kept on the board: a write can clear
/// a wide glyph whose other half sits just outside the cells it touched.
fn widen_for_wide_glyphs(bounds: Bounds, canvas: &Canvas) -> Bounds {
    let last_x = canvas.width.saturating_sub(1);
    let last_y = canvas.height.saturating_sub(1);
    Bounds {
        min_x: bounds.min_x.saturating_sub(1).min(last_x),
        max_x: (bounds.max_x + 1).min(last_x),
        min_y: bounds.min_y.min(last_y),
        max_y: bounds.max_y.min(last_y),
    }
}

/// The glyphs whose origin lies inside `area`, on an empty board the size
/// of `canvas`: all a diff confined to `area` reads.
fn crop_canvas(canvas: &Canvas, area: Bounds) -> Canvas {
    let mut crop = Canvas::with_size(canvas.width, canvas.height);
    for y in area.min_y..=area.max_y {
        for x in area.min_x..=area.max_x {
            let pos = Pos { x, y };
            let Some(glyph) = canvas.glyph_at(pos) else {
                continue;
            };
            if glyph.pos != pos {
                continue;
            }
            let _ = match glyph.fg {
                Some(fg) => crop.put_glyph_colored(pos, glyph.ch, fg),
                None => crop.put_glyph(pos, glyph.ch),
            };
        }
    }
    crop
}

fn union_bounds(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        min_x: a.min_x.min(b.min_x),
        max_x: a.max_x.max(b.max_x),
        min_y: a.min_y.min(b.min_y),
        max_y: a.max_y.max(b.max_y),
    }
}

/// The cells a clipboard stamped at `pos` covers.
fn stamp_bounds(pos: Pos, clipboard: &Clipboard) -> Bounds {
    Bounds {
        min_x: pos.x,
        max_x: pos.x + clipboard.width.saturating_sub(1),
        min_y: pos.y,
        max_y: pos.y + clipboard.height.saturating_sub(1),
    }
}

/// Backspace steps left, then clears the glyph there.
fn backspace_bounds(cursor: Pos) -> Bounds {
    Bounds::from_points(
        Pos {
            x: cursor.x.saturating_sub(1),
            y: cursor.y,
        },
        cursor,
    )
}

/// The transparent stamp a terminal paste arms: spaces become blank cells,
/// rows clip at the canvas size. The glyph picker writes its text straight
/// to the canvas through `paste_text_block` instead, so a change to line or
/// width handling here should check that path too.
fn clipboard_from_text(text: &str, max_width: usize, max_height: usize) -> Option<Clipboard> {
    if max_width == 0 || max_height == 0 {
        return None;
    }
    let mut rows: Vec<Vec<Option<CellValue>>> = Vec::new();
    let mut current_row: Vec<Option<CellValue>> = Vec::new();
    let mut width = 0;
    let mut has_glyph = false;
    // Set once something on the row does not fit: the rest of the row is cut
    // at the edge, so a narrow glyph never closes up behind a clipped wide one.
    let mut row_clipped = false;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if matches!(chars.peek(), Some('\n')) {
                    chars.next();
                }
                width = width.max(current_row.len());
                rows.push(std::mem::take(&mut current_row));
                row_clipped = false;
                if rows.len() >= max_height {
                    break;
                }
            }
            '\n' => {
                width = width.max(current_row.len());
                rows.push(std::mem::take(&mut current_row));
                row_clipped = false;
                if rows.len() >= max_height {
                    break;
                }
            }
            ch if ch.is_control() => {}
            _ if row_clipped => {}
            ' ' => {
                if current_row.len() < max_width {
                    current_row.push(None);
                } else {
                    row_clipped = true;
                }
            }
            ch => match Canvas::display_width(ch) {
                0 => {}
                2 => {
                    if current_row.len() + 2 <= max_width {
                        has_glyph = true;
                        current_row.push(Some(CellValue::Wide(ch)));
                        current_row.push(Some(CellValue::WideCont));
                    } else {
                        row_clipped = true;
                    }
                }
                _ => {
                    if current_row.len() < max_width {
                        has_glyph = true;
                        current_row.push(Some(CellValue::Narrow(ch)));
                    } else {
                        row_clipped = true;
                    }
                }
            },
        }
    }
    if rows.len() < max_height {
        width = width.max(current_row.len());
        rows.push(current_row);
    }

    if width == 0 || !has_glyph {
        return None;
    }

    let height = rows.len();
    let mut cells = Vec::with_capacity(width * height);
    for mut row in rows {
        row.resize(width, None);
        cells.extend(row);
    }
    Some(Clipboard::new(width, height, cells))
}

/// The cells `paste_text_block` writes when `text` lands at `start`: one row
/// per line, as wide as the widest line in display cells.
fn text_block_bounds(start: Pos, text: &str) -> Bounds {
    let mut widest = 0;
    let mut line = 0;
    let mut lines = 1;
    for ch in text.chars() {
        match ch {
            '\r' => {}
            '\n' => {
                lines += 1;
                line = 0;
            }
            ch => {
                line += Canvas::display_width(ch);
                widest = widest.max(line);
            }
        }
    }
    Bounds {
        min_x: start.x,
        max_x: start.x + widest.saturating_sub(1),
        min_y: start.y,
        max_y: start.y + lines - 1,
    }
}

fn clear_bounds_on(canvas: &mut Canvas, bounds: Bounds) {
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let pos = Pos { x, y };
            if let Some(origin) = canvas.glyph_origin(pos)
                && origin.x >= bounds.min_x
                && origin.x <= bounds.max_x
                && origin.y >= bounds.min_y
                && origin.y <= bounds.max_y
            {
                canvas.clear(pos);
            }
        }
    }
}

/// Where the cursor lands after `paste_text_block` writes `text` at `start`.
fn paste_cursor_end(start: Pos, text: &str, width: usize, height: usize) -> Pos {
    let mut cursor = start;
    for ch in text.chars() {
        if cursor.y >= height {
            break;
        }
        match ch {
            '\r' => {}
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

fn capture_bounds_colors(canvas: &Canvas, bounds: Bounds) -> Vec<Option<RgbColor>> {
    let mut colors = Vec::with_capacity(bounds.width() * bounds.height());
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            colors.push(canvas.fg(Pos { x, y }));
        }
    }
    colors
}

fn selection_covers_cell(canvas: &Canvas, selection: EditorSelection, pos: Pos) -> bool {
    super::ui::selection_covers_cell(
        canvas,
        SelectionView {
            anchor: selection.anchor,
            cursor: selection.cursor,
            shape: selection_shape_to_tui(selection.shape),
        },
        pos,
    )
}

fn capture_selection_colors(canvas: &Canvas, selection: EditorSelection) -> Vec<Option<RgbColor>> {
    let bounds = selection.bounds().normalized_for_canvas(canvas);
    let mut colors = Vec::with_capacity(bounds.width() * bounds.height());
    for y in bounds.min_y..=bounds.max_y {
        for x in bounds.min_x..=bounds.max_x {
            let pos = Pos { x, y };
            let include = selection_covers_cell(canvas, selection, pos);
            colors.push(include.then(|| canvas.fg(pos)).flatten());
        }
    }
    colors
}

fn push_swatch_colors(
    swatch_colors: &mut [Option<Vec<Option<RgbColor>>>; SWATCH_CAPACITY],
    swatches: &[Option<Swatch>; SWATCH_CAPACITY],
    colors: Vec<Option<RgbColor>>,
) {
    let unpinned_slots: Vec<usize> = (0..SWATCH_CAPACITY)
        .filter(|&i| !matches!(&swatches[i], Some(swatch) if swatch.pinned))
        .collect();
    if unpinned_slots.is_empty() {
        return;
    }

    let mut queue: Vec<Option<Vec<Option<RgbColor>>>> = unpinned_slots
        .iter()
        .filter(|&&i| swatches[i].is_some())
        .map(|&i| swatch_colors[i].take())
        .collect();
    queue.insert(0, Some(colors));
    queue.truncate(unpinned_slots.len());

    for &slot_idx in &unpinned_slots {
        swatch_colors[slot_idx] = None;
    }
    for (slot_idx, slot_colors) in unpinned_slots.iter().zip(queue) {
        swatch_colors[*slot_idx] = slot_colors;
    }
}

fn stamp_clipboard_colored(
    canvas: &mut Canvas,
    clipboard: &Clipboard,
    colors: Option<&[Option<RgbColor>]>,
    pos: Pos,
    fallback_color: RgbColor,
    transparent: bool,
) {
    for y in 0..clipboard.height {
        for x in 0..clipboard.width {
            let target_x = pos.x + x;
            let target_y = pos.y + y;
            if target_x >= canvas.width || target_y >= canvas.height {
                continue;
            }
            let target = Pos {
                x: target_x,
                y: target_y,
            };
            match clipboard.get(x, y) {
                Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) => {
                    let fg = colors
                        .and_then(|c| c.get(y * clipboard.width + x).copied().flatten())
                        .unwrap_or(fallback_color);
                    let _ = canvas.put_glyph_colored(target, ch, fg);
                }
                Some(CellValue::WideCont) => {}
                None if !transparent => canvas.clear(target),
                None => {}
            }
        }
    }
}

fn paint_floating_pointer_colored(
    editor: &mut EditorSession,
    canvas: &mut Canvas,
    mouse: AppPointerEvent,
    colors: Option<&[Option<RgbColor>]>,
    fallback_color: RgbColor,
) -> EditorPointerDispatch {
    let canvas_pos = editor.canvas_pos_for_pointer(mouse.column, mouse.row, canvas);
    match mouse.kind {
        AppPointerKind::Down(AppPointerButton::Left) => {
            if let Some(pos) = canvas_pos {
                editor.cursor = pos;
                editor_begin_paint_stroke(editor);
                paint_floating_drag_colored(editor, canvas, pos, colors, fallback_color);
                return EditorPointerDispatch {
                    outcome: PointerOutcome::Consumed,
                    stroke_hint: Some(PointerStrokeHint::Begin),
                };
            }
            EditorPointerDispatch::default()
        }
        AppPointerKind::Drag(AppPointerButton::Left) => {
            if let Some(pos) = canvas_pos {
                paint_floating_drag_colored(editor, canvas, pos, colors, fallback_color);
                return EditorPointerDispatch {
                    outcome: PointerOutcome::Consumed,
                    stroke_hint: None,
                };
            }
            if editor.paint_stroke_anchor.is_some() {
                return EditorPointerDispatch {
                    outcome: PointerOutcome::Consumed,
                    stroke_hint: None,
                };
            }
            EditorPointerDispatch::default()
        }
        _ => editor_handle_pointer(editor, canvas, mouse, fallback_color),
    }
}

fn paint_floating_at_cursor_colored(
    editor: &mut EditorSession,
    canvas: &mut Canvas,
    colors: Option<&[Option<RgbColor>]>,
    fallback_color: RgbColor,
) -> bool {
    let Some(floating) = editor.floating.as_ref() else {
        return false;
    };
    stamp_clipboard_colored(
        canvas,
        &floating.clipboard,
        colors,
        editor.cursor,
        fallback_color,
        floating.transparent,
    );
    editor.paint_stroke_last = Some(editor.cursor);
    true
}

fn paint_floating_drag_colored(
    editor: &mut EditorSession,
    canvas: &mut Canvas,
    raw_pos: Pos,
    colors: Option<&[Option<RgbColor>]>,
    fallback_color: RgbColor,
) -> bool {
    let Some(last) = editor.paint_stroke_last else {
        editor.cursor = raw_pos;
        return paint_floating_at_cursor_colored(editor, canvas, colors, fallback_color);
    };

    let anchor = editor.paint_stroke_anchor.unwrap_or(last);
    let brush_width = editor.floating_brush_width();
    let is_pure_horizontal =
        brush_width > 1 && raw_pos.y == last.y && raw_pos.y == anchor.y && last.y == anchor.y;

    if is_pure_horizontal {
        let target = Pos {
            x: snap_horizontal_brush_x(anchor.x, raw_pos.x, brush_width),
            y: raw_pos.y,
        };
        if target == last {
            return false;
        }
        editor.cursor = target;
        return paint_floating_at_cursor_colored(editor, canvas, colors, fallback_color);
    }

    if brush_width > 1 && raw_pos.y == last.y {
        if raw_pos.x.abs_diff(last.x) < brush_width {
            return false;
        }
        editor.cursor = raw_pos;
        return paint_floating_at_cursor_colored(editor, canvas, colors, fallback_color);
    }

    if brush_width > 1 && raw_pos.y != last.y {
        return paint_floating_diagonal_segment_colored(
            editor,
            canvas,
            last,
            raw_pos,
            brush_width,
            colors,
            fallback_color,
        );
    }

    if raw_pos == last {
        return false;
    }

    editor.cursor = raw_pos;
    paint_floating_at_cursor_colored(editor, canvas, colors, fallback_color)
}

fn paint_floating_diagonal_segment_colored(
    editor: &mut EditorSession,
    canvas: &mut Canvas,
    start: Pos,
    end: Pos,
    brush_width: usize,
    colors: Option<&[Option<RgbColor>]>,
    fallback_color: RgbColor,
) -> bool {
    let mut changed = false;
    let mut last_stamped = start;
    for point in line_points(start, end).into_iter().skip(1) {
        let should_stamp =
            point.y != last_stamped.y || point.x.abs_diff(last_stamped.x) >= brush_width;
        if !should_stamp {
            continue;
        }

        editor.cursor = point;
        changed |= paint_floating_at_cursor_colored(editor, canvas, colors, fallback_color);
        last_stamped = point;
    }

    let should_stamp_end = end.y != last_stamped.y || end.x.abs_diff(last_stamped.x) >= brush_width;
    if should_stamp_end {
        editor.cursor = end;
        changed |= paint_floating_at_cursor_colored(editor, canvas, colors, fallback_color);
    }
    changed
}

fn snap_horizontal_brush_x(anchor_x: usize, raw_x: usize, brush_width: usize) -> usize {
    if brush_width <= 1 {
        return raw_x;
    }

    if raw_x >= anchor_x {
        anchor_x + ((raw_x - anchor_x) / brush_width) * brush_width
    } else {
        anchor_x - ((anchor_x - raw_x) / brush_width) * brush_width
    }
}

fn line_points(start: Pos, end: Pos) -> Vec<Pos> {
    let mut points = Vec::new();
    let mut x = start.x as isize;
    let mut y = start.y as isize;
    let target_x = end.x as isize;
    let target_y = end.y as isize;
    let dx = (target_x - x).abs();
    let sx = if x < target_x { 1 } else { -1 };
    let dy = -(target_y - y).abs();
    let sy = if y < target_y { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        points.push(Pos {
            x: x as usize,
            y: y as usize,
        });

        if x == target_x && y == target_y {
            break;
        }

        let twice_err = 2 * err;
        if twice_err >= dy {
            err += dy;
            x += sx;
        }
        if twice_err <= dx {
            err += dx;
            y += sy;
        }
    }

    points
}

#[cfg(test)]
#[path = "state_internal_test.rs"]
mod state_internal_test;
