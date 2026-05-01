use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::MutexRecover;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    games::chips::svc::ChipService,
    rooms::blackjack::{
        player::BlackjackPlayerDirectory,
        settings::BlackjackTableSettings,
        state::BlackjackSnapshot,
        svc::{BlackjackEvent, BlackjackService},
    },
};

#[derive(Clone)]
pub struct BlackjackTableManager {
    chip_svc: ChipService,
    player_directory: BlackjackPlayerDirectory,
    tables: Arc<Mutex<HashMap<Uuid, BlackjackService>>>,
    event_tx: broadcast::Sender<BlackjackEvent>,
}

impl BlackjackTableManager {
    pub fn new(chip_svc: ChipService, player_directory: BlackjackPlayerDirectory) -> Self {
        let (event_tx, _) = broadcast::channel::<BlackjackEvent>(256);
        Self {
            chip_svc,
            player_directory,
            tables: Arc::new(Mutex::new(HashMap::new())),
            event_tx,
        }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<BlackjackEvent> {
        self.event_tx.subscribe()
    }

    pub fn get_or_create(
        &self,
        room_id: Uuid,
        settings: BlackjackTableSettings,
    ) -> BlackjackService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room_id)
            .or_insert_with(|| {
                let (event_tx, _) = broadcast::channel::<BlackjackEvent>(64);
                self.forward_table_events(room_id, event_tx.subscribe());
                BlackjackService::new_with_settings(
                    room_id,
                    self.chip_svc.clone(),
                    self.player_directory.clone(),
                    event_tx,
                    settings,
                )
            })
            .clone()
    }

    fn forward_table_events(&self, room_id: Uuid, mut rx: broadcast::Receiver<BlackjackEvent>) {
        let event_tx = self.event_tx.clone();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        let _ = event_tx.send(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(%room_id, skipped, "blackjack table event forwarder lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub fn table_snapshots(&self) -> HashMap<Uuid, BlackjackSnapshot> {
        self.tables
            .lock_recover()
            .iter()
            .map(|(room_id, service)| (*room_id, service.current_snapshot()))
            .collect()
    }
}
