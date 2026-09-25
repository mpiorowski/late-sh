//! Per-session realm state: the snapshot/event drains, and the full-screen
//! board state (`Screen::Realm`) — map viewport, selection, the target table,
//! and async row/log loads. Mirrors `daily::state` shapes.
//!
//! There is no local action draft any more: an action is sent the moment it
//! is taken and comes back as a result, so the only client-side state is what
//! you are looking at.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use late_core::models::realm_game::RealmGame;
use ratatui::layout::Rect;
use tokio::sync::{broadcast, oneshot, watch};
use uuid::Uuid;

use crate::app::common::primitives::{Banner, Screen};

use super::map::{TerritoryId, WorldMap, map_handle};
use super::resolver::{
    ActionRejected, ReachIndex, RealmAction, RealmGameState, RealmPlayerStatus,
    projected_probability,
};
use super::rulesets::Reach;
use super::svc::{
    ArchivedDay, RealmEvent, RealmService, RealmSnapshot, countdown_label, muster_left,
    parse_state, realm_day,
};

/// Which face of the realm screen is showing; Tab cycles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RealmView {
    Map,
    /// The target table: every territory you can reach, what it costs you in
    /// hops, who holds it, and the odds — the map without the squinting.
    Targets,
    Overview,
    Log,
    /// The realm as it was: a day picked with the arrows, the map painted
    /// from that day's closing board, and what happened on it beside.
    History,
}

impl RealmView {
    pub fn next(self) -> Self {
        match self {
            RealmView::Map => RealmView::Targets,
            RealmView::Targets => RealmView::Overview,
            RealmView::Overview => RealmView::Log,
            RealmView::Log => RealmView::History,
            RealmView::History => RealmView::Map,
        }
    }
}

/// Where the create-game overlay is: the rules, then the tempo, then the
/// hour its daily points come back, then a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateStep {
    Ruleset,
    /// The world it is fought over. Its own step rather than a property of
    /// the rules: the same rules on another map is another game.
    Map,
    /// The shape of a generated world — continents, islands, countries. Only
    /// reached when the map picked is the generated one, so choosing Earth
    /// still walks the same short path it always did.
    MapShape,
    Pace,
    ResetHour,
    Name,
    /// Which colour the creator plays in.
    Color,
    /// The rule toggles. Last, because everything before it has a sensible
    /// default and this is where a creator tinkers.
    Options,
}

#[derive(Clone, Debug)]
pub struct RealmCreateDraft {
    pub step: CreateStep,
    /// Cursor into `rulesets::OPTIONS`.
    pub option: usize,
    /// What is switched on, by option id.
    pub options: BTreeMap<String, bool>,
    pub ruleset: usize,
    /// Cursor into `map::MAPS`.
    pub map: usize,
    /// The generated world's parameters, and which of the three the cursor is
    /// on. Carried even when Earth is picked, so stepping back and forth
    /// between the maps does not lose what was dialled in.
    pub shape: super::mapgen::GeneratedMapSpec,
    pub shape_field: usize,
    /// Cursor into `state::PLAYER_PALETTES` — the creator's colour. They are
    /// first, so every colour is theirs to take.
    pub color: usize,
    /// Index into `rulesets::PACES`.
    pub pace: usize,
    /// UTC hour the game refills at.
    pub hour: i16,
    /// What the creator is calling it. Left blank, the service fills in
    /// "<creator>'s realm".
    pub name: String,
}

/// Arriving at a realm someone else is already playing. The only thing to
/// settle is which colour is yours, and the one thing it must not do is let
/// two players wear the same one.
#[derive(Clone, Debug)]
pub struct RealmJoinDraft {
    pub game_id: Uuid,
    pub game_name: String,
    /// Cursor into `PLAYER_PALETTES`.
    pub color: usize,
    /// Palette slots already on that map, and who is wearing them.
    pub taken: Vec<(usize, String)>,
}

pub struct RealmState {
    svc: RealmService,
    pub user_id: Uuid,
    pub username: String,
    /// The viewer's account timezone, mirrored from `App` — every hour shown
    /// is written in both their clock and UTC.
    pub viewer_tz: Option<chrono_tz::Tz>,
    pub snapshot: Arc<RealmSnapshot>,
    snapshot_rx: watch::Receiver<Arc<RealmSnapshot>>,
    event_rx: broadcast::Receiver<RealmEvent>,
    pub board: Option<RealmBoardState>,
    /// The create-game overlay in the Lobby modal.
    pub create_draft: Option<RealmCreateDraft>,
    /// The join overlay: the colour you will be arriving as.
    pub join_draft: Option<RealmJoinDraft>,
    /// The second a countdown on screen was last drawn at. A muster clock is
    /// the one thing here that changes without anything arriving, so it is
    /// the one thing that has to ask for its own frames.
    countdown_second: Option<i64>,
}

pub struct RealmBoardState {
    pub game_id: Uuid,
    pub return_screen: Screen,
    pub view: RealmView,
    /// Viewport origin in BASE-grid cells (pixel rows are 2 per terminal row
    /// via half blocks). Cells because the renderer fits/clamps them against
    /// the drawn area (which only it knows), daily-geometry style.
    pub view_x: Cell<f32>,
    pub view_y: Cell<f32>,
    /// Base-grid cells per drawn pixel: 1.0 is native (~10km/px), below 1
    /// magnifies, above 1 shrinks. Continuous, so zoom steps are small
    /// multiplications rather than power-of-two jumps; the renderer picks
    /// whichever mip level is closest underneath and samples within it.
    pub scale: Cell<f32>,
    /// True until the first render fits the scale/viewport to the area.
    pub needs_fit: Cell<bool>,
    pub selected: Option<TerritoryId>,
    /// Loaded game row's parsed state; None while loading.
    pub detail: Option<RealmDetail>,
    pub load_error: Option<String>,
    load_rx: Option<oneshot::Receiver<Result<Option<RealmGame>, String>>>,
    reload_pending: bool,
    /// An action is in flight; the board shows it and refuses a second one
    /// until the result lands, so a double-tap can't spend two points.
    pub act_in_flight: bool,
    /// Cursor into the target table.
    pub targets_cursor: usize,
    /// Cursor into the overview's holdings list.
    pub holdings_cursor: usize,
    /// The results card is up. Raised by itself when a finished game loads
    /// and lowered by any key, so the news is unmissable but not in the way.
    pub results_open: bool,
    /// This session has already been shown the result, so reloading the
    /// board does not put the card back up over and over.
    results_seen: bool,
    /// A territory to frame on the map, set by "show me this one" from the
    /// lists or the next-holding key. Only the renderer knows the drawn area,
    /// so it does the framing and clears this.
    pub focus_request: Cell<Option<TerritoryId>>,
    /// Archived day logs, newest first (log view); the live day rides
    /// `detail.state.last_day`.
    pub logs: Vec<ArchivedDay>,
    /// Which archived day the history view is showing. `None` follows the
    /// newest one there is, so opening the view lands on the most recent day
    /// rather than on whatever was picked last time.
    pub history_day: Option<i32>,
    pub log_scroll: usize,
    logs_rx: Option<oneshot::Receiver<Result<Vec<ArchivedDay>, String>>>,
    pub logs_exhausted: bool,
    /// Overview scroll.
    pub overview_scroll: usize,
    /// Last drawn map rect, for the mouse hit test. Cleared each draw.
    pub map_geometry: Cell<Option<Rect>>,
    /// Where the left button went down, while a drag-pan is running.
    pub drag_anchor: Cell<Option<(u16, u16)>>,
    /// The pointer moved while held, so the release pans instead of selects.
    pub dragged: Cell<bool>,
}

