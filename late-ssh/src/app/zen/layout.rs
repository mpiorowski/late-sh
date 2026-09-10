//! Pure rect math for Rice. Shared by the renderer and by `App`, which needs
//! the aquarium's rect ahead of the draw to size the reef simulation.

use ratatui::layout::Rect;

use super::state::{BorderKind, Dir, Look, Node, TileKind};

/// One row under the tree for its status line.
pub const BONSAI_STATUS_ROWS: u16 = 1;
/// The pet box at its smallest: the Home strip's height.
pub const FLOOR_ROWS: u16 = crate::app::pet::ui::PET_STRIP_HEIGHT;
/// The Rice page: one hint row at the bottom, tiles above.
pub fn rice_areas(area: Rect) -> (Rect, Rect) {
    let hint = Rect::new(
        area.x,
        area.bottom().saturating_sub(1),
        area.width,
        1.min(area.height),
    );
    let tiles = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    (tiles, hint)
}

/// Every tile's rect in layout order, with `gap` cells between siblings.
/// With `zoomed` set only that tile is returned, on the whole area.
pub fn tile_rects(
    root: &Node,
    area: Rect,
    gap: u16,
    zoomed: Option<usize>,
) -> Vec<(TileKind, Rect)> {
    let mut out = Vec::with_capacity(root.leaf_count());
    collect_rects(root, area, gap, &mut out);
    match zoomed {
        Some(ordinal) => match out.get(ordinal) {
            Some((kind, _)) => vec![(*kind, area)],
            None => out,
        },
        None => out,
    }
}

fn collect_rects(node: &Node, area: Rect, gap: u16, out: &mut Vec<(TileKind, Rect)>) {
    match node {
        Node::Leaf { kind } => out.push((*kind, area)),
        Node::Split {
            dir,
            share,
            first,
            second,
        } => {
            let (a, b) = split_rect(area, *dir, *share, gap);
            collect_rects(first, a, gap, out);
            collect_rects(second, b, gap, out);
        }
    }
}

/// The cells a split can hand out along its direction: its length minus the
/// gap between the two children.
pub fn usable_len(area: Rect, dir: Dir, gap: u16) -> u16 {
    match dir {
        Dir::Row => area.width.saturating_sub(gap),
        Dir::Column => area.height.saturating_sub(gap),
    }
}

/// How many of `usable` cells the first child gets for a per-mille `share`.
pub fn first_len(usable: u16, share: u16) -> u16 {
    (usable as u32 * share as u32 / 1000) as u16
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

pub fn split_rect(area: Rect, dir: Dir, share: u16, gap: u16) -> (Rect, Rect) {
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

/// The area left inside a tile once its chrome is drawn: a border takes one
/// cell on every side, a bare title takes the top row, nothing else.
pub fn tile_inner(rect: Rect, look: &Look) -> Rect {
    match look.border {
        BorderKind::None => {
            if look.titles {
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
    }
}
