//! Pure rect math for Rice. Shared by the renderer and by `App`, which needs
//! the aquarium's rect ahead of the draw to size the reef simulation.

use ratatui::layout::Rect;

use super::state::{BorderKind, Dir, Look, Node, Share, TileKind};
use crate::app::pet::ui::{Neighbours, WatchSide};

/// The cells between siblings: `row` between the children of a row split
/// (side by side), `col` between those of a column split (stacked).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gaps {
    pub row: u16,
    pub col: u16,
}

impl Gaps {
    pub fn uniform(gap: u16) -> Self {
        Self { row: gap, col: gap }
    }

    pub fn along(self, dir: Dir) -> u16 {
        match dir {
            Dir::Row => self.row,
            Dir::Column => self.col,
        }
    }
}

/// What a tile wears. Zen boxes every tile in its look; Home draws rules:
/// a `── title ──` row on top of a tile that has a title, a column of air
/// on each side, and the gutters between columns painted as one line.
/// In arrange mode every tile wears a title, so the focus has a ring to
/// land on; outside it the page's chat and the rail draw their own chrome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Chrome {
    Boxed(Look),
    Rules { arrange: bool },
}

impl Chrome {
    /// Whether a tile of `kind` wears a title row under this chrome;
    /// `page_chat` marks the page's chat of the moment (Home's card).
    pub fn titled(&self, kind: TileKind, page_chat: bool) -> bool {
        match self {
            Chrome::Boxed(look) => look.titles,
            Chrome::Rules { arrange } => *arrange || !bare_under_rules(kind, page_chat),
        }
    }
}

/// The tiles that draw their own chrome on Home (the rail's hints, the
/// card's ticker and composer), so a title rule is only arrange mode's.
fn bare_under_rules(kind: TileKind, page_chat: bool) -> bool {
    match kind {
        TileKind::Chat => page_chat,
        TileKind::Rooms | TileKind::Presence => true,
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
        | TileKind::Inbox
        | TileKind::Headlines
        | TileKind::Live
        | TileKind::Blank => false,
    }
}

/// The pet box at its smallest: its three art rows.
pub const FLOOR_ROWS: u16 = crate::app::pet::ui::PET_BOX_MIN_ROWS;
/// The smallest page Rice draws. Under it the page is its too-small notice
/// alone: no tiles, and no status row to click.
pub const RICE_MIN_COLS: u16 = 40;
pub const RICE_MIN_ROWS: u16 = 12;

pub fn rice_fits(area: Rect) -> bool {
    area.width >= RICE_MIN_COLS && area.height >= RICE_MIN_ROWS
}

/// Whether the page gives its bottom row to the status line: whenever it
/// is drawn at all (`rice_fits`), unless a tile is zoomed, when that tile
/// is the whole page, the way the screensaver is. Every caller of
/// `rice_areas` that draws, clicks, or sizes what is on show reads it
/// from here.
pub fn rice_row(area: Rect, zoomed: bool) -> bool {
    rice_fits(area) && !zoomed
}

/// The Rice page: the tiles, then the status row at the bottom (the user's
/// status line and the page's keys) when `status_row` (`rice_row`); a page
/// too small to draw has no row, nor does a zoomed one.
pub fn rice_areas(area: Rect, status_row: bool) -> (Rect, Option<Rect>) {
    match status_row {
        true => {
            let row = Rect::new(
                area.x,
                area.bottom().saturating_sub(1),
                area.width,
                1.min(area.height),
            );
            let tiles = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
            (tiles, Some(row))
        }
        false => (area, None),
    }
}

/// Every tile's rect in layout order, with `gap` cells between siblings.
/// With `zoomed` set only that tile is returned, on the whole area.
pub fn tile_rects(
    root: &Node,
    area: Rect,
    gaps: Gaps,
    zoomed: Option<usize>,
) -> Vec<(TileKind, Rect)> {
    let mut out = Vec::with_capacity(root.leaf_count());
    collect_rects(root, area, gaps, &mut out);
    match zoomed {
        Some(ordinal) => match out.get(ordinal) {
            Some((kind, _)) => vec![(*kind, area)],
            None => out,
        },
        None => out,
    }
}

fn collect_rects(node: &Node, area: Rect, gaps: Gaps, out: &mut Vec<(TileKind, Rect)>) {
    match node {
        Node::Leaf { kind, .. } => out.push((*kind, area)),
        Node::Split {
            dir,
            share,
            first,
            second,
        } => {
            let (a, b) = split_rect(area, *dir, *share, gaps);
            collect_rects(first, a, gaps, out);
            collect_rects(second, b, gaps, out);
        }
    }
}

/// The gutters of every row split, in tree order: the column between two
/// side-by-side children, full height of the split. Empty when the row
/// gap is zero. The rules chrome paints a line down each.
pub fn row_gutters(root: &Node, area: Rect, gaps: Gaps) -> Vec<Rect> {
    let mut out = Vec::new();
    collect_gutters(root, area, gaps, &mut out);
    out
}

fn collect_gutters(node: &Node, area: Rect, gaps: Gaps, out: &mut Vec<Rect>) {
    let Node::Split {
        dir,
        share,
        first,
        second,
    } = node
    else {
        return;
    };
    let (a, b) = split_rect(area, *dir, *share, gaps);
    if *dir == Dir::Row && gaps.row > 0 && b.x > a.right() {
        out.push(Rect::new(a.right(), area.y, b.x - a.right(), area.height));
    }
    collect_gutters(first, a, gaps, out);
    collect_gutters(second, b, gaps, out);
}