pub struct RealmDetail {
    pub name: String,
    pub row_status: String,
    /// When the realm was made — the muster counts from here.
    pub created: DateTime<Utc>,
    pub last_resolved_day: i32,
    /// The game's own refill hour (UTC).
    pub reset_hour_utc: i16,
    pub winner_user_id: Option<Uuid>,
    pub state: RealmGameState,
}

impl RealmDetail {
    /// The board this game is played on. An `Arc` rather than a borrow
    /// because a generated world is built on demand and shared between every
    /// session looking at it; callers hold it for the frame and drop it.
    pub fn map(&self) -> Option<Arc<WorldMap>> {
        map_handle(
            &self.state.ruleset.map_id,
            self.state.ruleset.map_spec.as_ref(),
        )
    }

    /// The game's current realm day — the day its action budget counts on.
    pub fn today(&self) -> i32 {
        realm_day(Utc::now(), self.reset_hour_utc)
    }

    /// What is left of the opening freeze, or `None` once the realm is open.
    pub fn muster_left(&self) -> Option<chrono::Duration> {
        muster_left(self.created, Utc::now())
    }
}

/// What the viewer is to the game on screen. A realm can be opened by
/// anyone — to decide whether to join, or to follow one you are not in — so
/// every surface has to know whether it is showing a player their war or a
/// visitor someone else's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewerRole {
    /// In the game and alive: the board is theirs to act on.
    Playing,
    /// In the roster but out of the war. Their history is theirs; the map is
    /// now someone else's business.
    Fallen,
    /// Not in this game at all. Read-only, and told so.
    Watching,
}

impl ViewerRole {
    pub fn acts(self) -> bool {
        self == ViewerRole::Playing
    }

    pub fn label(self) -> &'static str {
        match self {
            ViewerRole::Playing => "playing",
            ViewerRole::Fallen => "conquered · watching",
            ViewerRole::Watching => "watching",
        }
    }
}

/// One row of the target table: a territory you could act on right now.
#[derive(Clone, Debug)]
pub struct TargetRow {
    pub id: TerritoryId,
    pub name: String,
    pub owner: Option<Uuid>,
    pub owner_name: Option<String>,
    pub color: Option<(u8, u8, u8)>,
    pub population: u64,
    pub area_km2: u64,
    pub reach: Reach,
    /// How deep its holder has dug in, 1 for plain ground.
    pub fort_level: u8,
    /// Odds if you acted on it now; None for your own land.
    pub probability: Option<f64>,
    /// Claim (free) or attack (held by someone else).
    pub attack: bool,
    /// Your own territory — listed so the table doubles as the atlas of
    /// what you hold, and so you can jump to it on the map.
    pub mine: bool,
    /// Set when the early-game grace forbids this attack for now.
    pub locked: Option<ActionRejected>,
}

impl TargetRow {
    /// Can this row be attacked or claimed right now? Your own ground never
    /// can — digging in is its own key.
    pub fn actionable(&self) -> bool {
        !self.mine && self.locked.is_none()
    }

    pub fn action(&self) -> RealmAction {
        if self.attack {
            RealmAction::Attack { target: self.id }
        } else {
            RealmAction::Claim { target: self.id }
        }
    }
}

/// The three numbers a generated world is shaped by, in the order the shape
/// step shows them.
pub const SHAPE_FIELDS: [&str; 3] = ["continents", "islands", "countries"];

impl RealmCreateDraft {
    /// Is the map picked the generated one? The shape step and the spec sent
    /// to the service both hang off this, so it is asked rather than stored —
    /// a flag could disagree with the cursor.
    pub fn generated(&self) -> bool {
        super::map::MAPS
            .get(self.map)
            .is_some_and(|m| m.id == super::map::GENERATED_MAP_ID)
    }

    /// h/l on the shape step. Countries move in tens because the range is
    /// 50-400 and nobody wants to press a key 350 times; the two landmass
    /// counts move one at a time because their ranges are small and every
    /// step visibly changes the world.
    pub fn adjust_shape(&mut self, delta: i32) {
        use super::mapgen::{
            GEN_MAX_CONTINENTS, GEN_MAX_ISLANDS, GEN_MAX_TERRITORIES, GEN_MIN_TERRITORIES,
        };
        match self.shape_field {
            0 => {
                self.shape.continents = (self.shape.continents as i32 + delta)
                    .clamp(0, GEN_MAX_CONTINENTS as i32)
                    as u8;
            }
            1 => {
                self.shape.islands =
                    (self.shape.islands as i32 + delta).clamp(0, GEN_MAX_ISLANDS as i32) as u8;
            }
            _ => {
                self.shape.territories = (self.shape.territories as i32 + delta * 10)
                    .clamp(GEN_MIN_TERRITORIES as i32, GEN_MAX_TERRITORIES as i32)
                    as u16;
            }
        }
    }

    /// A world with no land is the one combination the generator cannot
    /// build, so the step says so instead of letting Enter make it.
    pub fn shape_is_buildable(&self) -> bool {
        self.shape.continents > 0 || self.shape.islands > 0
    }
}

/// What one frame of realm work produced. `changed` is the important half:
/// realm state arrives from a service task rather than from the keystroke
/// that asked for it, so nothing else in the frame knows a repaint is due.
#[derive(Default)]
pub struct RealmTick {
    pub banner: Option<Banner>,
    pub changed: bool,
}

impl RealmState {
    pub fn new(svc: RealmService, user_id: Uuid, username: String) -> Self {
        let snapshot_rx = svc.subscribe_snapshot();
        let event_rx = svc.subscribe_events();
        let snapshot = snapshot_rx.borrow().clone();
        Self {
            svc,
            user_id,
            username,
            viewer_tz: None,
            countdown_second: None,
            snapshot,
            snapshot_rx,
            event_rx,
            board: None,
            create_draft: None,
            join_draft: None,
        }
    }

    /// Mirrored from `App` at bootstrap and on profile changes, the way the
    /// chat pane gets it.
    pub fn set_viewer_tz(&mut self, tz: Option<chrono_tz::Tz>) {
        self.viewer_tz = tz;
    }

    pub fn service(&self) -> &RealmService {
        &self.svc
    }

    /// Games I'm in (running, then recently finished), for the modal's realm
    /// section. A realm is live from creation, so there is no waiting-room
    /// list any more — `open_games` only ever holds pre-continuous-join rows.
    pub fn my_games(&self) -> Vec<&super::svc::RealmGameItem> {
        self.snapshot
            .open_games
            .iter()
            .chain(self.snapshot.active_games.iter())
            .chain(self.snapshot.finished_games.iter())
            .filter(|g| g.is_member(self.user_id))
            .collect()
    }

    /// Everyone else's realms: joinable while the map still has room, and
    /// watchable once it does not.
    pub fn other_games(&self) -> Vec<&super::svc::RealmGameItem> {
        self.snapshot
            .open_games
            .iter()
            .chain(self.snapshot.active_games.iter())
            .filter(|g| !g.is_member(self.user_id))
            .collect()
    }

