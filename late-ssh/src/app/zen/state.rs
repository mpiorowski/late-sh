//! The Rice layout: a binary split tree of tiles, the look that dresses it,
//! and the focus that edits it. Everything here is pure data; persistence is
//! the orchestration layer's job (`App::persist_zen_layout`).

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

use super::layout;

/// What a tile shows. Closed set: a new kind is a new arm in `ui.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileKind {
    Bonsai,
    Aquarium,
    Pet,
    Chat,
    Music,
    Clock,
    Visualizer,
    Presence,
    Lobby,
    Blank,
}

impl TileKind {
    pub const ALL: [TileKind; 10] = [
        TileKind::Bonsai,
        TileKind::Aquarium,
        TileKind::Pet,
        TileKind::Chat,
        TileKind::Music,
        TileKind::Clock,
        TileKind::Visualizer,
        TileKind::Presence,
        TileKind::Lobby,
        TileKind::Blank,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TileKind::Bonsai => "bonsai",
            TileKind::Aquarium => "aquarium",
            TileKind::Pet => "pet",
            TileKind::Chat => "chat",
            TileKind::Music => "music",
            TileKind::Clock => "clock",
            TileKind::Visualizer => "visualizer",
            TileKind::Presence => "presence",
            TileKind::Lobby => "lobby",
            TileKind::Blank => "blank",
        }
    }

    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(idx + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// How a split lays its two children out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dir {
    /// Side by side, first on the left.
    Row,
    /// Stacked, first on top.
    Column,
}

impl Dir {
    pub fn flipped(self) -> Self {
        match self {
            Dir::Row => Dir::Column,
            Dir::Column => Dir::Row,
        }
    }
}

