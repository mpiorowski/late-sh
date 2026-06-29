use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use super::svc::RoomsEvent;
use crate::app::notify::Notification;
use crate::app::{common::primitives::Banner, state::App};

const TURN_NOTIFY_SCAN_INTERVAL: Duration = Duration::from_millis(500);

impl App {
    pub(crate) fn tick_rooms(&mut self) -> Option<Banner> {
        self.notify_game_turn();
        if self.rooms_snapshot_rx.has_changed().unwrap_or(false) {
            self.rooms_snapshot = self.rooms_snapshot_rx.borrow_and_update().clone();
            self.clamp_rooms_selection();
            self.refresh_active_room();
            self.prune_dashboard_room_joins();
            crate::app::dashboard::state::seed_persisted_room_joins_from_rooms(
                &mut self.dashboard_room_joins,
                &self.rooms_snapshot,
            );
        }
        self.drain_room_join_events();
        self.drain_rooms_events()
    }

    /// Push one "your turn" desktop notification per pending game action.
    fn notify_game_turn(&mut self) {
        let now = Instant::now();
        if self
            .rooms_last_turn_scan_at
            .is_some_and(|last| now.duration_since(last) < TURN_NOTIFY_SCAN_INTERVAL)
        {
            return;
        }
        self.rooms_last_turn_scan_at = Some(now);

        let awaiting_room_ids = self
            .rooms_snapshot
            .rooms
            .iter()
            .filter(|room| {
                self.room_game_registry
                    .is_awaiting_user_action(room, self.user_id)
            })
            .map(|room| room.id)
            .collect::<std::collections::HashSet<_>>();
        self.rooms_turn_notified_room_ids
            .retain(|room_id| awaiting_room_ids.contains(room_id));

        for room in self
            .rooms_snapshot
            .rooms
            .iter()
            .filter(|room| awaiting_room_ids.contains(&room.id))
        {
            if !self.rooms_turn_notified_room_ids.insert(room.id) {
                continue;
            }
            self.notifier.push(Notification::your_turn(
                self.room_game_registry.label(room.game_kind),
                &room.display_name,
            ));
        }
    }

    fn clamp_rooms_selection(&mut self) {
        let count = self.visible_real_rooms_count();
        if count == 0 {
            self.rooms_selected_index = 0;
        } else {
            self.rooms_selected_index = self.rooms_selected_index.min(count - 1);
        }
    }

    fn visible_real_rooms_count(&self) -> usize {
        let q = self.rooms_search_query.trim().to_lowercase();
        self.rooms_snapshot
            .rooms
            .iter()
            .filter(|room| self.rooms_filter.matches_real(room.game_kind))
            .filter(|room| q.is_empty() || room.display_name.to_lowercase().contains(&q))
            .count()
    }

    fn refresh_active_room(&mut self) {
        if let Some(active_id) = self.rooms_active_room.as_ref().map(|room| room.id) {
            let refreshed = self
                .rooms_snapshot
                .rooms
                .iter()
                .find(|room| room.id == active_id)
                .cloned();
            if refreshed.is_none()
                && self
                    .active_room_game
                    .as_ref()
                    .is_some_and(|game| game.room_id() == active_id)
            {
                self.active_room_game = None;
            }
            self.rooms_active_room = refreshed;
        }
        self.prune_deleted_active_room_game();
    }

    fn prune_deleted_active_room_game(&mut self) {
        let Some(room_id) = self.active_room_game.as_ref().map(|game| game.room_id()) else {
            return;
        };
        if !self
            .rooms_snapshot
            .rooms
            .iter()
            .any(|room| room.id == room_id)
        {
            self.active_room_game = None;
            self.rooms_turn_notified_room_ids.remove(&room_id);
        }
    }

    fn prune_dashboard_room_joins(&mut self) {
        self.dashboard_room_joins.retain(|join| {
            self.rooms_snapshot
                .rooms
                .iter()
                .any(|room| room.id == join.room_id)
        });
    }

    fn drain_room_join_events(&mut self) {
        let Some(rx) = &mut self.room_join_rx else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(join) => crate::app::dashboard::state::push_recent_room_join(
                    &mut self.dashboard_room_joins,
                    join,
                ),
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "dashboard room-join feed lagged");
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
    }

    fn drain_rooms_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.rooms_event_rx.try_recv() {
                Ok(event) => match event {
                    RoomsEvent::Created {
                        user_id,
                        game_kind,
                        display_name,
                    } if user_id == self.user_id => {
                        banner = Some(Banner::success(&format!(
                            "Created {} table: {}",
                            self.room_game_registry.label(game_kind),
                            display_name
                        )));
                    }
                    RoomsEvent::Deleted {
                        user_id,
                        display_name,
                    } if user_id == self.user_id => {
                        banner = Some(Banner::success(&format!("Deleted table: {}", display_name)));
                    }
                    RoomsEvent::Error {
                        user_id,
                        game_kind,
                        display_name,
                        message,
                    } if user_id == self.user_id => {
                        let table = if display_name.is_empty() {
                            "table".to_string()
                        } else {
                            format!("table: {display_name}")
                        };
                        banner = Some(Banner::error(&format!(
                            "Failed to create {} {}: {}",
                            self.room_game_registry.label(game_kind),
                            table,
                            message
                        )));
                    }
                    RoomsEvent::DeleteError {
                        user_id,
                        display_name,
                        message,
                    } if user_id == self.user_id => {
                        banner = Some(Banner::error(&format!(
                            "Failed to delete table {}: {}",
                            display_name, message
                        )));
                    }
                    RoomsEvent::EnterReady {
                        user_id,
                        request_id,
                        room,
                    } if user_id == self.user_id
                        && self.rooms_pending_enter_request_id == Some(request_id) =>
                    {
                        self.rooms_pending_enter_request_id = None;
                        crate::app::rooms::input::complete_enter_room(self, room);
                    }
                    RoomsEvent::EnterError {
                        user_id,
                        request_id,
                        room_id,
                        display_name,
                        message,
                    } if user_id == self.user_id
                        && self.rooms_pending_enter_request_id == Some(request_id) =>
                    {
                        self.rooms_pending_enter_request_id = None;
                        if self
                            .rooms_active_room
                            .as_ref()
                            .is_some_and(|room| room.id == room_id)
                        {
                            self.rooms_active_room = None;
                        }
                        if self
                            .active_room_game
                            .as_ref()
                            .is_some_and(|game| game.room_id() == room_id)
                        {
                            self.active_room_game = None;
                        }
                        self.rooms_turn_notified_room_ids.remove(&room_id);
                        banner = Some(Banner::error(&format!(
                            "Failed to enter table {}: {}",
                            display_name, message
                        )));
                    }
                    _ => {}
                },
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive rooms event");
                    break;
                }
            }
        }
        banner
    }
}
