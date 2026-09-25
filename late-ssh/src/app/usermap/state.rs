use std::collections::BTreeMap;
use std::sync::Arc;

use tokio::sync::oneshot;

use crate::app::common::worldmap::data::{TerritoryId, WorldMap, map_by_id};
use ratatui::layout::Rect;

use crate::app::common::worldmap::view::{
    BLANK_LAND_COLOR, MIN_SCALE, Viewport, ZOOM_STEP, level_for, sample,
};
use crate::app::profile::svc::ProfileService;

/// The shading ladder, sparsest to busiest. Warm on purpose: the sea is dark
/// blue and unclaimed land is olive, so a country with people in it should
/// not be a shade of either. Lightness climbs monotonically, which is what
/// makes "more people" readable without consulting the legend.
pub const HEAT: [(u8, u8, u8); 5] = [
    (124, 92, 48),
    (168, 122, 52),
    (208, 158, 62),
    (236, 196, 96),
    (255, 232, 158),
];

/// The counts each rung starts at, worked out from the data rather than
/// fixed.
///
/// A ladder of 1/2/4/8/16 is right for a server with twenty people on it and
/// useless at either end of the scale: with four users everyone is on the
/// bottom rung, and with four thousand everyone is on the top one. These
/// floors are drawn from the busiest country instead, on the 1-2-5 ladder
/// that axis labels use, so the shading spreads itself over whatever range
/// the data actually covers. Fewer than five rungs when the range is small —
/// a legend reading "1, 2, 3" beats one reading "1, 1, 1, 2, 3".
pub fn heat_floors(busiest: usize) -> Vec<usize> {
    if busiest <= 1 {
        return vec![1];
    }
    // Every 1-2-5 step up to the busiest country.
    let mut ladder: Vec<usize> = Vec::new();
    let mut decade = 1usize;
    'outer: loop {
        for step in [1, 2, 5] {
            let value = decade * step;
            if value > busiest {
                break 'outer;
            }
            ladder.push(value);
        }
        let Some(next) = decade.checked_mul(10) else {
            break;
        };
        decade = next;
    }
    // Keep the top of the ladder: the busy end is where the differences are
    // worth seeing, and 1 always stays because "one person" is its own thing.
    let keep = HEAT.len();
    if ladder.len() > keep {
        ladder.drain(1..=(ladder.len() - keep));
    }
    ladder
}

/// Which rung a headcount sits on, given the floors in play.
pub fn heat_index_for(floors: &[usize], count: usize) -> usize {
    floors
        .iter()
        .rposition(|floor| count >= *floor)
        .unwrap_or(0)
        .min(HEAT.len() - 1)
}

/// How a rung reads in the legend: the count it starts at, with a `+` on the
/// open-ended top one.
///
/// Just the floor, not the range. A legend beside a 30-column list has to fit
/// in 30 columns, and "1 50 100 200 500+" does where "1-49 50-99 100-199
/// 200-499 500+" does not — it was silently clipped at the right edge, which
/// loses the top rung, which is the one somebody is looking for.
pub fn heat_label_for(floors: &[usize], rung: usize) -> String {
    let floor = floors.get(rung).copied().unwrap_or(1);
    // The top rung is open-ended, and a single rung is both top and bottom.
    if rung + 1 >= floors.len() {
        format!("{floor}+")
    } else {
        format!("{floor}")
    }
}

/// Which slice of people the map is showing. Tab walks them in this order:
/// the narrowest first, because "who is here right now" is the question that
/// brought most people to the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapMode {
    /// Connected right now.
    #[default]
    Online,
    /// Seen within `RECENT_DAYS` — the people who actually use the place,
    /// without the accounts that signed up once and never came back.
    Recent,
    /// Every account that ever set a country, dormant or not.
    Everyone,
}

/// How far back "recent" reaches. A month is long enough to include somebody
/// who was away for a fortnight and short enough that the map is of a living
/// population rather than a registry.
pub const RECENT_DAYS: i64 = 30;

impl MapMode {
    pub const ALL: [MapMode; 3] = [MapMode::Online, MapMode::Recent, MapMode::Everyone];

