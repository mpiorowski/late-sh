//! Looking at a world map: where the viewport is, and painting it.
//!
//! Terminals are twice as tall per cell as they are wide, so a map is drawn
//! in half-blocks — `▀` with a foreground for the top pixel and a background
//! for the bottom, which doubles the vertical resolution for free. That, the
//! zoom pyramid, and screen-space border detection are the same work whether
//! the colours mean "who holds this" (realm) or "how many people are here"
//! (`/map`), so they live here and the callers bring a palette.

use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;

use super::data::{TerritoryGrid, TerritoryId, WATER, WorldMap};

pub const WATER_COLOR: (u8, u8, u8) = (16, 34, 56);
/// Land nobody is associated with: unowned in realm, nobody-from-here on the
/// active map. Deliberately drab — it is the background the data sits on.
pub const BLANK_LAND_COLOR: (u8, u8, u8) = (92, 96, 78);
/// The ring round whatever is selected. Deliberately not a theme colour:
/// palettes carry ambers that are violet or mint, and one of those landing
/// on a player's hue would make the selection unfindable in exactly the
/// game where it matters. White belongs to no player and to no terrain.
pub const SELECTION_COLOR: (u8, u8, u8) = (255, 255, 255);

/// Zoom bounds. Below 1.0 the base grid is magnified (each ~10km cell drawn
/// several pixels wide); the ceiling is whatever shows the whole world.
pub const MIN_SCALE: f32 = 0.25;
/// Territory outlines are drawn only once you are looking at a region rather
/// than the globe (roughly 60km per pixel and finer). Outlining every country
/// on a whole-world view turns the map into noise — at that zoom most
/// countries are a pixel or two wide, so the outline IS the country.
pub const BORDER_MAX_SCALE: f32 = 6.0;
/// One zoom step. Small enough that holding `+` feels continuous, big enough
/// that a few presses get somewhere.
pub const ZOOM_STEP: f32 = 1.25;

/// The drawn viewport: where we are on the base grid and how tightly it is
/// packed. `scale` is base cells per drawn pixel, so it is continuous — the
/// zoom keys multiply it by a small factor instead of stepping mip levels.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub scale: f32,
    /// Top-left of the drawn area, in base-grid cells.
    pub vx: f32,
    pub vy: f32,
    /// Drawn size in pixels (a terminal row is two pixels tall).
    pub px_w: i32,
    pub px_h: i32,
}

impl Viewport {
    /// Base-grid cell under a pixel of the drawn area.
    pub fn base_at(&self, px: i32, py: i32) -> (f32, f32) {
        (
            self.vx + px as f32 * self.scale,
            self.vy + py as f32 * self.scale,
        )
    }

    /// The whole map, centred and just fitting the area.
    pub fn fitted(map: &WorldMap, px_w: i32, px_h: i32) -> Self {
        let scale = max_scale(map, px_w, px_h);
        Self {
            scale,
            vx: (map.grid.width as f32 - px_w as f32 * scale) / 2.0,
            vy: (map.grid.height as f32 - px_h as f32 * scale) / 2.0,
            px_w,
            px_h,
        }
    }

    /// Frame a territory with room around it — "show me this country".
    pub fn focused(map: &WorldMap, target: TerritoryId, px_w: i32, px_h: i32) -> Option<Self> {
        let territory = map.territory(target)?;
        let (x0, y0, x1, y1) = territory.bbox;
        let w = (x1 - x0 + 1) as f32;
        let h = (y1 - y0 + 1) as f32;
        // 3x the box, so the country sits in its neighbourhood rather than
        // filling the screen edge to edge.
        let needed = (w * 3.0 / px_w.max(1) as f32).max(h * 3.0 / px_h.max(1) as f32);
        let scale = needed.clamp(MIN_SCALE, max_scale(map, px_w, px_h));
        let cx = (x0 as f32 + x1 as f32) / 2.0;
        let cy = (y0 as f32 + y1 as f32) / 2.0;
        Some(
            Self {
                scale,
                vx: cx - px_w as f32 * scale / 2.0,
                vy: cy - px_h as f32 * scale / 2.0,
                px_w,
                px_h,
            }
            .clamped(map),
        )
    }

