use std::{cell::Cell, collections::HashMap, collections::HashSet, sync::Arc};

use chrono::{DateTime, Utc};
use cozy_chess::{BitBoard, Board};
use late_core::models::daily_match::DailyMatch;
use ratatui::layout::Rect;
use tokio::sync::{broadcast, oneshot, watch};
use uuid::Uuid;

use crate::app::{
    common::primitives::{Banner, Screen},
    games::chess_core::{
        board_ui::Tier,
        cursor, rules,
        types::{ChessColor, ChessMoveSpec, ChessPiece, ChessPieceRenderMode},
    },
    notify::{Notification, Notifier},
};

use super::{
    battleship::DailyBattleshipState,
    connect4::DailyConnect4State,
    games::DailyGame,
    svc::{
        DAILY_MAX_ACTIVE_ENTRIES, DailyChallengeItem, DailyChessState, DailyEvent, DailyMatchItem,
        DailyService, DailySnapshot,
    },
};

/// One selectable row in the Daily Games modal: your matches first, then the
/// open lobby, then other people's live games you can watch.
pub enum DailyModalEntry<'a> {
    Match(&'a DailyMatchItem),
    Challenge(&'a DailyChallengeItem),
    Spectate(&'a DailyMatchItem),
}

/// A challenge being composed: a small picker overlay on the Lobby modal.
/// Step one picks the game from the roster (one row per game, prize shown);
/// directed challenges add a username step. A vertical list scales to any
/// roster size where an inline one-row picker would not.
pub struct ChallengeDraft {
    /// Picker cursor into `DailyGame::ALL`.
    pub selected: usize,
    /// Directed challenges ask for a username after the game is picked.
    pub directed: bool,
    /// `Some` once the game is chosen and the username prompt is active.
    pub username: Option<String>,
}

impl ChallengeDraft {
    pub fn game(&self) -> DailyGame {
        DailyGame::ALL[self.selected.min(DailyGame::ALL.len() - 1)]
    }
}

/// Per-session daily-games UI state: the modal, the lobby glow, and the
/// full-screen board. The system of record is `DailyService`'s snapshot;
/// everything here is presentation plus the in-flight optimistic move.
pub struct DailyState {
    user_id: Uuid,
    svc: DailyService,
    snapshot_rx: watch::Receiver<Arc<DailySnapshot>>,
    snapshot: Arc<DailySnapshot>,
    event_rx: broadcast::Receiver<DailyEvent>,

    /// Modal cursor over `modal_entry_count()` rows.
    pub selected: usize,
    /// Challenge awaiting claim confirmation (Enter pressed once).
    pub confirm_claim: Option<Uuid>,
    /// Challenge being composed (game picker + optional username prompt).
    pub challenge_draft: Option<ChallengeDraft>,
    /// Open-challenge ids already seen; anything newer glows the lobby line
    /// until the modal is opened.
    seen_open_ids: HashSet<Uuid>,
    lobby_glow: bool,
    notifier: Notifier,
    /// Match ids whose current my-turn edge already notified. Seeded from the
    /// first snapshot that arrives (see `notify_turn_edges`) so connecting
    /// never notifies; the sidebar panel is the on-login nudge.
    turn_notified_match_ids: HashSet<Uuid>,
    /// Whether `turn_notified_match_ids` has been seeded yet. False until the
    /// first snapshot update, so a cold-start empty snapshot can't make the
    /// first real snapshot notify for every my-turn match at once.
    turn_notify_seeded: bool,

    pub board: Option<DailyBoardState>,
}

/// Full-screen correspondence board (`Screen::DailyMatch`).
pub struct DailyBoardState {
    pub match_id: Uuid,
    /// You aren't a player in this match: the board is read-only. No cursor,
    /// no move/resign input, hints say "watching".
    pub spectating: bool,
    /// Screen to restore when the board closes.
    pub return_screen: Screen,
    pub cursor: usize,
    pub selected: Option<usize>,
    pub piece_render_mode: ChessPieceRenderMode,
    pub resign_confirm: bool,
    pub detail: Option<DailyMatchDetail>,
    pub load_error: Option<String>,
    load_rx: Option<oneshot::Receiver<Result<Option<DailyMatch>, String>>>,
    /// A reload arrived while one was in flight; run another when it lands.
    reload_pending: bool,
    /// Usernames captured from the snapshot when the board opened, so names
    /// survive the match leaving the active list on finish.
    pub names: HashMap<Uuid, String>,
    /// Last drawn chess board rect + tier, set during render and consumed by
    /// the mouse hit test. Cleared before every board draw.
    pub board_geometry: Cell<Option<(Rect, Tier)>>,
    /// Last drawn battleship target-grid rect (cells only, no labels), same
    /// render-recorded contract as `board_geometry`.
    pub target_geometry: Cell<Option<Rect>>,
}

/// Canonical match detail derived from one `daily_matches` row: the row
/// plus the parsed, per-game view of its state JSON.
pub struct DailyMatchDetail {
    pub row: DailyMatch,
    pub game: DailyGameDetail,
}

pub enum DailyGameDetail {
    Chess(ChessDetail),
    Battleship(BattleshipDetail),
    Connect4(Connect4Detail),
}

impl DailyGameDetail {
    /// Back to the roster enum, for dispatch that must stay exhaustive.
    pub fn kind(&self) -> DailyGame {
        match self {
            Self::Chess(_) => DailyGame::Chess,
            Self::Battleship(_) => DailyGame::Battleship,
            Self::Connect4(_) => DailyGame::ConnectFour,
        }
    }
}

pub struct ChessDetail {
    pub state: DailyChessState,
    pub pieces: [Option<ChessPiece>; 64],
    pub legal_moves: Vec<ChessMoveSpec>,
    pub turn: ChessColor,
    pub in_check: bool,
}

pub struct BattleshipDetail {
    pub state: DailyBattleshipState,
    /// A shot left this session and hasn't come back via reload yet; blocks
    /// firing again until the canonical row lands.
    pub shot_in_flight: bool,
}

pub struct Connect4Detail {
    pub state: DailyConnect4State,
    /// A drop left this session and hasn't come back via reload yet; blocks
    /// dropping again until the canonical row lands.
    pub drop_in_flight: bool,
}

impl ChessDetail {
    fn from_row(row: &DailyMatch) -> Result<Self, String> {
        let state = DailyChessState::parse(&row.state).map_err(|e| e.to_string())?;
        let board: Board = state
            .fen
            .parse()
            .map_err(|_| "corrupt daily match position".to_string())?;
        let pieces = rules::board_pieces(&board);
        let legal_moves = if row.status == DailyMatch::STATUS_ACTIVE {
            rules::legal_moves(&board)
        } else {
            Vec::new()
        };
        let turn = rules::chess_color(board.side_to_move());
        let in_check =
            row.status == DailyMatch::STATUS_ACTIVE && board.checkers() != BitBoard::EMPTY;
        Ok(Self {
            state,
            pieces,
            legal_moves,
            turn,
            in_check,
        })
    }
}

impl DailyMatchDetail {
    fn from_row(row: DailyMatch) -> Result<Self, String> {
        let game = match DailyGame::from_kind(&row.game_kind) {
            Some(DailyGame::Chess) => DailyGameDetail::Chess(ChessDetail::from_row(&row)?),
            Some(DailyGame::Battleship) => DailyGameDetail::Battleship(BattleshipDetail {
                state: DailyBattleshipState::parse(&row.state).map_err(|e| e.to_string())?,
                shot_in_flight: false,
            }),
            Some(DailyGame::ConnectFour) => DailyGameDetail::Connect4(Connect4Detail {
                state: DailyConnect4State::parse(&row.state).map_err(|e| e.to_string())?,
                drop_in_flight: false,
            }),
            None => return Err(format!("unknown daily game: {}", row.game_kind)),
        };
        Ok(Self { row, game })
    }

    pub fn chess(&self) -> Option<&ChessDetail> {
        match &self.game {
            DailyGameDetail::Chess(chess) => Some(chess),
            _ => None,
        }
    }

    fn chess_mut(&mut self) -> Option<&mut ChessDetail> {
        match &mut self.game {
            DailyGameDetail::Chess(chess) => Some(chess),
            _ => None,
        }
    }

    pub fn battleship(&self) -> Option<&BattleshipDetail> {
        match &self.game {
            DailyGameDetail::Battleship(battleship) => Some(battleship),
            _ => None,
        }
    }

    pub fn connect4(&self) -> Option<&Connect4Detail> {
        match &self.game {
            DailyGameDetail::Connect4(connect4) => Some(connect4),
            _ => None,
        }
    }

    pub fn color_of(&self, user_id: Uuid) -> Option<ChessColor> {
        self.chess().and_then(|chess| chess.state.color_of(user_id))
    }

    pub fn is_active(&self) -> bool {
        self.row.status == DailyMatch::STATUS_ACTIVE
    }
}

impl DailyState {
    pub(crate) fn new(svc: DailyService, user_id: Uuid, notifier: Notifier) -> Self {
        let snapshot_rx = svc.subscribe_snapshot();
        let snapshot = snapshot_rx.borrow().clone();
        let event_rx = svc.subscribe_events();
        // Challenges that predate the session don't glow; only ones posted
        // while connected count as news.
        let seen_open_ids = snapshot
            .open_challenges
            .iter()
            .map(|challenge| challenge.id)
            .collect();
        Self {
            user_id,
            svc,
            snapshot_rx,
            snapshot,
            event_rx,
            selected: 0,
            confirm_claim: None,
            challenge_draft: None,
            seen_open_ids,
            lobby_glow: false,
            notifier,
            turn_notified_match_ids: HashSet::new(),
            turn_notify_seeded: false,
            board: None,
        }
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    pub fn lobby_glow(&self) -> bool {
        self.lobby_glow
    }

    /// Drain the snapshot watch, the event feed, and any board load in
    /// flight. Returns a banner for events targeted at this user.
    pub fn tick(&mut self) -> Option<Banner> {
        let mut banner = None;
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            self.refresh_lobby_glow();
            self.notify_turn_edges();
            self.clamp_selection();
        }
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => {
                    if let Some(b) = self.apply_event(event) {
                        banner = Some(b);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "daily event feed lagged");
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        self.poll_board_load();
        banner
    }

    fn apply_event(&mut self, event: DailyEvent) -> Option<Banner> {
        match event {
            DailyEvent::Error { user_id, message } if user_id == self.user_id => {
                // A rejected action (a refused optimistic move, an expired
                // turn) leaves an open board desynced from the DB; reload it so
                // the optimistic state is discarded and input works again.
                if self.board.is_some() {
                    self.request_board_reload();
                }
                // svc errors are lowercase; the banner keeps sentence case.
                Some(Banner::error(&format!("Daily games: {message}")))
            }
            DailyEvent::ChallengePosted {
                game,
                challenger_id,
                target_username,
                ..
            } if challenger_id == self.user_id => Some(match target_username {
                Some(name) => {
                    Banner::success(&format!("Daily {} challenge sent to @{name}", game.label()))
                }
                None => Banner::success(&format!(
                    "Daily {} challenge posted to the lobby",
                    game.label()
                )),
            }),
            DailyEvent::MatchFinished {
                match_id,
                game,
                winner_user_id,
                ..
            } => {
                if self.board.as_ref().is_some_and(|b| b.match_id == match_id) {
                    self.request_board_reload();
                }
                (winner_user_id == Some(self.user_id)).then(|| {
                    Banner::success(&format!(
                        "Daily {}: you won the match (+{} chips)",
                        game.label(),
                        game.win_payout()
                    ))
                })
            }
            DailyEvent::MovePlayed { match_id, .. }
            | DailyEvent::ChallengeClaimed { match_id, .. } => {
                if self.board.as_ref().is_some_and(|b| b.match_id == match_id) {
                    self.request_board_reload();
                }
                None
            }
            _ => None,
        }
    }

    fn refresh_lobby_glow(&mut self) {
        let open_ids: HashSet<Uuid> = self
            .snapshot
            .open_challenges
            .iter()
            .map(|challenge| challenge.id)
            .collect();
        // Own challenges never glow; mark them seen immediately.
        let own_ids: Vec<Uuid> = self
            .snapshot
            .open_challenges
            .iter()
            .filter(|challenge| challenge.challenger_id == self.user_id)
            .map(|challenge| challenge.id)
            .collect();
        self.seen_open_ids.extend(own_ids);
        if self
            .snapshot
            .open_challenges
            .iter()
            .any(|challenge| !self.seen_open_ids.contains(&challenge.id))
        {
            self.lobby_glow = true;
        }
        // Drop ids that left the lobby so the set can't grow unbounded.
        self.seen_open_ids.retain(|id| open_ids.contains(id));
    }

    /// Push one desktop notification per match that just became this user's
    /// turn while connected.
    fn notify_turn_edges(&mut self) {
        let my_turn_ids: Vec<Uuid> = self
            .snapshot
            .active_matches
            .iter()
            .filter(|item| item.turn_user_id == Some(self.user_id))
            .map(|item| item.id)
            .collect();
        // First snapshot only establishes the baseline: everything currently
        // on this user's turn is treated as already notified, so login is
        // silent even if the construction snapshot was the empty default.
        if !self.turn_notify_seeded {
            self.turn_notify_seeded = true;
            self.turn_notified_match_ids = my_turn_ids.into_iter().collect();
            return;
        }
        for match_id in fresh_turn_edges(&mut self.turn_notified_match_ids, &my_turn_ids) {
            let item = self
                .snapshot
                .active_matches
                .iter()
                .find(|item| item.id == match_id);
            let opponent = item
                .and_then(|item| self.opponent_of(item).1)
                .unwrap_or_else(|| "player".to_string());
            let game = item.map(|item| item.game.label()).unwrap_or("games");
            self.notifier
                .push(Notification::daily_your_turn(game, &opponent));
        }
    }

    /// Called when the modal opens: the lobby has been looked at.
    pub fn mark_lobby_seen(&mut self) {
        self.seen_open_ids = self
            .snapshot
            .open_challenges
            .iter()
            .map(|challenge| challenge.id)
            .collect();
        self.lobby_glow = false;
        self.clamp_selection();
    }

    // ── Snapshot views ─────────────────────────────────────────

    /// This user's active matches: your-turn first, then nearest deadline.
    pub fn my_matches(&self) -> Vec<&DailyMatchItem> {
        let mut matches: Vec<&DailyMatchItem> = self
            .snapshot
            .active_matches
            .iter()
            .filter(|item| item.challenger_id == self.user_id || item.opponent_id == self.user_id)
            .collect();
        matches.sort_by_key(|item| {
            (
                item.turn_user_id != Some(self.user_id),
                item.turn_deadline_at,
                item.id,
            )
        });
        matches
    }

    /// Every open challenge, oldest first (snapshot order).
    pub fn lobby(&self) -> Vec<&DailyChallengeItem> {
        self.snapshot.open_challenges.iter().collect()
    }

    /// Active matches you're not playing in and may watch read-only, nearest
    /// deadline first. Battleship spectators see only the public hit/miss
    /// record, never the fleets (see `battleship_ui`).
    pub fn live_games(&self) -> Vec<&DailyMatchItem> {
        let mut matches: Vec<&DailyMatchItem> = self
            .snapshot
            .active_matches
            .iter()
            .filter(|item| item.challenger_id != self.user_id && item.opponent_id != self.user_id)
            .collect();
        matches.sort_by_key(|item| (item.turn_deadline_at, item.id));
        matches
    }

    /// Open challenges + active matches counted against the per-user cap.
    pub fn entry_count(&self) -> usize {
        let challenges = self
            .snapshot
            .open_challenges
            .iter()
            .filter(|challenge| challenge.challenger_id == self.user_id)
            .count();
        challenges + self.my_matches().len()
    }

    pub fn entry_cap(&self) -> usize {
        DAILY_MAX_ACTIVE_ENTRIES as usize
    }

    pub fn opponent_of(&self, item: &DailyMatchItem) -> (Uuid, Option<String>) {
        if item.challenger_id == self.user_id {
            (item.opponent_id, item.opponent_username.clone())
        } else {
            (item.challenger_id, item.challenger_username.clone())
        }
    }

    pub fn my_turn(&self, item: &DailyMatchItem) -> bool {
        item.turn_user_id == Some(self.user_id)
    }

    // ── Modal navigation ───────────────────────────────────────

    pub fn modal_entry_count(&self) -> usize {
        self.my_matches().len() + self.lobby().len() + self.live_games().len()
    }

    pub fn modal_entry_at(&self, index: usize) -> Option<DailyModalEntry<'_>> {
        let matches = self.my_matches();
        if index < matches.len() {
            return Some(DailyModalEntry::Match(matches[index]));
        }
        let index = index - matches.len();
        let lobby = self.lobby();
        if index < lobby.len() {
            return Some(DailyModalEntry::Challenge(lobby[index]));
        }
        self.live_games()
            .get(index - lobby.len())
            .copied()
            .map(DailyModalEntry::Spectate)
    }

    pub fn selected_entry(&self) -> Option<DailyModalEntry<'_>> {
        self.modal_entry_at(self.selected)
    }

    pub fn move_selection(&mut self, delta: isize) {
        let count = self.modal_entry_count();
        if count == 0 {
            self.selected = 0;
            return;
        }
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, count as isize - 1) as usize;
        self.confirm_claim = None;
    }

    fn clamp_selection(&mut self) {
        let count = self.modal_entry_count();
        if count == 0 {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(count - 1);
        }
        if let Some(pending) = self.confirm_claim
            && !self
                .snapshot
                .open_challenges
                .iter()
                .any(|challenge| challenge.id == pending)
        {
            self.confirm_claim = None;
        }
    }

    // ── Modal actions ──────────────────────────────────────────

    pub fn post_open_challenge(&self, game: DailyGame) {
        self.svc.post_challenge_task(self.user_id, game, None);
    }

    pub fn post_directed_challenge(&self, username: &str, game: DailyGame) {
        let username = username.trim().trim_start_matches('@').to_string();
        if username.is_empty() {
            return;
        }
        self.svc
            .post_challenge_to_username_task(self.user_id, game, username);
    }

    /// `c` / `C` in the modal: open the challenge picker overlay.
    pub fn begin_challenge_draft(&mut self, directed: bool) {
        self.confirm_claim = None;
        self.challenge_draft = Some(ChallengeDraft {
            selected: 0,
            directed,
            username: None,
        });
    }

    /// Move the picker cursor; ignored while the username prompt is active.
    pub fn draft_move_selection(&mut self, delta: isize) {
        if let Some(draft) = &mut self.challenge_draft
            && draft.username.is_none()
        {
            let max = DailyGame::ALL.len() as isize - 1;
            draft.selected = (draft.selected as isize + delta).clamp(0, max) as usize;
        }
    }

    /// Enter on the draft: post an open challenge, advance a directed draft
    /// to its username step, or send it. An empty username is a no-op so a
    /// stray Enter can't fire a challenge at nobody.
    pub fn draft_advance(&mut self) {
        let Some(draft) = &mut self.challenge_draft else {
            return;
        };
        match &draft.username {
            None if draft.directed => draft.username = Some(String::new()),
            None => {
                let game = draft.game();
                self.challenge_draft = None;
                self.post_open_challenge(game);
            }
            Some(username) => {
                if username.trim().trim_start_matches('@').is_empty() {
                    return;
                }
                let game = draft.game();
                let username = username.clone();
                self.challenge_draft = None;
                self.post_directed_challenge(&username, game);
            }
        }
    }

    /// Esc on the draft: the username step falls back to the picker, the
    /// picker closes the draft.
    pub fn draft_back(&mut self) {
        let Some(draft) = &mut self.challenge_draft else {
            return;
        };
        if draft.username.take().is_none() {
            self.challenge_draft = None;
        }
    }

    pub fn claim_challenge(&mut self, match_id: Uuid) {
        self.svc.claim_challenge_task(self.user_id, match_id);
        self.confirm_claim = None;
    }

    pub fn cancel_challenge(&self, match_id: Uuid) {
        self.svc.cancel_challenge_task(self.user_id, match_id);
    }

    // ── Board screen ───────────────────────────────────────────

    pub fn open_board(&mut self, item: &DailyMatchItem, return_screen: Screen) {
        let mut names = HashMap::new();
        if let Some(name) = &item.challenger_username {
            names.insert(item.challenger_id, name.clone());
        }
        if let Some(name) = &item.opponent_username {
            names.insert(item.opponent_id, name.clone());
        }
        // You're a spectator unless you're one of the two players.
        let spectating = item.challenger_id != self.user_id && item.opponent_id != self.user_id;
        self.board = Some(DailyBoardState {
            match_id: item.id,
            spectating,
            return_screen,
            // Start the cursor mid-board for each game's grid.
            cursor: match item.game {
                DailyGame::Chess => 12,
                DailyGame::Battleship => 44,
                // The connect4 cursor is a column, not a cell.
                DailyGame::ConnectFour => 3,
            },
            selected: None,
            piece_render_mode: ChessPieceRenderMode::Graphics,
            resign_confirm: false,
            detail: None,
            load_error: None,
            load_rx: None,
            reload_pending: false,
            names,
            board_geometry: Cell::new(None),
            target_geometry: Cell::new(None),
        });
        self.request_board_reload();
    }

    pub fn close_board(&mut self) {
        self.board = None;
    }

    fn request_board_reload(&mut self) {
        let Some(board) = &mut self.board else {
            return;
        };
        if board.load_rx.is_some() {
            board.reload_pending = true;
            return;
        }
        board.reload_pending = false;
        let (tx, rx) = oneshot::channel();
        let svc = self.svc.clone();
        let match_id = board.match_id;
        tokio::spawn(async move {
            let result = svc
                .load_match(match_id)
                .await
                .map_err(|e| e.root_cause().to_string());
            let _ = tx.send(result);
        });
        board.load_rx = Some(rx);
    }

    fn poll_board_load(&mut self) {
        let Some(board) = &mut self.board else {
            return;
        };
        let Some(rx) = &mut board.load_rx else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(Some(row))) => {
                board.load_rx = None;
                match DailyMatchDetail::from_row(row) {
                    Ok(detail) => {
                        board.detail = Some(detail);
                        board.load_error = None;
                        self.drop_stale_board_selection();
                    }
                    Err(message) => board.load_error = Some(message),
                }
            }
            Ok(Ok(None)) => {
                board.load_rx = None;
                board.load_error = Some("match not found".to_string());
            }
            Ok(Err(message)) => {
                board.load_rx = None;
                board.load_error = Some(message);
            }
            Err(oneshot::error::TryRecvError::Empty) => {}
            Err(oneshot::error::TryRecvError::Closed) => {
                board.load_rx = None;
            }
        }
        if self
            .board
            .as_ref()
            .is_some_and(|board| board.load_rx.is_none() && board.reload_pending)
        {
            self.request_board_reload();
        }
    }

    pub fn board_orientation(&self) -> ChessColor {
        self.board
            .as_ref()
            .and_then(|board| board.detail.as_ref())
            .and_then(|detail| detail.color_of(self.user_id))
            .unwrap_or(ChessColor::White)
    }

    pub fn board_legal_targets(&self) -> Vec<usize> {
        let Some(board) = &self.board else {
            return Vec::new();
        };
        let Some(chess) = board.detail.as_ref().and_then(DailyMatchDetail::chess) else {
            return Vec::new();
        };
        cursor::legal_targets(&chess.legal_moves, board.selected)
    }

    pub fn board_move_cursor(&mut self, dx: isize, dy: isize) {
        let orientation = self.board_orientation();
        let Some(board) = &mut self.board else {
            return;
        };
        board.resign_confirm = false;
        match board.detail.as_ref().map(|detail| &detail.game) {
            Some(DailyGameDetail::Battleship(_)) => {
                // Target grid: row 0 is drawn at the top, so "up" (dy=1)
                // moves toward row 0. No orientation flip.
                let col = (board.cursor % super::battleship::GRID) as isize + dx;
                let row = (board.cursor / super::battleship::GRID) as isize - dy;
                let max = super::battleship::GRID as isize - 1;
                board.cursor = (row.clamp(0, max) * (max + 1) + col.clamp(0, max)) as usize;
            }
            Some(DailyGameDetail::Connect4(_)) => {
                // One-dimensional: the cursor slides along the columns and
                // gravity does the rest.
                let max = super::connect4::COLS as isize - 1;
                board.cursor = (board.cursor as isize + dx).clamp(0, max) as usize;
            }
            _ => {
                board.cursor = cursor::move_cursor(board.cursor, orientation, dx, dy);
            }
        }
    }

    pub fn board_click_square(&mut self, index: usize) {
        if index >= 64 {
            return;
        }
        if let Some(board) = &mut self.board {
            board.cursor = index;
        }
        self.board_select_or_move();
    }

    /// Mouse on the battleship target grid (a cell) or the connect4 board
    /// (a column): aim there and play.
    pub fn board_click_target(&mut self, cell: usize) {
        if cell >= super::battleship::CELLS {
            return;
        }
        if let Some(board) = &mut self.board {
            board.cursor = cell;
        }
        self.board_select_or_move();
    }

    /// Space/Enter on the board. Chess: pick up a piece or play the move.
    /// Battleship: fire at the cursor. Connect4: drop into the cursor column.
    /// All apply optimistically; the canonical row arrives on the next
    /// `MovePlayed`/`MatchFinished` reload.
    pub fn board_select_or_move(&mut self) {
        let user_id = self.user_id;
        let svc = self.svc.clone();
        let Some(board) = &mut self.board else {
            return;
        };
        board.resign_confirm = false;
        let Some(detail) = &board.detail else {
            return;
        };
        if !detail.is_active() || detail.row.turn_user_id != Some(user_id) {
            return;
        }
        // Copy the game kind out first: the per-game handlers need `board`
        // whole, and matching the roster enum keeps this exhaustive.
        match detail.game.kind() {
            DailyGame::Chess => Self::chess_select_or_move(board, user_id, &svc),
            DailyGame::Battleship => Self::battleship_fire(board, user_id, &svc),
            DailyGame::ConnectFour => Self::connect4_drop(board, user_id, &svc),
        }
    }

    fn chess_select_or_move(board: &mut DailyBoardState, user_id: Uuid, svc: &DailyService) {
        let detail = board.detail.as_mut().expect("checked by caller");
        if detail.color_of(user_id) != detail.chess().map(|chess| chess.turn) {
            return;
        }
        let Some(chess) = detail.chess_mut() else {
            return;
        };
        let my_color = chess.state.color_of(user_id);
        if let Some(from) = board.selected {
            if from == board.cursor {
                board.selected = None;
                return;
            }
            let to = board.cursor;
            if chess
                .legal_moves
                .iter()
                .any(|mv| mv.from == from && mv.to == to)
            {
                Self::apply_optimistic_move(detail, from, to);
                board.selected = None;
                svc.play_move_task(user_id, board.match_id, from, to);
                return;
            }
            // Not a legal destination for the current selection: if it's
            // another piece of ours, switch the selection to it instead of
            // silently ignoring the click.
            let reselect = chess.pieces.get(to).and_then(|piece| *piece).is_some_and(
                |piece| Some(piece.color) == my_color && chess.legal_moves.iter().any(|mv| mv.from == to),
            );
            board.selected = if reselect { Some(to) } else { None };
            return;
        }

        let Some(piece) = chess.pieces.get(board.cursor).and_then(|piece| *piece) else {
            return;
        };
        if Some(piece.color) == my_color
            && chess.legal_moves.iter().any(|mv| mv.from == board.cursor)
        {
            board.selected = Some(board.cursor);
        }
    }

    /// Fire at the cursor cell. The shot applies optimistically (both fleets
    /// live in session memory, so hit/miss is known locally); `shot_in_flight`
    /// blocks a second salvo until the reload reconciles.
    fn battleship_fire(board: &mut DailyBoardState, user_id: Uuid, svc: &DailyService) {
        let detail = board.detail.as_mut().expect("checked by caller");
        let row_turn = detail.row.turn_user_id;
        let DailyGameDetail::Battleship(battleship) = &mut detail.game else {
            return;
        };
        if battleship.shot_in_flight || row_turn != Some(user_id) {
            return;
        }
        let Some(shooter) = battleship.state.side_index_of(user_id) else {
            return;
        };
        let cell = board.cursor;
        let Ok(outcome) = battleship.state.apply_shot(shooter, cell, Utc::now()) else {
            return; // already fired there — a silent no-op, like an illegal chess move
        };
        battleship.shot_in_flight = true;
        if !outcome.hit {
            let opponent = DailyBattleshipState::opponent_index(shooter);
            detail.row.turn_user_id = Some(battleship.state.side(opponent).user_id);
        }
        svc.play_move_task(user_id, board.match_id, cell, cell);
    }

    /// Drop into the cursor column. The drop applies optimistically (nothing
    /// is hidden in connect4, so the landing spot is known locally);
    /// `drop_in_flight` blocks a second disc until the reload reconciles.
    fn connect4_drop(board: &mut DailyBoardState, user_id: Uuid, svc: &DailyService) {
        let detail = board.detail.as_mut().expect("checked by caller");
        let row_turn = detail.row.turn_user_id;
        let DailyGameDetail::Connect4(connect4) = &mut detail.game else {
            return;
        };
        if connect4.drop_in_flight || row_turn != Some(user_id) {
            return;
        }
        let Some(disc) = connect4.state.disc_of(user_id) else {
            return;
        };
        if connect4.state.turn() != disc {
            return;
        }
        let column = board.cursor;
        if connect4.state.apply_drop(column).is_err() {
            return; // full column — a silent no-op, like an illegal chess move
        }
        connect4.drop_in_flight = true;
        // The turn always passes; wins and draws wait for the reload.
        detail.row.turn_user_id = Some(connect4.state.user_of(disc.other()));
        svc.play_move_task(user_id, board.match_id, column, column);
    }

    fn apply_optimistic_move(detail: &mut DailyMatchDetail, from: usize, to: usize) {
        let Some(chess) = detail.chess_mut() else {
            return;
        };
        let Ok(board) = chess.state.fen.parse::<Board>() else {
            return;
        };
        let Some(mv) = rules::legal_move_for(&board, from, to) else {
            return;
        };
        let label = rules::san_label(&board, mv);
        let mut board = board;
        board.play(mv);
        chess.state.fen = format!("{board}");
        chess.state.move_history.push(super::svc::DailyMoveRecord {
            from,
            to,
            label,
            at: Utc::now(),
        });
        chess.pieces = rules::board_pieces(&board);
        chess.turn = rules::chess_color(board.side_to_move());
        chess.in_check = !board.checkers().is_empty();
        // Opponent to move until the reload says otherwise; clearing the
        // legal moves keeps the cursor from picking up their pieces.
        chess.legal_moves.clear();
        let next = chess.state.user_for_color(chess.turn);
        detail.row.turn_user_id = Some(next);
    }

    pub fn board_resign(&mut self) {
        let user_id = self.user_id;
        let svc = self.svc.clone();
        let Some(board) = &mut self.board else {
            return;
        };
        // A spectator has nothing to resign; the service would reject it too.
        if board.spectating {
            return;
        }
        let active = board
            .detail
            .as_ref()
            .is_some_and(DailyMatchDetail::is_active);
        if !active {
            board.resign_confirm = false;
            return;
        }
        if board.resign_confirm {
            board.resign_confirm = false;
            svc.resign_task(user_id, board.match_id);
        } else {
            board.resign_confirm = true;
        }
    }

    fn drop_stale_board_selection(&mut self) {
        let Some(board) = &mut self.board else {
            return;
        };
        let Some(detail) = &board.detail else {
            return;
        };
        let selectable = |from: usize| {
            detail
                .chess()
                .is_some_and(|chess| chess.legal_moves.iter().any(|mv| mv.from == from))
        };
        if let Some(selected) = board.selected
            && (!detail.is_active() || !selectable(selected))
        {
            board.selected = None;
        }
    }
}