    pub fn next(self) -> Self {
        match self {
            MapMode::Online => MapMode::Recent,
            MapMode::Recent => MapMode::Everyone,
            MapMode::Everyone => MapMode::Online,
        }
    }

    /// What the title says this map is of.
    pub fn title(self) -> &'static str {
        match self {
            MapMode::Online => "online now",
            MapMode::Recent => "here this month",
            MapMode::Everyone => "everyone",
        }
    }

    /// The tab strip, which is also how somebody finds out the other two
    /// exist.
    pub fn tab_label(self) -> &'static str {
        match self {
            MapMode::Online => "online",
            MapMode::Recent => "month",
            MapMode::Everyone => "all",
        }
    }

    /// The word for one of the people counted, for the header line.
    pub fn noun(self, count: usize) -> String {
        match (self, count) {
            (_, 1) => "1 person".to_string(),
            (_, n) => format!("{n} people"),
        }
    }

    fn index(self) -> usize {
        match self {
            MapMode::Online => 0,
            MapMode::Recent => 1,
            MapMode::Everyone => 2,
        }
    }
}

/// The mouse actions this modal cares about, named where it can see them —
/// the input layer's `MouseEventKind` carries more than a map needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MousePress {
    Down,
    Drag,
    Up,
    ScrollUp,
    ScrollDown,
    Other,
}

/// What a count load comes back as: the tally, or why it could not be read.
type CountLoad = (MapMode, Result<Vec<(String, usize)>, String>);

/// One country on the list beside the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CountryTally {
    pub code: String,
    pub name: String,
    pub count: usize,
    /// None when the code is one the map has no country for (a territory
    /// Natural Earth does not carry, or a code that has since changed).
    pub territory: Option<TerritoryId>,
}

#[derive(Default)]
pub struct UserMapState {
    open: bool,
    mode: MapMode,
    /// One cache slot per mode, so Tab is instant after the first look at
    /// each. A refresh clears the slot it refills.
    cached: [Option<Vec<CountryTally>>; 3],
    /// The last counts loaded. Kept across a close so reopening is instant;
    /// a reopen refreshes them behind the shown map.
    tallies: Vec<CountryTally>,
    loading: bool,
    error: Option<String>,
    rx: Option<oneshot::Receiver<CountLoad>>,
    /// Set until the first draw knows how big the map area is.
    needs_fit: bool,
    view: Option<Viewport>,
    /// Cursor into `tallies`, which is also what is ringed on the map.
    cursor: usize,
    /// How many country rows the list had room for last time it drew. Set by
    /// the draw; a click needs the same number to work out which row it hit.
    visible_rows: usize,
    /// Where the map and the list were last drawn, so a click can be turned
    /// into a place. Recorded by the draw because only the draw knows.
    map_area: Option<Rect>,
    list_area: Option<Rect>,
    /// Where a left-button press landed, and whether the pointer has moved
    /// since. A press that never moves is a click; one that does is a drag.
    drag_anchor: Option<(u16, u16)>,
    dragged: bool,
}

