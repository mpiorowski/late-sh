//! The Rice layout: a binary split tree of tiles, the look that dresses it,
//! and the focus that edits it. Everything here is pure data; persistence is
//! the orchestration layer's job (`App::flush_home_layout`).

use std::time::Instant;

use late_core::models::user::{AsciiPiece, AsciiSelection};
use rand::Rng;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::layout::{self, Gaps};
use crate::app::ascii::playback::Playback;

/// What a tile shows. Closed set: a new kind is a new arm in `ui.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileKind {
    Bonsai,
    /// The reef: the whole simulation, bound to the tile.
    Aquarium,
    /// The sidebar's small tank: the fish in a six-row band, whatever
    /// the tile's size. The same tank two ways.
    Tank,
    /// An animated ascii piece (`app/ascii`), the screensaver's kind in a tile.
    Ascii,
    Pet,
    /// A room's chat. The active one (the focused chat tile, else the
    /// first) is the page's chat: the composer, the selection, the read
    /// marking, and on Home the rail's selection, synthetic entries
    /// included. Stored `center` layouts read as this kind.
    #[serde(alias = "center")]
    Chat,
    Music,
    Clock,
    Visualizer,
    Lobby,
    Activity,
    Friends,
    Pulse,
    /// The Live panel's rows (`app/live/panel.rs`), as the sidebar draws
    /// them.
    Live,
    /// A stored `inbox` or `headlines` (kinds that are gone) reads as
    /// blank space.
    #[serde(alias = "inbox", alias = "headlines")]
    Blank,
    /// Home's room rail. One per page.
    Rooms,
    /// The sidebar's pinned rows: the online count and the clock, then
    /// the connected friends.
    Presence,
}

impl TileKind {
    /// The tile picker's rows: every kind by name, then the ones a page
    /// holds once.
    pub const ALL: [TileKind; 17] = [
        TileKind::Activity,
        TileKind::Aquarium,
        TileKind::Ascii,
        TileKind::Blank,
        TileKind::Bonsai,
        TileKind::Chat,
        TileKind::Clock,
        TileKind::Friends,
        TileKind::Live,
        TileKind::Lobby,
        TileKind::Music,
        TileKind::Pet,
        TileKind::Presence,
        TileKind::Pulse,
        TileKind::Tank,
        TileKind::Visualizer,
        TileKind::Rooms,
    ];

    /// The kinds Home draws from its own render input (`chat_view`): each
    /// is allowed once per page.
    pub fn one_per_page(self) -> bool {
        match self {
            TileKind::Rooms => true,
            TileKind::Bonsai
            | TileKind::Aquarium
            | TileKind::Tank
            | TileKind::Ascii
            | TileKind::Pet
            | TileKind::Chat
            | TileKind::Music
            | TileKind::Clock
            | TileKind::Visualizer
            | TileKind::Lobby
            | TileKind::Activity
            | TileKind::Friends
            | TileKind::Pulse
            | TileKind::Live
            | TileKind::Blank
            | TileKind::Presence => false,
        }
    }

    /// A tile that holds the keys: the chat you type in. Outside arrange
    /// mode only these take the focus; everything else is furniture, drawn
    /// and clicked for its one gesture, never focused.
    pub fn takes_focus(self) -> bool {
        match self {
            TileKind::Chat => true,
            TileKind::Bonsai
            | TileKind::Aquarium
            | TileKind::Tank
            | TileKind::Ascii
            | TileKind::Pet
            | TileKind::Music
            | TileKind::Clock
            | TileKind::Visualizer
            | TileKind::Lobby
            | TileKind::Activity
            | TileKind::Friends
            | TileKind::Pulse
            | TileKind::Live
            | TileKind::Blank
            | TileKind::Rooms
            | TileKind::Presence => false,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TileKind::Bonsai => "bonsai",
            TileKind::Aquarium => "aquarium",
            TileKind::Tank => "tank",
            TileKind::Ascii => "ascii",
            TileKind::Pet => "pet",
            TileKind::Chat => "chat",
            TileKind::Music => "music",
            TileKind::Clock => "clock",
            TileKind::Visualizer => "visualizer",
            TileKind::Lobby => "lobby",
            TileKind::Activity => "activity",
            TileKind::Friends => "friends",
            TileKind::Pulse => "pulse",
            TileKind::Live => "live",
            TileKind::Blank => "blank",
            TileKind::Rooms => "rooms",
            TileKind::Presence => "presence",
        }
    }

    /// The keys a tile of this kind names on its title in arrange mode:
    /// the ones it takes itself (`TileHint::Takes`, shown with the focus
    /// on it) and the house keys it only points at (`TileHint::Names`,
    /// shown on every tile). One table for the hint and the handler
    /// (`input::act`), so a title never names a key nothing answers.
    pub fn hints(self) -> &'static [TileHint] {
        match self {
            TileKind::Bonsai => &[TileHint::Names(HouseKey::Tend)],
            TileKind::Ascii => &[TileHint::Takes(TileKey::PickPiece)],
            TileKind::Aquarium | TileKind::Tank => &[TileHint::Takes(TileKey::Feed)],
            TileKind::Pet => &[TileHint::Names(HouseKey::PetClick)],
            TileKind::Chat => &[
                TileHint::Takes(TileKey::CycleRoom),
                TileHint::Names(HouseKey::PickRoom),
            ],
            TileKind::Music => &[TileHint::Names(HouseKey::Mute)],
            TileKind::Lobby => &[TileHint::Names(HouseKey::OpenLobby)],
            TileKind::Live => &[TileHint::Names(HouseKey::OpenLive)],
            TileKind::Rooms
            | TileKind::Clock
            | TileKind::Visualizer
            | TileKind::Activity
            | TileKind::Friends
            | TileKind::Pulse
            | TileKind::Presence
            | TileKind::Blank => &[],
        }
    }
}

