use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::rooms::{
    backend::{
        ActiveRoomBackend, CreateRoomModal, DirectoryHints, DirectoryMeta, RoomGameEvent,
        RoomGameManager,
    },
    svc::{GameKind, RoomListItem},
};

#[derive(Clone)]
pub struct SshattrickRoomManager {
    event_tx: broadcast::Sender<RoomGameEvent>,
}

impl SshattrickRoomManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self { event_tx }
    }
}

impl Default for SshattrickRoomManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RoomGameManager for SshattrickRoomManager {
    fn kind(&self) -> GameKind {
        GameKind::Sshattrick
    }

    fn label(&self) -> &'static str {
        "ssHattrick"
    }

    fn slug_prefix(&self) -> &'static str {
        "sshattrick"
    }

    fn default_room_name(&self) -> &'static str {
        "Hockey Rink"
    }

    fn default_settings(&self) -> Value {
        Value::Object(serde_json::Map::new())
    }

    fn open_create_modal(&self) -> Box<dyn CreateRoomModal> {
        unimplemented!("sshattrick create modal arrives in a later milestone")
    }

    fn directory_meta(&self, _room: &RoomListItem) -> DirectoryMeta {
        unimplemented!("sshattrick directory_meta arrives in a later milestone")
    }

    fn directory_hints(&self, _room_id: Uuid) -> Option<DirectoryHints> {
        None
    }

    fn subscribe_room_events(&self) -> broadcast::Receiver<RoomGameEvent> {
        self.event_tx.subscribe()
    }

    fn seat_join_ascii(&self) -> &'static [&'static str] {
        &["╭───╮", "│ ⚒ │", "╰───╯"]
    }

    fn enter(
        &self,
        _room: &RoomListItem,
        _user_id: Uuid,
        _chip_balance: i64,
    ) -> Box<dyn ActiveRoomBackend> {
        unimplemented!("sshattrick enter() arrives in a later milestone")
    }
}
