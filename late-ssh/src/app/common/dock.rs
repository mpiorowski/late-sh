//! Dockable-panel layout for the home screen.
//!
//! The home screen is three columns: a left rail, the centre chat, and a right
//! rail. Historically the two side rails were fixed-width and the right rail's
//! panels (visualizer / music / bonsai / lobby) sat in a fixed stack. This
//! module turns that into a **dockable, resizable** layout that a user can drive
//! with the mouse: drag a divider to resize a column, or drag a panel's header to
//! move it to the other side or reorder it.
//!
//! This file is the pure model + geometry: it owns the layout data, computes the
//! `Rect` for every column, panel, and draggable divider from a terminal area,
//! hit-tests a mouse point against those, and applies resize / dock edits with
//! sensible clamps. It renders nothing and reads no global state, so it is fully
//! unit-testable. The rendering and mouse wiring live in the home UI/input.

use ratatui::layout::Rect;

/// Which side column a panel is docked in. The centre (chat) is not a dock side;
/// it always takes the space the two rails leave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
}

/// The movable/resizable panels of the home screen. `RoomList` is the channel
/// rail; the rest mirror the right-sidebar components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DockPanel {
    RoomList,
    Visualizer,
    Music,
    Bonsai,
    Lobby,
}

impl DockPanel {
    pub const ALL: [DockPanel; 5] = [
        DockPanel::RoomList,
        DockPanel::Visualizer,
        DockPanel::Music,
        DockPanel::Bonsai,
        DockPanel::Lobby,
    ];

    /// Stable persistence key (never rename).
    pub fn key(self) -> &'static str {
        match self {
            DockPanel::RoomList => "room_list",
            DockPanel::Visualizer => "visualizer",
            DockPanel::Music => "music",
            DockPanel::Bonsai => "bonsai",
            DockPanel::Lobby => "lobby",
        }
    }

    pub fn from_key(key: &str) -> Option<DockPanel> {
        DockPanel::ALL.into_iter().find(|p| p.key() == key)
    }

    /// A short human label for the panel's drag header.
    pub fn label(self) -> &'static str {
        match self {
            DockPanel::RoomList => "rooms",
            DockPanel::Visualizer => "visualizer",
            DockPanel::Music => "music",
            DockPanel::Bonsai => "bonsai",
            DockPanel::Lobby => "lobby",
        }
    }
}

/// One panel's placement: which side, a vertical size weight (its share of its
/// column's height, relative to its siblings), and whether it is shown at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelSlot {
    pub panel: DockPanel,
    pub side: DockSide,
    /// Relative height weight within the column (min 1). Columns divide their
    /// height among enabled panels in proportion to these.
    pub weight: u16,
    pub enabled: bool,
}

/// The smallest a column may be dragged to, and the largest as a fraction of the
/// whole width (so the centre chat can never be squeezed away).
pub const MIN_COL_WIDTH: u16 = 14;
pub const MAX_COL_FRACTION: u16 = 40; // percent of total width per side column

/// The full home layout: the ordered panel slots plus the two column widths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DockLayout {
    /// Panels in display order (top-to-bottom within each side).
    pub slots: Vec<PanelSlot>,
    pub left_width: u16,
    pub right_width: u16,
}

/// A draggable divider between a side column and the centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Divider {
    Left,
    Right,
}

/// What a mouse point is over, for starting a drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A column border - dragging resizes that column.
    Divider(Divider),
    /// A panel's header row - dragging moves the panel.
    PanelHeader(DockPanel),
    /// A panel's body.
    PanelBody(DockPanel),
}

/// A drop target while dragging a panel: which side, and before which panel in
/// that side's stack (`None` = append to the end).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropZone {
    pub side: DockSide,
    pub before: Option<DockPanel>,
}

/// The computed geometry for one frame: the three column rects and, for each
/// visible panel, its rect (the header is its top row).
#[derive(Clone, Debug, Default)]
pub struct DockFrame {
    pub left: Option<Rect>,
    pub center: Rect,
    pub right: Option<Rect>,
    /// (panel, rect) for every visible panel, in draw order.
    pub panels: Vec<(DockPanel, Rect)>,
}

