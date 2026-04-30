use tokio::sync::broadcast;

use super::svc::{RoomsEvent, game_kind_label};
use crate::app::{common::primitives::Banner, state::App};

impl App {
    pub(crate) fn tick_rooms(&mut self) -> Option<Banner> {
        if self.rooms_snapshot_rx.has_changed().unwrap_or(false) {
            self.rooms_snapshot = self.rooms_snapshot_rx.borrow_and_update().clone();
            self.clamp_rooms_selection();
            self.refresh_active_room();
        }
        self.drain_rooms_events()
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
        let Some(active_id) = self.rooms_active_room.as_ref().map(|room| room.id) else {
            return;
        };
        self.rooms_active_room = self
            .rooms_snapshot
            .rooms
            .iter()
            .find(|room| room.id == active_id)
            .cloned();
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
                            game_kind_label(game_kind),
                            display_name
                        )));
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
                            game_kind_label(game_kind),
                            table,
                            message
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
