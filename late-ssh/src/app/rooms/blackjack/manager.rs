use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use late_core::MutexRecover;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::app::{
    games::chips::svc::ChipService,
    rooms::blackjack::svc::{BlackjackEvent, BlackjackService},
};

#[derive(Clone)]
pub struct BlackjackTableManager {
    chip_svc: ChipService,
    tables: Arc<Mutex<HashMap<Uuid, BlackjackService>>>,
}

impl BlackjackTableManager {
    pub fn new(chip_svc: ChipService) -> Self {
        Self {
            chip_svc,
            tables: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub fn get_or_create(&self, room_id: Uuid) -> BlackjackService {
        let mut tables = self.tables.lock_recover();
        tables
            .entry(room_id)
            .or_insert_with(|| {
                let (event_tx, _) = broadcast::channel::<BlackjackEvent>(64);
                BlackjackService::new(self.chip_svc.clone(), event_tx)
            })
            .clone()
    }
}