/// The layout tree. Leaves are tiles; splits carry the direction and the
/// first child's share in per-mille, fine enough that one row or column of
/// any split a terminal can hold is a distinct value (see `resize_leaf`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Node {
    Leaf {
        kind: TileKind,
    },
    Split {
        dir: Dir,
        share: u16,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// Per-mille bounds for a split's first child, so a tile can never be
/// squeezed to nothing.
pub const MIN_SHARE: u16 = 100;
pub const MAX_SHARE: u16 = 900;

impl Node {
    pub fn leaf(kind: TileKind) -> Self {
        Node::Leaf { kind }
    }

    pub fn split(dir: Dir, share: u16, first: Node, second: Node) -> Self {
        Node::Split {
            dir,
            share: share.clamp(MIN_SHARE, MAX_SHARE),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            Node::Leaf { .. } => 1,
            Node::Split { first, second, .. } => first.leaf_count() + second.leaf_count(),
        }
    }

    /// Tile kinds in layout order (depth first, first child before second).
    pub fn leaf_kinds(&self) -> Vec<TileKind> {
        let mut out = Vec::with_capacity(self.leaf_count());
        self.collect_kinds(&mut out);
        out
    }

    fn collect_kinds(&self, out: &mut Vec<TileKind>) {
        match self {
            Node::Leaf { kind } => out.push(*kind),
            Node::Split { first, second, .. } => {
                first.collect_kinds(out);
                second.collect_kinds(out);
            }
        }
    }

    pub fn kind_at(&self, ordinal: usize) -> Option<TileKind> {
        self.leaf_kinds().get(ordinal).copied()
    }

    fn leaf_mut(&mut self, ordinal: &mut usize) -> Option<&mut Node> {
        match self {
            Node::Leaf { .. } => {
                if *ordinal == 0 {
                    Some(self)
                } else {
                    *ordinal -= 1;
                    None
                }
            }
            Node::Split { first, second, .. } => {
                if let Some(leaf) = first.leaf_mut(ordinal) {
                    return Some(leaf);
                }
                second.leaf_mut(ordinal)
            }
        }
    }

    /// Replace the leaf at `ordinal` with a split holding it and a new tile.
    pub fn split_leaf(&mut self, ordinal: usize, dir: Dir, new_kind: TileKind) -> bool {
        let mut remaining = ordinal;
        let Some(leaf) = self.leaf_mut(&mut remaining) else {
            return false;
        };
        let Node::Leaf { kind } = *leaf else {
            return false;
        };
        *leaf = Node::split(dir, 500, Node::leaf(kind), Node::leaf(new_kind));
        true
    }

    pub fn set_kind(&mut self, ordinal: usize, new_kind: TileKind) -> bool {
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf { kind }) => {
                *kind = new_kind;
                true
            }
            _ => false,
        }
    }

    /// Run `f` on the split directly above the leaf at `ordinal`, told whether
    /// the leaf is that split's first child. A root leaf has no parent.
    fn with_parent(&mut self, ordinal: &mut usize, f: &mut dyn FnMut(&mut Node, bool)) -> bool {
        let Node::Split { first, second, .. } = self else {
            return false;
        };
        let first_count = first.leaf_count();
        if *ordinal < first_count {
            if matches!(**first, Node::Leaf { .. }) {
                f(self, true);
                return true;
            }
            return first.with_parent(ordinal, f);
        }
        *ordinal -= first_count;
        if matches!(**second, Node::Leaf { .. }) {
            f(self, false);
            return true;
        }
        second.with_parent(ordinal, f)
    }

    /// Remove the leaf at `ordinal`; its sibling takes the parent's place.
    pub fn close_leaf(&mut self, ordinal: usize) -> bool {
        let mut remaining = ordinal;
        self.with_parent(&mut remaining, &mut |parent, is_first| {
            let sibling = match parent {
                Node::Split { first, second, .. } => {
                    if is_first {
                        (**second).clone()
                    } else {
                        (**first).clone()
                    }
                }
                Node::Leaf { .. } => return,
            };
            *parent = sibling;
        })
    }

    /// Grow (positive) or shrink the leaf at `ordinal` by `delta_cells`
    /// along `dir`: the nearest ancestor split of that direction moves, the
    /// way i3 resizes. The tree is walked with the rects it lays out on
    /// `area` (the same math the renderer uses), so a press moves the split
    /// by exactly that many rows or columns on the current terminal, and the
    /// share written back reproduces that cell count next frame. `false`
    /// when no ancestor runs that way (a row of two tiles has no height to
    /// trade).
    pub fn resize_leaf(
        &mut self,
        ordinal: usize,
        dir: Dir,
        delta_cells: i16,
        area: Rect,
        gap: u16,
    ) -> bool {
        let mut remaining = ordinal;
        self.resize_toward(&mut remaining, dir, delta_cells, area, gap)
    }

    fn resize_toward(
        &mut self,
        ordinal: &mut usize,
        dir: Dir,
        delta_cells: i16,
        area: Rect,
        gap: u16,
    ) -> bool {
        let Node::Split {
            dir: my_dir,
            share,
            first,
            second,
        } = self
        else {
            return false;
        };
        let (first_area, second_area) = layout::split_rect(area, *my_dir, *share, gap);
        let first_count = first.leaf_count();
        let (child, child_area, is_first) = if *ordinal < first_count {
            (first, first_area, true)
        } else {
            *ordinal -= first_count;
            (second, second_area, false)
        };
        // Deeper first: the split nearest the tile is the one that moves.
        if child.resize_toward(ordinal, dir, delta_cells, child_area, gap) {
            return true;
        }
        if *my_dir != dir {
            return false;
        }
        let usable = layout::usable_len(area, *my_dir, gap);
        let signed = if is_first { delta_cells } else { -delta_cells };
        let target = (layout::first_len(usable, *share) as i32 + signed as i32)
            .clamp(0, usable as i32) as u16;
        *share = layout::share_for(usable, target).clamp(MIN_SHARE, MAX_SHARE);
        true
    }

    /// Flip the parent split of the leaf at `ordinal` between row and column.
    pub fn flip_parent(&mut self, ordinal: usize) -> bool {
        let mut remaining = ordinal;
        self.with_parent(&mut remaining, &mut |parent, _| {
            if let Node::Split { dir, .. } = parent {
                *dir = dir.flipped();
            }
        })
    }
}

/// Border drawn around every tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderKind {
    None,
    Plain,
    Rounded,
    Double,
    Thick,
}

impl BorderKind {
    pub fn next(self) -> Self {
        match self {
            BorderKind::None => BorderKind::Plain,
            BorderKind::Plain => BorderKind::Rounded,
            BorderKind::Rounded => BorderKind::Double,
            BorderKind::Double => BorderKind::Thick,
            BorderKind::Thick => BorderKind::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BorderKind::None => "none",
            BorderKind::Plain => "plain",
            BorderKind::Rounded => "rounded",
            BorderKind::Double => "double",
            BorderKind::Thick => "thick",
        }
    }
}

pub const MAX_GAP: u8 = 3;

/// The dressing: borders, gaps between tiles, and whether tiles wear titles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Look {
    pub border: BorderKind,
    pub gap: u8,
    pub titles: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            border: BorderKind::Rounded,
            gap: 0,
            titles: true,
        }
    }
}

/// What gets persisted: the tree and the look.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiceLayout {
    pub root: Node,
    pub look: Look,
}