/// One key hint on a tile's title in arrange mode, and who answers it.
/// Normal mode names no keys at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileHint {
    /// A key the focused tile takes itself (`input::act`, `TileKey::takes`).
    /// Shown only with the focus on the tile, since that is where it works.
    Takes(TileKey),
    /// A key the house answers (`app/input.rs`, the mouse): the tile only
    /// names it, so the key is found from the tile it serves. Shown on
    /// every tile in arrange mode.
    Names(HouseKey),
}

impl TileHint {
    /// The key and the word, as `hint_line` draws them.
    pub fn text(self) -> (&'static str, &'static str) {
        match self {
            TileHint::Takes(key) => key.text(),
            TileHint::Names(key) => key.text(),
        }
    }
}

/// A key the focused tile takes in arrange mode. Each is one entry of
/// `takes` and one arm of `input::act`, so the hint, the bytes and the
/// handler never drift apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKey {
    /// `j` `k` walk the attention guest's rows, Enter acts on the selected
    /// one.
    Attend,
    /// `[` `]` step an ascii tile through the pieces, Enter opens the
    /// picker.
    PickPiece,
    /// `a` feeds the tank, on the reef or the tank tile.
    Feed,
    /// `[` `]` rebind a chat tile to the previous or next room.
    CycleRoom,
}

impl TileKey {
    pub fn text(self) -> (&'static str, &'static str) {
        match self {
            TileKey::Attend => ("j k Enter", "act"),
            TileKey::PickPiece => ("[ ] Enter", "pick"),
            TileKey::Feed => ("a", "feed"),
            TileKey::CycleRoom => ("[ ]", "cycle"),
        }
    }

    /// Whether `byte` is one of this key's.
    pub fn takes(self, byte: u8) -> bool {
        match self {
            TileKey::Attend => matches!(byte, b'j' | b'J' | b'k' | b'K' | b'\r' | b'\n'),
            TileKey::PickPiece => matches!(byte, b'[' | b']' | b'\r' | b'\n'),
            TileKey::Feed => byte == b'a',
            TileKey::CycleRoom => matches!(byte, b'[' | b']'),
        }
    }
}

/// A key or gesture the house answers, not the tile: a tile names it on
/// its title so it is found from the tile it serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HouseKey {
    /// `w`: the bonsai's care modal.
    Tend,
    /// `Ctrl+/`: the room picker, for a chat tile.
    PickRoom,
    /// `m`: mute the paired client.
    Mute,
    /// `Ctrl+G`: the lobby.
    OpenLobby,
    /// `s` then `1`..`4`: a row of the Live panel.
    OpenLive,
    /// A click on the pet pets it.
    PetClick,
}

impl HouseKey {
    pub fn text(self) -> (&'static str, &'static str) {
        match self {
            HouseKey::Tend => ("w", "tend"),
            HouseKey::PickRoom => ("ctrl+/", "room"),
            HouseKey::Mute => ("m", "mute"),
            HouseKey::OpenLobby => ("^G", "open"),
            HouseKey::OpenLive => ("s1-4", "open"),
            HouseKey::PetClick => ("click", "pet"),
        }
    }

    /// Who answers it. Arrange mode swallows stray letters so none reach
    /// the chat; a `Letter` here is let through to `app/input.rs`, which
    /// runs after Home, so a key a title names works while arranging.
    pub fn taken_by(self) -> TakenBy {
        match self {
            HouseKey::Tend => TakenBy::Letter(b'w'),
            HouseKey::Mute => TakenBy::Letter(b'm'),
            HouseKey::OpenLive => TakenBy::Letter(b's'),
            HouseKey::PickRoom | HouseKey::OpenLobby => TakenBy::Chord,
            HouseKey::PetClick => TakenBy::Click,
        }
    }
}

/// Where a house key is answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakenBy {
    /// A letter `app/input.rs` takes after Home, in either case.
    Letter(u8),
    /// A control chord reserved ahead of every page; Home never sees it.
    Chord,
    /// The mouse.
    Click,
}

/// A widget that comes and goes, hosted by a tile that is otherwise
/// something else: invisible while it has nothing to say, the whole tile
/// while it does, then gone again. Each is on the page once, as a guest
/// somewhere or not at all, and a tile hosts one. Both end: the strip on
/// the queue's clock, attention when its last row clears. Widgets that
/// stay up for an evening (the Live roster) are tile kinds, never guests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Guest {
    /// The now strip (`app/live`): one thing just happened in the house.
    Now,
    /// The attention list (`attention.rs`): what waits on you, a row
    /// each, while anything does.
    Attention,
}

impl Guest {
    /// Every guest, the guest picker's rows.
    pub const ALL: [Guest; 2] = [Guest::Now, Guest::Attention];

    pub fn label(self) -> &'static str {
        match self {
            Guest::Now => "now",
            Guest::Attention => "attention",
        }
    }

    /// When the picker says it shows.
    pub fn when(self) -> &'static str {
        match self {
            Guest::Now => "something just happened",
            Guest::Attention => "something waits on you",
        }
    }

    /// The keys a guest takes while it is up on a tile, ahead of its
    /// host's: attention's rows take `j` `k` and Enter, the strip none.
    pub fn hints(self) -> &'static [TileHint] {
        match self {
            Guest::Now => &[],
            Guest::Attention => &[TileHint::Takes(TileKey::Attend)],
        }
    }

    /// A stored guest key this build knows; `live` and `banner`, guests
    /// once, read as none, and `lobby`, the guest attention grew out of,
    /// reads as attention.
    fn from_key(key: &str) -> Option<Guest> {
        match key {
            "now" => Some(Guest::Now),
            "attention" | "lobby" => Some(Guest::Attention),
            _ => None,
        }
    }
}

