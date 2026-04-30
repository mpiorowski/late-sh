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
}

impl BlackjackTableManager {
    pub fn new(chip_svc: ChipService, player_directory: BlackjackPlayerDirectory) -> Self {
        Self {
            chip_svc,
            player_directory,
            tables: Arc::new(Mutex::new(HashMap::new())),
        }
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
                BlackjackService::new_with_settings(
                    self.chip_svc.clone(),
                    self.player_directory.clone(),
                    event_tx,
                    settings,
                )
            })
            .clone()
    }

    pub fn table_snapshots(&self) -> HashMap<Uuid, BlackjackSnapshot> {
        self.tables
            .lock_recover()
            .iter()
            .map(|(room_id, service)| (*room_id, service.current_snapshot()))
            .collect()
    }
}
