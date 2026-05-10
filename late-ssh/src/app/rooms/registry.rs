use serde_json::Value;
use uuid::Uuid;

use crate::app::chat::svc::ChatService;

use super::{
    backend::{
        ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
        RoomGameManager,
    },
    blackjack::manager::BlackjackTableManager,
    poker::manager::PokerTableManager,
    svc::{GameKind, RoomListItem},
    tictactoe::manager::TicTacToeTableManager,
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
    blackjack: BlackjackTableManager,
    poker: PokerTableManager,
    tictactoe: TicTacToeTableManager,
}

impl RoomGameRegistry {
    pub fn new(
        blackjack: BlackjackTableManager,
        poker: PokerTableManager,
        tictactoe: TicTacToeTableManager,
    ) -> Self {
        Self {
            blackjack,
            poker,
            tictactoe,
        }
    }

    pub fn manager(&self, kind: GameKind) -> &dyn RoomGameManager {
        match kind {
            GameKind::Blackjack => &self.blackjack,
            GameKind::Poker => &self.poker,
            GameKind::TicTacToe => &self.tictactoe,
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

    pub fn subscribe_room_events(
        &self,
        kind: GameKind,
    ) -> tokio::sync::broadcast::Receiver<RoomGameEvent> {
        self.manager(kind).subscribe_room_events()
    }

    pub fn start_general_seat_announcer_task(&self, chat_service: ChatService) {
        for kind in self.ordered_kinds().iter().copied() {
            let mut rx = self.subscribe_room_events(kind);
            let chat_service = chat_service.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(RoomGameEvent::SeatJoined {
                            user_id,
                            game_kind,
                            display_name,
                            seat_index,
                            ..
                        }) => {
                            let body = room_seat_announcement(game_kind, &display_name, seat_index);
                            chat_service.announce_general_task(user_id, body);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(
                                kind = kind.as_str(),
                                skipped,
                                "room game seat announcer lagged"
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
        let hints = self.directory_hints(room.id, room.game_kind);
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

fn room_seat_announcement(game_kind: GameKind, display_name: &str, seat_index: usize) -> String {
    let game_label = match game_kind {
        GameKind::Blackjack => "Blackjack",
        GameKind::Poker => "Poker",
        GameKind::TicTacToe => "Tic-Tac-Toe",
    };
    let display_name = display_name
        .split('\n')
        .next()
        .unwrap_or("")
        .trim()
        .replace('@', "at ");
    let display_name = if display_name.is_empty() {
        "table".to_string()
    } else {
        display_name
    };
    format!(
        "sat down at {}: {} (seat {})",
        game_label,
        display_name,
        seat_index + 1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seat_announcement_names_game_room_and_one_based_seat() {
        assert_eq!(
            room_seat_announcement(GameKind::Poker, "Night Table", 1),
            "sat down at Poker: Night Table (seat 2)"
        );
    }

    #[test]
    fn seat_announcement_neutralizes_mentions_from_room_names() {
        assert_eq!(
            room_seat_announcement(GameKind::Blackjack, "@admin table", 0),
            "sat down at Blackjack: at admin table (seat 1)"
        );
    }
}