/// A stored `guests` list, or the one `guest` key a layout saved while a
/// tile hosted one. The guests this build knows stay, each once, in
/// `Guest::ALL` order.
#[derive(Deserialize)]
#[serde(untagged)]
enum StoredGuests {
    One(String),
    Many(Vec<String>),
}

fn known_guests<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Guest>, D::Error> {
    let stored: Option<StoredGuests> = Option::deserialize(deserializer)?;
    let keys: Vec<String> = match stored {
        None => Vec::new(),
        Some(StoredGuests::One(key)) => vec![key],
        Some(StoredGuests::Many(keys)) => keys,
    };
    Ok(Guest::ALL
        .into_iter()
        .filter(|guest| keys.iter().any(|key| Guest::from_key(key) == Some(*guest)))
        .collect())
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

/// How a split divides its length: the first child's share in per-mille,
/// fine enough that one row or column of any split a terminal can hold is
/// a distinct value (see `resize_leaf`), or a fixed cell count for one
/// child, the way Home's rail and sidebar hold their width whatever the
/// terminal does. Untagged, so a stored bare number reads as per-mille.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Share {
    Mille(u16),
    First { first: u16 },
    Second { second: u16 },
}

/// The layout tree. Leaves are tiles; splits carry the direction and the
/// share (`Share`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum Node {
    Leaf {
        kind: TileKind,
        /// The guests this tile hosts, in `Guest::ALL` order: the whole
        /// tile while one has something to show, stacked while two do.
        /// Empty for most. Read leniently: a layout saved with one `guest`
        /// key reads as that guest, and keys this build does not know are
        /// dropped.
        #[serde(
            default,
            alias = "guest",
            deserialize_with = "known_guests",
            skip_serializing_if = "Vec::is_empty"
        )]
        guests: Vec<Guest>,
        /// The room a chat tile is bound to; `None` (the stored default,
        /// and every layout saved before rooms were per tile) means the
        /// current room, Home's selection or #lounge. Other kinds ignore it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        room: Option<Uuid>,
        /// The selection an ascii tile plays; `None` (the stored default) is
        /// [`AsciiPiece::DEFAULT`]. Other kinds ignore it. A key this build does
        /// not know (a piece dropped, a rolled-back deploy) reads as `None`
        /// rather than failing the whole layout, which `from_json` would
        /// replace with the default, room bindings and all.
        #[serde(
            default,
            deserialize_with = "known_piece",
            skip_serializing_if = "Option::is_none"
        )]
        piece: Option<AsciiSelection>,
        /// What the tile wears on its top row on Home, outside arrange
        /// mode. `t` in arrange mode walks the three.
        #[serde(
            default,
            deserialize_with = "known_title",
            skip_serializing_if = "TileTitle::is_named"
        )]
        title: TileTitle,
    },
    Split {
        dir: Dir,
        share: Share,
        first: Box<Node>,
        second: Box<Node>,
    },
}

/// What a tile wears on its top row on Home, outside arrange mode. Arrange
/// mode names every tile whatever it says, so the focus has a rule to light.
/// Under the boxed chrome only `Named` draws text; the border is already a
/// rule, so `Rule` and `Hidden` look the same there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TileTitle {
    /// `── name ───── keys`: the rule with the tile's name and key hints.
    #[default]
    Named,
    /// `─────────────────`: the rule alone, no text.
    Rule,
    /// No row at all: the tile starts at its rect's top.
    Hidden,
}

impl TileTitle {
    /// The next one `t` lands on.
    pub fn next(self) -> Self {
        match self {
            TileTitle::Named => TileTitle::Rule,
            TileTitle::Rule => TileTitle::Hidden,
            TileTitle::Hidden => TileTitle::Named,
        }
    }

