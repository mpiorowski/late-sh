use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use late_core::{
    MutexRecover,
    models::game_room::{ROOM_SEAT_MARKER, ROOM_SEAT_SEPARATOR},
};
use serde_json::Value;
use uuid::Uuid;

use crate::app::chat::svc::{ChatService, SendGeneralMessageTask};

use super::{
    backend::{
        ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
        RoomGameManager,
    },
    blackjack::manager::BlackjackTableManager,
    poker::manager::PokerTableManager,
    svc::{GameKind, RoomListItem, sanitize_room_display_name},
    tictactoe::manager::TicTacToeTableManager,
};

/// Window during which a repeat seat-announcement for the same
/// (user, room) is suppressed. Keeps reconnect/leave-rejoin storms
/// from spamming #general while still re-announcing later returns.
const SEAT_ANNOUNCE_DEDUPE_WINDOW: Duration = Duration::from_secs(60);

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
        let dedupe: Arc<Mutex<HashMap<(Uuid, Uuid), Instant>>> =
            Arc::new(Mutex::new(HashMap::new()));
        for kind in self.ordered_kinds().iter().copied() {
            let manager = self.manager(kind);
            let game_label = manager.label();
            let ascii = manager.seat_join_ascii();
            let mut rx = manager.subscribe_room_events();
            let chat_service = chat_service.clone();
            let dedupe = dedupe.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(RoomGameEvent::SeatJoined {
                            room_id,
                            user_id,
                            display_name,
                            meta,
                            ..
                        }) => {
                            if !should_announce_seat(&dedupe, user_id, room_id, Instant::now()) {
                                continue;
                            }
                            let body =
                                room_seat_announcement(game_label, &display_name, &meta, ascii);
                            chat_service.send_general_message_task(SendGeneralMessageTask {
                                user_id,
                                body,
                                request_id: None,
                                join_if_needed: true,
                                failure_log: "failed to announce room seat in general chat",
                            });
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

/// Build the chat-body payload that the chat renderer recognises and
/// turns into the boxed seat-joined card. Format mirrors the news
/// pattern: `MARKER title || meta || ascii_escaped` where `\n` in the
/// art is escaped so the body stays single-line on the wire.
fn room_seat_announcement(
    game_label: &str,
    display_name: &str,
    meta: &str,
    ascii_lines: &[&str],
) -> String {
    let room_name = sanitize_room_display_name(display_name);
    let title = if room_name.is_empty() {
        format!("{game_label} table")
    } else {
        format!("{game_label} · {room_name}")
    };
    let meta = sanitize_room_seat_field(meta);
    let ascii_escaped = ascii_lines.join("\\n");
    format!(
        "{marker} {title}{sep}{meta}{sep}{ascii}",
        marker = ROOM_SEAT_MARKER,
        title = title,
        sep = ROOM_SEAT_SEPARATOR,
        meta = meta,
        ascii = ascii_escaped,
    )
}

fn sanitize_room_seat_field(input: &str) -> String {
    input
        .replace(ROOM_SEAT_SEPARATOR, " | ")
        .replace('@', "＠")
        .replace(['\n', '\r'], " ")
        .trim()
        .to_string()
}

/// Returns true if this seat join should be announced. Updates the
/// dedupe map's last-seen timestamp for the (user, room) pair.
fn should_announce_seat(
    dedupe: &Arc<Mutex<HashMap<(Uuid, Uuid), Instant>>>,
    user_id: Uuid,
    room_id: Uuid,
    now: Instant,
) -> bool {
    let mut map = dedupe.lock_recover();
    if let Some(&last) = map.get(&(user_id, room_id))
        && now.duration_since(last) < SEAT_ANNOUNCE_DEDUPE_WINDOW
    {
        return false;
    }
    map.insert((user_id, room_id), now);
    // Cheap opportunistic prune to keep the map bounded.
    map.retain(|_, when| now.duration_since(*when) < SEAT_ANNOUNCE_DEDUPE_WINDOW);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seat_announcement_payload_is_marker_title_meta_ascii() {
        let ascii: &[&str] = &["╭───╮", "│A♠ │", "╰───╯"];
        assert_eq!(
            room_seat_announcement("Poker", "Night Table", "50/100 blinds · 30s/turn", ascii),
            "---ROOM-SEAT--- Poker · Night Table || 50/100 blinds · 30s/turn || ╭───╮\\n│A♠ │\\n╰───╯"
        );
    }

    #[test]
    fn seat_announcement_sanitizes_newlines_from_room_name() {
        let ascii: &[&str] = &[];
        assert_eq!(
            room_seat_announcement("Tic-Tac-Toe", "Quick\nBoard", "", ascii),
            "---ROOM-SEAT--- Tic-Tac-Toe · Quick Board ||  || "
        );
    }

    #[test]
    fn seat_announcement_falls_back_when_room_name_blank() {
        let ascii: &[&str] = &[];
        assert_eq!(
            room_seat_announcement("Blackjack", "   ", "10 chips", ascii),
            "---ROOM-SEAT--- Blackjack table || 10 chips || "
        );
    }

    #[test]
    fn seat_announcement_neutralizes_room_name_mentions() {
        let ascii: &[&str] = &[];
        let body = room_seat_announcement("Poker", "@everyone fall sale", "50/100", ascii);

        assert_eq!(
            body,
            "---ROOM-SEAT--- Poker · ＠everyone fall sale || 50/100 || "
        );
        assert!(!body.contains("@everyone"));
    }

    #[test]
    fn seat_announcement_sanitizes_separator_in_fields() {
        let ascii: &[&str] = &[];

        assert_eq!(
            room_seat_announcement("Poker", "Casual || Fun", "10 || fast", ascii),
            "---ROOM-SEAT--- Poker · Casual | Fun || 10 | fast || "
        );
    }

    #[test]
    fn dedupe_suppresses_repeats_within_window() {
        let dedupe = Arc::new(Mutex::new(HashMap::new()));
        let user = Uuid::now_v7();
        let room = Uuid::now_v7();
        let t0 = Instant::now();
        assert!(should_announce_seat(&dedupe, user, room, t0));
        assert!(!should_announce_seat(
            &dedupe,
            user,
            room,
            t0 + Duration::from_secs(30)
        ));
        assert!(should_announce_seat(
            &dedupe,
            user,
            room,
            t0 + SEAT_ANNOUNCE_DEDUPE_WINDOW + Duration::from_secs(1)
        ));
    }

    #[test]
    fn dedupe_is_per_user_per_room() {
        let dedupe = Arc::new(Mutex::new(HashMap::new()));
        let user_a = Uuid::now_v7();
        let user_b = Uuid::now_v7();
        let room_1 = Uuid::now_v7();
        let room_2 = Uuid::now_v7();
        let t0 = Instant::now();
        assert!(should_announce_seat(&dedupe, user_a, room_1, t0));
        assert!(should_announce_seat(&dedupe, user_b, room_1, t0));
        assert!(should_announce_seat(&dedupe, user_a, room_2, t0));
        assert!(!should_announce_seat(&dedupe, user_a, room_1, t0));
    }
}