    /// Keep the view on the map: no scrolling into the void past an edge.
    ///
    /// An axis with no room to scroll is *centred* rather than pinned to
    /// zero. The world is twice as wide as it is tall and a terminal usually
    /// is not, so at most zooms the whole map height fits with room to spare
    /// — pinning it to the top puts the entire dead band under the map and
    /// the Antarctic edge hard against the frame.
    pub fn clamped(mut self, map: &WorldMap) -> Self {
        let ceiling = max_scale(map, self.px_w, self.px_h);
        self.scale = self.scale.clamp(MIN_SCALE, ceiling);
        let span_x = map.grid.width as f32 - self.px_w as f32 * self.scale;
        let span_y = map.grid.height as f32 - self.px_h as f32 * self.scale;
        self.vx = if span_x > 0.0 {
            self.vx.clamp(0.0, span_x)
        } else {
            span_x / 2.0
        };
        self.vy = if span_y > 0.0 {
            self.vy.clamp(0.0, span_y)
        } else {
            span_y / 2.0
        };
        self
    }

    /// Is there anywhere to pan on this axis, or is the whole map already in
    /// view? The board says "the whole world fits" rather than letting a key
    /// do nothing silently.
    pub fn can_pan_vertically(&self, map: &WorldMap) -> bool {
        map.grid.height as f32 - self.px_h as f32 * self.scale > 0.0
    }

    pub fn can_pan_horizontally(&self, map: &WorldMap) -> bool {
        map.grid.width as f32 - self.px_w as f32 * self.scale > 0.0
    }

    /// Zoom about the middle of the view, which is where the eye is.
    pub fn zoomed(mut self, factor: f32, map: &WorldMap) -> Self {
        let before = self.scale;
        let after = (before * factor).clamp(MIN_SCALE, max_scale(map, self.px_w, self.px_h));
        // Keep the centre cell under the centre pixel: zooming that walks the
        // map out from under you is the classic way to lose your place.
        let cx = self.vx + self.px_w as f32 * before / 2.0;
        let cy = self.vy + self.px_h as f32 * before / 2.0;
        self.scale = after;
        self.vx = cx - self.px_w as f32 * after / 2.0;
        self.vy = cy - self.px_h as f32 * after / 2.0;
        self.clamped(map)
    }

    /// Pan by whole drawn pixels.
    pub fn panned(mut self, dx: f32, dy: f32, map: &WorldMap) -> Self {
        self.vx += dx * self.scale;
        self.vy += dy * self.scale;
        self.clamped(map)
    }

    /// The territory at the centre pixel — what a crosshair is pointing at.
    pub fn center_territory(&self, map: &WorldMap) -> Option<TerritoryId> {
        let (level, level_scale) = level_for(map, self.scale);
        let (bx, by) = self.base_at(self.px_w / 2, self.px_h / 2);
        sample(level, level_scale, bx, by).filter(|cell| *cell != WATER)
    }
}

/// The widest useful scale for an area: the whole map, just fitting.
pub fn max_scale(map: &WorldMap, px_w: i32, px_h: i32) -> f32 {
    let fit_x = map.grid.width as f32 / px_w.max(1) as f32;
    let fit_y = map.grid.height as f32 / px_h.max(1) as f32;
    fit_x.max(fit_y).max(MIN_SCALE)
}

/// The mip level to sample for `scale`, and how many base cells one of its
/// cells covers. Sampling a level coarser than the scale would lose small
/// countries, so this picks the finest level that is still no finer than
/// needed — the majority-vote mips keep microstates visible when zoomed out,
/// while the fractional remainder keeps zooming smooth.
pub fn level_for(map: &WorldMap, scale: f32) -> (&TerritoryGrid, f32) {
    let mut level_scale = 1.0f32;
    let mut index = 0usize;
    while index < map.mips.len() && level_scale * 2.0 <= scale {
        level_scale *= 2.0;
        index += 1;
    }
    let grid = if index == 0 {
        &map.grid
    } else {
        &map.mips[index - 1]
    };
    (grid, level_scale)
}

/// Sample a mip level at a base-grid coordinate.
pub fn sample(level: &TerritoryGrid, level_scale: f32, bx: f32, by: f32) -> Option<u16> {
    if bx < 0.0 || by < 0.0 {
        return None;
    }
    let lx = (bx / level_scale) as i32;
    let ly = (by / level_scale) as i32;
    if lx < 0 || ly < 0 || lx >= level.width as i32 || ly >= level.height as i32 {
        return None;
    }
    Some(level.cells[ly as usize * level.width as usize + lx as usize])
}

/// What a drawn pixel is, beyond which territory it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Plain fill.
    None,
    /// A boundary between two territories.
    Border,
    /// The edge of whatever is selected.
    SelectionEdge,
}