/// The cells a split can hand out along its direction: its length minus the
/// gap between the two children.
pub fn usable_len(area: Rect, dir: Dir, gaps: Gaps) -> u16 {
    match dir {
        Dir::Row => area.width.saturating_sub(gaps.row),
        Dir::Column => area.height.saturating_sub(gaps.col),
    }
}

/// How many of `usable` cells the first child gets: its per-mille share,
/// or the fixed count of whichever child holds one.
pub fn first_len(usable: u16, share: Share) -> u16 {
    match share {
        Share::Mille(share) => (usable as u32 * share as u32 / 1000) as u16,
        Share::First { first } => first.min(usable),
        Share::Second { second } => usable.saturating_sub(second),
    }
}

/// The per-mille share that lays out exactly `target` first-child cells out
/// of `usable`: the smallest share whose `first_len` is not below the
/// target, which is also not above it while `usable` stays under 1000
/// cells, so a one-cell resize lands on exactly one cell.
pub fn share_for(usable: u16, target: u16) -> u16 {
    if usable == 0 {
        return 500;
    }
    (target as u32 * 1000).div_ceil(usable as u32) as u16
}

pub fn split_rect(area: Rect, dir: Dir, share: Share, gaps: Gaps) -> (Rect, Rect) {
    let gap = gaps.along(dir);
    match dir {
        Dir::Row => {
            let usable = area.width.saturating_sub(gap);
            let first_w = first_len(usable, share);
            let second_w = usable.saturating_sub(first_w);
            (
                Rect::new(area.x, area.y, first_w, area.height),
                Rect::new(
                    area.x + first_w + gap.min(area.width),
                    area.y,
                    second_w,
                    area.height,
                ),
            )
        }
        Dir::Column => {
            let usable = area.height.saturating_sub(gap);
            let first_h = first_len(usable, share);
            let second_h = usable.saturating_sub(first_h);
            (
                Rect::new(area.x, area.y, area.width, first_h),
                Rect::new(
                    area.x,
                    area.y + first_h + gap.min(area.height),
                    area.width,
                    second_h,
                ),
            )
        }
    }
}

/// The area left inside a tile of `kind` once its chrome is drawn. Boxed:
/// a border takes one cell on every side, a bare title takes the top row,
/// nothing else. Rules: a title takes the top row, and every tile but
/// the page's chat and the rail (which keep their own insets) gives up a
/// column on each side.
pub fn tile_inner(rect: Rect, chrome: &Chrome, kind: TileKind, page_chat: bool) -> Rect {
    let titled = chrome.titled(kind, page_chat);
    match chrome {
        Chrome::Boxed(look) => match look.border {
            BorderKind::None => {
                if titled {
                    Rect::new(
                        rect.x,
                        rect.y + 1.min(rect.height),
                        rect.width,
                        rect.height.saturating_sub(1),
                    )
                } else {
                    rect
                }
            }
            _ => Rect::new(
                rect.x + 1.min(rect.width),
                rect.y + 1.min(rect.height),
                rect.width.saturating_sub(2),
                rect.height.saturating_sub(2),
            ),
        },
        Chrome::Rules { .. } => {
            let top = if titled { 1.min(rect.height) } else { 0 };
            let (x, width) = match (kind, page_chat) {
                (TileKind::Chat, true) | (TileKind::Rooms, _) => (rect.x, rect.width),
                _ => (rect.x + 1.min(rect.width), rect.width.saturating_sub(2)),
            };
            Rect::new(x, rect.y + top, width, rect.height.saturating_sub(top))
        }
    }
}

/// Which side of `from` the tile `to` sits against, if the two share an
/// edge: touching across exactly the layout gap (the row gap for a side,
/// the column gap above and below), and overlapping along that edge.
/// Tiles that only meet at a corner, or with another tile between them,
/// are not neighbours.
pub fn neighbour_side(from: Rect, to: Rect, gaps: Gaps) -> Option<WatchSide> {
    let rows_overlap = to.y < from.bottom() && from.y < to.bottom();
    let cols_overlap = to.x < from.right() && from.x < to.right();
    if rows_overlap && to.x == from.right() + gaps.row {
        return Some(WatchSide::Right);
    }
    if rows_overlap && from.x == to.right() + gaps.row {
        return Some(WatchSide::Left);
    }
    if cols_overlap && to.y == from.bottom() + gaps.col {
        return Some(WatchSide::Below);
    }
    if cols_overlap && from.y == to.bottom() + gaps.col {
        return Some(WatchSide::Above);
    }
    None
}

/// Where the pet's tile sees a tank and a bonsai tile, among `rects` as
/// `tile_rects` returns them. No pet tile, or no tank or bonsai sharing an
/// edge with it, leaves that side empty.
pub fn pet_neighbours(rects: &[(TileKind, Rect)], gaps: Gaps) -> Neighbours {
    match rects.iter().find(|(kind, _)| *kind == TileKind::Pet) {
        None => Neighbours::default(),
        Some((_, pet_rect)) => {
            let side_of = |want: &[TileKind]| {
                rects
                    .iter()
                    .filter(|(kind, _)| want.contains(kind))
                    .find_map(|(_, other)| neighbour_side(*pet_rect, *other, gaps))
            };
            // The reef and the small tank are the one tank: the pet
            // watches either.
            Neighbours {
                tank: side_of(&[TileKind::Aquarium, TileKind::Tank]),
                bonsai: side_of(&[TileKind::Bonsai]),
            }
        }
    }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod layout_test;
