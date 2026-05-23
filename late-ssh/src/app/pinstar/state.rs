use crate::app::pinstar::data::{CanvasData, DiagramLockMode};
use anyhow::Result;
use ratatui_textarea::{TextArea, WrapMode};

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
use std::path::{Path, PathBuf};

/// Dual mode: local file editing vs shared collaborative diagram.
#[derive(Clone)]
pub enum PinstarMode {
    Local {
        path: PathBuf,
    },
    Shared {
        service: crate::app::pinstar::svc::PinstarService,
        role: String, // "owner" | "editor" | "viewer"
    },
}

impl PinstarMode {
    pub fn is_viewer(&self) -> bool {
        matches!(self, PinstarMode::Shared { role, .. } if role == "viewer")
    }
}

#[derive(Clone)]
pub struct PinstarSnapshot {
    pub data: CanvasData,
}

pub struct PinstarState {
    pub path: PathBuf,
    pub data: CanvasData,
    pub mode: PinstarMode,
    pub viewport_x: f64,
    pub viewport_y: f64,
    pub zoom: f64,
    pub selected_node_id: Option<String>,
    pub selected_edge_id: Option<String>,
    pub floating_editor: Option<TextArea<'static>>,
    pub raw_editor: TextArea<'static>,
    pub editor_focus: bool,
    pub last_mouse_pos: Option<(u16, u16)>,
    pub last_click: Option<(u16, u16, std::time::Instant)>,
    pub context_menu: Option<PinstarContextMenu>,
    pub context_menu_pos: (f64, f64),
    pub connection_source_id: Option<String>,
    pub resizing_node_id: Option<String>,
    pub is_dragging_resize_handle: bool,
    pub deleting_connection_source_id: Option<String>,
    pub show_editor_pane: bool,
    pub drag_start_pos: Option<(f64, f64)>,
    pub rename_popup: Option<TextArea<'static>>,
    pub last_mouse_canvas_pos: Option<(f64, f64)>,
    pub drag_captured_nodes: std::collections::HashSet<String>,
    pub drag_group_children: std::collections::HashSet<String>,
    pub show_grid: bool,
    pub mouse_selecting: bool,
    pub mouse_dragged: bool,
    pub locked: bool,
    pub last_modified: std::time::SystemTime,
    pub orthogonal_connections: bool,
    pub show_help: bool,
    pub select_rect_start: Option<(f64, f64)>,
    pub select_rect_end: Option<(f64, f64)>,
    pub undo_stack: Vec<PinstarSnapshot>,
    pub redo_stack: Vec<PinstarSnapshot>,
    pub last_synced_seq: u64,
    pub synced_once: bool,
    pub show_invite_dialog: bool,
    pub invite_token: Option<String>,
    pub invite_error: Option<String>,
    pub invite_result_rx: Option<tokio::sync::oneshot::Receiver<Result<String, String>>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinstarMenuType {
    Canvas,
    Editor,
    ColorPicker,
    ShapePicker,
    EdgeMenu,
    EdgeColorPicker,
    EdgeStylePicker,
    OrientationPicker,
}

pub struct PinstarContextMenu {
    pub x: u16,
    pub y: u16,
    pub selected: usize,
    pub items: Vec<String>,
    pub menu_type: PinstarMenuType,
}

impl PinstarState {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        let mut data: CanvasData = serde_json::from_str(&content)?;
        if data.lock_mode == DiagramLockMode::Unlocked && data.locked {
            data.lock_mode = DiagramLockMode::All;
        }
        data.locked = matches!(data.lock_mode, DiagramLockMode::All);
        let mut raw_editor = TextArea::from(content.lines().map(String::from).collect::<Vec<_>>());
        raw_editor.set_cursor_line_style(ratatui::style::Style::default());
        raw_editor.set_wrap_mode(WrapMode::WordOrGlyph);

        let last_modified = std::fs::metadata(path)
            .and_then(|m| m.modified())
            .unwrap_or_else(|_| std::time::SystemTime::now());