impl UserMapState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn map(&self) -> Option<Arc<WorldMap>> {
        map_by_id("earth")
    }

    pub fn tallies(&self) -> &[CountryTally] {
        &self.tallies
    }

    pub fn mode(&self) -> MapMode {
        self.mode
    }

    /// The shading floors for what is on screen: derived from the busiest
    /// country, so the same five colours mean something on a server with six
    /// people and on one with six thousand.
    pub fn floors(&self) -> Vec<usize> {
        heat_floors(self.tallies.iter().map(|t| t.count).max().unwrap_or(1))
    }

    pub fn heat_index(&self, count: usize) -> usize {
        heat_index_for(&self.floors(), count)
    }

    /// Tab: the next slice of people. Cached results show at once; anything
    /// not looked at yet is asked for.
    pub fn cycle_mode(&mut self, profile: &ProfileService) {
        self.mode = self.mode.next();
        self.cursor = 0;
        self.error = None;
        match self.cached[self.mode.index()].clone() {
            Some(tallies) => {
                self.tallies = tallies;
                self.loading = false;
                // A cached slice is still worth re-asking for in the
                // background: the answer moves, especially for "online now".
                self.refresh(profile);
            }
            None => {
                self.tallies = Vec::new();
                self.refresh(profile);
            }
        }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn view(&self) -> Option<Viewport> {
        self.view
    }

    /// Called by the draw: this is the only place that knows where things
    /// ended up on screen, and the mouse needs to know.
    #[cfg(test)]
    pub fn list_area_for_test(&self) -> Option<Rect> {
        self.list_area
    }

    pub fn record_geometry(&mut self, map_area: Rect, list_area: Option<Rect>, rows: usize) {
        self.map_area = Some(map_area);
        self.list_area = list_area;
        self.visible_rows = rows;
    }

    /// Everyone counted, which is the headline: the map shades countries, and
    /// this says how many people those shades add up to.
    pub fn total(&self) -> usize {
        self.tallies.iter().map(|t| t.count).sum()
    }

    /// The country the cursor is on, ringed on the map.
    pub fn selected(&self) -> Option<TerritoryId> {
        self.tallies.get(self.cursor).and_then(|t| t.territory)
    }

    /// Territory colours for the painter: a rung per country, and nothing at
    /// all for countries nobody online claims.
    pub fn colors(&self) -> BTreeMap<TerritoryId, (u8, u8, u8)> {
        self.tallies
            .iter()
            .filter_map(|tally| Some((tally.territory?, HEAT[self.heat_index(tally.count)])))
            .collect()
    }

    pub fn open(&mut self, profile: &ProfileService) {
        self.open = true;
        self.needs_fit = true;
        self.cursor = 0;
        self.mode = MapMode::default();
        self.refresh(profile);
    }

    /// Open with nothing in flight. Only for tests and for the draw-path
    /// checks: the real opener always asks for fresh numbers.
    #[cfg(test)]
    pub fn open_without_loading(&mut self) {
        self.open = true;
        self.needs_fit = true;
        self.loading = true;
    }

    #[cfg(test)]
    pub fn set_mode_for_test(&mut self, mode: MapMode) {
        self.mode = mode;
    }

    #[cfg(test)]
    pub fn set_tallies_for_test(&mut self, counts: &[(&str, usize)]) {
        self.loading = false;
        self.tallies = counts
            .iter()
            .map(|(code, count)| CountryTally {
                name: country_name(code),
                territory: territory_for_code(code),
                code: code.to_string(),
                count: *count,
            })
            .collect();
        self.cursor = 0;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    /// Ask for the counts again. Safe to call while one is in flight — the
    /// second request simply replaces the first.
    pub fn refresh(&mut self, profile: &ProfileService) {
        self.loading = true;
        self.error = None;
        let (tx, rx) = oneshot::channel();
        self.rx = Some(rx);
        let profile = profile.clone();
        // The mode goes with the request and comes back with it: switching
        // tabs while a slow query is in flight must not paint one slice's
        // numbers under another slice's title.
        let mode = self.mode;
        tokio::spawn(async move {
            let result = profile
                .country_counts(mode)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send((mode, result));
        });
    }

    /// Poll the in-flight load. True when something changed and the frame
    /// needs drawing.
    pub fn tick(&mut self) -> bool {
        let Some(rx) = self.rx.as_mut() else {
            return false;
        };
        match rx.try_recv() {
            Ok((mode, Ok(counts))) => {
                self.rx = None;
                let tallies: Vec<CountryTally> = counts
                    .into_iter()
                    .map(|(code, count)| CountryTally {
                        name: country_name(&code),
                        territory: territory_for_code(&code),
                        code,
                        count,
                    })
                    .collect();
                self.cached[mode.index()] = Some(tallies.clone());
                // An answer for a mode nobody is looking at any more is
                // banked, not shown.
                if mode == self.mode {
                    self.loading = false;
                    self.tallies = tallies;
                    self.cursor = self.cursor.min(self.tallies.len().saturating_sub(1));
                }
                true
            }
            Ok((mode, Err(e))) => {
                self.rx = None;
                if mode == self.mode {
                    self.loading = false;
                    self.error = Some(e);
                }
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => false,
            Err(oneshot::error::TryRecvError::Closed) => {
                self.rx = None;
                self.loading = false;
                self.error = Some("could not read who is online".to_string());
                true
            }
        }
    }

    /// Set the view for an area, fitting the whole world on the first draw
    /// and clamping every time (the area changes when the terminal does).
    pub fn sync_view(&mut self, px_w: i32, px_h: i32) -> Viewport {
        let map = self.map();
        let Some(map) = map else {
            return Viewport {
                scale: 1.0,
                vx: 0.0,
                vy: 0.0,
                px_w,
                px_h,
            };
        };
        let view = match self.view {
            Some(view) if !self.needs_fit => Viewport { px_w, px_h, ..view }.clamped(&map),
            _ => Viewport::fitted(&map, px_w, px_h),
        };
        self.needs_fit = false;
        self.view = Some(view);
        view
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let (Some(map), Some(view)) = (self.map(), self.view) else {
            return;
        };
        // A step is a tenth of the view, so panning feels the same zoomed in
        // and zoomed out.
        let step_x = (view.px_w as f32 / 10.0).max(1.0);
        let step_y = (view.px_h as f32 / 10.0).max(1.0);
        self.view = Some(view.panned(dx * step_x, dy * step_y, &map));
    }

    pub fn zoom(&mut self, factor: f32) {
        let (Some(map), Some(view)) = (self.map(), self.view) else {
            return;
        };
        self.view = Some(view.zoomed(factor, &map));
    }

    pub fn zoom_in(&mut self) {
        self.zoom(1.0 / ZOOM_STEP);
    }

    pub fn zoom_out(&mut self) {
        self.zoom(ZOOM_STEP);
    }

    pub fn fit(&mut self) {
        self.needs_fit = true;
    }

    /// Move down the country list, and take the map with you: picking a
    /// country frames it, because finding Luxembourg on a world view by hand
    /// is not a thing anybody should have to do.
    pub fn move_cursor(&mut self, delta: isize) {
        if self.tallies.is_empty() {
            return;
        }
        let len = self.tallies.len() as isize;
        self.cursor = (self.cursor as isize + delta).rem_euclid(len) as usize;
        let (Some(map), Some(view)) = (self.map(), self.view) else {
            return;
        };
        if let Some(target) = self.selected()
            && let Some(focused) = Viewport::focused(&map, target, view.px_w, view.px_h)
        {
            self.view = Some(focused);
        }
    }

    // ----- mouse

    /// Wheel, drag and click, in the coordinates the terminal reports
    /// (1-based). True when the event was ours, so the caller knows it was
    /// spent here.
    pub fn handle_mouse(&mut self, kind: MousePress, x: u16, y: u16) -> bool {
        let x = x.saturating_sub(1);
        let y = y.saturating_sub(1);
        let in_map = self
            .map_area
            .is_some_and(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height);
        let in_list = self
            .list_area
            .is_some_and(|r| x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height);

        match kind {
            MousePress::ScrollUp if in_map => {
                self.zoom_in();
                true
            }
            MousePress::ScrollDown if in_map => {
                self.zoom_out();
                true
            }
            // The list scrolls with the wheel too, since that is what a list
            // under a pointer is expected to do.
            MousePress::ScrollUp if in_list => {
                self.move_cursor(-1);
                true
            }
            MousePress::ScrollDown if in_list => {
                self.move_cursor(1);
                true
            }
            MousePress::Down if in_map => {
                // Whether this is a click or a drag is decided by whether the
                // pointer moves before it comes up.
                self.drag_anchor = Some((x, y));
                self.dragged = false;
                true
            }
            MousePress::Down if in_list => {
                self.select_list_row(y);
                true
            }
            MousePress::Drag => {
                let Some((from_x, from_y)) = self.drag_anchor else {
                    return false;
                };
                // The map follows the hand: dragging right pulls the world
                // right, so the view moves left.
                let dx = f32::from(from_x) - f32::from(x);
                // Two map pixels to a terminal row.
                let dy = (f32::from(from_y) - f32::from(y)) * 2.0;
                if dx != 0.0 || dy != 0.0 {
                    self.dragged = true;
                    self.drag_anchor = Some((x, y));
                    self.pan_pixels(dx, dy);
                }
                true
            }
            MousePress::Up => {
                let was_drag = self.dragged;
                let had_anchor = self.drag_anchor.is_some();
                self.drag_anchor = None;
                self.dragged = false;
                // A press that never moved is a pick.
                if had_anchor && !was_drag && in_map {
                    self.select_at(x, y);
                }
                had_anchor
            }
            _ => false,
        }
    }

    /// Pan by raw drawn pixels (the mouse works in pixels; the keys work in
    /// fractions of a screen).
    fn pan_pixels(&mut self, dx: f32, dy: f32) {
        let (Some(map), Some(view)) = (self.map(), self.view) else {
            return;
        };
        self.view = Some(view.panned(dx, dy, &map));
    }

    /// Put the cursor on whatever country was clicked, if anybody online is
    /// in it. Clicking an empty country is not an error — there is simply
    /// nothing in the list to point at — so the selection stays where it was.
    fn select_at(&mut self, x: u16, y: u16) -> bool {
        let (Some(map), Some(view), Some(rect)) = (self.map(), self.view, self.map_area) else {
            return false;
        };
        let (level, level_scale) = level_for(&map, view.scale);
        let (bx, by) = view.base_at(i32::from(x - rect.x), i32::from(y - rect.y) * 2);
        let Some(cell) = sample(level, level_scale, bx, by) else {
            return false;
        };
        let Some(index) = self
            .tallies
            .iter()
            .position(|tally| tally.territory == Some(cell))
        else {
            return false;
        };
        self.cursor = index;
        true
    }

    /// A click in the list picks the row under the pointer.
    fn select_list_row(&mut self, y: u16) -> bool {
        let Some(rect) = self.list_area else {
            return false;
        };
        let row = y.saturating_sub(rect.y) as usize;
        let Some(index) = self.first_visible_row().checked_add(row) else {
            return false;
        };
        if index >= self.tallies.len() {
            return false;
        }
        // Go through the cursor move so the map frames it, exactly as the
        // keyboard would.
        let delta = index as isize - self.cursor as isize;
        self.move_cursor(delta);
        true
    }

    /// The first tally the list is showing, which both the draw and a click
    /// have to agree on.
    pub fn first_visible_row(&self) -> usize {
        let rows = self.visible_rows.max(1);
        self.cursor.saturating_sub(rows.saturating_sub(1))
    }

    /// The colour a legend swatch draws in, including the "nobody here" one.
    pub fn blank_color() -> (u8, u8, u8) {
        BLANK_LAND_COLOR
    }

    /// Guard for the smallest sane zoom, used by the input layer.
    pub fn at_min_zoom(&self) -> bool {
        self.view
            .is_some_and(|v| v.scale <= MIN_SCALE + f32::EPSILON)
    }
}

/// The map's territory for an ISO-3166 alpha-2 code. The atlas carries the
/// same codes the profile country picker offers, so this is a lookup rather
/// than a translation — but not every code has a country on a 242-territory
/// map (a few small territories are folded into their neighbours), and those
/// simply do not shade.
pub fn territory_for_code(code: &str) -> Option<TerritoryId> {
    let map = map_by_id("earth")?;
    let code = code.trim().to_ascii_uppercase();
    map.territories
        .iter()
        .find(|t| t.iso.eq_ignore_ascii_case(&code))
        .map(|t| t.id)
}

/// The display name for a code: the map's own name where it has one, else the
/// profile picker's list, else the bare code.
pub fn country_name(code: &str) -> String {
    let code_upper = code.trim().to_ascii_uppercase();
    if let Some(map) = map_by_id("earth")
        && let Some(territory) = map
            .territories
            .iter()
            .find(|t| t.iso.eq_ignore_ascii_case(&code_upper))
    {
        return territory.name.clone();
    }
    crate::app::settings_modal::data::country_name(&code_upper)
        .map(str::to_string)
        .unwrap_or(code_upper)
}