    fn is_named(&self) -> bool {
        match self {
            TileTitle::Named => true,
            TileTitle::Rule | TileTitle::Hidden => false,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredTitle {
    Mode(TileTitle),
    /// What `t` wrote when a title was only on or off.
    OnOrOff(bool),
    Unknown(serde::de::IgnoredAny),
}

/// A stored `title`: a mode this build knows, the older on or off, or the
/// name for anything else, so the rest of the layout reads as stored.
fn known_title<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<TileTitle, D::Error> {
    Ok(match StoredTitle::deserialize(deserializer)? {
        StoredTitle::Mode(title) => title,
        StoredTitle::OnOrOff(true) | StoredTitle::Unknown(_) => TileTitle::Named,
        StoredTitle::OnOrOff(false) => TileTitle::Hidden,
    })
}

/// A stored `piece` key: a piece this build knows, or `None` for one it
/// does not, so the tile falls back to [`AsciiPiece::DEFAULT`] and the rest of
/// the layout reads as stored.
fn known_piece<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<AsciiSelection>, D::Error> {
    let key: Option<String> = Option::deserialize(deserializer)?;
    Ok(key.as_deref().and_then(AsciiSelection::from_key))
}

/// Per-mille bounds for a split's first child, so neither side is ever
/// handed the whole split.
pub const MIN_SHARE: u16 = 1;
pub const MAX_SHARE: u16 = 999;

/// The fewest cells a resize leaves on either side of a split: a border
/// and one row (or column) inside it, so a clock can sit in a one-row tile.
pub const MIN_TILE_CELLS: u16 = 3;

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
        Node::Leaf {
            kind,
            guests: Vec::new(),
            room: None,
            piece: None,
            title: TileTitle::Named,
        }
    }

    /// A leaf hosting `guests`, given in any order.
    pub fn hosting(kind: TileKind, guests: &[Guest]) -> Self {
        Node::Leaf {
            kind,
            guests: Guest::ALL
                .into_iter()
                .filter(|guest| guests.contains(guest))
                .collect(),
            room: None,
            piece: None,
            title: TileTitle::Named,
        }
    }

    pub fn split(dir: Dir, share: u16, first: Node, second: Node) -> Self {
        Node::Split {
            dir,
            share: Share::Mille(share.clamp(MIN_SHARE, MAX_SHARE)),
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// A split whose first child is `cells` long, the rest to the second.
    pub fn split_first(dir: Dir, cells: u16, first: Node, second: Node) -> Self {
        Node::Split {
            dir,
            share: Share::First { first: cells },
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// A split whose second child is `cells` long, the rest to the first.
    pub fn split_second(dir: Dir, cells: u16, first: Node, second: Node) -> Self {
        Node::Split {
            dir,
            share: Share::Second { second: cells },
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

    /// Every leaf's guests in layout order, parallel to `leaf_kinds`.
    pub fn leaf_guests(&self) -> Vec<Vec<Guest>> {
        let mut out = Vec::with_capacity(self.leaf_count());
        self.collect_guests(&mut out);
        out
    }

    fn collect_guests(&self, out: &mut Vec<Vec<Guest>>) {
        match self {
            Node::Leaf { guests, .. } => out.push(guests.clone()),
            Node::Split { first, second, .. } => {
                first.collect_guests(out);
                second.collect_guests(out);
            }
        }
    }

    /// The leaf hosting `guest`, if any: a guest is on the page once.
    pub fn host_of(&self, guest: Guest) -> Option<usize> {
        self.leaf_guests()
            .iter()
            .position(|hosted| hosted.contains(&guest))
    }

    /// Put `guest` on the leaf at `ordinal`, taking it off whichever leaf
    /// held it and beside whatever `ordinal` hosts already; on its own
    /// host already, take it off instead. `false` when `ordinal` is no
    /// leaf.
    pub fn toggle_guest(&mut self, ordinal: usize, guest: Guest) -> bool {
        let host = self.host_of(guest);
        if let Some(host) = host {
            let mut remaining = host;
            if let Some(Node::Leaf { guests, .. }) = self.leaf_mut(&mut remaining) {
                guests.retain(|hosted| *hosted != guest);
            }
        }
        if host == Some(ordinal) {
            return true;
        }
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf { guests, .. }) => {
                guests.push(guest);
                let mut ordered: Vec<Guest> = Guest::ALL
                    .into_iter()
                    .filter(|known| guests.contains(known))
                    .collect();
                std::mem::swap(guests, &mut ordered);
                true
            }
            _ => false,
        }
    }

    /// The configured selection of the leaf at `ordinal`, when it is an ascii tile.
    pub fn selection_at(&self, ordinal: usize) -> Option<AsciiSelection> {
        let mut remaining = ordinal;
        match self.leaf_ref(&mut remaining) {
            Some(Node::Leaf {
                kind: TileKind::Ascii,
                piece,
                ..
            }) => Some(piece.unwrap_or(AsciiSelection::DEFAULT)),
            _ => None,
        }
    }

    /// Set the selection of the leaf at `ordinal`; `false` when it is not an
    /// ascii tile.
    pub fn set_selection(&mut self, ordinal: usize, new_piece: AsciiSelection) -> bool {
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf {
                kind: TileKind::Ascii,
                piece,
                ..
            }) => {
                *piece = Some(new_piece);
                true
            }
            _ => false,
        }
    }

    fn leaf_ref(&self, ordinal: &mut usize) -> Option<&Node> {
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
                if let Some(leaf) = first.leaf_ref(ordinal) {
                    return Some(leaf);
                }
                second.leaf_ref(ordinal)
            }
        }
    }

    /// What the leaf at `ordinal` wears on its top row outside arrange mode.
    pub fn title_at(&self, ordinal: usize) -> TileTitle {
        let mut remaining = ordinal;
        match self.leaf_ref(&mut remaining) {
            Some(Node::Leaf { title, .. }) => *title,
            _ => TileTitle::Named,
        }
    }

    /// This leaf with no top row outside arrange mode.
    pub fn untitled(mut self) -> Self {
        self.set_title(0, TileTitle::Hidden);
        self
    }

    /// Set what the leaf at `ordinal` wears on its top row.
    pub fn set_title(&mut self, ordinal: usize, new_title: TileTitle) -> bool {
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf { title, .. }) => {
                *title = new_title;
                true
            }
            _ => false,
        }
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
        let Node::Leaf {
            kind,
            guests,
            room,
            piece,
            title,
        } = leaf
        else {
            return false;
        };
        let kept = Node::Leaf {
            kind: *kind,
            guests: std::mem::take(guests),
            room: *room,
            piece: *piece,
            title: *title,
        };
        *leaf = Node::split(dir, 500, kept, Node::leaf(new_kind));
        true
    }

    /// Change what the leaf at `ordinal` shows. A tile that stops being a
    /// chat forgets its room, so cycling back lands on the current room;
    /// one that stops being an ascii tile forgets its piece the same way,
    /// and a new kind starts with its name in its title.
    pub fn set_kind(&mut self, ordinal: usize, new_kind: TileKind) -> bool {
        let mut remaining = ordinal;
        match self.leaf_mut(&mut remaining) {
            Some(Node::Leaf {
                kind,
                room,
                piece,
                title,
                ..
            }) => {
                *kind = new_kind;
                if new_kind != TileKind::Chat {
                    *room = None;
                }
                if new_kind != TileKind::Ascii {
                    *piece = None;
                }
                *title = TileTitle::Named;
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
        gaps: Gaps,
    ) -> bool {
        let mut remaining = ordinal;
        self.resize_toward(&mut remaining, dir, delta_cells, area, gaps)
    }

    fn resize_toward(
        &mut self,
        ordinal: &mut usize,
        dir: Dir,
        delta_cells: i16,
        area: Rect,
        gaps: Gaps,
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
        let (first_area, second_area) = layout::split_rect(area, *my_dir, *share, gaps);
        let first_count = first.leaf_count();
        let (child, child_area, is_first) = if *ordinal < first_count {
            (first, first_area, true)
        } else {
            *ordinal -= first_count;
            (second, second_area, false)
        };
        // Deeper first: the split nearest the tile is the one that moves.
        if child.resize_toward(ordinal, dir, delta_cells, child_area, gaps) {
            return true;
        }
        if *my_dir != dir {
            return false;
        }
        let usable = layout::usable_len(area, *my_dir, gaps);
        let signed = if is_first { delta_cells } else { -delta_cells };
        // A split too small for two floors splits what it has evenly.
        let floor = MIN_TILE_CELLS.min(usable / 2);
        let target = (layout::first_len(usable, *share) as i32 + signed as i32)
            .clamp(floor as i32, (usable - floor) as i32) as u16;
        // A fixed split keeps its shape: the fixed child's count moves.
        *share = match *share {
            Share::Mille(_) => {
                Share::Mille(layout::share_for(usable, target).clamp(MIN_SHARE, MAX_SHARE))
            }
            Share::First { .. } => Share::First { first: target },
            Share::Second { .. } => Share::Second {
                second: usable - target,
            },
        };
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
    /// The stock `zen` Home layout (`presets.rs`), also what `R` resets it
    /// to: on the left, the
    /// bonsai beside the reef over the current room's chat; on the right a
    /// rail of clock, pet, lobby, live, then music (the pet sits against
    /// the tank, to its left, and watches it; unowned, the pet tile points
    /// at the shop).
    fn default() -> Self {
        Self {
            root: Node::split(
                Dir::Row,
                720,
                Node::split(
                    Dir::Column,
                    490,
                    Node::split(
                        Dir::Row,
                        500,
                        Node::leaf(TileKind::Bonsai),
                        Node::leaf(TileKind::Aquarium),
                    ),
                    Node::leaf(TileKind::Chat),
                ),
                Node::split(
                    Dir::Column,
                    170,
                    Node::leaf(TileKind::Clock),
                    Node::split(
                        Dir::Column,
                        360,
                        Node::leaf(TileKind::Pet),
                        Node::split(
                            Dir::Column,
                            250,
                            Node::leaf(TileKind::Lobby),
                            Node::split(
                                Dir::Column,
                                340,
                                Node::leaf(TileKind::Live),
                                Node::leaf(TileKind::Music),
                            ),
                        ),
                    ),
                ),
            ),
            look: Look::default(),
        }
    }
}

/// Home's rail width, the gutter beside it included.
pub const HOME_RAIL_COLS: u16 = 24;
/// Home's sidebar width, the gutter beside it included.
pub const HOME_SIDEBAR_COLS: u16 = 24;
/// The sidebar's pinned rows: the online count with the clock, the friends.
pub const PRESENCE_ROWS: u16 = 2;
/// The stock Home's reef, across the top of the chat: the reef's minimum,
/// its surface and floor around the tallest creature (`ReefState::min_height`,
/// eleven rows while that is Bigbert), so the fish swim in arrange mode too,
/// where the title takes the extra row this tile has outside it.
pub const HOME_AQUARIUM_ROWS: u16 = 11;
/// The stock lobby tile: two tables over the status line.
pub const HOME_LOBBY_ROWS: u16 = 3;

impl RiceLayout {
    /// Parse a stored layout; anything unreadable falls back to `fallback`,
    /// so a bad row never locks someone out of the page.
    pub fn from_json_or(value: Option<&serde_json::Value>, fallback: RiceLayout) -> Self {
        match value {
            Some(value) => serde_json::from_value(value.clone()).unwrap_or(fallback),
            None => fallback,
        }
    }

    /// Parse a stored layout; anything unreadable falls back to the stock
    /// `zen` layout, so a bad row never locks someone out of the page.
    pub fn from_json(value: Option<&serde_json::Value>) -> Self {
        Self::from_json_or(value, Self::default())
    }

    /// The stock Home: the room rail, then the reef across the top of the
    /// chat hosting the now strip, the chat under it, and a sidebar column
    /// of the presence row, the visualizer, the music dock, the lobby
    /// hosting attention, live and the pet (the shop's note while none is
    /// owned) at their panel rows, with the bonsai taking the rest. Fixed,
    /// the same for every account: the page is arranged in place
    /// (`Ctrl+E`), and this is what `R` resets to.
    pub fn home() -> Self {
        let center = Node::split_first(
            Dir::Column,
            HOME_AQUARIUM_ROWS + 1,
            // The row the title takes in arrange mode stays the reef's
            // outside it, so nothing moves when the mode comes up.
            Node::hosting(TileKind::Aquarium, &[Guest::Now]).untitled(),
            Node::leaf(TileKind::Chat),
        );
        // The sidebar's panels, each at its rows plus its title rule; the
        // bonsai, last, takes the rest of the column.
        let panels: [(TileKind, u16); 5] = [
            (
                TileKind::Visualizer,
                crate::app::common::sidebar::MUSIC_VIZ_HEIGHT,
            ),
            (
                TileKind::Music,
                crate::app::common::sidebar::MUSIC_DOCK_HEIGHT,
            ),
            (TileKind::Lobby, HOME_LOBBY_ROWS),
            (TileKind::Live, crate::app::live::panel::LIVE_PANEL_HEIGHT),
            // The pet's box and nothing else: no mood row.
            (TileKind::Pet, super::layout::FLOOR_ROWS),
        ];
        // The lobby hosts attention: what waits on you takes its tile over
        // and the latest tables come back when the list clears.
        let mut column = Node::leaf(TileKind::Bonsai);
        for (kind, rows) in panels.iter().rev() {
            let tile = match kind {
                TileKind::Lobby => Node::hosting(*kind, &[Guest::Attention]),
                TileKind::Visualizer
                | TileKind::Music
                | TileKind::Live
                | TileKind::Bonsai
                | TileKind::Rooms
                | TileKind::Aquarium
                | TileKind::Tank
                | TileKind::Ascii
                | TileKind::Pet
                | TileKind::Chat
                | TileKind::Clock
                | TileKind::Activity
                | TileKind::Friends
                | TileKind::Pulse
                | TileKind::Blank
                | TileKind::Presence => Node::leaf(*kind),
            };
            column = Node::split_first(Dir::Column, rows + 1, tile, column);
        }
        let sidebar = Node::split_first(
            Dir::Column,
            PRESENCE_ROWS,
            Node::leaf(TileKind::Presence).untitled(),
            column,
        );
        Self {
            root: Node::split_first(
                Dir::Row,
                HOME_RAIL_COLS - 1,
                Node::leaf(TileKind::Rooms).untitled(),
                Node::split_second(Dir::Row, HOME_SIDEBAR_COLS - 1, center, sidebar),
            ),
            look: Look {
                border: BorderKind::None,
                gap: 0,
                titles: true,
            },
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

/// What Enter in the guest picker did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuestPick {
    /// The guest moved onto, or off, the focused tile; the layout is dirty.
    Changed,
    /// Nothing to pick (no picker, or the focus is not on a tile).
    Unchanged,
}

/// Session state for Home's tile tree: the current layout and its focus.
pub struct ZenState {
    pub rice: RiceLayout,
    /// What `R` resets to.
    stock: RiceLayout,
    /// Whether the layout keys are live: only while arrange mode is up
    /// (`Ctrl+E`).
    pub arrange: bool,
    /// The guest picker `+` opens over the focused tile in arrange mode:
    /// the selected row, an index into `Guest::ALL`, while it is open.
    pub guest_picker: Option<usize>,
    /// Session-only playback slots in leaf order. Structural edits move these
    /// with their leaves; serialization only ever sees `rice`.
    playback: Vec<Option<Playback>>,
    visiting: bool,
    /// Focused tile, as an ordinal into `rice.root.leaf_kinds()`.
    pub focus: usize,
    /// The tile picker `space` opens over the focused tile: the selected
    /// row, an index into `TileKind::ALL`, while it is open.
    pub kind_picker: Option<usize>,
    /// The attention guest's selected row, clamped the same way.
    pub attention_selected: usize,
}

impl ZenState {
    /// The page's state: `rice` is what it shows, `stock` what `R` resets
    /// to. It starts outside arrange mode, its keys Home's, with the focus
    /// on its first chat tile from the first frame.
    pub fn new(rice: RiceLayout, stock: RiceLayout) -> Self {
        let focus = rice
            .root
            .leaf_kinds()
            .iter()
            .position(|kind| *kind == TileKind::Chat)
            .unwrap_or(0);
        Self {
            playback: vec![None; rice.root.leaf_count()],
            visiting: false,
            rice,
            stock,
            arrange: false,
            guest_picker: None,
            focus,
            kind_picker: None,
            attention_selected: 0,
        }
    }

    pub fn leaf_count(&self) -> usize {
        self.rice.root.leaf_count()
    }

    pub fn focused_kind(&self) -> Option<TileKind> {
        self.rice.root.kind_at(self.focus)
    }

    /// Whether the page draws a music equalizer: a visualizer tile, or a
    /// music tile whose track row marquees. Those paint on the anim_half
    /// edge, so the render loop has to wake that often while one is up.
    pub fn shows_equalizer(&self) -> bool {
        self.rice
            .root
            .leaf_kinds()
            .into_iter()
            .any(|kind| match kind {
                TileKind::Music | TileKind::Visualizer => true,
                TileKind::Bonsai
                | TileKind::Aquarium
                | TileKind::Tank
                | TileKind::Ascii
                | TileKind::Pet
                | TileKind::Chat
                | TileKind::Clock
                | TileKind::Lobby
                | TileKind::Activity
                | TileKind::Friends
                | TileKind::Pulse
                | TileKind::Live
                | TileKind::Blank
                | TileKind::Rooms
                | TileKind::Presence => false,
            })
    }

    /// Whether the page paints something on the anim_half edge: an
    /// equalizer, or a bonsai tile's sway. The render loop pays that edge
    /// only while a page shows one; a rail and a chat alone settle.
    pub fn paints_on_half_edge(&self) -> bool {
        self.shows_equalizer() || self.shows(TileKind::Bonsai)
    }

    /// The tile hosting `guest`, if any.
    pub fn host_of(&self, guest: Guest) -> Option<usize> {
        self.rice.root.host_of(guest)
    }

    /// Whether `guest` is on the page at all.
    pub fn hosts(&self, guest: Guest) -> bool {
        self.host_of(guest).is_some()
    }

    /// Whether the active chat tile hosts `guest`: there the strip inserts
    /// above the messages, the way Home's card has always carried it,
    /// instead of taking the tile.
    pub fn active_chat_hosts(&self, guest: Guest) -> bool {
        self.host_of(guest).is_some() && self.host_of(guest) == self.active_chat_ordinal()
    }

    /// Whether a tile on the page hosts `guest`.
    pub fn hosts_guest(&self, guest: Guest) -> bool {
        self.host_of(guest).is_some()
    }

    /// Whether the page draws its look's boxes: when the layout has a
    /// border (the stock `zen` layout, or `b` in arrange mode), since a
    /// layout with none draws Home's rules instead.
    fn boxed(&self) -> bool {
        self.rice.look.border != BorderKind::None
    }

    /// The layout's gaps: the look's uniform gap when boxed; under rules
    /// a one-column gutter between columns and none between rows (a
    /// tile's title rule is the row divider).
    pub fn gaps(&self) -> Gaps {
        match self.boxed() {
            true => Gaps::uniform(self.rice.look.gap as u16),
            false => Gaps { row: 1, col: 0 },
        }
    }

    /// The chrome tiles wear: the look's boxes, or Home's rules.
    pub fn chrome(&self) -> layout::Chrome {
        match self.boxed() {
            true => layout::Chrome::Boxed(self.rice.look.clone()),
            false => layout::Chrome::Rules {
                arrange: self.arrange,
            },
        }
    }

    /// Whether any tile on the page is `kind`: every tile on the page is
    /// drawn, so this is also the gate for work that only pays off on
    /// screen (the live strip's view and its wake tier).
    pub fn shows(&self, kind: TileKind) -> bool {
        self.rice.root.leaf_kinds().contains(&kind)
    }

    /// Resolve all tiles when a visit begins, before the first frame.
    pub(crate) fn enter(&mut self, now: Instant, rng: &mut impl Rng) {
        if !self.visiting {
            self.visiting = true;
            self.sync_playback(now, rng);
        }
    }

    pub(crate) fn leave(&mut self) {
        self.visiting = false;
        self.playback.fill(None);
    }

    /// Covered tiles still belong to this visit. Advancing them costs no
    /// rendering or persistence work.
    pub(crate) fn sync_playback(&mut self, now: Instant, rng: &mut impl Rng) -> bool {
        if !self.visiting {
            return false;
        }
        self.playback.resize(self.leaf_count(), None);
        let mut changed = false;
        for (ordinal, slot) in self.playback.iter_mut().enumerate() {
            let moved = Playback::sync(slot, self.rice.root.selection_at(ordinal), now, rng);
            changed |= moved;
        }
        changed
    }

    pub fn focused_selection(&self) -> Option<AsciiSelection> {
        self.rice.root.selection_at(self.focus)
    }

    /// The piece the tile at `ordinal` shows, when it is an ascii tile.
    pub fn piece_at(&self, ordinal: usize) -> Option<AsciiPiece> {
        let selection = self.rice.root.selection_at(ordinal)?;
        Playback::resolve(self.playback.get(ordinal).copied().flatten(), selection)
    }

    pub fn focused_piece(&self) -> Option<AsciiPiece> {
        self.piece_at(self.focus)
    }

    pub fn piece_label(&self, ordinal: usize) -> Option<String> {
        let selection = self.rice.root.selection_at(ordinal)?;
        let piece = self.piece_at(ordinal)?;
        Some(match selection {
            AsciiSelection::Piece(_) => piece.label(),
            AsciiSelection::Random | AsciiSelection::Randomer => {
                format!("{} · {}", selection.label(), piece.label())
            }
        })
    }

    /// The pieces the page's ascii tiles play, for the animation wake tier.
    pub fn pieces(&self) -> Vec<AsciiPiece> {
        (0..self.leaf_count())
            .filter_map(|i| self.piece_at(i))
            .collect()
    }

    /// Give the focused ascii tile `selection`, a random one drawn before the
    /// next frame; `false` when it already plays it or is not an ascii tile.
    pub fn set_focused_selection(
        &mut self,
        selection: AsciiSelection,
        now: Instant,
        rng: &mut impl Rng,
    ) -> bool {
        if self.focused_selection() == Some(selection)
            || !self.rice.root.set_selection(self.focus, selection)
        {
            return false;
        }
        self.sync_playback(now, rng);
        true
    }

    pub fn cycle_focused_piece(&mut self, forward: bool, now: Instant, rng: &mut impl Rng) -> bool {
        let Some(selection) = self.focused_selection() else {
            return false;
        };
        self.set_focused_selection(selection.cycle(forward), now, rng)
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
    /// active tile is the page's chat: it has the composer, the selection
    /// and the read marking, and on Home it is what the rail shows; every
    /// other chat tile only watches its room.
    pub fn active_chat_index(&self) -> Option<usize> {
        let tiles = self.chat_tiles();
        if tiles.is_empty() {
            return None;
        }
        match tiles.iter().position(|(ordinal, _)| *ordinal == self.focus) {
            Some(index) => Some(index),
            None => Some(0),
        }
    }

    /// The active chat tile's ordinal in the layout.
    pub fn active_chat_ordinal(&self) -> Option<usize> {
        let index = self.active_chat_index()?;
        Some(self.chat_tiles()[index].0)
    }

    /// Whether the keys belong to a chat tile: the focus is on one.
    pub fn chat_tile_focused(&self) -> bool {
        self.focused_kind() == Some(TileKind::Chat)
    }

    /// The active chat tile's bound room (`None` for the rail's room),
    /// when the page has a chat tile.
    pub fn active_chat_room(&self) -> Option<Option<Uuid>> {
        let index = self.active_chat_index()?;
        Some(self.chat_tiles()[index].1)
    }

    /// Bind the active chat tile to `room`; `false` when the page has no
    /// chat tile.
    pub fn bind_active_chat_room(&mut self, room: Option<Uuid>) -> bool {
        match self.active_chat_ordinal() {
            Some(ordinal) => self.rice.root.set_room(ordinal, room),
            None => false,
        }
    }

    /// Bind every chat tile bound to no room to `room_id`.
    pub fn pin_unbound_chats(&mut self, room_id: Uuid) {
        for (ordinal, bound) in self.chat_tiles() {
            if bound.is_none() {
                self.rice.root.set_room(ordinal, Some(room_id));
            }
        }
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
            self.playback.insert(self.focus + 1, None);
            self.focus += 1;
        }
        done
    }

    pub fn close_focused(&mut self) -> bool {
        let done = self.rice.root.close_leaf(self.focus);
        if done {
            self.playback.remove(self.focus);
        }
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
    /// chat counts itself out, so it can leave and come back); a kind the
    /// page holds once is refused once it holds one.
    pub fn kind_allowed(&self, kind: TileKind) -> bool {
        if kind.one_per_page() {
            return self.focused_kind() == Some(kind) || !self.shows(kind);
        }
        if kind != TileKind::Chat {
            return true;
        }
        let others =
            self.chat_tile_count() - usize::from(self.focused_kind() == Some(TileKind::Chat));
        others < MAX_CHAT_TILES
    }

    /// Open the guest picker over the focused tile, on its first row.
    pub fn open_guest_picker(&mut self) {
        if self.focused_kind().is_some() {
            self.guest_picker = Some(0);
        }
    }

    pub fn close_guest_picker(&mut self) {
        self.guest_picker = None;
    }

    pub fn move_guest_picker(&mut self, delta: isize) {
        let Some(selected) = self.guest_picker else {
            return;
        };
        let len = Guest::ALL.len() as isize;
        self.guest_picker = Some((selected as isize + delta).rem_euclid(len) as usize);
    }

    /// The guests of the focused tile, empty when it hosts none.
    pub fn focused_guests(&self) -> Vec<Guest> {
        self.rice
            .root
            .leaf_guests()
            .get(self.focus)
            .cloned()
            .unwrap_or_default()
    }

    /// Put the picker's guest on the focused tile, beside the ones it
    /// hosts, or take it off when it is there already. The picker stays
    /// open.
    pub fn pick_guest(&mut self) -> GuestPick {
        let Some(index) = self.guest_picker else {
            return GuestPick::Unchanged;
        };
        match self.rice.root.toggle_guest(self.focus, Guest::ALL[index]) {
            true => GuestPick::Changed,
            false => GuestPick::Unchanged,
        }
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
            // A tile that becomes ascii plays the fixed default (`set_kind`
            // forgets the piece), which needs no draw.
            self.playback[self.focus] = None;
            KindPick::Changed
        } else {
            KindPick::Unchanged
        }
    }

    /// Move the focused tile's edge by `delta_cells` along `dir`, on the
    /// tiles area the page currently lays out.
    pub fn resize_focused(&mut self, dir: Dir, delta_cells: i16, tiles_area: Rect) -> bool {
        let gaps = self.gaps();
        self.rice
            .root
            .resize_leaf(self.focus, dir, delta_cells, tiles_area, gaps)
    }

    pub fn flip_focused(&mut self) -> bool {
        self.rice.root.flip_parent(self.focus)
    }

    pub fn cycle_border(&mut self) {
        self.rice.look.border = self.rice.look.border.next();
    }

    pub fn cycle_gap(&mut self) {
        self.rice.look.gap = (self.rice.look.gap + 1) % (MAX_GAP + 1);
    }

    /// `t` in arrange mode: the focused tile's title moves on to the next
    /// mode: named, the rule alone, no row.
    pub fn cycle_focused_title(&mut self) -> bool {
        let next = self.rice.root.title_at(self.focus).next();
        self.rice.root.set_title(self.focus, next)
    }

    /// Back to the stock layout (`zen`'s page, or Home as it always drew),
    /// with the focus on its chat so the chat keys work at once.
    pub fn reset(&mut self) {
        self.rice = self.stock.clone();
        // Stock leaves carry no random selection, so nothing needs a draw.
        self.playback = vec![None; self.leaf_count()];
        self.focus = self.first_tile_of(TileKind::Chat).unwrap_or(0);
    }

    /// What `R` puts back from now on: a page just saved under a new name
    /// keeps its tree and takes that name's stock.
    pub fn set_stock(&mut self, stock: RiceLayout) {
        self.stock = stock;
    }

    /// Show another layout (`presets`): `rice` is the page now, `stock`
    /// what `R` puts back. The pickers close, the focus lands on its chat,
    /// and its ascii tiles are drawn afresh on the next playback sync.
    pub fn load(&mut self, rice: RiceLayout, stock: RiceLayout, now: Instant, rng: &mut impl Rng) {
        self.rice = rice;
        self.stock = stock;
        self.playback = vec![None; self.leaf_count()];
        self.focus = self.first_tile_of(TileKind::Chat).unwrap_or(0);
        self.kind_picker = None;
        self.guest_picker = None;
        self.sync_playback(now, rng);
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
