use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::rooms::{
    backend::RoomGameEvent,
    chess::svc as chess_svc,
    svc::{GameKind, RoomsSnapshot},
};

pub(crate) const DASHBOARD_RECENT_ROOM_JOIN_LIMIT: usize = 8;
pub type DashboardRoomJoinSender = broadcast::Sender<DashboardRoomJoin>;
pub type DashboardRoomJoinReceiver = broadcast::Receiver<DashboardRoomJoin>;
pub type DashboardRoomJoinHistory = Arc<Mutex<VecDeque<DashboardRoomJoin>>>;

#[derive(Clone, Debug)]
pub struct DashboardRoomJoin {
    pub room_id: Uuid,
    pub user_id: Uuid,
}

impl DashboardRoomJoin {
    pub fn from_room_event(event: RoomGameEvent) -> Self {
        match event {
            RoomGameEvent::SeatJoined {
                room_id, user_id, ..
            } => Self { room_id, user_id },
        }
    }
}

pub fn push_recent_room_join(joins: &mut VecDeque<DashboardRoomJoin>, join: DashboardRoomJoin) {
    joins.retain(|existing| existing.room_id != join.room_id);
    joins.push_front(join);
    while joins.len() > DASHBOARD_RECENT_ROOM_JOIN_LIMIT {
        joins.pop_back();
    }
}

pub(crate) fn seed_persisted_room_joins_from_rooms(
    joins: &mut VecDeque<DashboardRoomJoin>,
    snapshot: &RoomsSnapshot,
) {
    let mut seeded = VecDeque::new();
    for room in &snapshot.rooms {
        if joins.iter().any(|join| join.room_id == room.id) {
            continue;
        }
        let Some(user_id) = persisted_recent_join_user_id(room) else {
            continue;
        };
        push_recent_room_join(
            &mut seeded,
            DashboardRoomJoin {
                room_id: room.id,
                user_id,
            },
        );
    }
    for join in joins.drain(..).rev() {
        push_recent_room_join(&mut seeded, join);
    }
    *joins = seeded;
}

fn persisted_recent_join_user_id(room: &crate::app::rooms::svc::RoomListItem) -> Option<Uuid> {
    match room.game_kind {
        GameKind::Chess => chess_svc::runtime_state_seated_user_ids(&room.runtime_state)
            .into_iter()
            .last(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn join(room_id: Uuid, user_id: Uuid) -> DashboardRoomJoin {
        DashboardRoomJoin { room_id, user_id }
    }

    fn room(
        id: Uuid,
        game_kind: GameKind,
        runtime_state: serde_json::Value,
    ) -> crate::app::rooms::svc::RoomListItem {
        crate::app::rooms::svc::RoomListItem {
            id,
            chat_room_id: Uuid::now_v7(),
            game_kind,
            slug: format!("{}-test", game_kind.as_str()),
            display_name: "Test Room".to_string(),
            status: "open".to_string(),
            settings: json!({}),
            runtime_state,
            created_by: None,
            created_by_username: None,
        }
    }

    #[test]
    fn push_recent_room_join_moves_existing_room_to_front() {
        let room = Uuid::now_v7();
        let other_room = Uuid::now_v7();
        let user_a = Uuid::now_v7();
        let user_b = Uuid::now_v7();
        let mut joins = VecDeque::new();

        push_recent_room_join(&mut joins, join(room, user_a));
        push_recent_room_join(&mut joins, join(other_room, user_a));
        push_recent_room_join(&mut joins, join(room, user_b));

        assert_eq!(joins.len(), 2);
        assert_eq!(joins[0].room_id, room);
        assert_eq!(joins[0].user_id, user_b);
        assert_eq!(joins[1].room_id, other_room);
    }

    #[test]
    fn push_recent_room_join_caps_feed_length() {
        let mut joins = VecDeque::new();

        for _ in 0..DASHBOARD_RECENT_ROOM_JOIN_LIMIT + 2 {
            push_recent_room_join(&mut joins, join(Uuid::now_v7(), Uuid::now_v7()));
        }

        assert_eq!(joins.len(), DASHBOARD_RECENT_ROOM_JOIN_LIMIT);
    }

    #[test]
    fn persisted_chess_seats_seed_recent_room_joins() {
        let room_id = Uuid::now_v7();
        let white = Uuid::now_v7();
        let black = Uuid::now_v7();
        let snapshot = RoomsSnapshot {
            rooms: vec![room(
                room_id,
                GameKind::Chess,
                json!({
                    "version": 1,
                    "seats": [white, black],
                }),
            )],
        };
        let mut joins = VecDeque::new();

        seed_persisted_room_joins_from_rooms(&mut joins, &snapshot);

        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].room_id, room_id);
        assert_eq!(joins[0].user_id, black);
    }

    #[test]
    fn persisted_room_join_seed_keeps_live_join_for_same_room() {
        let room_id = Uuid::now_v7();
        let persisted = Uuid::now_v7();
        let live = Uuid::now_v7();
        let snapshot = RoomsSnapshot {
            rooms: vec![room(
                room_id,
                GameKind::Chess,
                json!({
                    "version": 1,
                    "seats": [persisted, null],
                }),
            )],
        };
        let mut joins = VecDeque::from([join(room_id, live)]);

        seed_persisted_room_joins_from_rooms(&mut joins, &snapshot);

        assert_eq!(joins.len(), 1);
        assert_eq!(joins[0].room_id, room_id);
        assert_eq!(joins[0].user_id, live);
    }
}