impl Default for RiceLayout {
    /// The out-of-the-box page, also what `R` resets to: the bonsai at full
    /// canvas over the current room's chat on the left, and a rail of
    /// clock, music, lobby, then the pet over the live reef on the right
    /// (the pet sits against the tank and watches; unowned, the tile points
    /// at the shop).
    fn default() -> Self {
        Self {
            root: Node::split(
                Dir::Row,
                640,
                Node::split(
                    Dir::Column,
                    600,
                    Node::leaf(TileKind::Bonsai),
                    Node::leaf(TileKind::Chat),
                ),
                Node::split(
                    Dir::Column,
                    180,
                    Node::leaf(TileKind::Clock),
                    Node::split(
                        Dir::Column,
                        170,
                        Node::leaf(TileKind::Music),
                        Node::split(
                            Dir::Column,
                            240,
                            Node::leaf(TileKind::Lobby),
                            Node::split(
                                Dir::Column,
                                300,
                                Node::leaf(TileKind::Pet),
                                Node::leaf(TileKind::Aquarium),
                            ),
                        ),
                    ),
                ),
            ),
            look: Look::default(),
        }
    }
}

impl RiceLayout {
    /// Parse a stored layout; anything unreadable falls back to the default,
    /// so a bad row never locks someone out of the page.
    pub fn from_json(value: Option<&serde_json::Value>) -> Self {
        match value {
            Some(value) => serde_json::from_value(value.clone()).unwrap_or_default(),
            None => Self::default(),
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Session state for the Zen page.
pub struct ZenState {
    pub rice: RiceLayout,
    /// Focused tile, as an ordinal into `rice.root.leaf_kinds()`.
    pub focus: usize,
    /// The focused tile takes the whole page while set.
    pub zoomed: bool,
}

impl ZenState {
    pub fn new(rice: RiceLayout) -> Self {
        Self {
            rice,
            focus: 0,
            zoomed: false,
        }
    }

    pub fn leaf_count(&self) -> usize {
        self.rice.root.leaf_count()
    }

    pub fn focused_kind(&self) -> Option<TileKind> {
        self.rice.root.kind_at(self.focus)
    }

    fn clamp_focus(&mut self) {
        let count = self.leaf_count();
        if self.focus >= count {
            self.focus = count.saturating_sub(1);
        }
    }

    pub fn focus_next(&mut self) {
        let count = self.leaf_count();
        if count > 0 {
            self.focus = (self.focus + 1) % count;
        }
    }

    pub fn focus_prev(&mut self) {
        let count = self.leaf_count();
        if count > 0 {
            self.focus = (self.focus + count - 1) % count;
        }
    }

    /// Split the focused tile; the new tile is blank so the person picks
    /// what goes there. Direction follows the tile's shape: wide splits into
    /// a row, tall into a column.
    pub fn split_focused(&mut self, wide: bool) -> bool {
        let dir = if wide { Dir::Row } else { Dir::Column };
        let done = self.rice.root.split_leaf(self.focus, dir, TileKind::Blank);
        if done {
            self.focus += 1;
        }
        done
    }

    pub fn close_focused(&mut self) -> bool {
        let done = self.rice.root.close_leaf(self.focus);
        self.zoomed = false;
        self.clamp_focus();
        done
    }

    pub fn cycle_focused_kind(&mut self, forward: bool) -> bool {
        let Some(kind) = self.focused_kind() else {
            return false;
        };
        let next = if forward { kind.next() } else { kind.prev() };
        self.rice.root.set_kind(self.focus, next)
    }

    /// Move the focused tile's edge by `delta_cells` along `dir`, on the
    /// tiles area the page currently lays out.
    pub fn resize_focused(&mut self, dir: Dir, delta_cells: i16, tiles_area: Rect) -> bool {
        let gap = self.rice.look.gap as u16;
        self.rice
            .root
            .resize_leaf(self.focus, dir, delta_cells, tiles_area, gap)
    }

    pub fn flip_focused(&mut self) -> bool {
        self.rice.root.flip_parent(self.focus)
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = !self.zoomed;
    }

    pub fn cycle_border(&mut self) {
        self.rice.look.border = self.rice.look.border.next();
    }

    pub fn cycle_gap(&mut self) {
        self.rice.look.gap = (self.rice.look.gap + 1) % (MAX_GAP + 1);
    }

    pub fn toggle_titles(&mut self) {
        self.rice.look.titles = !self.rice.look.titles;
    }

    pub fn reset(&mut self) {
        self.rice = RiceLayout::default();
        self.focus = 0;
        self.zoomed = false;
    }

    /// Ordinal of the first tile of `kind`, for surfaces that can only be
    /// drawn once per frame (the chat, the aquarium sim).
    pub fn first_tile_of(&self, kind: TileKind) -> Option<usize> {
        self.rice.root.leaf_kinds().iter().position(|k| *k == kind)
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