impl Default for DockLayout {
    fn default() -> Self {
        // The historical layout: the room rail on the left; visualizer, music,
        // bonsai, and the lobby stacked on the right (bonsai takes the slack, so
        // it gets the largest default weight).
        DockLayout {
            slots: vec![
                PanelSlot {
                    panel: DockPanel::RoomList,
                    side: DockSide::Left,
                    weight: 1,
                    enabled: true,
                },
                PanelSlot {
                    panel: DockPanel::Visualizer,
                    side: DockSide::Right,
                    weight: 2,
                    enabled: true,
                },
                PanelSlot {
                    panel: DockPanel::Music,
                    side: DockSide::Right,
                    weight: 3,
                    enabled: true,
                },
                PanelSlot {
                    panel: DockPanel::Bonsai,
                    side: DockSide::Right,
                    weight: 5,
                    enabled: true,
                },
                PanelSlot {
                    panel: DockPanel::Lobby,
                    side: DockSide::Right,
                    weight: 2,
                    enabled: true,
                },
            ],
            left_width: 24,
            right_width: 24,
        }
    }
}

impl DockLayout {
    /// The enabled panels docked on `side`, in order.
    pub fn panels_on(&self, side: DockSide) -> Vec<DockPanel> {
        self.slots
            .iter()
            .filter(|s| s.enabled && s.side == side)
            .map(|s| s.panel)
            .collect()
    }

    fn slot(&self, panel: DockPanel) -> Option<&PanelSlot> {
        self.slots.iter().find(|s| s.panel == panel)
    }

    /// Whether a side column has any enabled panels (else it isn't drawn).
    fn side_shown(&self, side: DockSide) -> bool {
        self.slots.iter().any(|s| s.enabled && s.side == side)
    }

    /// Clamp a requested column width to `[MIN_COL_WIDTH, MAX_COL_FRACTION%]`.
    pub fn clamp_width(total_width: u16, requested: u16) -> u16 {
        let max = (total_width as u32 * MAX_COL_FRACTION as u32 / 100) as u16;
        requested.clamp(MIN_COL_WIDTH, max.max(MIN_COL_WIDTH))
    }

    /// Resize a side column to a new width (clamped).
    pub fn resize(&mut self, divider: Divider, new_width: u16, total_width: u16) {
        let w = Self::clamp_width(total_width, new_width);
        match divider {
            Divider::Left => self.left_width = w,
            Divider::Right => self.right_width = w,
        }
    }

    /// Move `panel` to `zone` (a side, before an optional anchor), reordering the
    /// slot list so display order matches. A no-op if the panel is unknown.
    pub fn dock(&mut self, panel: DockPanel, zone: DropZone) {
        let Some(pos) = self.slots.iter().position(|s| s.panel == panel) else {
            return;
        };
        let mut slot = self.slots.remove(pos);
        slot.side = zone.side;
        slot.enabled = true;
        // Find the insertion index: just before the anchor slot (matched by panel
        // and side), or at the end of that side's run.
        let insert_at = match zone.before {
            Some(anchor) => self
                .slots
                .iter()
                .position(|s| s.panel == anchor)
                .unwrap_or(self.slots.len()),
            None => {
                // After the last slot already on this side, else the end.
                self.slots
                    .iter()
                    .rposition(|s| s.side == zone.side)
                    .map(|i| i + 1)
                    .unwrap_or(self.slots.len())
            }
        };
        self.slots.insert(insert_at.min(self.slots.len()), slot);
    }

    /// Compute the frame geometry for a terminal `area`. The side columns take
    /// their stored widths (only if they hold panels and fit); the centre gets
    /// the rest. Panels split their column's height by weight.
    pub fn frame(&self, area: Rect) -> DockFrame {
        let mut frame = DockFrame {
            center: area,
            ..Default::default()
        };
        if area.width == 0 || area.height == 0 {
            return frame;
        }

        // Decide the two column widths, leaving the centre at least a sliver.
        let left_w = if self.side_shown(DockSide::Left) {
            Self::clamp_width(area.width, self.left_width)
        } else {
            0
        };
        let right_w = if self.side_shown(DockSide::Right) {
            Self::clamp_width(area.width, self.right_width)
        } else {
            0
        };
        // Never eat the whole width: keep >= MIN_COL_WIDTH for the centre.
        let (left_w, right_w) = fit_columns(area.width, left_w, right_w);

        let mut x = area.x;
        if left_w > 0 {
            let r = Rect::new(x, area.y, left_w, area.height);
            frame.left = Some(r);
            self.lay_column(DockSide::Left, r, &mut frame.panels);
            x += left_w;
        }
        let center_w = area.width - left_w - right_w;
        frame.center = Rect::new(x, area.y, center_w, area.height);
        x += center_w;
        if right_w > 0 {
            let r = Rect::new(x, area.y, right_w, area.height);
            frame.right = Some(r);
            self.lay_column(DockSide::Right, r, &mut frame.panels);
        }
        frame
    }