    // ----- join draft (Lobby modal overlay)

    /// Open the colour picker for a realm about to be joined. Lands on the
    /// first colour nobody there has taken.
    pub fn begin_join_draft(&mut self, game: &super::svc::RealmGameItem) {
        let taken: Vec<(usize, String)> = game
            .players
            .iter()
            .map(|p| (usize::from(p.color), p.username.clone()))
            .collect();
        let color = free_palette_indices(&taken).first().copied().unwrap_or(0);
        self.join_draft = Some(RealmJoinDraft {
            game_id: game.id,
            game_name: game.name.clone(),
            color,
            taken,
        });
    }

    /// Move the colour cursor, stepping over the ones already on that map —
    /// a taken colour is never sitting under the cursor to be confirmed.
    pub fn join_move_selection(&mut self, delta: isize) {
        let Some(draft) = self.join_draft.as_mut() else {
            return;
        };
        let free = free_palette_indices(&draft.taken);
        if free.is_empty() {
            return;
        }
        let at = free.iter().position(|c| *c == draft.color).unwrap_or(0) as isize;
        let next = (at + delta).clamp(0, free.len() as isize - 1) as usize;
        draft.color = free[next];
    }

    pub fn join_confirm(&mut self) {
        let Some(draft) = self.join_draft.take() else {
            return;
        };
        self.svc.join_game_task(
            self.user_id,
            self.username.clone(),
            draft.game_id,
            Some(draft.color as u8),
        );
    }

    /// Esc on the picker. Returns true when it was open, so the modal knows
    /// the key was spent here rather than closing the Lobby.
    pub fn join_cancel(&mut self) -> bool {
        self.join_draft.take().is_some()
    }

    // ----- create draft (Lobby modal overlay)

    pub fn begin_create_draft(&mut self) {
        // Default the refill to the viewer's own 18:00, so the common case is
        // one Enter away; everything else is a scroll.
        let hour = self.default_reset_hour();
        self.create_draft = Some(RealmCreateDraft {
            step: CreateStep::Ruleset,
            ruleset: 0,
            map: 0,
            shape: super::mapgen::GeneratedMapSpec::default(),
            shape_field: 0,
            color: 0,
            pace: super::rulesets::PACES
                .iter()
                .position(|p| p.id == super::rulesets::DEFAULT_PACE)
                .unwrap_or(0),
            hour,
            name: String::new(),
            option: 0,
            options: super::rulesets::default_options(),
        });
    }

    /// The UTC hour that reads as 18:00 on the viewer's own clock (plain
    /// 18:00 UTC when they have set no zone).
    fn default_reset_hour(&self) -> i16 {
        const EVENING: i64 = 18;
        match self.viewer_tz {
            Some(tz) => {
                use chrono::{Offset, TimeZone};
                let now = Utc::now();
                let offset_hours = tz
                    .from_utc_datetime(&now.naive_utc())
                    .offset()
                    .fix()
                    .local_minus_utc() as i64
                    / 3_600;
                (EVENING - offset_hours).rem_euclid(24) as i16
            }
            None => EVENING as i16,
        }
    }

    pub fn draft_move_selection(&mut self, delta: isize) {
        let Some(draft) = self.create_draft.as_mut() else {
            return;
        };
        match draft.step {
            CreateStep::Ruleset => {
                let count = super::rulesets::RULESETS.len() as isize;
                draft.ruleset = (draft.ruleset as isize + delta).clamp(0, count - 1) as usize;
            }
            CreateStep::Map => {
                let count = super::map::MAPS.len() as isize;
                draft.map = (draft.map as isize + delta).clamp(0, count - 1) as usize;
            }
            CreateStep::MapShape => {
                draft.shape_field = (draft.shape_field as isize + delta).clamp(0, 2) as usize;
            }
            CreateStep::Color => {
                let count = PLAYER_PALETTES.len() as isize;
                draft.color = (draft.color as isize + delta).clamp(0, count - 1) as usize;
            }
            CreateStep::Pace => {
                let count = super::rulesets::PACES.len() as isize;
                draft.pace = (draft.pace as isize + delta).clamp(0, count - 1) as usize;
            }
            CreateStep::ResetHour => {
                // Whole hours only, wrapping around the clock.
                draft.hour = ((draft.hour as isize + delta).rem_euclid(24)) as i16;
            }
            // The name step is typed, not scrolled.
            CreateStep::Name => {}
            CreateStep::Options => {
                let count = super::rulesets::OPTIONS.len() as isize;
                draft.option = (draft.option as isize + delta).clamp(0, count - 1) as usize;
            }
        }
    }

    /// h/l on the shape step, forwarded to the draft.
    pub fn draft_adjust_shape(&mut self, delta: i32) {
        if let Some(draft) = self.create_draft.as_mut()
            && draft.step == CreateStep::MapShape
        {
            draft.adjust_shape(delta);
        }
    }

    /// Type into the name. Capped at the same length the service enforces, so
    /// the overlay can never show something that would be cut on save.
    pub fn draft_push_name(&mut self, ch: char) {
        if let Some(draft) = self.create_draft.as_mut()
            && draft.step == CreateStep::Name
            && !ch.is_control()
            && draft.name.chars().count() < super::svc::REALM_NAME_MAX
        {
            draft.name.push(ch);
        }
    }

    pub fn draft_pop_name(&mut self) {
        if let Some(draft) = self.create_draft.as_mut()
            && draft.step == CreateStep::Name
        {
            draft.name.pop();
        }
    }

    /// Enter on the draft: first Enter picks the ruleset, the second creates
    /// the game at the chosen hour.
    pub fn draft_confirm(&mut self) {
        let Some(draft) = self.create_draft.as_mut() else {
            return;
        };
        match draft.step {
            CreateStep::Ruleset => draft.step = CreateStep::Map,
            CreateStep::Map => {
                draft.step = if draft.generated() {
                    CreateStep::MapShape
                } else {
                    CreateStep::Pace
                }
            }
            CreateStep::MapShape => {
                if !draft.shape_is_buildable() {
                    return;
                }
                draft.step = CreateStep::Pace;
            }
            CreateStep::Pace => draft.step = CreateStep::ResetHour,
            CreateStep::ResetHour => draft.step = CreateStep::Name,
            CreateStep::Name => draft.step = CreateStep::Color,
            CreateStep::Color => draft.step = CreateStep::Options,
            CreateStep::Options => {
                let draft = self.create_draft.take().expect("checked above");
                let ruleset = super::rulesets::RULESETS
                    [draft.ruleset.min(super::rulesets::RULESETS.len() - 1)]
                .id;
                let pace =
                    super::rulesets::PACES[draft.pace.min(super::rulesets::PACES.len() - 1)].id;
                let map = super::map::MAPS[draft.map.min(super::map::MAPS.len() - 1)].id;
                self.svc.create_game_task(
                    self.user_id,
                    self.username.clone(),
                    super::svc::NewRealm {
                        name: draft.name.clone(),
                        ruleset_id: ruleset.to_string(),
                        pace_id: pace.to_string(),
                        reset_hour_utc: draft.hour,
                        options: draft.options.clone(),
                        map_id: map.to_string(),
                        map_spec: draft.generated().then(|| draft.shape.normalized()),
                        color: Some(draft.color as u8),
                    },
                );
            }
        }
    }

