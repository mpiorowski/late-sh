//! Per-session realm state: the snapshot/event drains, and the full-screen
//! board state (`Screen::Realm`) — map viewport, selection, the target table,
//! and async row/log loads. Mirrors `daily::state` shapes.
//!
//! There is no local action draft: an action or an order is sent the moment
//! it is confirmed and comes back as a result, so the client-side state is
//! what you are looking at, plus the one order being written in the prompt.
//!
//! This is the board's one place that asks which engine runs the game
//! (`Resolution`): what `a`/`f` do, how a target is priced, and whether the
//! history offers the night's replay. Every view draws what it is handed.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::num::NonZeroU8;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use late_core::models::realm_game::RealmGame;
use ratatui::layout::Rect;
use tokio::sync::{broadcast, oneshot, watch};
use uuid::Uuid;

use crate::app::common::primitives::{Banner, Screen};

use super::map::{TerritoryId, WorldMap, map_handle};
use super::night::{self, Projection};
use super::resolver::{
    ActionRejected, DayResult, LogEntry, Order, OrderRejected, ReachIndex, RealmAction,
    RealmGameState, RealmPlayerStatus, projected_probability,
};
use super::rulesets::{OPTION_FORTIFY, Reach, Resolution};
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
    /// Tonight's orders and what the last night did: the list `x` withdraws
    /// from, and the dispatch beneath it.
    Orders,
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
            RealmView::Targets => RealmView::Orders,
            RealmView::Orders => RealmView::Overview,
            RealmView::Overview => RealmView::Log,
            RealmView::Log => RealmView::History,
            RealmView::History => RealmView::Map,
        }
    }
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
    /// The join overlay over the board: the colour you will be arriving as.
    pub join_draft: Option<RealmJoinDraft>,
    /// The second a countdown on screen was last drawn at. A muster clock is
    /// the one thing here that changes without anything arriving, so it is
    /// the one thing that has to ask for its own frames.
    countdown_second: Option<i64>,
    /// The world's chat is up in place of the map's rail. Hidden by default,
    /// so the board opens on the whole map; F4 flips it, and it stays as
    /// left for the rest of the session, across boards.
    pub chat_open: bool,
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
    /// The order being written: `a`/`f` open it on a game that takes orders,
    /// `+`/`-` and digits size it, Enter places it, Esc puts it away.
    pub prompt: Option<Order>,
    /// Cursor into the orders view's list.
    pub orders_cursor: usize,
    /// The day rolled with this board open: the night's dispatch is owed,
    /// and goes up once the reloaded row carrying it lands.
    dispatch_pending: bool,
    /// The history's night being played back, a fight a step.
    pub replay: Option<Replay>,
    /// Cursor into the target table.
    pub targets_cursor: usize,
    /// Cursor into the overview's holdings list.
    pub holdings_cursor: usize,
    /// The join for this world's chat room has been asked for, once per
    /// board: membership is what the pane's messages hang on.
    pub chat_join_requested: bool,
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
    /// The last tick with the board as the open screen. The board outlives a
    /// hop to another screen (it stays on the backtick cycle), so this is
    /// what closes a forgotten one; see [`RealmState::board_idle_expired`].
    last_seen: Instant,
    /// See [`RealmState::force_board_idle_for_test`].
    #[cfg(test)]
    idle_forced: bool,
}

/// A night played back in the history view, one `Fought` entry a step,
/// over the board the night opened on.
#[derive(Clone, Copy, Debug)]
pub struct Replay {
    /// The archived day whose night is playing.
    pub day: i32,
    /// The fight on screen, an index into [`replay_steps`].
    pub step: usize,
    /// Advancing by itself, a step every [`REPLAY_STEP`].
    pub playing: bool,
    last_step: Instant,
}

/// How long a replayed fight stays on screen while it plays by itself.
pub const REPLAY_STEP: Duration = Duration::from_millis(500);

/// A night's steps: its fights, in the order the night rolled them.
pub fn replay_steps(result: &DayResult) -> Vec<&LogEntry> {
    result
        .entries
        .iter()
        .filter(|entry| matches!(entry, LogEntry::Fought { .. }))
        .collect()
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
    /// What the pot did (`realm_games.payout`); `None` while the sweeper
    /// still owes it, which the results card says instead of "paid".
    pub payout: Option<String>,
    /// This world's own chat room; `None` only on a world opened before
    /// rooms were born with it, until the sweeper gives it one.
    pub chat_room_id: Option<Uuid>,
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
    pub route: Route,
    /// How deep its holder has dug in, 1 for plain ground.
    pub fort_level: u8,
    /// Odds if you acted on it now (one point, on a game that takes
    /// orders); None for your own land.
    pub probability: Option<f64>,
    /// Claim (free) or attack (held by someone else).
    pub attack: bool,
    /// Your own territory — listed so the table doubles as the atlas of
    /// what you hold, and so you can jump to it on the map.
    pub mine: bool,
    /// Set when a rule forbids taking it for now.
    pub locked: Option<Lock>,
}

impl TargetRow {
    /// Can this row be attacked or claimed right now? Your own ground never
    /// can — digging in is its own key.
    pub fn actionable(&self) -> bool {
        !self.mine && self.locked.is_none()
    }
}

/// How a take gets to a row: the instant engine prices the route there, the
/// orders engine the land behind the take.
#[derive(Clone, Copy, Debug)]
pub enum Route {
    Reach(Reach),
    /// The wall levels behind a take (`Projection::front`), and whether it
    /// comes off the sea.
    Front {
        strength: f64,
        landing: bool,
    },
    /// Nothing of yours borders it and no shore of yours reaches it.
    NoFront,
}