/// What the caller wants painted: a colour per territory, and optionally one
/// of them ringed. Everything else about how a map is drawn is the same for
/// every caller.
pub struct Paint<'a> {
    pub colors: &'a BTreeMap<TerritoryId, (u8, u8, u8)>,
    pub selected: Option<TerritoryId>,
    /// Draw a `┼` at the middle — the keyboard cursor. Off for a map nobody
    /// is pointing at.
    pub crosshair: bool,
}

/// The colour of one drawn pixel.
///
/// The fill already carries the caller's meaning (whose it is, how many are
/// there), so a selection cannot speak in shade too or the two compete and
/// neither reads. It is drawn as an outline instead, which is a channel the
/// fill does not use: at world zoom a one-pixel country turns white whole,
/// and closer in it gets a ring.
fn cell_color(cell: u16, colors: &BTreeMap<TerritoryId, (u8, u8, u8)>, mark: Mark) -> Color {
    if mark == Mark::SelectionEdge {
        let (r, g, b) = SELECTION_COLOR;
        return Color::Rgb(r, g, b);
    }
    let (r, g, b) = if cell == WATER {
        WATER_COLOR
    } else {
        colors.get(&cell).copied().unwrap_or(BLANK_LAND_COLOR)
    };
    if mark == Mark::Border {
        // Darkened outline: two neighbouring countries in the same colour
        // stay tellable apart, which is the whole point when a continent has
        // gone one colour.
        return Color::Rgb(r / 3, g / 3, b / 3);
    }
    Color::Rgb(r, g, b)
}

/// Paint the map into `area`. One terminal row is two map pixels.
pub fn paint(frame: &mut Frame, area: Rect, map: &WorldMap, view: &Viewport, spec: &Paint) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (level, level_scale) = level_for(map, view.scale);

    // Sample the whole drawn area first: borders are decided in SCREEN space
    // (does this pixel's territory differ from the one left of it, or above
    // it?), so an outline stays one pixel wide at any zoom instead of
    // fattening up as the map is magnified.
    let rows = area.height as usize * 2;
    let cols = area.width as usize;
    let mut cells: Vec<u16> = vec![WATER; rows * cols];
    for py in 0..rows {
        for px in 0..cols {
            let (bx, by) = view.base_at(px as i32, py as i32);
            cells[py * cols + px] = sample(level, level_scale, bx, by).unwrap_or(WATER);
        }
    }

    let outlines = view.scale <= BORDER_MAX_SCALE;
    let neighbours = |py: usize, px: usize| -> [Option<u16>; 4] {
        [
            px.checked_sub(1).map(|p| cells[py * cols + p]),
            py.checked_sub(1).map(|p| cells[p * cols + px]),
            (px + 1 < cols).then(|| cells[py * cols + px + 1]),
            (py + 1 < rows).then(|| cells[(py + 1) * cols + px]),
        ]
    };
    // Is this pixel on the edge of whatever is selected? Computed whatever
    // the zoom, because the selection has to be findable even when ordinary
    // borders are off.
    let is_selection_edge = |py: usize, px: usize| -> bool {
        let here = cells[py * cols + px];
        if here == WATER || spec.selected != Some(here) {
            return false;
        }
        // A pixel at the drawn edge counts, so a country running off the
        // side of the viewport still shows its ring.
        neighbours(py, px)
            .into_iter()
            .any(|other| other != Some(here))
    };
    let is_border = |py: usize, px: usize| -> bool {
        if !outlines {
            return false;
        }
        let here = cells[py * cols + px];
        if here == WATER {
            return false;
        }
        neighbours(py, px)
            .into_iter()
            .flatten()
            .any(|other| other != here && other != WATER)
    };

    let buf = frame.buffer_mut();
    for row in 0..area.height {
        let top = row as usize * 2;
        let bottom = top + 1;
        for col in 0..area.width {
            let px = col as usize;
            let mark_at = |py: usize| -> Mark {
                if is_selection_edge(py, px) {
                    Mark::SelectionEdge
                } else if is_border(py, px) {
                    Mark::Border
                } else {
                    Mark::None
                }
            };
            let top_color = cell_color(cells[top * cols + px], spec.colors, mark_at(top));
            let bottom_color = cell_color(cells[bottom * cols + px], spec.colors, mark_at(bottom));
            if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                cell.set_symbol("▀");
                cell.set_fg(top_color);
                cell.set_bg(bottom_color);
            }
        }
    }

    if spec.crosshair
        && let Some(cell) = buf.cell_mut((area.x + area.width / 2, area.y + area.height / 2))
    {
        cell.set_symbol("┼");
        cell.set_fg(crate::app::common::theme::AMBER());
    }
}

#[cfg(test)]
#[path = "view_test.rs"]
mod view_test;
