use tokio::sync::broadcast;

use super::svc::{RoomsEvent, game_kind_label};
use crate::app::{common::primitives::Banner, state::App};

impl App {
    pub(crate) fn tick_rooms(&mut self) -> Option<Banner> {
        if self.rooms_snapshot_rx.has_changed().unwrap_or(false) {
            self.rooms_snapshot = self.rooms_snapshot_rx.borrow_and_update().clone();
        }
        self.drain_rooms_events()
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