impl Route {
    /// The column's heading, as the table's rows read it.
    pub fn heading(self) -> &'static str {
        match self {
            Route::Reach(_) => "reach",
            Route::Front { .. } | Route::NoFront => "front",
        }
    }

    /// The route in words, for the territory card.
    pub fn describe(self) -> String {
        match self {
            Route::Reach(reach) => reach.describe(),
            Route::Front {
                strength,
                landing: true,
            } => format!("a landing, front {strength:.1}"),
            Route::Front {
                strength,
                landing: false,
            } => format!("front {strength:.1} behind it"),
            Route::NoFront => "nothing of yours borders it".to_string(),
        }
    }
}

/// Why a row cannot be taken right now, as whichever engine runs the game
/// says it.
#[derive(Clone, Copy, Debug)]
pub enum Lock {
    Action(ActionRejected),
    Order(OrderRejected),
}

impl Lock {
    pub fn message(self) -> String {
        match self {
            Lock::Action(rejected) => rejected.message(),
            Lock::Order(rejected) => rejected.message(),
        }
    }
}

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
            join_draft: None,
            chat_open: false,
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

    /// The standing world, as the hub card and the board's join key read it.
    pub fn live(&self) -> Option<&super::svc::RealmGameItem> {
        self.snapshot.live()
    }

    /// The open board's chat room, shown or not.
    pub fn chat_room_id(&self) -> Option<Uuid> {
        self.board.as_ref()?.detail.as_ref()?.chat_room_id
    }

    /// The chat room the board is drawing a pane for: the board's room while
    /// the chat is up, `None` while it is hidden or the board has no room.
    pub fn shown_chat_room_id(&self) -> Option<Uuid> {
        match self.chat_open {
            true => self.chat_room_id(),
            false => None,
        }
    }

    /// Show or hide the chat in place of the rail. Returns the new value.
    pub fn toggle_chat(&mut self) -> bool {
        self.chat_open ^= true;
        self.chat_open
    }

    /// The chat room to join for the open board, once per board. `None`
    /// once asked for, or while there is no room to join yet.
    pub fn take_chat_join(&mut self) -> Option<Uuid> {
        let room_id = self.chat_room_id()?;
        let board = self.board.as_mut()?;
        match board.chat_join_requested {
            true => None,
            false => {
                board.chat_join_requested = true;
                Some(room_id)
            }
        }
    }

    /// Whether the viewer can march into the standing world right now:
    /// there is one, they are not already on its roster, and its gates are
    /// open. The service has the final say when they do.
    pub fn can_join(&self) -> bool {
        self.live()
            .is_some_and(|game| !game.is_member(self.user_id) && game.joinable)
    }

    // ----- join draft (the colour picker over the board)

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

    /// Esc on the picker. Returns true when it was open, so the board knows
    /// the key was spent here rather than closing the board.
    pub fn join_cancel(&mut self) -> bool {
        self.join_draft.take().is_some()
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
        if let Some(dispatch) = self.take_dispatch() {
            banner = Some(dispatch);
            changed = true;
        }
        changed |= self.advance_replay();
        changed |= self.countdown_ticked();
        RealmTick { banner, changed }
    }

    /// Has a countdown on screen moved to another second? The muster clock
    /// counts down in the board header and on the hub card, and nothing
    /// else would ask for the frame that shows 1:22 after 1:23.
    fn countdown_ticked(&mut self) -> bool {
        let showing = self
            .board
            .as_ref()
            .and_then(|b| b.detail.as_ref())
            .and_then(|d| d.muster_left())
            .map(|left| left.num_seconds())
            .or_else(|| {
                // The hub card counts down too, for the world it shows.
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
            // Your own order stands: the banner says it, and what it would
            // fetch if the night fell now.
            RealmEvent::OrderPlaced {
                game_id,
                user_id,
                order,
                probability,
                points_left,
            } if user_id == self.user_id => {
                let name = self.territory_name(game_id, order.target());
                self.order_answered(game_id);
                Some(Banner::success(&match (order, probability) {
                    (Order::Take { points, .. }, Some(p)) => format!(
                        "Realm: ordered a take on {name} with {} · {:.0}% as it stands · \
                         {points_left} left to order",
                        points_label(points.get()),
                        p * 100.0
                    ),
                    (Order::Take { points, .. }, None) => format!(
                        "Realm: ordered a take on {name} with {} · {points_left} left to order",
                        points_label(points.get())
                    ),
                    (Order::Fortify { levels, .. }, _) => format!(
                        "Realm: ordered {} of walls on {name} · {points_left} left to order",
                        match levels.get() {
                            1 => "a level".to_string(),
                            n => format!("{n} levels"),
                        }
                    ),
                }))
            }
            RealmEvent::OrderPlaced { .. } => None,
            RealmEvent::OrderWithdrawn {
                game_id,
                user_id,
                target,
                points_left,
            } if user_id == self.user_id => {
                let name = self.territory_name(game_id, target);
                self.order_answered(game_id);
                Some(Banner::info(&format!(
                    "Realm: order on {name} withdrawn · {points_left} left to order"
                )))
            }
            RealmEvent::OrderWithdrawn { .. } => None,
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
                let here = self.board.as_ref().is_some_and(|b| b.game_id == game_id);
                if let Some(board) = self.board.as_mut()
                    && here
                {
                    // A load already on its way carries the board from
                    // before the night; the dispatch has to wait for one
                    // that started after it.
                    board.load_rx = None;
                    board.reload_pending = true;
                    board.dispatch_pending = true;
                    board.logs.clear();
                    board.logs_exhausted = false;
                    board.prompt = None;
                    board.replay = None;
                }
                // With the board open the night's dispatch says it instead,
                // once the row carrying the night has landed.
                (mine && !here).then(|| Banner::info(&format!("The Realm calls you! ({name})")))
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
            // A world stands frozen for its first day, and this is the only
            // window in which everyone can start level, so it interrupts
            // whatever anyone is doing, wherever they are.
            RealmEvent::SeasonOpened { name, opens_at, .. } => {
                let left = (opens_at - Utc::now()).max(chrono::Duration::zero());
                Some(Banner::info(&format!(
                    "A new realm rises: {name} · the gates open in {} · press 3, then Realm",
                    countdown_label(left)
                )))
            }
        }
    }

    /// A territory's name on the board open on `game_id`, for a banner.
    fn territory_name(&self, game_id: Uuid, target: TerritoryId) -> String {
        self.board
            .as_ref()
            .filter(|b| b.game_id == game_id)
            .and_then(|b| b.detail.as_ref())
            .and_then(|d| d.map())
            .and_then(|m| m.territory(target).map(|t| t.name.clone()))
            .unwrap_or_else(|| "the territory".to_string())
    }

    /// An order's answer landed: the board may write the next one, and
    /// reloads to show this one standing.
    fn order_answered(&mut self, game_id: Uuid) {
        if let Some(board) = self.board.as_mut()
            && board.game_id == game_id
        {
            board.act_in_flight = false;
            board.reload_pending = true;
        }
    }

    /// Open the full-screen board for a game.
    pub fn open_board(&mut self, game_id: Uuid, return_screen: Screen) {
        // A picker belongs to the board it was opened over.
        self.join_draft = None;
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
            prompt: None,
            orders_cursor: 0,
            dispatch_pending: false,
            replay: None,
            chat_join_requested: false,
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
            last_seen: Instant::now(),
            #[cfg(test)]
            idle_forced: false,
        });
        self.start_board_load();
    }

    /// Mark the open board as still looked at. `App::tick` calls this while
    /// the board is the open screen; every key lands there too, so the clock
    /// measures time away from the board.
    pub fn touch_board(&mut self) {
        if let Some(board) = self.board.as_mut() {
            board.last_seen = Instant::now();
            #[cfg(test)]
            {
                board.idle_forced = false;
            }
        }
    }

    /// An open board nobody has looked at for the shared door window
    /// (`door::game::IDLE_WINDOW`): the visit is over, and the caller closes
    /// it, which takes it off the backtick cycle and the hub's live list.
    pub fn board_idle_expired(&self) -> bool {
        let Some(board) = self.board.as_ref() else {
            return false;
        };
        #[cfg(test)]
        if board.idle_forced {
            return true;
        }
        board.last_seen.elapsed() >= crate::app::door::game::IDLE_WINDOW
    }

    /// Test-only: force the idle deadline, so the close in `App::tick` can be
    /// exercised without waiting half an hour. A flag rather than an aged
    /// `last_seen`, because winding an `Instant` back half an hour panics on a
    /// machine booted more recently than that. Cleared by
    /// [`RealmState::touch_board`].
    #[cfg(test)]
    pub fn force_board_idle_for_test(&mut self) {
        if let Some(board) = self.board.as_mut() {
            board.idle_forced = true;
        }
    }

    pub fn close_board(&mut self) -> Option<Screen> {
        self.join_draft = None;
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
                            name: super::svc::game_name(&row.name, row.season),
                            row_status: row.status,
                            created: row.created,
                            last_resolved_day: row.last_resolved_day,
                            reset_hour_utc: row.reset_hour_utc,
                            winner_user_id: row.winner_user_id,
                            payout: row.payout,
                            chat_room_id: row.chat_room_id,
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
            board.replay = None;
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
            board.replay = None;
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
    /// someone else holds it, or open the order prompt on a game that takes
    /// orders. Returns a banner when the board itself can say no without
    /// bothering the server.
    pub fn act_selected(&mut self) -> Option<Banner> {
        let target = self.board.as_ref()?.selected?;
        self.act_on(target)
    }

    /// `a` on `target`, whatever view picked it.
    pub fn act_on(&mut self, target: TerritoryId) -> Option<Banner> {
        let resolution = match self.gate() {
            Ok(resolution) => resolution,
            Err(refusal) => return refusal,
        };
        match resolution {
            Resolution::Instant => self.act_now(target),
            Resolution::Orders => self.open_prompt(target, OrderKind::Take),
        }
    }

    /// Dig in on the selected territory — the only way a point is ever
    /// spent on walls.
    pub fn fortify_selected(&mut self) -> Option<Banner> {
        let target = self.board.as_ref()?.selected?;
        self.fortify_on(target)
    }

    /// `f` on `target`, whatever view picked it.
    pub fn fortify_on(&mut self, target: TerritoryId) -> Option<Banner> {
        let resolution = match self.gate() {
            Ok(resolution) => resolution,
            Err(refusal) => return refusal,
        };
        match resolution {
            Resolution::Instant => self.fortify_now(target),
            Resolution::Orders => self.open_prompt(target, OrderKind::Fortify),
        }
    }

    /// What every key that spends points asks first: a running game, open,
    /// with the viewer alive in it and nothing already in flight. `Err` is
    /// the board saying no, with a banner when there is something to say.
    fn gate(&self) -> Result<Resolution, Option<Banner>> {
        let board = self.board.as_ref().ok_or(None)?;
        let detail = board.detail.as_ref().ok_or(None)?;
        if detail.row_status != "active" {
            return Err(Some(Banner::info("Realm: the game is not running")));
        }
        if let Some(left) = detail.muster_left() {
            return Err(Some(Banner::info(&format!(
                "Realm: this one opens in {} — everyone starts together",
                countdown_label(left)
            ))));
        }
        match detail.state.player(self.user_id).map(|p| p.status) {
            Some(RealmPlayerStatus::Alive) => {}
            Some(_) => {
                return Err(Some(Banner::info(
                    "Realm: you are out of this war — watching only",
                )));
            }
            None => {
                return Err(Some(Banner::info(
                    "Realm: you are watching this realm, not playing it",
                )));
            }
        }
        if board.act_in_flight {
            return Err(None);
        }
        Ok(detail.state.ruleset.resolution)
    }

    /// The instant engine's `a`: the action is sent immediately and the
    /// result arrives as a banner.
    fn act_now(&mut self, target: TerritoryId) -> Option<Banner> {
        let user_id = self.user_id;
        let board = self.board.as_ref()?;
        let game_id = board.game_id;
        let detail = board.detail.as_ref()?;
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
                let reach = super::resolver::reach_for(
                    &detail.state,
                    &map,
                    user_id,
                    target,
                    detail.today(),
                );
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

    /// The instant engine's `f`: a level of walls, on the spot.
    fn fortify_now(&mut self, target: TerritoryId) -> Option<Banner> {
        let user_id = self.user_id;
        let board = self.board.as_ref()?;
        let game_id = board.game_id;
        let detail = board.detail.as_ref()?;
        if !detail.state.option(OPTION_FORTIFY) {
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

    // ----- orders (written by day, resolved at the night)

    /// Open the prompt on `target`, sized at the viewer's standing order
    /// there if they have one, else at one. Refused on the spot, for free,
    /// when the night could not carry it out at any size.
    fn open_prompt(&mut self, target: TerritoryId, kind: OrderKind) -> Option<Banner> {
        let standing = self
            .my_orders()
            .into_iter()
            .find(|order| order.target() == target);
        let order = match (kind, standing) {
            (OrderKind::Take, Some(order @ Order::Take { .. }))
            | (OrderKind::Fortify, Some(order @ Order::Fortify { .. })) => order,
            (OrderKind::Take, _) => Order::Take {
                target,
                points: NonZeroU8::MIN,
            },
            (OrderKind::Fortify, _) => Order::Fortify {
                target,
                levels: NonZeroU8::MIN,
            },
        };
        if let Err(refused) = self.check_order(order)? {
            return Some(Banner::info(&format!("Realm: {}", refused.message())));
        }
        if let Some(board) = self.board.as_mut() {
            board.prompt = Some(order);
        }
        None
    }

    /// Would the service take this order, on the board as it stands? The
    /// same `night::place_order` the service runs, on a copy, so the round
    /// trip never carries an order the board could have refused.
    fn check_order(&self, order: Order) -> Option<Result<night::Placed, OrderRejected>> {
        let detail = self.board.as_ref()?.detail.as_ref()?;
        let map = detail.map()?;
        let mut state = detail.state.clone();
        Some(night::place_order(
            &mut state,
            &map,
            self.user_id,
            order,
            detail.today(),
        ))
    }

    /// The viewer's orders for tonight, in the order they were placed.
    pub fn my_orders(&self) -> Vec<Order> {
        self.board
            .as_ref()
            .and_then(|b| b.detail.as_ref())
            .and_then(|d| d.state.player(self.user_id))
            .map(|p| p.orders.clone())
            .unwrap_or_default()
    }

    /// The most the prompt's order can be sized to, as the engine counts it
    /// (`night::order_quote`): never below one, so the prompt always has a
    /// size to show and the engine's refusal says why.
    fn prompt_max(&self, order: Order) -> u8 {
        let Some(detail) = self.board.as_ref().and_then(|b| b.detail.as_ref()) else {
            return 1;
        };
        night::order_quote(&detail.state, self.user_id, order, detail.today())
            .max
            .max(1)
    }

    /// Size the prompt's order, held between one and what can be afforded.
    pub fn prompt_resize(&mut self, amount: u8) {
        let Some(order) = self.board.as_ref().and_then(|b| b.prompt) else {
            return;
        };
        let amount =
            NonZeroU8::new(amount.clamp(1, self.prompt_max(order))).unwrap_or(NonZeroU8::MIN);
        let sized = match order {
            Order::Take { target, .. } => Order::Take {
                target,
                points: amount,
            },
            Order::Fortify { target, .. } => Order::Fortify {
                target,
                levels: amount,
            },
        };
        if let Some(board) = self.board.as_mut() {
            board.prompt = Some(sized);
        }
    }

    /// `+`/`-` on the prompt.
    pub fn prompt_adjust(&mut self, delta: i16) {
        let Some(order) = self.board.as_ref().and_then(|b| b.prompt) else {
            return;
        };
        let amount = (i16::from(order.cost()) + delta).clamp(1, i16::from(u8::MAX));
        self.prompt_resize(amount as u8);
    }

    /// Place the prompt's order. The prompt stays up when the board can say
    /// no, so the size can be fixed rather than typed again.
    pub fn prompt_confirm(&mut self) -> Option<Banner> {
        let order = self.board.as_ref()?.prompt?;
        let game_id = self.board.as_ref()?.game_id;
        if let Err(refused) = self.check_order(order)? {
            return Some(Banner::info(&format!("Realm: {}", refused.message())));
        }
        if let Some(board) = self.board.as_mut() {
            board.prompt = None;
            board.act_in_flight = true;
        }
        self.svc.place_order_task(self.user_id, game_id, order);
        None
    }

    /// Esc on the prompt. Returns true when it was up, so the key is spent
    /// here rather than closing the board.
    pub fn prompt_cancel(&mut self) -> bool {
        self.board
            .as_mut()
            .is_some_and(|board| board.prompt.take().is_some())
    }

    /// The prompt's take priced at the size on screen, as the night would
    /// price it with the board as it stands. `None` for walls.
    pub fn prompt_projection(&self) -> Option<Projection> {
        let board = self.board.as_ref()?;
        let detail = board.detail.as_ref()?;
        let map = detail.map()?;
        match board.prompt? {
            Order::Take { target, points } => night::projection(
                &detail.state,
                &map,
                self.user_id,
                target,
                points.get(),
                detail.today(),
            ),
            Order::Fortify { .. } => None,
        }
    }

    /// A standing order priced as the board now stands, for the orders
    /// list: the odds can move during the day as others' walls go up.
    pub fn order_projection(&self, order: Order) -> Option<Projection> {
        let detail = self.board.as_ref()?.detail.as_ref()?;
        let map = detail.map()?;
        match order {
            Order::Take { target, points } => night::projection(
                &detail.state,
                &map,
                self.user_id,
                target,
                points.get(),
                detail.today(),
            ),
            Order::Fortify { .. } => None,
        }
    }

    /// Walk the orders list, mirroring the pick into the map selection.
    pub fn move_orders_cursor(&mut self, delta: isize) {
        let orders = self.my_orders();
        let Some(board) = self.board.as_mut() else {
            return;
        };
        if orders.is_empty() {
            board.orders_cursor = 0;
            return;
        }
        let next = (board.orders_cursor as isize + delta).clamp(0, orders.len() as isize - 1);
        board.orders_cursor = next as usize;
        board.selected = Some(orders[next as usize].target());
    }

    /// The order the list's cursor is on.
    pub fn selected_order(&self) -> Option<Order> {
        let board = self.board.as_ref()?;
        let orders = self.my_orders();
        let at = board.orders_cursor.min(orders.len().saturating_sub(1));
        orders.get(at).copied()
    }

    /// `x` on the orders list: take the highlighted order back, and its
    /// points with it.
    pub fn withdraw_selected(&mut self) -> Option<Banner> {
        let order = self.selected_order()?;
        if let Err(refusal) = self.gate() {
            return refusal;
        }
        let game_id = self.board.as_ref()?.game_id;
        if let Some(board) = self.board.as_mut() {
            board.act_in_flight = true;
        }
        self.svc
            .withdraw_order_task(self.user_id, game_id, order.target());
        None
    }

    /// The banner the board owes after a night: once the reloaded row that
    /// carries it has landed. A game with no nights to report (or a viewer
    /// with no part in it) gets the day's summons instead.
    fn take_dispatch(&mut self) -> Option<Banner> {
        let board = self.board.as_mut()?;
        let ready = board.dispatch_pending
            && board.load_rx.is_none()
            && !board.reload_pending
            && board.load_error.is_none();
        if !ready {
            return None;
        }
        board.dispatch_pending = false;
        let detail = board.detail.as_ref()?;
        let player = detail.state.player(self.user_id)?;
        match (detail.state.last_night.as_ref(), player.status) {
            (Some(night), _) => Some(Banner::info(&format!(
                "The night has passed: {}",
                dispatch_line(night, self.user_id)
            ))),
            (None, RealmPlayerStatus::Alive) => Some(Banner::info(&format!(
                "The Realm calls you! ({})",
                detail.name
            ))),
            (None, _) => None,
        }
    }

    // ----- the replay (the history's night, a fight a step)

    /// Does the history offer the day on screen's night as a replay? Only a
    /// game that resolves at night has one, and only a day whose opening
    /// board was kept and that saw a fight.
    pub fn replay_available(&self) -> bool {
        let Some(detail) = self.board.as_ref().and_then(|b| b.detail.as_ref()) else {
            return false;
        };
        match detail.state.ruleset.resolution {
            Resolution::Instant => false,
            Resolution::Orders => self.history_selected().is_some_and(|day| {
                day.board_open.is_some() && !replay_steps(&day.result).is_empty()
            }),
        }
    }

    /// Does the standings table say how many points `user_id` has left? On
    /// the instant engine a point spent is a move everyone saw. On the
    /// campaign a rival's points left is how much they committed tonight,
    /// and orders are secret: only the viewer's own show.
    pub fn points_shown(&self, user_id: Uuid) -> bool {
        let Some(detail) = self.board.as_ref().and_then(|b| b.detail.as_ref()) else {
            return false;
        };
        match detail.state.ruleset.resolution {
            Resolution::Instant => true,
            Resolution::Orders => user_id == self.user_id,
        }
    }

    /// `p`: start the day on screen's replay on its first fight, still; and
    /// once it is up, set it playing or hold it.
    pub fn replay_toggle(&mut self) -> Option<Banner> {
        if let Some(replay) = self.board.as_mut().and_then(|b| b.replay.as_mut()) {
            replay.playing ^= true;
            replay.last_step = Instant::now();
            return None;
        }
        if !self.replay_available() {
            return Some(Banner::info("Realm: this day has no night to play back"));
        }
        let day = self.history_selected()?.day();
        if let Some(board) = self.board.as_mut() {
            board.replay = Some(Replay {
                day,
                step: 0,
                playing: false,
                last_step: Instant::now(),
            });
        }
        None
    }

    /// `.` and `,`: one fight on or back, which also holds the replay.
    pub fn replay_step(&mut self, delta: isize) {
        let steps = self.replay_len();
        if let Some(replay) = self.board.as_mut().and_then(|b| b.replay.as_mut()) {
            replay.playing = false;
            replay.step = history_step_index(replay.step, steps, delta);
        }
    }

    /// Esc on the replay. Returns true when one was up.
    pub fn replay_stop(&mut self) -> bool {
        self.board
            .as_mut()
            .is_some_and(|board| board.replay.take().is_some())
    }

    /// A lone Esc on the board peels what is on top, one layer a press: the
    /// colour picker, then the results card, then the order prompt, then a
    /// replaying night. Returns true when a layer went, so the key was
    /// spent here and the board stays up. The one order, read by the root's
    /// Esc and by the board's own `q`/Esc alike.
    pub fn peel_overlay(&mut self) -> bool {
        self.join_cancel() || self.close_results() || self.prompt_cancel() || self.replay_stop()
    }

    /// How many fights the playing replay has.
    fn replay_len(&self) -> usize {
        let Some(replay) = self.board.as_ref().and_then(|b| b.replay) else {
            return 0;
        };
        self.history_days()
            .into_iter()
            .find(|day| day.day() == replay.day)
            .map(|day| replay_steps(&day.result).len())
            .unwrap_or(0)
    }

    /// Move a playing replay on when its step is up; it stops on the last
    /// fight. True when the frame changed.
    fn advance_replay(&mut self) -> bool {
        let steps = self.replay_len();
        let Some(replay) = self.board.as_mut().and_then(|b| b.replay.as_mut()) else {
            return false;
        };
        if !replay.playing || replay.last_step.elapsed() < REPLAY_STEP {
            return false;
        }
        replay.last_step = Instant::now();
        match replay.step + 1 < steps {
            true => replay.step += 1,
            false => replay.playing = false,
        }
        true
    }

    /// How a row is priced and whether it is open: one map lookup per
    /// territory in `only` (every territory when `None`), by whichever
    /// engine runs the game.
    fn rows_for(&self, only: Option<TerritoryId>) -> Vec<TargetRow> {
        let Some(detail) = self.board.as_ref().and_then(|b| b.detail.as_ref()) else {
            return Vec::new();
        };
        let Some(map) = detail.map() else {
            return Vec::new();
        };
        let territories = map
            .territories
            .iter()
            .filter(|t| only.is_none_or(|id| t.id == id));
        match detail.state.ruleset.resolution {
            Resolution::Instant => instant_rows(detail, &map, self.user_id, territories),
            Resolution::Orders => order_rows(detail, &map, self.user_id, territories),
        }
    }

    /// The selected territory's row, for the map's territory card.
    pub fn target_row(&self, target: TerritoryId) -> Option<TargetRow> {
        self.rows_for(Some(target)).pop()
    }

    /// Every territory worth a row in the target table: your own holdings
    /// first (so the table is also the atlas of what you hold), then
    /// everything you could act on, nearest first and best odds within a
    /// distance.
    pub fn target_rows(&self) -> Vec<TargetRow> {
        self.rows_for(None)
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

/// Action points left today, and what today is worth, for the viewer. The
/// cap is the day's real allowance (the tier for who is involved, plus the
/// bank), the same figure the engine spends against.
pub fn points_left(board: &RealmBoardState, user_id: Uuid) -> Option<(usize, usize)> {
    let detail = board.detail.as_ref()?;
    let standing = detail.state.standing(user_id, detail.today());
    Some((
        usize::from(standing.left()),
        usize::from(standing.allowance),
    ))
}

/// The instant engine's rows: priced along the route there, yours first,
/// then nearest, then best odds.
fn instant_rows<'a>(
    detail: &RealmDetail,
    map: &WorldMap,
    user_id: Uuid,
    territories: impl Iterator<Item = &'a super::map::Territory>,
) -> Vec<TargetRow> {
    let state = &detail.state;
    let today = detail.today();
    let reach_index = ReachIndex::build(state, map, user_id, today);
    // Whether the short-reach cap is biting at all: it lifts entirely for a
    // player with nothing in range, so the table must not mark rows
    // unreachable when they are the only thing left.
    let out_of_range_applies = state.reach_cap().is_some_and(|cap| {
        super::resolver::anything_within(state, map, &reach_index, user_id, cap)
    });
    let mut rows: Vec<(f64, TargetRow)> = territories
        .map(|t| {
            let owner = state.ownership.get(&t.id).copied();
            let mine = owner == Some(user_id);
            // Your own land has no route to price.
            let reach = reach_index.reach(map, &state.ruleset, t.id);
            let probability = if mine {
                None
            } else {
                projected_probability(state, map, user_id, t.id, reach, today)
            };
            let locked = if !mine && reach.is_unreachable() {
                // Nothing to sail from, or nothing to land on. Said plainly,
                // because the row used to read like any other.
                Some(ActionRejected::NoRoute)
            } else if !mine
                && state.reach_cap().is_some_and(|cap| reach.cost > cap)
                && out_of_range_applies
            {
                // This game keeps war close to home, and something nearer
                // is still on the table.
                Some(ActionRejected::OutOfRange)
            } else {
                owner.filter(|_| !mine).and_then(|defender| {
                    state.attack_allowed(user_id, defender, reach, today).err()
                })
            };
            (
                reach.cost,
                row_of(
                    state,
                    t,
                    owner,
                    mine,
                    Route::Reach(reach),
                    probability,
                    locked.map(Lock::Action),
                ),
            )
        })
        .collect();
    rows.sort_by(|(a_cost, a), (b_cost, b)| {
        b.mine
            .cmp(&a.mine)
            .then_with(|| a_cost.total_cmp(b_cost))
            .then_with(|| {
                b.probability
                    .unwrap_or(0.0)
                    .total_cmp(&a.probability.unwrap_or(0.0))
            })
            .then_with(|| a.name.cmp(&b.name))
    });
    rows.into_iter().map(|(_, row)| row).collect()
}

/// The orders engine's rows: priced at one point as tonight looks from the
/// viewer's seat, yours first, then what is open, best odds first.
fn order_rows<'a>(
    detail: &RealmDetail,
    map: &WorldMap,
    user_id: Uuid,
    territories: impl Iterator<Item = &'a super::map::Territory>,
) -> Vec<TargetRow> {
    let state = &detail.state;
    let today = detail.today();
    let mut rows: Vec<TargetRow> = territories
        .map(|t| {
            let owner = state.ownership.get(&t.id).copied();
            let mine = owner == Some(user_id);
            let projection = match mine {
                true => None,
                false => night::projection(state, map, user_id, t.id, 1, today),
            };
            let route = match projection {
                Some(p) => Route::Front {
                    strength: p.front,
                    landing: p.landing,
                },
                None => Route::NoFront,
            };
            let locked = match mine {
                true => None,
                false => night::check_take(state, map, user_id, t.id, today).err(),
            };
            row_of(
                state,
                t,
                owner,
                mine,
                route,
                projection.map(|p| p.probability),
                locked.map(Lock::Order),
            )
        })
        .collect();
    let front = |row: &TargetRow| match row.route {
        Route::Front { strength, .. } => strength,
        Route::Reach(_) | Route::NoFront => 0.0,
    };
    rows.sort_by(|a, b| {
        b.mine
            .cmp(&a.mine)
            .then_with(|| a.locked.is_some().cmp(&b.locked.is_some()))
            .then_with(|| {
                b.probability
                    .unwrap_or(0.0)
                    .total_cmp(&a.probability.unwrap_or(0.0))
            })
            .then_with(|| front(b).total_cmp(&front(a)))
            .then_with(|| a.name.cmp(&b.name))
    });
    rows
}

fn row_of(
    state: &RealmGameState,
    t: &super::map::Territory,
    owner: Option<Uuid>,
    mine: bool,
    route: Route,
    probability: Option<f64>,
    locked: Option<Lock>,
) -> TargetRow {
    TargetRow {
        id: t.id,
        name: t.name.clone(),
        owner,
        owner_name: owner.and_then(|o| state.player(o).map(|p| p.username.clone())),
        color: owner.map(|o| player_color(state, o)),
        population: t.population,
        area_km2: t.area_km2,
        route,
        fort_level: state.fort_level(t.id),
        probability,
        attack: owner.is_some(),
        mine,
        locked,
    }
}

/// Which kind of order a key opens the prompt on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrderKind {
    Take,
    Fortify,
}

/// How a finished game was won, from the `Won` line its last log carries:
/// the night's on a game that resolves at night, the day's otherwise.
pub fn win_kind(state: &RealmGameState) -> Option<super::resolver::WinKind> {
    [state.last_night.as_ref(), state.last_day.as_ref()]
        .into_iter()
        .flatten()
        .flat_map(|day| day.entries.iter())
        .find_map(|entry| {
            let LogEntry::Won { kind, .. } = entry else {
                return None;
            };
            Some(*kind)
        })
}

/// "1 pt", "3 pts".
pub fn points_label(points: u8) -> String {
    match points {
        1 => "1 pt".to_string(),
        n => format!("{n} pts"),
    }
}

/// What a night did to one player, counted: the dispatch banner's line,
/// "you took 2, lost 1, 1 bounced".
pub fn dispatch_line(night: &DayResult, user_id: Uuid) -> String {
    let (mut took, mut lost, mut bounced, mut failed) = (0u32, 0u32, 0u32, 0u32);
    for entry in &night.entries {
        let LogEntry::Fought {
            holder,
            taken_by,
            takers,
            bounced: level,
            ..
        } = entry
        else {
            continue;
        };
        if let Some(roll) = takers.iter().find(|roll| roll.user_id == user_id) {
            match (*taken_by == Some(user_id), roll.success && *level) {
                (true, _) => took += 1,
                (false, true) => bounced += 1,
                (false, false) => failed += 1,
            }
        }
        if *holder == Some(user_id) && taken_by.is_some_and(|by| by != user_id) {
            lost += 1;
        }
    }
    let mut parts = Vec::new();
    if took > 0 {
        parts.push(format!("you took {took}"));
    }
    if lost > 0 {
        parts.push(format!("lost {lost}"));
    }
    if bounced > 0 {
        parts.push(format!("{bounced} bounced"));
    }
    if failed > 0 {
        parts.push(format!("{failed} failed"));
    }
    match parts.is_empty() {
        true => "nothing of yours was fought over".to_string(),
        false => parts.join(", "),
    }
}

/// One player's colour: a name to pick it by and a ladder of three shades,
/// palest first.
pub struct PlayerPalette {
    pub name: &'static str,
    pub shades: [(u8, u8, u8); FORT_SHADES],
}

/// The twenty colours a realm can be played in.
///
/// Both of the map's fills live in here: whose a territory is (the hue) and
/// how dug in they are (where along the ladder it sits, dark meaning walled
/// up). Twenty hues times three steps is sixty fills that have to stay apart
/// by eye, which hand-picking does badly, so the ladders are laid out in
/// CIELAB and the distances are held to by a test rather than by eye. The
/// first ten are the middle of season one's five-step ladders, so a game
/// from then draws as it did; the other ten were placed around them by a
/// search for the largest gap, with every shade light enough to read on a
/// dark terminal.
pub const PLAYER_PALETTES: [PlayerPalette; 20] = [
    PlayerPalette {
        name: "rose",
        shades: [(255, 161, 197), (255, 116, 174), (238, 76, 150)],
    },
    PlayerPalette {
        name: "orange",
        shades: [(255, 166, 130), (246, 132, 87), (226, 100, 52)],
    },
    PlayerPalette {
        name: "gold",
        shades: [(226, 184, 96), (201, 157, 51), (175, 132, 0)],
    },
    PlayerPalette {
        name: "olive",
        shades: [(175, 199, 105), (145, 175, 62), (113, 151, 0)],
    },
    PlayerPalette {
        name: "green",
        shades: [(113, 209, 141), (63, 185, 108), (0, 160, 78)],
    },
    PlayerPalette {
        name: "teal",
        shades: [(0, 213, 187), (0, 184, 162), (0, 156, 137)],
    },
    PlayerPalette {
        name: "cyan",
        shades: [(0, 208, 237), (0, 180, 206), (0, 153, 175)],
    },
    PlayerPalette {
        name: "blue",
        shades: [(101, 199, 255), (0, 175, 240), (0, 148, 205)],
    },
    PlayerPalette {
        name: "violet",
        shades: [(193, 179, 255), (164, 150, 255), (134, 123, 242)],
    },
    PlayerPalette {
        name: "magenta",
        shades: [(239, 163, 243), (219, 131, 226), (199, 99, 208)],
    },
    PlayerPalette {
        name: "brick",
        shades: [(227, 110, 116), (193, 90, 95), (160, 70, 76)],
    },
    PlayerPalette {
        name: "blush",
        shades: [(248, 190, 184), (215, 158, 153), (183, 128, 123)],
    },
    PlayerPalette {
        name: "sand",
        shades: [(213, 208, 178), (181, 176, 148), (149, 145, 119)],
    },
    PlayerPalette {
        name: "lemon",
        shades: [(249, 232, 109), (227, 210, 72), (204, 190, 25)],
    },
    PlayerPalette {
        name: "lime",
        shades: [(159, 254, 119), (132, 233, 92), (105, 213, 64)],
    },
    PlayerPalette {
        name: "moss",
        shades: [(113, 158, 96), (88, 132, 72), (63, 106, 49)],
    },
    PlayerPalette {
        name: "seafoam",
        shades: [(169, 254, 242), (156, 229, 218), (143, 205, 195)],
    },
    PlayerPalette {
        name: "cobalt",
        shades: [(52, 142, 242), (50, 123, 210), (47, 106, 179)],
    },
    PlayerPalette {
        name: "slate",
        shades: [(144, 145, 172), (121, 122, 146), (99, 99, 120)],
    },
    PlayerPalette {
        name: "plum",
        shades: [(175, 121, 171), (161, 98, 158), (147, 74, 144)],
    },
];

/// Steps in a player's ladder. Walls map onto it as 1 pale, 2 to 3 mid, 4 to
/// 5 dark (`shade_index`).
pub const FORT_SHADES: usize = 3;

/// Which step stands for a player away from the map — in the standings, the
/// results card, the rail. The middle one: bright enough to read as text on
/// the dark background, and the centre of the range their territories span.
const IDENTITY_SHADE: usize = 1;

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

#[cfg(test)]
mod dispatch_test {
    use uuid::Uuid;

    use super::dispatch_line;
    use crate::app::door::realm::resolver::{DayResult, LogEntry, TakerRoll};

    fn roll(user: u128, success: bool) -> TakerRoll {
        TakerRoll {
            user_id: Uuid::from_u128(user),
            points: 2,
            from: 0,
            force: 2.0,
            probability: 0.5,
            shown: 0.5,
            success,
        }
    }

    fn fought(
        target: u16,
        holder: Option<u128>,
        taken_by: Option<u128>,
        takers: Vec<TakerRoll>,
        bounced: bool,
    ) -> LogEntry {
        LogEntry::Fought {
            target,
            holder: holder.map(Uuid::from_u128),
            taken_by: taken_by.map(Uuid::from_u128),
            defence: 1.0,
            takers,
            bounced,
        }
    }

    /// The banner after a night counts what it did to you: takes of yours
    /// that landed, holdings you lost, level bounces, takes that failed.
    /// Somebody else's fight is not yours to count.
    #[test]
    fn the_dispatch_counts_your_night() {
        let me = 1;
        let night = DayResult {
            day: 1,
            seed: 1,
            entries: vec![
                fought(10, None, Some(me), vec![roll(me, true)], false),
                fought(
                    11,
                    Some(2),
                    Some(me),
                    vec![roll(me, true), roll(3, false)],
                    false,
                ),
                fought(12, Some(me), Some(3), vec![roll(3, true)], false),
                fought(13, Some(2), None, vec![roll(me, true), roll(3, true)], true),
                fought(14, Some(2), None, vec![roll(me, false)], false),
                fought(15, Some(2), Some(3), vec![roll(3, true)], false),
            ],
        };
        assert_eq!(
            dispatch_line(&night, Uuid::from_u128(me)),
            "you took 2, lost 1, 1 bounced, 1 failed"
        );
        assert_eq!(
            dispatch_line(&night, Uuid::from_u128(9)),
            "nothing of yours was fought over"
        );
    }
}