    /// Stack a side column's panels by weight into `out`.
    fn lay_column(&self, side: DockSide, col: Rect, out: &mut Vec<(DockPanel, Rect)>) {
        let panels = self.panels_on(side);
        if panels.is_empty() {
            return;
        }
        let total_weight: u32 = panels
            .iter()
            .map(|p| self.slot(*p).map(|s| s.weight.max(1)).unwrap_or(1) as u32)
            .sum();
        let mut y = col.y;
        let mut used = 0u16;
        for (i, panel) in panels.iter().enumerate() {
            let w = self.slot(*panel).map(|s| s.weight.max(1)).unwrap_or(1) as u32;
            // The last panel takes the remainder so rounding never leaves a gap.
            let h = if i + 1 == panels.len() {
                col.height.saturating_sub(used)
            } else {
                ((col.height as u32 * w) / total_weight.max(1)) as u16
            };
            out.push((*panel, Rect::new(col.x, y, col.width, h)));
            y = y.saturating_add(h);
            used = used.saturating_add(h);
        }
    }

    /// Hit-test a mouse point against a computed frame: a column border (within
    /// 1 cell) is a `Divider`; a panel's top row is its header; otherwise its
    /// body. Returns `None` over the centre or outside.
    pub fn hit(&self, frame: &DockFrame, x: u16, y: u16) -> Option<Hit> {
        // Dividers: the last column of the left rail and the first of the right.
        if let Some(l) = frame.left
            && y >= l.y
            && y < l.y + l.height
            && x >= l.right().saturating_sub(1)
            && x <= l.right()
        {
            return Some(Hit::Divider(Divider::Left));
        }
        if let Some(r) = frame.right
            && y >= r.y
            && y < r.y + r.height
            && x >= r.x.saturating_sub(1)
            && x <= r.x
        {
            return Some(Hit::Divider(Divider::Right));
        }
        for (panel, rect) in &frame.panels {
            if x >= rect.x && x < rect.right() && y >= rect.y && y < rect.bottom() {
                return Some(if y == rect.y {
                    Hit::PanelHeader(*panel)
                } else {
                    Hit::PanelBody(*panel)
                });
            }
        }
        None
    }

    /// Which drop zone a point falls in while dragging a panel: the side column
    /// it is over (by x), and the panel it would land above (`None` = the end).
    pub fn drop_zone(&self, frame: &DockFrame, x: u16, y: u16) -> Option<DropZone> {
        let side = if frame.left.is_some_and(|l| x < l.right()) {
            DockSide::Left
        } else if frame.right.is_some_and(|r| x >= r.x) {
            DockSide::Right
        } else if x < frame.center.x + frame.center.width / 2 {
            // Over the centre: nearest side by half.
            DockSide::Left
        } else {
            DockSide::Right
        };
        // The anchor is the first same-side panel whose midline is below `y`.
        let before = frame
            .panels
            .iter()
            .filter(|(p, _)| self.slot(*p).is_some_and(|s| s.side == side))
            .find(|(_, r)| y < r.y + r.height / 2)
            .map(|(p, _)| *p);
        Some(DropZone { side, before })
    }

    /// Serialize to a JSON object for per-user persistence. Panels are keyed by
    /// their stable `key()` and sides by `"left"`/`"right"`, so the blob
    /// survives enum reordering or renames of the Rust variants.
    pub fn to_value(&self) -> serde_json::Value {
        let slots: Vec<serde_json::Value> = self
            .slots
            .iter()
            .map(|s| {
                serde_json::json!({
                    "panel": s.panel.key(),
                    "side": s.side.key(),
                    "weight": s.weight,
                    "enabled": s.enabled,
                })
            })
            .collect();
        serde_json::json!({
            "left_width": self.left_width,
            "right_width": self.right_width,
            "slots": slots,
        })
    }