    /// Esc on the draft: step back to the ruleset, or close the overlay.
    /// Returns true when the overlay is still open.
    /// Space on the options step: flip the highlighted rule.
    pub fn draft_toggle_option(&mut self) {
        let Some(draft) = self.create_draft.as_mut() else {
            return;
        };
        if draft.step != CreateStep::Options {
            return;
        }
        let Some(option) = super::rulesets::OPTIONS.get(draft.option) else {
            return;
        };
        let entry = draft
            .options
            .entry(option.id.to_string())
            .or_insert(option.default_on);
        *entry = !*entry;
    }

    pub fn draft_back(&mut self) -> bool {
        match self.create_draft.as_mut() {
            Some(draft) if draft.step == CreateStep::Name => {
                draft.step = CreateStep::ResetHour;
                true
            }
            Some(draft) if draft.step == CreateStep::Options => {
                draft.step = CreateStep::Color;
                true
            }
            Some(draft) if draft.step == CreateStep::Color => {
                draft.step = CreateStep::Name;
                true
            }
            Some(draft) if draft.step == CreateStep::ResetHour => {
                draft.step = CreateStep::Pace;
                true
            }
            Some(draft) if draft.step == CreateStep::Pace => {
                draft.step = if draft.generated() {
                    CreateStep::MapShape
                } else {
                    CreateStep::Map
                };
                true
            }
            Some(draft) if draft.step == CreateStep::MapShape => {
                draft.step = CreateStep::Map;
                true
            }
            Some(draft) if draft.step == CreateStep::Map => {
                draft.step = CreateStep::Ruleset;
                true
            }
            _ => {
                self.create_draft = None;
                false
            }
        }
    }