/// Update the notified set against the matches currently on this user's
/// turn and return the ids that just appeared (the became-my-turn edges).
/// Dropping ids whose turn passed back to the opponent means a later flip
/// to this user notifies again.
fn fresh_turn_edges(notified: &mut HashSet<Uuid>, my_turn_ids: &[Uuid]) -> Vec<Uuid> {
    notified.retain(|id| my_turn_ids.contains(id));
    my_turn_ids
        .iter()
        .copied()
        .filter(|id| notified.insert(*id))
        .collect()
}

/// Compact time-until-deadline: `2d 3h`, `23h 59m`, `41m`. Clamps at zero.
pub fn format_deadline(deadline: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (deadline - now).num_seconds().max(0);
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3600;
    let minutes = (secs % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn format_deadline_scales_units() {
        let now = Utc.with_ymd_and_hms(2026, 7, 8, 12, 0, 0).unwrap();
        assert_eq!(
            format_deadline(now + chrono::Duration::hours(50), now),
            "2d 2h"
        );
        assert_eq!(
            format_deadline(now + chrono::Duration::minutes(90), now),
            "1h 30m"
        );
        assert_eq!(
            format_deadline(now + chrono::Duration::minutes(41), now),
            "41m"
        );
        assert_eq!(format_deadline(now - chrono::Duration::hours(1), now), "0m");
    }

    #[test]
    fn fresh_turn_edges_notifies_each_became_my_turn_edge_once() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut notified = HashSet::from([a]);

        // Already-notified id stays quiet; a new my-turn match is an edge.
        assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), vec![b]);
        assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), Vec::<Uuid>::new());

        // Turn passes to the opponent and comes back: a fresh edge.
        assert_eq!(fresh_turn_edges(&mut notified, &[b]), Vec::<Uuid>::new());
        assert_eq!(fresh_turn_edges(&mut notified, &[a, b]), vec![a]);

        // Finished matches fall out of the set.
        assert_eq!(fresh_turn_edges(&mut notified, &[]), Vec::<Uuid>::new());
        assert!(notified.is_empty());
    }
}