    /// Rebuild a layout from a persisted blob, falling back to the default for
    /// anything missing or malformed. Unknown panel keys are skipped and any
    /// panel absent from the blob (e.g. one added in a newer build) is appended
    /// from the default, so a saved layout can never hide a panel. Widths are
    /// coarsely bounded here; `frame()` still clamps them to the live area.
    pub fn from_value(value: &serde_json::Value) -> DockLayout {
        let default = DockLayout::default();
        let Some(obj) = value.as_object() else {
            return default;
        };
        let width = |key: &str, fallback: u16| -> u16 {
            obj.get(key)
                .and_then(serde_json::Value::as_u64)
                .map(|n| n.clamp(1, 500) as u16)
                .unwrap_or(fallback)
        };
        let mut slots: Vec<PanelSlot> = Vec::new();
        if let Some(arr) = obj.get("slots").and_then(|v| v.as_array()) {
            for entry in arr {
                let Some(panel) = entry
                    .get("panel")
                    .and_then(|v| v.as_str())
                    .and_then(DockPanel::from_key)
                else {
                    continue;
                };
                if slots.iter().any(|s| s.panel == panel) {
                    continue; // ignore a duplicate panel in the blob
                }
                let side = entry
                    .get("side")
                    .and_then(|v| v.as_str())
                    .and_then(DockSide::from_key)
                    .unwrap_or(DockSide::Right);
                let weight = entry
                    .get("weight")
                    .and_then(serde_json::Value::as_u64)
                    .map(|n| (n as u16).max(1))
                    .unwrap_or(1);
                let enabled = entry
                    .get("enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                slots.push(PanelSlot {
                    panel,
                    side,
                    weight,
                    enabled,
                });
            }
        }
        // Append any panel the blob didn't mention, from the default layout.
        for def in &default.slots {
            if !slots.iter().any(|s| s.panel == def.panel) {
                slots.push(*def);
            }
        }
        DockLayout {
            slots,
            left_width: width("left_width", default.left_width),
            right_width: width("right_width", default.right_width),
        }
    }
}

impl DockSide {
    /// Stable persistence key (never rename).
    pub fn key(self) -> &'static str {
        match self {
            DockSide::Left => "left",
            DockSide::Right => "right",
        }
    }

    pub fn from_key(key: &str) -> Option<DockSide> {
        match key {
            "left" => Some(DockSide::Left),
            "right" => Some(DockSide::Right),
            _ => None,
        }
    }
}