    /// Drain snapshot + events; returns at most one banner for this tick.
    /// Drain everything that arrived since the last frame, and say whether
    /// any of it is worth a repaint.
    ///
    /// The `changed` half is not decoration. A realm action resolves in the
    /// service and comes back as an event, and the board it changed is
    /// reloaded from the database a moment later — so the banner ("you took
    /// Peru") and the new map are two different frames. Reporting only the
    /// banner left the second one waiting for the next keypress: you claimed
    /// a territory, were told you had it, and the map went on showing it as
    /// somebody else's until you moved the cursor.
    pub fn tick(&mut self) -> RealmTick {
        let mut changed = false;
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            changed = true;
        }
        let mut banner = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => {
                    changed = true;
                    if let Some(b) = self.apply_event(event) {
                        banner = Some(b);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        changed |= self.poll_board_load();
        changed |= self.poll_logs_load();
        self.pull_history_archive();
        self.start_pending_reload();
        changed |= self.countdown_ticked();
        RealmTick { banner, changed }
    }

    /// Has a countdown on screen moved to another second? The muster clock
    /// counts down in the board header and in the Lobby row, and nothing
    /// else would ask for the frame that shows 1:22 after 1:23.
    fn countdown_ticked(&mut self) -> bool {
        let showing = self
            .board
            .as_ref()
            .and_then(|b| b.detail.as_ref())
            .and_then(|d| d.muster_left())
            .map(|left| left.num_seconds())
            .or_else(|| {
                // The Lobby list counts down too, for every realm on it.
                self.snapshot
                    .active_games
                    .iter()
                    .filter_map(|g| muster_left(g.created, Utc::now()))
                    .map(|left| left.num_seconds())
                    .max()
            });
        if showing == self.countdown_second {
            return false;
        }
        self.countdown_second = showing;
        true
    }

    fn apply_event(&mut self, event: RealmEvent) -> Option<Banner> {
        match event {
            RealmEvent::Error { user_id, message } if user_id == self.user_id => {
                if let Some(board) = self.board.as_mut() {
                    board.act_in_flight = false;
                    board.reload_pending = true;
                }
                Some(Banner::error(&format!("Realm: {message}")))
            }
            RealmEvent::Error { .. } => None,
            // Your own action came back: the result is the banner.
            RealmEvent::ActionResolved {
                game_id,
                user_id,
                target,
                probability,
                success,
                attack,
                points_left,
            } if user_id == self.user_id => {
                let name = self
                    .board
                    .as_ref()
                    .filter(|b| b.game_id == game_id)
                    .and_then(|b| b.detail.as_ref())
                    .and_then(|d| d.map())
                    .and_then(|m| m.territory(target).map(|t| t.name.clone()))
                    .unwrap_or_else(|| "the territory".to_string());
                if let Some(board) = self.board.as_mut()
                    && board.game_id == game_id
                {
                    board.act_in_flight = false;
                    board.reload_pending = true;
                }
                let odds = (probability * 100.0).round() as i64;
                let verb = if attack { "attack on" } else { "claim on" };
                Some(if success {
                    Banner::success(&format!(
                        "Realm: your {verb} {name} succeeded ({odds}%) · {points_left} left today"
                    ))
                } else {
                    Banner::info(&format!(
                        "Realm: your {verb} {name} failed ({odds}%) · {points_left} left today"
                    ))
                })
            }
            RealmEvent::ActionResolved { .. } => None,
            // Someone else moved: reload quietly, no banner storm.
            RealmEvent::GameChanged { game_id } => {
                if let Some(board) = self.board.as_mut()
                    && board.game_id == game_id
                {
                    board.reload_pending = true;
                }
                None
            }
            RealmEvent::DayRolled { game_id, name, .. } => {
                // The summons reaches you wherever you are in the app, not
                // only with the board open: a day's points that nobody is
                // told about is a day's points nobody spends.
                let mine = self.snapshot.active_games.iter().any(|g| {
                    g.id == game_id
                        && g.players.iter().any(|p| {
                            p.user_id == self.user_id && p.status == RealmPlayerStatus::Alive
                        })
                });
                if let Some(board) = self.board.as_mut()
                    && board.game_id == game_id
                {
                    board.reload_pending = true;
                    board.logs.clear();
                    board.logs_exhausted = false;
                }
                mine.then(|| Banner::info(&format!("The Realm calls you! ({name})")))
            }
            // The end of a realm is the one event worth interrupting for.
            RealmEvent::GameFinished {
                game_id,
                winner_user_id,
            } => {
                let mine = self.board.as_ref().is_some_and(|b| b.game_id == game_id);
                if let Some(board) = self.board.as_mut()
                    && board.game_id == game_id
                {
                    board.reload_pending = true;
                    // The card goes up as soon as the reloaded row says
                    // finished; this just makes sure it is allowed to.
                    board.results_seen = false;
                }
                mine.then(|| match winner_user_id {
                    Some(winner) if winner == self.user_id => {
                        Banner::success("Realm: the realm is yours — see the final standings")
                    }
                    Some(_) => Banner::info("Realm: the realm has been taken — see how it ended"),
                    None => Banner::info("Realm: the realm dissolved — nobody held it"),
                })
            }
            RealmEvent::GameStarted { game_id }
            | RealmEvent::PlayerJoined { game_id, .. }
            | RealmEvent::PlayerLeft { game_id, .. } => {
                if let Some(board) = self.board.as_mut()
                    && board.game_id == game_id
                {
                    board.reload_pending = true;
                }
                None
            }
            // A realm stands frozen for its first couple of minutes, and
            // this is the only window in which everyone can start level — so
            // it interrupts whatever anyone is doing, wherever they are.
            RealmEvent::GameCreated { name, opens_at, .. } => {
                let left = (opens_at - Utc::now()).max(chrono::Duration::zero());
                Some(Banner::info(&format!(
                    "A realm is forming: {name} — it opens in {} · join it from the lobby",
                    countdown_label(left)
                )))
            }
        }
    }

    /// Open the full-screen board for a game.
    pub fn open_board(&mut self, game_id: Uuid, return_screen: Screen) {
        self.board = Some(RealmBoardState {
            game_id,
            return_screen,
            view: RealmView::Map,
            view_x: Cell::new(0.0),
            view_y: Cell::new(0.0),
            scale: Cell::new(1.0),
            needs_fit: Cell::new(true),
            selected: None,
            detail: None,
            load_error: None,
            load_rx: None,
            reload_pending: false,
            act_in_flight: false,
            targets_cursor: 0,
            holdings_cursor: 0,
            results_open: false,
            results_seen: false,
            focus_request: Cell::new(None),
            logs: Vec::new(),
            history_day: None,
            log_scroll: 0,
            logs_rx: None,
            logs_exhausted: false,
            overview_scroll: 0,
            map_geometry: Cell::new(None),
            drag_anchor: Cell::new(None),
            dragged: Cell::new(false),
        });
        self.start_board_load();
    }

    pub fn close_board(&mut self) -> Option<Screen> {
        self.board.take().map(|b| b.return_screen)
    }

    fn start_board_load(&mut self) {
        let Some(board) = self.board.as_mut() else {
            return;
        };
        if board.load_rx.is_some() {
            board.reload_pending = true;
            return;
        }
        board.reload_pending = false;
        let (tx, rx) = oneshot::channel();
        board.load_rx = Some(rx);
        let svc = self.svc.clone();
        let game_id = board.game_id;
        tokio::spawn(async move {
            let result = svc.load_game(game_id).await.map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    /// True when the board changed under us — a reload landed, or failed.
    fn poll_board_load(&mut self) -> bool {
        let Some(board) = self.board.as_mut() else {
            return false;
        };
        let Some(rx) = board.load_rx.as_mut() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(Some(row))) => {
                board.load_rx = None;
                match parse_state(&row.state) {
                    Ok(state) => {
                        board.detail = Some(RealmDetail {
                            name: super::svc::game_name(
                                &row.name,
                                state
                                    .players
                                    .iter()
                                    .find(|p| p.user_id == row.creator_id)
                                    .map(|p| p.username.as_str())
                                    .unwrap_or("someone"),
                            ),
                            row_status: row.status,
                            created: row.created,
                            last_resolved_day: row.last_resolved_day,
                            reset_hour_utc: row.reset_hour_utc,
                            winner_user_id: row.winner_user_id,
                            state,
                        });
                        board.load_error = None;
                        // A finished game announces itself: once per session,
                        // whether you won it, lost it or only watched.
                        if board
                            .detail
                            .as_ref()
                            .is_some_and(|d| d.row_status == "finished")
                            && !board.results_seen
                        {
                            board.results_open = true;
                            board.results_seen = true;
                        }
                    }
                    Err(e) => board.load_error = Some(e.to_string()),
                }
                true
            }
            Ok(Ok(None)) => {
                board.load_rx = None;
                board.load_error = Some("game not found".to_string());
                true
            }
            Ok(Err(e)) => {
                board.load_rx = None;
                board.load_error = Some(e);
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => false,
            Err(oneshot::error::TryRecvError::Closed) => {
                board.load_rx = None;
                true
            }
        }
    }

    fn start_pending_reload(&mut self) {
        let wants = self
            .board
            .as_ref()
            .is_some_and(|b| b.load_rx.is_none() && b.reload_pending);
        if wants {
            self.start_board_load();
        }
    }

    // ----- history (the finished realm, walked day by day)

    /// Keep pulling pages while the history is the view on screen.
    ///
    /// The archive pages fourteen days at a time and the history counts "day
    /// 1 of N", so N has to be the whole run or the first day it lands on is
    /// the oldest day of the newest page — day 32 of 45, labelled day 1.
    /// Driven from the frame loop rather than from a page arriving, because
    /// the first page usually lands while some other view is on screen and
    /// nothing would then chase the rest.
    fn pull_history_archive(&mut self) {
        let wants = self.board.as_ref().is_some_and(|board| {
            board.view == RealmView::History && !board.logs_exhausted && board.logs_rx.is_none()
        });
        if wants {
            self.load_more_logs();
        }
    }

    /// Is the run still being read in? The strip says so, because the map
    /// changes under you when the rest of it arrives.
    pub fn history_loading(&self) -> bool {
        self.board
            .as_ref()
            .is_some_and(|board| !board.logs_exhausted)
    }

    /// The archived days that can actually be *shown* — oldest first. A day
    /// archived before boards were kept has a log but no positions, and the
    /// slider is about positions.
    pub fn history_days(&self) -> Vec<&ArchivedDay> {
        let Some(board) = self.board.as_ref() else {
            return Vec::new();
        };
        let mut days: Vec<&ArchivedDay> = board
            .logs
            .iter()
            .filter(|day| day.board.is_some())
            .collect();
        days.sort_by_key(|day| day.day());
        days
    }

    /// The day the history view is on, and where it sits in the run. `None`
    /// when there is nothing archived with a board yet.
    pub fn history_position(&self) -> Option<(usize, usize)> {
        let days = self.history_days();
        if days.is_empty() {
            return None;
        }
        let selected = self
            .board
            .as_ref()
            .and_then(|b| b.history_day)
            .and_then(|day| days.iter().position(|d| d.day() == day))
            // No pick yet: the first day of the run. Opening on the last one
            // showed a board all but identical to the live map, which is the
            // one thing this view must not be mistaken for — and day one,
            // with five spawns on an empty world, could not be mistaken for
            // anything else.
            .unwrap_or(0);
        Some((selected.min(days.len() - 1), days.len()))
    }

    pub fn history_selected(&self) -> Option<&ArchivedDay> {
        let (index, _) = self.history_position()?;
        self.history_days().into_iter().nth(index)
    }

    /// Walk the slider. Clamped rather than wrapping: the ends of a game are
    /// meaningful places and skipping past them loses your bearings.
    pub fn history_step(&mut self, delta: isize) {
        let Some((index, len)) = self.history_position() else {
            return;
        };
        let next = history_step_index(index, len, delta);
        let day = self.history_days().into_iter().nth(next).map(|d| d.day());
        if let Some(board) = self.board.as_mut() {
            board.history_day = day;
        }
        // Older pages load as the slider reaches for them. The view also
        // chases the rest of the archive on its own; this is what covers a
        // page that failed and left the run short.
        if next == 0 {
            self.load_more_logs();
        }
    }

    /// Jump to one end of the run.
    pub fn history_jump(&mut self, to_end: bool) {
        let days = self.history_days();
        let day = if to_end {
            days.last().map(|d| d.day())
        } else {
            days.first().map(|d| d.day())
        };
        if let Some(board) = self.board.as_mut() {
            board.history_day = day;
        }
        if !to_end {
            self.load_more_logs();
        }
    }

    /// Kick an archived-log page load (log view). `before_day` pages older.
    pub fn load_more_logs(&mut self) {
        let Some(board) = self.board.as_mut() else {
            return;
        };
        if board.logs_rx.is_some() || board.logs_exhausted {
            return;
        }
        let before = board.logs.last().map(|d| d.day());
        let (tx, rx) = oneshot::channel();
        board.logs_rx = Some(rx);
        let svc = self.svc.clone();
        let game_id = board.game_id;
        tokio::spawn(async move {
            let result = svc
                .load_day_logs(game_id, before, 14)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
    }

    /// True when a page of archived days landed.
    fn poll_logs_load(&mut self) -> bool {
        let Some(board) = self.board.as_mut() else {
            return false;
        };
        let Some(rx) = board.logs_rx.as_mut() else {
            return false;
        };
        match rx.try_recv() {
            Ok(Ok(days)) => {
                board.logs_rx = None;
                if days.is_empty() {
                    board.logs_exhausted = true;
                } else {
                    board.logs.extend(days);
                }
                true
            }
            Ok(Err(_)) | Err(oneshot::error::TryRecvError::Closed) => {
                board.logs_rx = None;
                true
            }
            Err(oneshot::error::TryRecvError::Empty) => false,
        }
    }

    // ----- acting (resolved on the spot)

    /// Put the results card away.
    pub fn close_results(&mut self) -> bool {
        match self.board.as_mut() {
            Some(board) if board.results_open => {
                board.results_open = false;
                true
            }
            _ => false,
        }
    }

    /// Call the result back up — `r` on a finished board.
    pub fn show_results(&mut self) {
        if let Some(board) = self.board.as_mut()
            && board
                .detail
                .as_ref()
                .is_some_and(|d| d.row_status == "finished")
        {
            board.results_open = true;
        }
    }

    /// What the viewer is to the board currently open.
    pub fn viewer_role(&self) -> ViewerRole {
        let Some(detail) = self.board.as_ref().and_then(|b| b.detail.as_ref()) else {
            return ViewerRole::Watching;
        };
        match detail.state.player(self.user_id).map(|p| p.status) {
            Some(RealmPlayerStatus::Alive) => ViewerRole::Playing,
            Some(_) => ViewerRole::Fallen,
            None => ViewerRole::Watching,
        }
    }

    /// Act on the selected territory: claim it when free, attack it when
    /// someone else holds it. The action is sent immediately; the result
    /// arrives as a banner. Returns a banner when the board itself can say
    /// no without bothering the server.
    pub fn act_selected(&mut self) -> Option<Banner> {
        let target = self.board.as_ref()?.selected?;
        self.act_on(target)
    }

    /// Act on `target`, whatever view picked it.
    pub fn act_on(&mut self, target: TerritoryId) -> Option<Banner> {
        let user_id = self.user_id;
        let board = self.board.as_ref()?;
        let game_id = board.game_id;
        let detail = board.detail.as_ref()?;
        if detail.row_status != "active" {
            return Some(Banner::info("Realm: the game is not running"));
        }
        if let Some(left) = detail.muster_left() {
            return Some(Banner::info(&format!(
                "Realm: this one opens in {} — everyone starts together",
                countdown_label(left)
            )));
        }
        match detail.state.player(user_id).map(|p| p.status) {
            Some(RealmPlayerStatus::Alive) => {}
            Some(_) => {
                return Some(Banner::info(
                    "Realm: you are out of this war — watching only",
                ));
            }
            None => {
                return Some(Banner::info(
                    "Realm: you are watching this realm, not playing it",
                ));
            }
        }
        if board.act_in_flight {
            return None;
        }
        if detail.state.points_left(user_id, detail.today()) == 0 {
            return Some(Banner::info(
                "Realm: no action points left until the next refill",
            ));
        }
        let action = match detail.state.ownership.get(&target) {
            None => RealmAction::Claim { target },
            // Never the spade by accident: a point spent digging is a point
            // not spent taking ground, so it takes its own key.
            Some(owner) if *owner == user_id => {
                return Some(Banner::info("Realm: yours already — press f to dig in"));
            }
            Some(owner) => {
                // A phase rule is worth answering on the spot: it is a rule,
                // not a race lost.
                let map = detail.map()?;
                let reach = super::resolver::reach_for(&detail.state, &map, user_id, target);
                let defender = *owner;
                if let Err(locked) =
                    detail
                        .state
                        .attack_allowed(user_id, defender, reach, detail.today())
                {
                    return Some(Banner::info(&format!("Realm: {}", locked.message())));
                }
                RealmAction::Attack { target }
            }
        };
        if let Some(board) = self.board.as_mut() {
            board.act_in_flight = true;
        }
        self.svc.act_task(user_id, game_id, action);
        None
    }

    /// Dig in on the selected territory — the only way a point is ever
    /// spent on walls.
    pub fn fortify_selected(&mut self) -> Option<Banner> {
        let target = self.board.as_ref()?.selected?;
        self.fortify_on(target)
    }

    pub fn fortify_on(&mut self, target: TerritoryId) -> Option<Banner> {
        let user_id = self.user_id;
        let board = self.board.as_ref()?;
        let game_id = board.game_id;
        let detail = board.detail.as_ref()?;
        if detail.row_status != "active" {
            return Some(Banner::info("Realm: the game is not running"));
        }
        if let Some(left) = detail.muster_left() {
            return Some(Banner::info(&format!(
                "Realm: this one opens in {} — everyone starts together",
                countdown_label(left)
            )));
        }
        if detail.state.player(user_id).map(|p| p.status) != Some(RealmPlayerStatus::Alive) {
            return Some(Banner::info(
                "Realm: you are watching this realm, not playing it",
            ));
        }
        if board.act_in_flight {
            return None;
        }
        if !detail.state.option(super::rulesets::OPTION_FORTIFY) {
            return Some(Banner::info(
                "Realm: this realm has no fortifications — take ground instead",
            ));
        }
        if detail.state.ownership.get(&target) != Some(&user_id) {
            return Some(Banner::info("Realm: you can only dig in on your own land"));
        }
        let level = detail.state.fort_level(target);
        if level >= detail.state.ruleset.fort_max_level {
            return Some(Banner::info(&format!(
                "Realm: already dug in as deep as it goes (level {level})"
            )));
        }
        if detail.state.points_left(user_id, detail.today()) == 0 {
            return Some(Banner::info(
                "Realm: no action points left until the next refill",
            ));
        }
        if let Some(board) = self.board.as_mut() {
            board.act_in_flight = true;
        }
        self.svc
            .act_task(user_id, game_id, RealmAction::Fortify { target });
        None
    }

    /// Odds for a territory from the viewer's seat, exactly as the engine
    /// would roll them right now.
    pub fn projected_probability(&self, action: &RealmAction) -> Option<f64> {
        let detail = self.board.as_ref()?.detail.as_ref()?;
        let map = detail.map()?;
        let target = action.target();
        let reach = super::resolver::reach_for(&detail.state, &map, self.user_id, target);
        projected_probability(
            &detail.state,
            &map,
            self.user_id,
            target,
            reach,
            detail.today(),
        )
    }

    /// How far the selected territory is from your land, for the map panel.
    pub fn selected_reach(&self) -> Option<Reach> {
        let board = self.board.as_ref()?;
        let detail = board.detail.as_ref()?;
        let map = detail.map()?;
        let target = board.selected?;
        Some(super::resolver::reach_for(
            &detail.state,
            &map,
            self.user_id,
            target,
        ))
    }

    /// Every territory worth a row in the target table: your own holdings
    /// first (so the table is also the atlas of what you hold), then
    /// everything you could act on, nearest first and best odds within a
    /// distance.
    pub fn target_rows(&self) -> Vec<TargetRow> {
        let Some(detail) = self.board.as_ref().and_then(|b| b.detail.as_ref()) else {
            return Vec::new();
        };
        let Some(map) = detail.map() else {
            return Vec::new();
        };
        let state = &detail.state;
        let today = detail.today();
        let reach_index = ReachIndex::build(state, &map, self.user_id);
        // Whether the short-reach cap is biting at all: it lifts entirely
        // for a player with nothing in range, so the table must not mark
        // rows unreachable when they are the only thing left.
        let out_of_range_applies = state.reach_cap().is_some_and(|cap| {
            super::resolver::anything_within(state, &map, &reach_index, self.user_id, cap)
        });
        let mut rows: Vec<TargetRow> = map
            .territories
            .iter()
            .map(|t| {
                let owner = state.ownership.get(&t.id).copied();
                let mine = owner == Some(self.user_id);
                // Your own land has no route to price.
                let reach = reach_index.reach(&map, &state.ruleset, t.id);
                let probability = if mine {
                    None
                } else {
                    projected_probability(state, &map, self.user_id, t.id, reach, today)
                };
                let locked = if !mine && reach.is_unreachable() {
                    // Nothing to sail from, or nothing to land on. Said
                    // plainly, because the row used to read like any other.
                    Some(ActionRejected::NoRoute)
                } else if !mine
                    && state.reach_cap().is_some_and(|cap| reach.cost > cap)
                    && out_of_range_applies
                {
                    // This game keeps war close to home, and something
                    // nearer is still on the table.
                    Some(ActionRejected::OutOfRange)
                } else {
                    owner.filter(|_| !mine).and_then(|defender| {
                        state
                            .attack_allowed(self.user_id, defender, reach, today)
                            .err()
                    })
                };
                TargetRow {
                    id: t.id,
                    name: t.name.clone(),
                    owner,
                    owner_name: owner.and_then(|o| state.player(o).map(|p| p.username.clone())),
                    color: owner.map(|o| player_color(state, o)),
                    population: t.population,
                    area_km2: t.area_km2,
                    reach,
                    fort_level: state.fort_level(t.id),
                    probability,
                    attack: owner.is_some(),
                    mine,
                    locked,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            // Yours first, then by what the route costs, then by odds.
            b.mine
                .cmp(&a.mine)
                .then_with(|| {
                    a.reach
                        .cost
                        .partial_cmp(&b.reach.cost)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    b.probability
                        .unwrap_or(0.0)
                        .partial_cmp(&a.probability.unwrap_or(0.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.name.cmp(&b.name))
        });
        rows
    }

    /// Move the target-table cursor and mirror the pick into the map
    /// selection, so Tab between the two views keeps its place.
    pub fn move_targets_cursor(&mut self, delta: isize) {
        let rows = self.target_rows();
        let Some(board) = self.board.as_mut() else {
            return;
        };
        if rows.is_empty() {
            board.targets_cursor = 0;
            return;
        }
        let next = (board.targets_cursor as isize + delta).clamp(0, rows.len() as isize - 1);
        board.targets_cursor = next as usize;
        board.selected = Some(rows[next as usize].id);
    }

    /// Your territories, in map order — what the overview lists and the
    /// holdings cursor walks.
    pub fn my_holdings(&self) -> Vec<TerritoryId> {
        self.board
            .as_ref()
            .and_then(|b| b.detail.as_ref())
            .map(|d| d.state.holdings(self.user_id))
            .unwrap_or_default()
    }

    /// Move the overview's holdings cursor, mirroring the pick into the map
    /// selection so jumping to the map lands on it.
    pub fn move_holdings_cursor(&mut self, delta: isize) {
        let holdings = self.my_holdings();
        let Some(board) = self.board.as_mut() else {
            return;
        };
        if holdings.is_empty() {
            board.holdings_cursor = 0;
            return;
        }
        let next = (board.holdings_cursor as isize + delta).rem_euclid(holdings.len() as isize);
        board.holdings_cursor = next as usize;
        board.selected = Some(holdings[next as usize]);
    }

    /// The territory the holdings cursor is on.
    pub fn selected_holding(&self) -> Option<TerritoryId> {
        let board = self.board.as_ref()?;
        self.my_holdings().get(board.holdings_cursor).copied()
    }

    /// The row the target table's cursor is on.
    pub fn selected_target(&self) -> Option<TargetRow> {
        let board = self.board.as_ref()?;
        let rows = self.target_rows();
        rows.get(board.targets_cursor).cloned()
    }
}

/// Action points left today, and the daily cap, for the viewer.
pub fn points_left(board: &RealmBoardState, user_id: Uuid) -> Option<(usize, usize)> {
    let detail = board.detail.as_ref()?;
    let cap = detail
        .state
        .ruleset
        .actions_per_day(detail.state.start_player_count) as usize;
    Some((
        detail.state.points_left(user_id, detail.today()) as usize,
        cap,
    ))
}

/// One player's colour: a name to pick it by and a ladder of five shades,
/// palest first.
pub struct PlayerPalette {
    pub name: &'static str,
    pub shades: [(u8, u8, u8); FORT_SHADES],
}

/// The ten colours a realm can be played in.
///
/// Both of the map's fills live in here — whose a territory is (the hue) and
/// how dug in they are (where along the ladder it sits, dark meaning walled
/// up). Ten hues times five steps is fifty fills that have to stay apart by
/// eye, which hand-picking does badly: the old table had a silver and a
/// brown whose shades were closer to each other than either was to its own
/// neighbouring step. So the ladders are laid out in CIELAB — even hue
/// spacing, a constant lightness and chroma span — and the distances are
/// held to by a test rather than by eye.
pub const PLAYER_PALETTES: [PlayerPalette; 10] = [
    PlayerPalette {
        name: "rose",
        shades: [
            (255, 200, 221),
            (255, 161, 197),
            (255, 116, 174),
            (238, 76, 150),
            (220, 4, 127),
        ],
    },
    PlayerPalette {
        name: "orange",
        shades: [
            (255, 204, 183),
            (255, 166, 130),
            (246, 132, 87),
            (226, 100, 52),
            (204, 67, 10),
        ],
    },
    PlayerPalette {
        name: "gold",
        shades: [
            (249, 211, 139),
            (226, 184, 96),
            (201, 157, 51),
            (175, 132, 0),
            (145, 108, 0),
        ],
    },
    PlayerPalette {
        name: "olive",
        shades: [
            (206, 224, 146),
            (175, 199, 105),
            (145, 175, 62),
            (113, 151, 0),
            (93, 124, 0),
        ],
    },
    PlayerPalette {
        name: "green",
        shades: [
            (156, 233, 176),
            (113, 209, 141),
            (63, 185, 108),
            (0, 160, 78),
            (0, 132, 63),
        ],
    },
    PlayerPalette {
        name: "teal",
        shades: [
            (109, 237, 214),
            (0, 213, 187),
            (0, 184, 162),
            (0, 156, 137),
            (0, 129, 113),
        ],
    },
    PlayerPalette {
        name: "cyan",
        shades: [
            (102, 233, 255),
            (0, 208, 237),
            (0, 180, 206),
            (0, 153, 175),
            (0, 126, 145),
        ],
    },
    PlayerPalette {
        name: "blue",
        shades: [
            (171, 222, 255),
            (101, 199, 255),
            (0, 175, 240),
            (0, 148, 205),
            (0, 122, 170),
        ],
    },
    PlayerPalette {
        name: "violet",
        shades: [
            (220, 210, 255),
            (193, 179, 255),
            (164, 150, 255),
            (134, 123, 242),
            (100, 96, 228),
        ],
    },
    PlayerPalette {
        name: "magenta",
        shades: [
            (255, 196, 255),
            (239, 163, 243),
            (219, 131, 226),
            (199, 99, 208),
            (178, 63, 190),
        ],
    },
];

/// Steps in a player's ladder.
pub const FORT_SHADES: usize = 5;

/// Which step stands for a player away from the map — in the standings, the
/// results card, the rail. The middle one: bright enough to read as text on
/// the dark background, and the centre of the range their territories span.
const IDENTITY_SHADE: usize = 2;

/// The colour a player picked, or the one their seat implies on a game made
/// before picking existed.
pub fn palette_index(state: &RealmGameState, user_id: Uuid) -> usize {
    let seat = state
        .players
        .iter()
        .position(|p| p.user_id == user_id)
        .unwrap_or(0);
    state
        .players
        .get(seat)
        .and_then(|p| p.color)
        .map(usize::from)
        .unwrap_or(seat)
        % PLAYER_PALETTES.len()
}

/// Colours nobody in this game is wearing, as palette indices.
pub fn free_palette_indices(taken: &[(usize, String)]) -> Vec<usize> {
    (0..PLAYER_PALETTES.len())
        .filter(|i| !taken.iter().any(|(slot, _)| slot == i))
        .collect()
}

pub fn player_color(state: &RealmGameState, user_id: Uuid) -> (u8, u8, u8) {
    player_shades(state, user_id)[IDENTITY_SHADE]
}

fn player_shades(state: &RealmGameState, user_id: Uuid) -> [(u8, u8, u8); FORT_SHADES] {
    PLAYER_PALETTES[palette_index(state, user_id)].shades
}

/// Where a step along the history slider lands. Clamped rather than wrapped:
/// the first and last days of a game are meaningful places, and walking off
/// one end into the other loses your bearings.
pub(super) fn history_step_index(index: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).clamp(0, len as isize - 1) as usize
}

/// The ladder for a palette slot, for callers that already know the slot —
/// a replayed day carries its own colours rather than asking the live roster,
/// which may no longer remember the player.
pub(super) fn palette_shades(color: u8) -> [(u8, u8, u8); FORT_SHADES] {
    PLAYER_PALETTES[(color as usize).min(PLAYER_PALETTES.len() - 1)].shades
}

/// Where a fortification level sits on the ladder, for whatever ceiling this
/// game's rules put on digging in.
pub(super) fn shade_index(level: u8, max: u8) -> usize {
    if max <= 1 {
        return FORT_SHADES - 1;
    }
    let level = level.clamp(1, max);
    let span = (FORT_SHADES - 1) as u32;
    let steps = u32::from(max - 1);
    ((u32::from(level - 1) * span + steps / 2) / steps) as usize
}

/// Owner lookup usable per-pixel without hashing each time.
pub fn ownership_colors(state: &RealmGameState) -> BTreeMap<TerritoryId, (u8, u8, u8)> {
    state
        .ownership
        .iter()
        .map(|(t, owner)| {
            // Dark means dug in; pale means soft ground. Same hue either
            // way, so the shade never has to be read to tell whose it is.
            let shades = player_shades(state, *owner);
            let idx = shade_index(state.fort_level(*t), state.ruleset.fort_max_level);
            (*t, shades[idx])
        })
        .collect()
}

#[cfg(test)]
mod draft_test {
    use super::*;
    use crate::app::lobby::realm::mapgen::{GEN_MAX_CONTINENTS, GEN_MAX_TERRITORIES};

    fn draft(map: usize) -> RealmCreateDraft {
        RealmCreateDraft {
            step: CreateStep::Map,
            option: 0,
            options: BTreeMap::new(),
            ruleset: 0,
            map,
            shape: super::super::mapgen::GeneratedMapSpec::default(),
            shape_field: 0,
            color: 0,
            pace: 0,
            hour: 18,
            name: String::new(),
        }
    }

    fn map_index(id: &str) -> usize {
        super::super::map::MAPS
            .iter()
            .position(|m| m.id == id)
            .expect("map in the roster")
    }

    /// The shape step exists for one map. Picking Earth has to walk the same
    /// path it always did, or every creator pays for a feature they did not
    /// ask for.
    #[test]
    fn only_the_generated_world_asks_for_a_shape() {
        assert!(!draft(map_index("earth")).generated());
        assert!(draft(map_index("generated")).generated());
    }

    #[test]
    fn the_shape_fields_move_in_their_own_ranges() {
        let mut d = draft(map_index("generated"));
        d.shape.continents = 0;
        d.shape_field = 0;
        d.adjust_shape(-1);
        assert_eq!(d.shape.continents, 0, "nothing goes below zero");
        for _ in 0..40 {
            d.adjust_shape(1);
        }
        assert_eq!(d.shape.continents, GEN_MAX_CONTINENTS);

        // Countries move in tens: the range is 50-400 and a key press has to
        // be worth pressing.
        d.shape_field = 2;
        d.shape.territories = 160;
        d.adjust_shape(1);
        assert_eq!(d.shape.territories, 170);
        for _ in 0..100 {
            d.adjust_shape(1);
        }
        assert_eq!(d.shape.territories, GEN_MAX_TERRITORIES);
        for _ in 0..100 {
            d.adjust_shape(-1);
        }
        assert_eq!(d.shape.territories, 50);
    }

    /// A sea with nothing in it is the one shape the generator cannot build,
    /// so the step refuses rather than quietly inventing a continent.
    #[test]
    fn a_world_of_pure_water_cannot_be_confirmed() {
        let mut d = draft(map_index("generated"));
        d.shape.continents = 0;
        d.shape.islands = 0;
        assert!(!d.shape_is_buildable());
        d.shape.islands = 1;
        assert!(d.shape_is_buildable());
    }
}

#[cfg(test)]
mod history_keys_test {
    use super::history_step_index;

    /// `[` and `]` walk one day at a time and stop at the ends; `g`/`G` jump
    /// to them. The stepping is what the brackets do now — they used to jump,
    /// which made the slider a thing you could only throw to one end or the
    /// other.
    #[test]
    fn a_step_moves_one_day_and_stops_at_the_ends() {
        assert_eq!(history_step_index(3, 45, 1), 4);
        assert_eq!(history_step_index(3, 45, -1), 2);
        assert_eq!(history_step_index(44, 45, 1), 44, "the last day is the end");
        assert_eq!(history_step_index(0, 45, -1), 0, "and so is the first");
    }
}
