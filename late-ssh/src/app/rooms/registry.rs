use serde_json::Value;
use uuid::Uuid;

use super::{
    asterion::manager::AsterionRoomManager,
    backend::{
        ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
        RoomGameManager,
    },
    blackjack::manager::BlackjackTableManager,
    chess::{manager::ChessTableManager, svc as chess_svc},
    poker::manager::PokerTableManager,
    sshattrick::manager::SshattrickRoomManager,
    svc::{GameKind, RoomListItem},
    tictactoe::manager::TicTacToeTableManager,
    tron::manager::TronTableManager,
};

#[derive(Clone, Debug)]
pub struct RoomDirectorySummary {
    pub game_label: &'static str,
    pub occupied_seats: Option<usize>,
    pub total_seats: usize,
    pub pace: String,
    pub stakes: String,
}

#[derive(Clone)]
pub struct RoomGameRegistry {
    asterion: AsterionRoomManager,
    blackjack: BlackjackTableManager,
    chess: ChessTableManager,
    poker: PokerTableManager,
    sshattrick: SshattrickRoomManager,
    tictactoe: TicTacToeTableManager,
    tron: TronTableManager,
}

impl RoomGameRegistry {
    pub fn new(
        asterion: AsterionRoomManager,
        blackjack: BlackjackTableManager,
        chess: ChessTableManager,
        poker: PokerTableManager,
        sshattrick: SshattrickRoomManager,
        tictactoe: TicTacToeTableManager,
        tron: TronTableManager,
    ) -> Self {
        Self {
            asterion,
            blackjack,
            chess,
            poker,
            sshattrick,
            tictactoe,
            tron,
        }
    }

    pub fn manager(&self, kind: GameKind) -> &dyn RoomGameManager {
        match kind {
            GameKind::Asterion => &self.asterion,
            GameKind::Blackjack => &self.blackjack,
            GameKind::Chess => &self.chess,
            GameKind::Poker => &self.poker,
            GameKind::Sshattrick => &self.sshattrick,
            GameKind::TicTacToe => &self.tictactoe,
            GameKind::Tron => &self.tron,
        }
    }

    pub fn ordered_kinds(&self) -> &'static [GameKind] {
        &GameKind::ALL
    }

    pub fn label(&self, kind: GameKind) -> &'static str {
        self.manager(kind).label()
    }

    pub fn slug_prefix(&self, kind: GameKind) -> &'static str {
        self.manager(kind).slug_prefix()
    }

    pub fn default_room_name(&self, kind: GameKind) -> &'static str {
        self.manager(kind).default_room_name()
    }

    pub fn default_settings(&self, kind: GameKind) -> Value {
        self.manager(kind).default_settings()
    }

    pub fn open_create_modal(&self, kind: GameKind) -> Box<dyn CreateRoomModal> {
        self.manager(kind).open_create_modal()
    }

    pub fn directory_meta(&self, room: &RoomListItem) -> DirectoryMeta {
        self.manager(room.game_kind).directory_meta(room)
    }

    pub fn directory_hints(&self, room_id: Uuid, kind: GameKind) -> Option<DirectoryHints> {
        self.manager(kind).directory_hints(room_id)
    }

    pub fn is_user_seated(&self, room: &RoomListItem, user_id: Uuid) -> bool {
        self.manager(room.game_kind)
            .is_user_seated(room.id, user_id)
            || matches!(room.game_kind, GameKind::Chess)
                && chess_svc::runtime_state_has_seated_user(&room.runtime_state, user_id)
    }

    pub fn subscribe_room_events(
        &self,
        kind: GameKind,
    ) -> tokio::sync::broadcast::Receiver<RoomGameEvent> {
        self.manager(kind).subscribe_room_events()
    }

    pub fn start_dashboard_room_join_feed_task(
        &self,
        tx: crate::app::dashboard::state::DashboardRoomJoinSender,
    ) {
        for kind in self.ordered_kinds().iter().copied() {
            let mut rx = self.manager(kind).subscribe_room_events();
            let tx = tx.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(event @ RoomGameEvent::SeatJoined { .. }) => {
                            let _ = tx.send(
                                crate::app::dashboard::state::DashboardRoomJoin::from_room_event(
                                    event,
                                ),
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                kind = kind.as_str(),
                                skipped,
                                "dashboard room-join feed lagged"
                            );
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }
    }

    pub fn directory_summary(&self, room: &RoomListItem) -> RoomDirectorySummary {
        let meta = self.directory_meta(room);
        let hints = self
            .directory_hints(room.id, room.game_kind)
            .or_else(|| persisted_directory_hints(room));
        RoomDirectorySummary {
            game_label: self.label(room.game_kind),
            occupied_seats: hints.as_ref().map(|hints| hints.occupied),
            total_seats: hints
                .as_ref()
                .map(|hints| hints.total)
                .unwrap_or(meta.seats as usize),
            pace: meta.pace,
            stakes: meta.stakes,
        }
    }

    pub fn enter(
        &self,
        room: &RoomListItem,
        user_id: Uuid,
        chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        self.manager(room.game_kind)
            .enter(room, user_id, chip_balance)
    }

    pub fn blackjack(&self) -> &BlackjackTableManager {
        &self.blackjack
    }
}

fn persisted_directory_hints(room: &RoomListItem) -> Option<DirectoryHints> {
    match room.game_kind {
        GameKind::Chess => chess_svc::runtime_state_occupied_seats(&room.runtime_state)
            .map(|occupied| DirectoryHints { occupied, total: 2 }),
        _ => None,
    }
}