/// Shrink two side widths so the centre keeps at least `MIN_COL_WIDTH`. Trims the
/// wider side first.
fn fit_columns(total: u16, mut left: u16, mut right: u16) -> (u16, u16) {
    let reserve = MIN_COL_WIDTH;
    while left + right + reserve > total && (left > 0 || right > 0) {
        if left >= right && left > 0 {
            left -= 1;
        } else if right > 0 {
            right -= 1;
        } else {
            break;
        }
    }
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Rect {
        Rect::new(0, 0, 120, 40)
    }

    #[test]
    fn default_layout_lays_three_columns() {
        let d = DockLayout::default();
        let f = d.frame(area());
        assert!(f.left.is_some() && f.right.is_some(), "both rails present");
        let l = f.left.unwrap();
        let r = f.right.unwrap();
        assert_eq!(l.width, 24);
        assert_eq!(r.width, 24);
        assert_eq!(f.center.width, 120 - 24 - 24, "centre gets the rest");
        // The right column stacks four panels; the left holds the room list.
        assert_eq!(f.panels.iter().filter(|(_, rc)| rc.x >= r.x).count(), 4);
        assert!(f.panels.iter().any(|(p, _)| *p == DockPanel::RoomList));
    }

    #[test]
    fn resize_clamps_to_min_and_max() {
        let mut d = DockLayout::default();
        d.resize(Divider::Left, 2, 120); // below MIN
        assert_eq!(d.left_width, MIN_COL_WIDTH);
        d.resize(Divider::Left, 200, 120); // above MAX (40% of 120 = 48)
        assert_eq!(d.left_width, 48);
    }

    #[test]
    fn columns_never_eat_the_centre() {
        let mut d = DockLayout::default();
        d.left_width = 48;
        d.right_width = 48; // 96 of 120; centre would be 24 - fine
        let f = d.frame(Rect::new(0, 0, 80, 40)); // now 96 > 80: must shrink
        let used = f.left.map(|l| l.width).unwrap_or(0) + f.right.map(|r| r.width).unwrap_or(0);
        assert!(f.center.width >= MIN_COL_WIDTH, "centre keeps a sliver");
        assert!(used + MIN_COL_WIDTH <= 80);
    }

    #[test]
    fn docking_moves_a_panel_across_sides() {
        let mut d = DockLayout::default();
        assert!(d.panels_on(DockSide::Left).contains(&DockPanel::RoomList));
        // Send the bonsai (cat) to the left, before the room list.
        d.dock(
            DockPanel::Bonsai,
            DropZone {
                side: DockSide::Left,
                before: Some(DockPanel::RoomList),
            },
        );
        let left = d.panels_on(DockSide::Left);
        assert_eq!(left, vec![DockPanel::Bonsai, DockPanel::RoomList]);
        assert!(!d.panels_on(DockSide::Right).contains(&DockPanel::Bonsai));
    }

    #[test]
    fn hit_test_finds_dividers_headers_and_bodies() {
        let d = DockLayout::default();
        let f = d.frame(area());
        // The left divider sits at the left rail's right edge.
        let l = f.left.unwrap();
        assert_eq!(
            d.hit(&f, l.right() - 1, 5),
            Some(Hit::Divider(Divider::Left))
        );
        // The room-list header is its top row.
        let (_, rl) = f
            .panels
            .iter()
            .find(|(p, _)| *p == DockPanel::RoomList)
            .unwrap();
        assert_eq!(
            d.hit(&f, rl.x + 1, rl.y),
            Some(Hit::PanelHeader(DockPanel::RoomList))
        );
        assert_eq!(
            d.hit(&f, rl.x + 1, rl.y + 1),
            Some(Hit::PanelBody(DockPanel::RoomList))
        );
        // The centre is not a dock target.
        assert_eq!(d.hit(&f, f.center.x + 2, 5), None);
    }

    #[test]
    fn drop_zone_picks_the_side_under_the_cursor() {
        let d = DockLayout::default();
        let f = d.frame(area());
        let z = d.drop_zone(&f, 2, 3).unwrap();
        assert_eq!(z.side, DockSide::Left);
        let z = d.drop_zone(&f, f.right.unwrap().x + 2, 3).unwrap();
        assert_eq!(z.side, DockSide::Right);
    }

    #[test]
    fn dragging_a_divider_resizes_its_column() {
        // Mirrors the home mouse path: hit-test a divider, then resize the
        // column to follow the pointer (the arithmetic in `handle_dock_drag`).
        let mut d = DockLayout::default();
        let a = area(); // 120 x 40 at origin
        let f = d.frame(a);

        // The left divider sits at the rail's right edge; grab it there.
        let l = f.left.unwrap();
        assert_eq!(
            d.hit(&f, l.right() - 1, 5),
            Some(Hit::Divider(Divider::Left))
        );
        // Drag it out to x = 30 (left edge is 0): the rail follows the pointer.
        d.resize(Divider::Left, 30 - a.x, a.width);
        assert_eq!(d.left_width, 30);
        assert_eq!(d.frame(a).left.unwrap().width, 30);

        // The right divider widens the sidebar as the pointer moves inward: a
        // pointer at x = 90 leaves a 30-wide sidebar (120 - 90).
        d.resize(Divider::Right, a.right() - 90, a.width);
        assert_eq!(d.right_width, 30);
        assert_eq!(d.frame(a).right.unwrap().width, 30);
    }

    #[test]
    fn layout_survives_a_json_round_trip() {
        // A moved-and-resized layout should reload exactly.
        let mut d = DockLayout::default();
        d.dock(
            DockPanel::Bonsai,
            DropZone {
                side: DockSide::Left,
                before: Some(DockPanel::RoomList),
            },
        );
        d.resize(Divider::Left, 30, 120);
        d.resize(Divider::Right, 18, 120);
        let reloaded = DockLayout::from_value(&d.to_value());
        assert_eq!(reloaded, d);
    }

    #[test]
    fn from_value_is_robust_to_junk() {
        // Non-object -> default.
        assert_eq!(
            DockLayout::from_value(&serde_json::json!("nope")),
            DockLayout::default()
        );
        // Unknown panel keys are skipped; missing panels are filled from the
        // default so none is ever hidden, and a bogus width falls back.
        let v = serde_json::json!({
            "left_width": "wide",
            "slots": [
                { "panel": "made_up", "side": "left", "weight": 2, "enabled": true },
                { "panel": "bonsai", "side": "left", "weight": 9, "enabled": false },
            ],
        });
        let d = DockLayout::from_value(&v);
        assert_eq!(
            d.left_width,
            DockLayout::default().left_width,
            "bad width -> default"
        );
        assert!(!d.slots.iter().any(|s| s.panel.key() == "made_up"));
        // Every real panel is present exactly once.
        for p in DockPanel::ALL {
            assert_eq!(
                d.slots.iter().filter(|s| s.panel == p).count(),
                1,
                "{p:?} present once"
            );
        }
        // The bonsai kept the blob's overrides.
        let bonsai = d
            .slots
            .iter()
            .find(|s| s.panel == DockPanel::Bonsai)
            .unwrap();
        assert_eq!(bonsai.side, DockSide::Left);
        assert!(!bonsai.enabled);
    }

    #[test]
    fn panel_keys_round_trip() {
        for p in DockPanel::ALL {
            assert_eq!(DockPanel::from_key(p.key()), Some(p));
        }
        assert_eq!(DockPanel::from_key("nope"), None);
    }
}
