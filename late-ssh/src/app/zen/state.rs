//! The Rice layout: a binary split tree of tiles, the look that dresses it,
//! and the focus that edits it. Everything here is pure data; persistence is
//! the orchestration layer's job (`App::flush_zen_layout`).

use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
        /// The room a chat tile is bound to; `None` (the stored default,
        /// and every layout saved before rooms were per tile) means the
        /// current room, Home's selection or #lounge. Other kinds ignore it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<Uuid>,
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

/// Most tiles a layout holds. Each split nests the stored JSON one level
/// deeper and the settings blob is read back through serde_json, which
/// stops at 128 levels; the cap keeps a held `S` from writing a layout the
/// login path can never parse.
pub const MAX_TILES: usize = 32;

/// Most chat tiles a layout holds. Every chat tile is a room the session
/// keeps drawn and its rows cached each frame, and a page of a hundred rooms
/// would swallow every unread count on the sidebar.
pub const MAX_CHAT_TILES: usize = 10;

impl Node {
    pub fn leaf(kind: TileKind) -> Self {
        Node::Leaf { kind, room: None }
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
            Node::Leaf { kind, .. } => out.push(*kind),
            Node::Split { first, second, .. } => {
                first.collect_kinds(out);
                second.collect_kinds(out);
            }
        }
    }

    /// Every leaf's bound room in layout order, parallel to `leaf_kinds`.
    pub fn leaf_rooms(&self) -> Vec<Option<Uuid>> {
        let mut out = Vec::with_capacity(self.leaf_count());
        self.collect_rooms(&mut out);
        out
    }

    fn collect_rooms(&self, out: &mut Vec<Option<Uuid>>) {
        match self {
            Node::Leaf { room, .. } => out.push(*room),
            Node::Split { first, second, .. } => {
                first.collect_rooms(out);
                second.collect_rooms(out);
            }
        }
    }

    pub fn kind_at(&self, ordinal: usize) -> Option<TileKind> {
        self.leaf_kinds().get(ordinal).copied()
    }

    /// Bind the leaf at `ordinal` to a room (or back to the current room).
    pub fn set_room(&mut self, ordinal: usize, new_room: Option<Uuid>) -> bool {
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf { room, .. }) => {
                *room = new_room;
                true
            }
            _ => false,
        }
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
        let Node::Leaf { kind, room } = *leaf else {
            return false;
        };
        *leaf = Node::split(dir, 500, Node::Leaf { kind, room }, Node::leaf(new_kind));
        true
    }

    /// Change what the leaf at `ordinal` shows. A tile that stops being a
    /// chat forgets its room, so cycling back lands on the current room.
    pub fn set_kind(&mut self, ordinal: usize, new_kind: TileKind) -> bool {
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf { kind, room }) => {
                *kind = new_kind;
                if new_kind != TileKind::Chat {
                    *room = None;
                }
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

/// What Enter in the tile picker did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindPick {
    /// The focused tile is the picked kind now; the layout is dirty.
    Changed,
    /// The picker closed and the tile is as it was (its own kind picked).
    Unchanged,
    /// Chat was picked with the page already holding `MAX_CHAT_TILES`
    /// chats; the picker stays open.
    ChatFull,
}

/// Session state for the Zen page.
pub struct ZenState {
    pub rice: RiceLayout,
    /// Focused tile, as an ordinal into `rice.root.leaf_kinds()`.
    pub focus: usize,
    /// The focused tile takes the whole page while set.
    pub zoomed: bool,
    /// The tile picker `space` opens over the focused tile: the selected
    /// row, an index into `TileKind::ALL`, while it is open.
    pub kind_picker: Option<usize>,
    /// Whether the page has been opened this session; the first opening
    /// lands the focus on the first chat tile so the chat keys work at once.
    opened: bool,
}

impl ZenState {
    pub fn new(rice: RiceLayout) -> Self {
        Self {
            rice,
            focus: 0,
            zoomed: false,
            kind_picker: None,
            opened: false,
        }
    }

    pub fn leaf_count(&self) -> usize {
        self.rice.root.leaf_count()
    }

    pub fn focused_kind(&self) -> Option<TileKind> {
        self.rice.root.kind_at(self.focus)
    }

    /// The page opening: the first time this session, the focus moves to
    /// the first chat tile (when there is one); later openings keep it.
    pub fn note_opened(&mut self) {
        if self.opened {
            return;
        }
        self.opened = true;
        if let Some(ordinal) = self.first_tile_of(TileKind::Chat) {
            self.focus = ordinal;
        }
    }

