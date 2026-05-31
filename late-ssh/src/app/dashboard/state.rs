use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::rooms::backend::RoomGameEvent;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn join(room_id: Uuid, user_id: Uuid) -> DashboardRoomJoin {
        DashboardRoomJoin { room_id, user_id }
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
}