        Ok(Self {
            path: path.to_path_buf(),
            data: data.clone(),
            mode: PinstarMode::Local {
                path: path.to_path_buf(),
            },
            locked: matches!(data.lock_mode, DiagramLockMode::All),
            last_modified,
            viewport_x: 0.0,
            viewport_y: 0.0,
            zoom: 0.1,
            selected_node_id: None,
            selected_edge_id: None,
            floating_editor: None,
            raw_editor,
            editor_focus: false,
            last_mouse_pos: None,
            last_click: None,
            context_menu: None,
            context_menu_pos: (0.0, 0.0),
            connection_source_id: None,
            resizing_node_id: None,
            is_dragging_resize_handle: false,
            deleting_connection_source_id: None,
            show_editor_pane: false,
            drag_start_pos: None,
            rename_popup: None,
            last_mouse_canvas_pos: None,
            drag_captured_nodes: std::collections::HashSet::new(),
            drag_group_children: std::collections::HashSet::new(),
            show_grid: true,
            mouse_selecting: false,
            mouse_dragged: false,
            orthogonal_connections: false,
            show_help: false,
            select_rect_start: None,
            select_rect_end: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_synced_seq: 0,
            synced_once: false,
            show_invite_dialog: false,
            invite_token: None,
            invite_error: None,
            invite_result_rx: None,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        self.normalize_lock_fields();
        self.locked = self.is_editing_locked_for_current_user();

        if let PinstarMode::Shared { .. } = &self.mode {
            self.submit_op(crate::app::pinstar::data::PinstarOp::ReplaceAll(
                self.data.clone(),
            ));
            return Ok(());
        }

        let content = serde_json::to_string_pretty(&self.data)?;
        std::fs::write(&self.path, &content)?;

        self.raw_editor = TextArea::from(content.lines().map(String::from).collect::<Vec<_>>());
        self.raw_editor
            .set_cursor_line_style(ratatui::style::Style::default());
        self.raw_editor.set_wrap_mode(WrapMode::WordOrGlyph);

        if let Ok(metadata) = std::fs::metadata(&self.path) {
            if let Ok(modified) = metadata.modified() {
                self.last_modified = modified;
            }
        }
        Ok(())
    }

    pub fn record_undo_state(&mut self) {
        let snapshot = PinstarSnapshot {
            data: self.data.clone(),
        };
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    pub fn undo(&mut self) -> Result<()> {
        if !self.check_mutation_permission() {
            return Ok(());
        }

        if let Some(snapshot) = self.undo_stack.pop() {
            let current = PinstarSnapshot {
                data: self.data.clone(),
            };
            self.redo_stack.push(current);

            self.data = snapshot.data;
            self.refresh_lock_state();

            // Clean up dangling selection references
            if let Some(sel_id) = &self.selected_node_id {
                if !self.data.nodes.iter().any(|n| n.id() == sel_id) {
                    self.selected_node_id = None;
                    self.drag_captured_nodes.clear();
                }
            }

            self.save()?;
            self.sync_raw_editor_from_data();
        }
        Ok(())
    }

    pub fn redo(&mut self) -> Result<()> {
        if !self.check_mutation_permission() {
            return Ok(());
        }

        if let Some(snapshot) = self.redo_stack.pop() {
            let current = PinstarSnapshot {
                data: self.data.clone(),
            };
            self.undo_stack.push(current);

            self.data = snapshot.data;
            self.refresh_lock_state();

            if let Some(sel_id) = &self.selected_node_id {
                if !self.data.nodes.iter().any(|n| n.id() == sel_id) {
                    self.selected_node_id = None;
                    self.drag_captured_nodes.clear();
                }
            }

            self.save()?;
            self.sync_raw_editor_from_data();
        }
        Ok(())
    }

    /// Sync the raw editor content from the current canvas data.
    /// Called after undo/redo or snapshot updates.
    fn sync_raw_editor_from_data(&mut self) {
        let new_content = serde_json::to_string_pretty(&self.data).unwrap_or_default();
        let new_lines: Vec<String> = new_content.lines().map(String::from).collect();
        self.raw_editor = TextArea::from(new_lines);
        self.raw_editor
            .set_cursor_line_style(ratatui::style::Style::default());
        self.raw_editor.set_wrap_mode(WrapMode::WordOrGlyph);
    }

    /// Create a PinstarState connected to a shared collaborative diagram.
    pub fn new_shared(
        service: crate::app::pinstar::svc::PinstarService,
        role: String,
        _title: String,
    ) -> Self {
        let snapshot = service.snapshot();
        let mut data = snapshot.data.clone();
        if data.lock_mode == DiagramLockMode::Unlocked && data.locked {
            data.lock_mode = DiagramLockMode::All;
        }
        data.locked = matches!(data.lock_mode, DiagramLockMode::All);
        let is_viewer = role == "viewer";
        let is_locked = is_viewer
            || matches!(data.lock_mode, DiagramLockMode::All)
            || (matches!(data.lock_mode, DiagramLockMode::EditorOnly) && role != "owner");
        let content = serde_json::to_string_pretty(&data).unwrap_or_default();
        let mut raw_editor = TextArea::from(content.lines().map(String::from).collect::<Vec<_>>());
        raw_editor.set_cursor_line_style(ratatui::style::Style::default());
        raw_editor.set_wrap_mode(WrapMode::WordOrGlyph);

        Self {
            path: PathBuf::from(format!("shared://{}", snapshot.diagram_id)),
            data,
            mode: PinstarMode::Shared {
                service,
                role: role.clone(),
            },
            viewport_x: 0.0,
            viewport_y: 0.0,
            zoom: 0.1,
            selected_node_id: None,
            selected_edge_id: None,
            floating_editor: None,
            raw_editor,
            editor_focus: false,
            last_mouse_pos: None,
            last_click: None,
            context_menu: None,
            context_menu_pos: (0.0, 0.0),
            connection_source_id: None,
            resizing_node_id: None,
            is_dragging_resize_handle: false,
            deleting_connection_source_id: None,
            show_editor_pane: false,
            drag_start_pos: None,
            rename_popup: None,
            last_mouse_canvas_pos: None,
            drag_captured_nodes: std::collections::HashSet::new(),
            drag_group_children: std::collections::HashSet::new(),
            show_grid: true,
            mouse_selecting: false,
            mouse_dragged: false,
            locked: is_locked,
            last_modified: std::time::SystemTime::now(),
            orthogonal_connections: false,
            show_help: false,
            select_rect_start: None,
            select_rect_end: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_synced_seq: 0,
            synced_once: false,
            show_invite_dialog: false,
            invite_token: None,
            invite_error: None,
            invite_result_rx: None,
        }
    }

    /// Returns true if this state is in shared (collaborative) mode.
    pub fn is_shared(&self) -> bool {
        matches!(self.mode, PinstarMode::Shared { .. })
    }

    pub fn generate_invite(&mut self, db: late_core::db::Db, _role: String) {
        if self.invite_result_rx.is_some() {
            return;
        }

        let (diagram_id, user_id) = match &self.mode {
            PinstarMode::Local { .. } => {
                self.invite_error =
                    Some("Invites are only available for collaborative diagrams".to_string());
                return;
            }
            PinstarMode::Shared { service, .. } => {
                let snapshot = service.snapshot();
                let Some(user_id) = snapshot.your_user_id else {
                    self.invite_error =
                        Some("User ID is missing. Try again in a moment.".to_string());
                    return;
                };
                (service.diagram_id(), user_id)
            }
        };

        if self.role() != "owner" {
            self.invite_error = Some("Only the owner can create invite links".to_string());
            return;
        }

        if diagram_id.is_nil() {
            self.invite_error = Some("Diagram ID is missing. Try again in a moment.".to_string());
            return;
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.invite_result_rx = Some(rx);
        self.invite_token = None;
        self.invite_error = None;

        tokio::spawn(async move {
            let res = tokio::time::timeout(std::time::Duration::from_secs(15), async {
                crate::app::pinstar::browser::create_invite_for_owner(&db, user_id, diagram_id)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap_or_else(|_| Err("Invite generation timed out".to_string()));

            let _ = tx.send(res);
        });
    }

    /// Returns true if the user is a viewer (read-only) in shared mode.
    pub fn is_viewer(&self) -> bool {
        self.mode.is_viewer()
    }

    pub fn lock_mode(&self) -> DiagramLockMode {
        if self.data.lock_mode == DiagramLockMode::Unlocked && self.data.locked {
            DiagramLockMode::All
        } else {
            self.data.lock_mode
        }
    }

    fn normalize_lock_fields(&mut self) {
        if self.data.lock_mode == DiagramLockMode::Unlocked && self.data.locked {
            self.data.lock_mode = DiagramLockMode::All;
        }
        self.data.locked = matches!(self.data.lock_mode, DiagramLockMode::All);
    }

    pub fn refresh_lock_state(&mut self) {
        self.normalize_lock_fields();
        self.locked = self.is_editing_locked_for_current_user();
    }

    pub fn set_lock_mode(&mut self, mode: DiagramLockMode) {
        self.data.lock_mode = mode;
        self.data.locked = matches!(mode, DiagramLockMode::All);
        self.locked = self.is_editing_locked_for_current_user();
    }

    pub fn is_editing_locked_for_current_user(&self) -> bool {
        if self.is_viewer() {
            return true;
        }

        match self.lock_mode() {
            DiagramLockMode::Unlocked => false,
            DiagramLockMode::All => true,
            DiagramLockMode::EditorOnly => self.role() != "owner",
        }
    }

    pub fn check_mutation_permission(&self) -> bool {
        !self.is_editing_locked_for_current_user()
    }

    /// Submit a mutation op in shared mode. No-op in local mode.
    pub fn submit_op(&self, op: crate::app::pinstar::data::PinstarOp) {
        if let PinstarMode::Shared { service, .. } = &self.mode {
            // Use a simple monotonic counter based on last_modified
            let seq = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            service.submit_op(seq, op);
        }
    }

    /// Drain incoming events from the shared service. Returns ops to apply.
    pub fn drain_service_events(&mut self) -> Vec<crate::app::pinstar::data::PinstarOp> {
        let PinstarMode::Shared { service, .. } = &self.mode else {
            return Vec::new();
        };

        let ops = Vec::new();
        // Check for snapshot updates
        let snapshot = service.snapshot();
        if !self.synced_once || snapshot.last_seq > self.last_synced_seq {
            self.data = snapshot.data.clone();
            self.refresh_lock_state();
            self.last_synced_seq = snapshot.last_seq;
            self.synced_once = true;
            self.sync_raw_editor_from_data();
        }

        ops
    }

    /// Get the peer list from the shared service.
    pub fn peers(&self) -> Vec<crate::app::pinstar::data::PinstarPeer> {
        if let PinstarMode::Shared { service, .. } = &self.mode {
            service.snapshot().peers
        } else {
            Vec::new()
        }
    }

    /// Get the current user's role.
    pub fn role(&self) -> &str {
        match &self.mode {
            PinstarMode::Shared { role, .. } => role,
            PinstarMode::Local { .. } => "owner",
        }
    }

    pub fn reload(&mut self) -> Result<()> {
        let content = std::fs::read_to_string(&self.path)?;
        let data: CanvasData = serde_json::from_str(&content)?;
        self.data = data;
        self.refresh_lock_state();
        self.raw_editor = TextArea::from(content.lines().map(String::from).collect::<Vec<_>>());
        self.raw_editor
            .set_cursor_line_style(ratatui::style::Style::default());
        self.raw_editor.set_wrap_mode(WrapMode::WordOrGlyph);

        if let Some(sel_id) = &self.selected_node_id {
            if !self.data.nodes.iter().any(|n| n.id() == sel_id) {
                self.selected_node_id = None;
            }
        }
        if let Some(sel_id) = &self.selected_edge_id {
            if !self.data.edges.iter().any(|e| e.id == *sel_id) {
                self.selected_edge_id = None;
            }
        }

        if let Ok(metadata) = std::fs::metadata(&self.path) {
            if let Ok(modified) = metadata.modified() {
                self.last_modified = modified;
            }
        }
        Ok(())
    }

    pub fn sync_from_raw_editor(&mut self) -> Result<()> {
        if !self.check_mutation_permission() {
            anyhow::bail!("Diagram is locked")
        }

        let content = self.raw_editor.lines().join("\n");
        if let Ok(data) = serde_json::from_str::<CanvasData>(&content) {
            self.record_undo_state();
            self.data = data;
            self.refresh_lock_state();
            let _ = self.save();
            Ok(())
        } else {
            anyhow::bail!("Invalid diagram syntax in editor")
        }
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.viewport_x += dx / self.zoom;
        self.viewport_y += dy / self.zoom;
    }

    pub fn center_on_selected(&mut self) {
        if let Some(id) = &self.selected_node_id
            && let Some(node) = self.data.nodes.iter().find(|n| n.id() == id)
        {
            let (nx, ny) = node.pos();
            let (nw, nh) = node.size();
            self.viewport_x = nx + nw / 2.0;
            self.viewport_y = ny + nh / 2.0;
        }
    }

    pub fn zoom_in(&mut self) {
        self.zoom *= 1.1;
    }

    pub fn zoom_out(&mut self) {
        self.zoom /= 1.1;
    }

    pub fn fit_to_view(&mut self, area: ratatui::layout::Rect) {
        if self.data.nodes.is_empty() {
            return;
        }

        let min_x = self
            .data
            .nodes
            .iter()
            .map(|n| n.pos().0)
            .reduce(f64::min)
            .unwrap_or(0.0);
        let min_y = self
            .data
            .nodes
            .iter()
            .map(|n| n.pos().1)
            .reduce(f64::min)
            .unwrap_or(0.0);
        let max_x = self
            .data
            .nodes
            .iter()
            .map(|n| n.pos().0 + n.size().0)
            .reduce(f64::max)
            .unwrap_or(0.0);
        let max_y = self
            .data
            .nodes
            .iter()
            .map(|n| n.pos().1 + n.size().1)
            .reduce(f64::max)
            .unwrap_or(0.0);

        // Center of bounding box
        let cx = (min_x + max_x) / 2.0;
        let cy = (min_y + max_y) / 2.0;

        // Bounding box dimensions with padding
        let padding = 100.0;
        let bbox_w = (max_x - min_x) + padding * 2.0;
        let bbox_h = (max_y - min_y) + padding * 2.0;

        // Available canvas area (account for status bar)
        let avail_w = area.width as f64;
        let avail_h = (area.height.saturating_sub(1)) as f64;

        // Pick zoom that fits the bounding box
        let zoom_x = if bbox_w > 0.0 { avail_w / bbox_w } else { 1.0 };
        let zoom_y = if bbox_h > 0.0 { avail_h / bbox_h } else { 1.0 };
        let zoom = zoom_x.min(zoom_y);

        // Clamp zoom to reasonable range
        let zoom = zoom.clamp(0.01, 10.0);

        self.viewport_x = cx;
        self.viewport_y = cy;
        self.zoom = zoom;
    }

    pub fn screen_to_canvas(&self, sx: u16, sy: u16, area: ratatui::layout::Rect) -> (f64, f64) {
        let cx =
            (sx as f64 - (area.x as f64 + area.width as f64 / 2.0)) / self.zoom + self.viewport_x;
        let cy =
            (sy as f64 - (area.y as f64 + area.height as f64 / 2.0)) / self.zoom + self.viewport_y;
        (cx, cy)
    }

    pub fn node_at(&self, mx: u16, my: u16, area: ratatui::layout::Rect) -> Option<String> {
        let mut best_hit: Option<(String, f64, usize)> = None;
        let mx_i = mx as i32;
        let my_i = my as i32;

        for (idx, node) in self.data.nodes.iter().enumerate() {
            let (nx, ny) = node.pos();
            let (nw, nh) = node.size();

            // Compute exact screen coordinates identically to render.rs
            let sx =
                ((nx - self.viewport_x) * self.zoom) + (area.x as f64 + area.width as f64 / 2.0);
            let sy =
                ((ny - self.viewport_y) * self.zoom) + (area.y as f64 + area.height as f64 / 2.0);
            let sw = nw * self.zoom;
            let sh = nh * self.zoom;

            // Round to discrete screen grid coordinates
            let left = sx.round() as i32;
            let top = sy.round() as i32;
            let right = (sx + sw).round() as i32;
            let bottom = (sy + sh).round() as i32;

            let is_hit = if matches!(node, crate::app::pinstar::data::CanvasNode::Group(_)) {
                // Groups are selectable by their title area (top line + titlebar background line)
                mx_i >= left && mx_i < right && my_i >= top && my_i <= top + 1
            } else {
                // Standard nodes are selectable in their entire bounding rectangle
                mx_i >= left && mx_i < right && my_i >= top && my_i < bottom
            };

            if is_hit {
                let area_size = nw * nh;
                let should_replace = match &best_hit {
                    None => true,
                    Some((_, best_area, _)) if area_size < *best_area => true,
                    Some((_, best_area, best_idx))
                        if (area_size - *best_area).abs() < 0.0001 && idx > *best_idx =>
                    {
                        true
                    }
                    _ => false,
                };
                if should_replace {
                    best_hit = Some((node.id().to_string(), area_size, idx));
                }
            }
        }

        best_hit.map(|(id, _, _)| id)
    }

    pub fn select_node_at(
        &mut self,
        mx: u16,
        my: u16,
        area: ratatui::layout::Rect,
    ) -> Option<String> {
        if let Some(id) = self.node_at(mx, my, area) {
            self.selected_node_id = Some(id.clone());
            self.selected_edge_id = None;
            Some(id)
        } else {
            self.selected_node_id = None;
            self.drag_captured_nodes.clear();
            None
        }
    }

    pub fn select_nodes_in_rect(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) {
        let (min_x, max_x) = if x1 < x2 { (x1, x2) } else { (x2, x1) };
        let (min_y, max_y) = if y1 < y2 { (y1, y2) } else { (y2, y1) };
        let mut selected = std::collections::HashSet::new();

        for node in &self.data.nodes {
            let (nx, ny) = node.pos();
            let (nw, nh) = node.size();
            let cx = nx + nw / 2.0;
            let cy = ny + nh / 2.0;
            if cx >= min_x && cx <= max_x && cy >= min_y && cy <= max_y {
                selected.insert(node.id().to_string());
            }
        }

        // Set first as primary, rest as captured
        let mut ids: Vec<String> = selected.into_iter().collect();
        ids.sort();
        if let Some(primary) = ids.first().cloned() {
            self.selected_node_id = Some(primary);
            self.drag_captured_nodes = ids.into_iter().skip(1).collect();
            self.selected_edge_id = None;
        } else {
            self.selected_node_id = None;
            self.drag_captured_nodes.clear();

            self.selected_edge_id = None;

            // If no nodes inside the box, fallback to selecting intersecting connections
            let mut found_edge = None;
            let line_intersects_rect = |sx: f64,
                                        sy: f64,
                                        ex: f64,
                                        ey: f64,
                                        min_x: f64,
                                        min_y: f64,
                                        max_x: f64,
                                        max_y: f64|
             -> bool {
                let inside = |x: f64, y: f64| x >= min_x && x <= max_x && y >= min_y && y <= max_y;
                if inside(sx, sy) || inside(ex, ey) {
                    return true;
                }
                let intersect = |x1: f64,
                                 y1: f64,
                                 x2: f64,
                                 y2: f64,
                                 x3: f64,
                                 y3: f64,
                                 x4: f64,
                                 y4: f64|
                 -> bool {
                    let denom = (y4 - y3) * (x2 - x1) - (x4 - x3) * (y2 - y1);
                    if denom.abs() < 0.0001 {
                        return false;
                    }
                    let ua = ((x4 - x3) * (y1 - y3) - (y4 - y3) * (x1 - x3)) / denom;
                    let ub = ((x2 - x1) * (y1 - y3) - (y2 - y1) * (x1 - x3)) / denom;
                    ua >= 0.0 && ua <= 1.0 && ub >= 0.0 && ub <= 1.0
                };
                intersect(sx, sy, ex, ey, min_x, min_y, max_x, min_y)
                    || intersect(sx, sy, ex, ey, min_x, max_y, max_x, max_y)
                    || intersect(sx, sy, ex, ey, min_x, min_y, min_x, max_y)
                    || intersect(sx, sy, ex, ey, max_x, min_y, max_x, max_y)
            };

            for edge in &self.data.edges {
                if let Some(segments) = self.get_edge_segments(edge) {
                    let intersects = segments.iter().any(|&(sx, sy, ex, ey)| {
                        line_intersects_rect(sx, sy, ex, ey, min_x, min_y, max_x, max_y)
                    });
                    if intersects {
                        found_edge = Some(edge.id.clone());
                        break;
                    }
                }
            }
            self.selected_edge_id = found_edge;
        }
    }

    pub fn select_node_in_direction(&mut self, dx: f64, dy: f64) {
        let current_node = if let Some(id) = &self.selected_node_id {
            self.data.nodes.iter().find(|n| n.id() == id)
        } else {
            None
        };

        let (cur_x, cur_y) = if let Some(n) = current_node {
            let (nx, ny) = n.pos();
            let (nw, nh) = n.size();
            (nx + nw / 2.0, ny + nh / 2.0)
        } else {
            (self.viewport_x, self.viewport_y)
        };

        let mut best_node = None;
        let mut min_dist = f64::MAX;

        for node in &self.data.nodes {
            if let Some(id) = &self.selected_node_id
                && node.id() == id
            {
                continue;
            }

            let (nx, ny) = node.pos();
            let (nw, nh) = node.size();
            let (tx, ty) = (nx + nw / 2.0, ny + nh / 2.0);

            let v_x = tx - cur_x;
            let v_y = ty - cur_y;

            let dot = v_x * dx + v_y * dy;
            if dot <= 0.0 {
                continue;
            }

            let dist_sq = v_x * v_x + v_y * v_y;
            let ortho_dist = (v_x * -dy + v_y * dx).abs();
            let score = dist_sq + ortho_dist * ortho_dist * 2.0;

            if score < min_dist {
                min_dist = score;
                best_node = Some(node.id().to_string());
            }
        }

        if let Some(id) = best_node {
            self.selected_node_id = Some(id);
        } else if self.selected_node_id.is_none() && !self.data.nodes.is_empty() {
            self.selected_node_id = Some(self.data.nodes[0].id().to_string());
        }
    }

    pub fn toggle_editor(&mut self) {
        if self.floating_editor.is_some() {
            if let Some(node_id) = &self.selected_node_id {
                if self.check_mutation_permission() {
                    let text = self.floating_editor.as_ref().unwrap().lines().join("\n");
                    for node in &mut self.data.nodes {
                        if node.id() == node_id {
                            node.set_text(text);
                            break;
                        }
                    }
                    let _ = self.save();
                }
            }
            self.floating_editor = None;
        } else if let Some(node_id) = &self.selected_node_id {
            let text_opt = self
                .data
                .nodes
                .iter()
                .find(|n| n.id() == node_id)
                .map(|n| n.text().to_string());
            if let Some(text) = text_opt {
                if self.check_mutation_permission() {
                    self.record_undo_state();
                }
                let mut textarea =
                    TextArea::from(text.lines().map(String::from).collect::<Vec<_>>());
                textarea.set_cursor_line_style(ratatui::style::Style::default());
                textarea.set_wrap_mode(WrapMode::WordOrGlyph);
                self.floating_editor = Some(textarea);
            }
        }
    }

    pub fn open_context_menu(&mut self, x: u16, y: u16, canvas_x: f64, canvas_y: f64) {
        let items = if self.selected_node_id.is_some() {
            vec![
                "Create Connection".to_string(),
                "Delete Connection".to_string(),
                "Rename Node".to_string(),
                "Resize Node".to_string(),
                "Set Color...".to_string(),
                "Delete All Connections".to_string(),
                "Delete Node".to_string(),
            ]
        } else {
            vec!["Add Text Node".to_string(), "Add Group".to_string()]
        };

        self.context_menu_pos = (canvas_x, canvas_y);
        self.context_menu = Some(PinstarContextMenu {
            x,
            y,
            selected: 0,
            items,
            menu_type: PinstarMenuType::Canvas,
        });
    }

    pub fn open_editor_context_menu(&mut self, x: u16, y: u16) {
        let items = vec![
            "Copy".to_string(),
            "Cut".to_string(),
            "Paste".to_string(),
            "Select All".to_string(),
        ];

        self.context_menu = Some(PinstarContextMenu {
            x,
            y,
            selected: 0,
            items,
            menu_type: PinstarMenuType::Editor,
        });
    }

    pub fn start_resize(&mut self) {
        if !self.check_mutation_permission() {
            return;
        }
        let id_opt = self.selected_node_id.clone();
        if let Some(id) = id_opt {
            self.record_undo_state();
            self.resizing_node_id = Some(id);
            self.is_dragging_resize_handle = false;
            self.context_menu = None;
        }
    }

    pub fn start_delete_connection(&mut self) {
        if !self.check_mutation_permission() {
            return;
        }
        if let Some(id) = &self.selected_node_id {
            self.deleting_connection_source_id = Some(id.clone());
            self.context_menu = None;
        }
    }

    pub fn rename_node(&mut self, target_id: String) {
        if !self.check_mutation_permission() {
            return;
        }
        if let Some(old_id) = self.selected_node_id.take() {
            if old_id == target_id {
                self.selected_node_id = Some(old_id);
                return;
            }
            let final_id = if target_id.is_empty() {
                crate::app::pinstar::state::new_id()
            } else {
                target_id
            };
            let new_id = final_id;
            if new_id != old_id && self.data.nodes.iter().any(|n| n.id() == new_id) {
                self.selected_node_id = Some(old_id);
                return;
            }

            self.record_undo_state();

            for node in &mut self.data.nodes {
                match node {
                    crate::app::pinstar::data::CanvasNode::Text(n) if n.id == old_id => {
                        n.id = new_id.clone()
                    }
                    crate::app::pinstar::data::CanvasNode::File(n) if n.id == old_id => {
                        n.id = new_id.clone()
                    }
                    crate::app::pinstar::data::CanvasNode::Link(n) if n.id == old_id => {
                        n.id = new_id.clone()
                    }
                    crate::app::pinstar::data::CanvasNode::Group(n) if n.id == old_id => {
                        n.id = new_id.clone()
                    }
                    _ => {}
                }
            }

            for edge in &mut self.data.edges {
                if edge.from_node == old_id {
                    edge.from_node = new_id.clone();
                }
                if edge.to_node == old_id {
                    edge.to_node = new_id.clone();
                }
            }

            self.selected_node_id = Some(new_id);
            let _ = self.save();
        }
    }

    pub fn all_selected_node_ids(&self) -> std::collections::HashSet<String> {
        let mut ids = std::collections::HashSet::new();
        if let Some(id) = &self.selected_node_id {
            ids.insert(id.clone());
        }
        for id in &self.drag_captured_nodes {
            ids.insert(id.clone());
        }
        ids
    }

    pub fn delete_selected_nodes(&mut self) {
        if !self.check_mutation_permission() {
            return;
        }
        let ids = self.all_selected_node_ids();
        if !ids.is_empty() {
            self.record_undo_state();
            self.data.nodes.retain(|n| !ids.contains(n.id()));
            self.data
                .edges
                .retain(|e| !ids.contains(&e.from_node) && !ids.contains(&e.to_node));
            self.selected_node_id = None;
            self.drag_captured_nodes.clear();
            let _ = self.save();
        }
    }

    pub fn delete_node_connections(&mut self) {
        if !self.check_mutation_permission() {
            return;
        }
        let ids = self.all_selected_node_ids();
        if !ids.is_empty() {
            self.record_undo_state();
            self.data
                .edges
                .retain(|e| !ids.contains(&e.from_node) && !ids.contains(&e.to_node));
            let _ = self.save();
        }
    }

    pub fn set_node_color(&mut self, color: Option<String>) {
        if !self.check_mutation_permission() {
            return;
        }
        let ids = self.all_selected_node_ids();
        if !ids.is_empty() {
            self.record_undo_state();
            for node in &mut self.data.nodes {
                if ids.contains(node.id()) {
                    match node {
                        crate::app::pinstar::data::CanvasNode::Text(n) => n.color = color.clone(),
                        crate::app::pinstar::data::CanvasNode::File(n) => n.color = color.clone(),
                        crate::app::pinstar::data::CanvasNode::Link(n) => n.color = color.clone(),
                        crate::app::pinstar::data::CanvasNode::Group(n) => n.color = color.clone(),
                    }
                }
            }
            let _ = self.save();
        }
    }

    pub fn add_text_node(&mut self, x: f64, y: f64) {
        if !self.check_mutation_permission() {
            return;
        }
        self.record_undo_state();
        let id = format!("node_{}", &new_id()[..8]);
        self.data
            .nodes
            .push(crate::app::pinstar::data::CanvasNode::Text(
                crate::app::pinstar::data::TextNode {
                    id: id.clone(),
                    x,
                    y,
                    width: 200.0,
                    height: 100.0,
                    text: "".to_string(),
                    color: None,
                },
            ));
        self.selected_node_id = Some(id.clone());
        self.resizing_node_id = Some(id);
        let _ = self.save();
    }

    pub fn add_group(&mut self, x: f64, y: f64) {
        if !self.check_mutation_permission() {
            return;
        }
        self.record_undo_state();
        let id = format!("group_{}", &new_id()[..8]);
        self.data.nodes.insert(
            0,
            crate::app::pinstar::data::CanvasNode::Group(crate::app::pinstar::data::GroupNode {
                id: id.clone(),
                x,
                y,
                width: 400.0,
                height: 300.0,
                label: Some("New Group".to_string()),
                color: None,
            }),
        );
        self.selected_node_id = Some(id.clone());
        self.resizing_node_id = Some(id);
        let _ = self.save();
    }

    pub fn start_connection(&mut self) {
        if !self.check_mutation_permission() {
            return;
        }
        if let Some(id) = &self.selected_node_id {
            self.connection_source_id = Some(id.clone());
            self.context_menu = None;
        }
    }

    pub fn finish_connection(&mut self, target_id: &str) {
        if !self.check_mutation_permission() {
            return;
        }
        if let Some(source_id) = self.connection_source_id.take()
            && source_id != target_id
        {
            self.record_undo_state();
            let edge_id = format!("edge_{}_{}", source_id, target_id);
            if !self
                .data
                .edges
                .iter()
                .any(|e| e.from_node == source_id && e.to_node == target_id)
            {
                self.data.edges.push(crate::app::pinstar::data::CanvasEdge {
                    id: edge_id,
                    from_node: source_id,
                    from_side: Some("right".to_string()),
                    to_node: target_id.to_string(),
                    to_side: Some("left".to_string()),
                    label: None,
                    color: None,
                    style: Default::default(),
                });
                let _ = self.save();
            }
        }
    }

    pub fn finish_delete_connection(&mut self, target_id: &str) {
        if !self.check_mutation_permission() {
            return;
        }
        if let Some(source_id) = self.deleting_connection_source_id.take()
            && source_id != target_id
        {
            self.record_undo_state();
            self.data
                .edges
                .retain(|e| !(e.from_node == source_id && e.to_node == target_id));
            let _ = self.save();
        }
    }

    pub fn resize_selected_node(&mut self, dw: f64, dh: f64) {
        if !self.check_mutation_permission() {
            return;
        }
        if (dw.abs() > 0.001 || dh.abs() > 0.001) && self.resizing_node_id.is_some() {}
        if let Some(id) = &self.resizing_node_id {
            for node in &mut self.data.nodes {
                if node.id() == id {
                    match node {
                        crate::app::pinstar::data::CanvasNode::Text(n) => {
                            n.width = (n.width + dw).max(10.0);
                            n.height = (n.height + dh).max(10.0);
                        }
                        crate::app::pinstar::data::CanvasNode::File(n) => {
                            n.width = (n.width + dw).max(10.0);
                            n.height = (n.height + dh).max(10.0);
                        }
                        crate::app::pinstar::data::CanvasNode::Link(n) => {
                            n.width = (n.width + dw).max(10.0);
                            n.height = (n.height + dh).max(10.0);
                        }
                        crate::app::pinstar::data::CanvasNode::Group(n) => {
                            n.width = (n.width + dw).max(10.0);
                            n.height = (n.height + dh).max(10.0);
                        }
                    }
                    break;
                }
            }
        }
    }

    pub fn capture_group_children(&mut self) {
        self.drag_group_children.clear();
        let mut group_bounds = Vec::new();

        if let Some(id) = &self.selected_node_id {
            if let Some(node) = self.data.nodes.iter().find(|n| n.id() == id) {
                if let crate::app::pinstar::data::CanvasNode::Group(n) = node {
                    group_bounds.push((n.x, n.y, n.width, n.height));
                }
            }
        }
        for id in &self.drag_captured_nodes {
            if let Some(node) = self.data.nodes.iter().find(|n| n.id() == id) {
                if let crate::app::pinstar::data::CanvasNode::Group(n) = node {
                    group_bounds.push((n.x, n.y, n.width, n.height));
                }
            }
        }

        let mut to_capture = Vec::new();
        for (gx, gy, gw, gh) in group_bounds {
            for node in &self.data.nodes {
                let nid = node.id();
                if self.selected_node_id.as_ref().map_or(true, |id| id != nid)
                    && !self.drag_captured_nodes.contains(nid)
                {
                    let (nx, ny) = node.pos();
                    let (nw, nh) = node.size();
                    if nx >= gx && ny >= gy && (nx + nw) <= (gx + gw) && (ny + nh) <= (gy + gh) {
                        to_capture.push(nid.to_string());
                    }
                }
            }
        }

        for id in to_capture {
            self.drag_group_children.insert(id);
        }
    }

    pub fn move_selected_node(&mut self, dx: f64, dy: f64) {
        if !self.check_mutation_permission() {
            return;
        }
        if (dx.abs() > 0.001 || dy.abs() > 0.001)
            && (self.selected_node_id.is_some() || !self.drag_captured_nodes.is_empty())
        {}
        if self.selected_node_id.is_some() || !self.drag_captured_nodes.is_empty() {
            let mut to_move = std::collections::HashSet::new();
            if let Some(id) = &self.selected_node_id {
                to_move.insert(id.clone());
            }
            for id in &self.drag_captured_nodes {
                to_move.insert(id.clone());
            }
            for id in &self.drag_group_children {
                to_move.insert(id.clone());
            }

            for node in &mut self.data.nodes {
                let nid = node.id();
                if to_move.contains(nid) {
                    match node {
                        crate::app::pinstar::data::CanvasNode::Text(n) => {
                            n.x += dx;
                            n.y += dy;
                        }
                        crate::app::pinstar::data::CanvasNode::File(n) => {
                            n.x += dx;
                            n.y += dy;
                        }
                        crate::app::pinstar::data::CanvasNode::Link(n) => {
                            n.x += dx;
                            n.y += dy;
                        }
                        crate::app::pinstar::data::CanvasNode::Group(n) => {
                            n.x += dx;
                            n.y += dy;
                        }
                    }
                }
            }
        }
    }

    pub fn get_edge_segments(
        &self,
        edge: &crate::app::pinstar::data::CanvasEdge,
    ) -> Option<Vec<(f64, f64, f64, f64)>> {
        let from_node = self.data.nodes.iter().find(|n| n.id() == edge.from_node)?;
        let to_node = self.data.nodes.iter().find(|n| n.id() == edge.to_node)?;

        let (fx, fy) = from_node.pos();
        let (fw, fh) = from_node.size();
        let (tx, ty) = to_node.pos();
        let (tw, th) = to_node.size();

        let scx = fx + fw / 2.0;
        let scy = fy + fh / 2.0;
        let tcx = tx + tw / 2.0;
        let tcy = ty + th / 2.0;

        let dx = tcx - scx;
        let dy = tcy - scy;
        let is_horiz = dx.abs() > dy.abs();

        let (ax, ay) = if is_horiz {
            if dx > 0.0 { (fx + fw, scy) } else { (fx, scy) }
        } else {
            if dy > 0.0 { (scx, fy + fh) } else { (scx, fy) }
        };

        let (bx, by) = if is_horiz {
            if dx > 0.0 { (tx, tcy) } else { (tx + tw, tcy) }
        } else {
            if dy > 0.0 { (tcx, ty) } else { (tcx, ty + th) }
        };

        let use_orthogonal = self.orthogonal_connections;

        let segments = if use_orthogonal {
            if is_horiz {
                let mid_x = (ax + bx) / 2.0;
                vec![
                    (ax, ay, mid_x, ay),
                    (mid_x, ay, mid_x, by),
                    (mid_x, by, bx, by),
                ]
            } else {
                let mid_y = (ay + by) / 2.0;
                vec![
                    (ax, ay, ax, mid_y),
                    (ax, mid_y, bx, mid_y),
                    (bx, mid_y, bx, by),
                ]
            }
        } else {
            vec![(ax, ay, bx, by)]
        };

        Some(segments)
    }

    pub fn select_edge_at(&mut self, x: f64, y: f64) -> Option<String> {
        let tolerance = 5.0;
        let mut best: Option<(String, f64)> = None;

        for edge in &self.data.edges {
            if let Some(segments) = self.get_edge_segments(edge) {
                for &(sx, sy, ex, ey) in &segments {
                    let seg_dx = ex - sx;
                    let seg_dy = ey - sy;
                    let len2 = seg_dx * seg_dx + seg_dy * seg_dy;
                    let dist = if len2 == 0.0 {
                        ((x - sx).powi(2) + (y - sy).powi(2)).sqrt()
                    } else {
                        let t = ((x - sx) * seg_dx + (y - sy) * seg_dy) / len2;
                        let t = t.clamp(0.0, 1.0);
                        let px = sx + t * seg_dx;
                        let py = sy + t * seg_dy;
                        ((x - px).powi(2) + (y - py).powi(2)).sqrt()
                    };

                    if dist < tolerance {
                        let should_replace = match &best {
                            None => true,
                            Some((_, best_dist)) if dist < *best_dist => true,
                            _ => false,
                        };
                        if should_replace {
                            best = Some((edge.id.clone(), dist));
                        }
                    }
                }
            }
        }

        if let Some((id, _)) = best {
            self.selected_edge_id = Some(id.clone());
            self.selected_node_id = None;
            Some(id)
        } else {
            self.selected_edge_id = None;
            None
        }
    }

    pub fn set_edge_color(&mut self, color: Option<String>) {
        if !self.check_mutation_permission() {
            return;
        }
        let edge_id_opt = self.selected_edge_id.clone();
        if let Some(id) = edge_id_opt {
            self.record_undo_state();
            for edge in &mut self.data.edges {
                if edge.id == id {
                    edge.color = color.clone();
                    break;
                }
            }
            let _ = self.save();
        }
    }

    pub fn set_edge_style(&mut self, style: crate::app::pinstar::data::EdgeStyle) {
        if !self.check_mutation_permission() {
            return;
        }
        let edge_id_opt = self.selected_edge_id.clone();
        if let Some(id) = edge_id_opt {
            self.record_undo_state();
            for edge in &mut self.data.edges {
                if edge.id == id {
                    edge.style = style;
                    break;
                }
            }
            let _ = self.save();
        }
    }

    pub fn set_orientation(&mut self, orientation: crate::app::pinstar::data::DiagramOrientation) {
        if !self.check_mutation_permission() {
            return;
        }
        self.record_undo_state();
        self.data.orientation = orientation;
        let _ = self.save();
    }

    pub fn open_edge_context_menu(&mut self, x: u16, y: u16) {
        let items = vec!["Set Color...".to_string(), "Set Style...".to_string()];
        self.context_menu = Some(PinstarContextMenu {
            x,
            y,
            selected: 0,
            items,
            menu_type: PinstarMenuType::EdgeMenu,
        });
    }
}