    /// The chat tiles in layout order: each one's ordinal and bound room
    /// (`None` for the current room).
    pub fn chat_tiles(&self) -> Vec<(usize, Option<Uuid>)> {
        let kinds = self.rice.root.leaf_kinds();
        let rooms = self.rice.root.leaf_rooms();
        kinds
            .iter()
            .zip(rooms)
            .enumerate()
            .filter(|(_, (kind, _))| **kind == TileKind::Chat)
            .map(|(ordinal, (_, room))| (ordinal, room))
            .collect()
    }

    pub fn chat_tile_count(&self) -> usize {
        self.chat_tiles().len()
    }

    /// Which chat tile is the active one, as an index into `chat_tiles`:
    /// the focused tile when it is a chat, else the first chat tile. The
    /// active tile has the composer, the selection, and the read marking;
    /// the others only watch their rooms.
    pub fn active_chat_index(&self) -> Option<usize> {
        let tiles = self.chat_tiles();
        if tiles.is_empty() {
            return None;
        }
        Some(
            tiles
                .iter()
                .position(|(ordinal, _)| *ordinal == self.focus)
                .unwrap_or(0),
        )
    }

    /// The focused tile's bound room, when the focused tile is a chat.
    pub fn focused_chat_room(&self) -> Option<Option<Uuid>> {
        self.chat_tiles()
            .into_iter()
            .find(|(ordinal, _)| *ordinal == self.focus)
            .map(|(_, room)| room)
    }

    /// Bind the focused chat tile to `room`; `false` when the focus is not
    /// on a chat tile.
    pub fn bind_focused_chat_room(&mut self, room: Option<Uuid>) -> bool {
        if self.focused_kind() != Some(TileKind::Chat) {
            return false;
        }
        self.rice.root.set_room(self.focus, room)
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
    /// a row, tall into a column. Refused at `MAX_TILES`.
    pub fn split_focused(&mut self, wide: bool) -> bool {
        if self.leaf_count() >= MAX_TILES {
            return false;
        }
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

    /// Open the tile picker on the focused tile's own kind, so Enter with
    /// no move changes nothing.
    pub fn open_kind_picker(&mut self) {
        let Some(kind) = self.focused_kind() else {
            return;
        };
        self.kind_picker = TileKind::ALL.iter().position(|k| *k == kind);
    }

    pub fn close_kind_picker(&mut self) {
        self.kind_picker = None;
    }

    /// Move the picker's row by `delta`, wrapping at both ends.
    pub fn move_kind_picker(&mut self, delta: isize) {
        let Some(selected) = self.kind_picker else {
            return;
        };
        let len = TileKind::ALL.len() as isize;
        self.kind_picker = Some((selected as isize + delta).rem_euclid(len) as usize);
    }

    /// The kind under the picker's row, while it is open.
    pub fn kind_picker_selection(&self) -> Option<TileKind> {
        self.kind_picker.map(|index| TileKind::ALL[index])
    }

    /// Whether the focused tile may become `kind`. Chat is refused once
    /// the page holds `MAX_CHAT_TILES` of them (a tile that already is a
    /// chat counts itself out, so it can leave and come back).
    pub fn kind_allowed(&self, kind: TileKind) -> bool {
        if kind != TileKind::Chat {
            return true;
        }
        let others =
            self.chat_tile_count() - usize::from(self.focused_kind() == Some(TileKind::Chat));
        others < MAX_CHAT_TILES
    }

    /// Set the focused tile to the picker's row and close the picker.
    /// A refused row (`kind_allowed`) keeps the picker open and changes
    /// nothing; the tile's own kind closes it and changes nothing.
    pub fn pick_kind(&mut self) -> KindPick {
        let Some(kind) = self.kind_picker_selection() else {
            return KindPick::Unchanged;
        };
        if !self.kind_allowed(kind) {
            return KindPick::ChatFull;
        }
        self.kind_picker = None;
        if self.focused_kind() == Some(kind) {
            return KindPick::Unchanged;
        }
        if self.rice.root.set_kind(self.focus, kind) {
            KindPick::Changed
        } else {
            KindPick::Unchanged
        }
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

    /// Back to the default layout, with the focus on its chat tile so the
    /// chat keys work at once, as on the first opening.
    pub fn reset(&mut self) {
        self.rice = RiceLayout::default();
        self.zoomed = false;
        self.focus = self.first_tile_of(TileKind::Chat).unwrap_or(0);
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
