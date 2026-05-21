use std::{
    collections::{HashMap, HashSet},
    future::poll_fn,
    sync::{Arc, Mutex},
};

use anyhow::Result;
use late_core::{
    MutexRecover,
    db::{Db, DbConfig},
    models::{
        chips::UserChips,
        marketplace::{
            CAT_COMPANION_SKU, MarketplaceItem, PurchaseStatus, SHOP_CATALOG_CHANGED_CHANNEL,
            SHOP_USER_CHANGED_CHANNEL, UserPurchase, listen_for_shop_changes,
            purchase_durable_item_by_sku,
        },
    },
};
use tokio::sync::{broadcast, watch};
use tokio_postgres::{AsyncMessage, NoTls};
use uuid::Uuid;

use super::entitlements::ShopEntitlements;

#[derive(Clone, Debug, Default)]
pub struct ShopSnapshot {
    pub user_id: Option<Uuid>,
    pub balance: i64,
    pub items: Vec<ShopCatalogItem>,
    pub entitlements: ShopEntitlements,
}

#[derive(Clone, Debug)]
pub struct ShopCatalogItem {
    pub sku: String,
    pub item_kind: String,
    pub slot: Option<String>,
    pub name: String,
    pub description: String,
    pub price_chips: i64,
    pub owned: bool,
    pub quantity: i32,
    pub remaining_uses: Option<i32>,
}

impl ShopCatalogItem {
    pub fn is_cat_companion(&self) -> bool {
        self.sku == CAT_COMPANION_SKU
    }
}

#[derive(Clone, Debug)]
pub enum ShopEvent {
    ActionCompleted { user_id: Uuid, message: String },
    ActionFailed { user_id: Uuid, message: String },
}

#[derive(Clone)]
pub struct ShopService {
    db: Db,
    snapshot_txs: Arc<Mutex<HashMap<Uuid, watch::Sender<ShopSnapshot>>>>,
    evt_tx: broadcast::Sender<ShopEvent>,
}

impl ShopService {
    pub fn new(db: Db) -> Self {
        let (evt_tx, _) = broadcast::channel(512);
        Self {
            db,
            snapshot_txs: Arc::new(Mutex::new(HashMap::new())),
            evt_tx,
        }
    }

    pub fn subscribe_snapshot(&self, user_id: Uuid) -> watch::Receiver<ShopSnapshot> {
        self.snapshot_sender(user_id).subscribe()
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ShopEvent> {
        self.evt_tx.subscribe()
    }

    fn snapshot_sender(&self, user_id: Uuid) -> watch::Sender<ShopSnapshot> {
        let mut channels = self.snapshot_txs.lock_recover();
        let make = || watch::channel(ShopSnapshot::default()).0;
        let sender = channels.entry(user_id).or_insert_with(&make);
        if sender.is_closed() {
            *sender = make();
        }
        sender.clone()
    }

    fn has_active_snapshot_receiver(&self, user_id: Uuid) -> bool {
        self.snapshot_txs
            .lock_recover()
            .get(&user_id)
            .is_some_and(|sender| sender.receiver_count() > 0)
    }

    fn active_snapshot_users(&self) -> Vec<Uuid> {
        self.snapshot_txs
            .lock_recover()
            .iter()
            .filter_map(|(user_id, sender)| (sender.receiver_count() > 0).then_some(*user_id))
            .collect()
    }

    fn publish_event(&self, event: ShopEvent) {
        let _ = self.evt_tx.send(event);
    }

    pub async fn refresh_user(&self, user_id: Uuid) -> Result<ShopSnapshot> {
        let snapshot = self.load_snapshot(user_id).await?;
        let _ = self.snapshot_sender(user_id).send(snapshot.clone());
        Ok(snapshot)
    }

    async fn refresh_user_if_active(&self, user_id: Uuid) -> Result<()> {
        if self.has_active_snapshot_receiver(user_id) {
            self.refresh_user(user_id).await?;
        }
        Ok(())
    }

    async fn refresh_catalog_for_active_users(&self) -> Result<()> {
        for user_id in self.active_snapshot_users() {
            self.refresh_user(user_id).await?;
        }
        Ok(())
    }

    pub fn refresh_user_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(async move {
            if let Err(error) = svc.refresh_user(user_id).await {
                tracing::warn!(error = ?error, user_id = %user_id, "failed to refresh shop snapshot");
            }
        });
    }

    pub fn purchase_item_task(&self, user_id: Uuid, sku: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            match svc.purchase_item(user_id, &sku).await {
                Ok(message) => svc.publish_event(ShopEvent::ActionCompleted { user_id, message }),
                Err(error) => {
                    tracing::warn!(error = ?error, user_id = %user_id, sku, "shop purchase failed");
                    svc.publish_event(ShopEvent::ActionFailed {
                        user_id,
                        message: "Purchase failed".to_string(),
                    });
                }
            }
        });
    }

    async fn purchase_item(&self, user_id: Uuid, sku: &str) -> Result<String> {
        let mut client = self.db.get().await?;
        let result = purchase_durable_item_by_sku(&mut client, user_id, sku).await?;
        drop(client);

        let message = match result {
            None => "Item is not available".to_string(),
            Some(result) => match result.status {
                PurchaseStatus::Purchased => format!("Unlocked {}", result.item.name),
                PurchaseStatus::AlreadyOwned => format!("{} already unlocked", result.item.name),
                PurchaseStatus::InsufficientFunds => {
                    format!(
                        "Need {} chips for {}",
                        result.item.price_chips, result.item.name
                    )
                }
            },
        };

        self.refresh_user(user_id).await?;
        Ok(message)
    }

    async fn load_snapshot(&self, user_id: Uuid) -> Result<ShopSnapshot> {
        let client = self.db.get().await?;
        let chips = UserChips::ensure(&client, user_id).await?;
        let items = MarketplaceItem::list_visible(&client).await?;
        let purchases = UserPurchase::list_for_user(&client, user_id).await?;

        let mut purchases_by_item = HashMap::with_capacity(purchases.len());
        for purchase in purchases {
            purchases_by_item.insert(purchase.item_id, purchase);
        }

        let mut owned_skus = HashSet::new();
        let catalog = items
            .into_iter()
            .map(|item| {
                let purchase = purchases_by_item.get(&item.id);
                let owned = purchase.is_some();
                if owned {
                    owned_skus.insert(item.sku.clone());
                }
                ShopCatalogItem {
                    sku: item.sku,
                    item_kind: item.item_kind,
                    slot: item.slot,
                    name: item.name,
                    description: item.description,
                    price_chips: item.price_chips,
                    owned,
                    quantity: purchase.map(|purchase| purchase.quantity).unwrap_or(0),
                    remaining_uses: purchase.and_then(|purchase| purchase.remaining_uses),
                }
            })
            .collect();

        Ok(ShopSnapshot {
            user_id: Some(user_id),
            balance: chips.balance,
            items: catalog,
            entitlements: ShopEntitlements::from_owned_skus(owned_skus),
        })
    }

    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let svc = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = svc.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "shop postgres listener stopped");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        })
    }

    async fn listen_once(&self, db_config: &DbConfig) -> Result<()> {
        let mut config = tokio_postgres::Config::new();
        config.host(&db_config.host);
        config.port(db_config.port);
        config.user(&db_config.user);
        config.password(&db_config.password);
        config.dbname(&db_config.dbname);

        let (client, mut connection) = config.connect(NoTls).await?;
        let listen = listen_for_shop_changes(&client);
        tokio::pin!(listen);
        loop {
            tokio::select! {
                result = &mut listen => {
                    result?;
                    break;
                }
                message = poll_fn(|cx| connection.poll_message(cx)) => {
                    let Some(message) = message else {
                        return Ok(());
                    };
                    self.handle_async_message(message?).await?;
                }
            }
        }

        loop {
            let Some(message) = poll_fn(|cx| connection.poll_message(cx)).await else {
                return Ok(());
            };

            self.handle_async_message(message?).await?;
        }
    }

    async fn handle_async_message(&self, message: AsyncMessage) -> Result<()> {
        match message {
            AsyncMessage::Notification(notification) => match notification.channel() {
                SHOP_USER_CHANGED_CHANNEL => {
                    if let Ok(user_id) = notification.payload().parse::<Uuid>() {
                        self.refresh_user_if_active(user_id).await?;
                    }
                }
                SHOP_CATALOG_CHANGED_CHANNEL => {
                    self.refresh_catalog_for_active_users().await?;
                }
                _ => {}
            },
            AsyncMessage::Notice(notice) => {
                tracing::debug!(notice = ?notice, "postgres shop listener notice");
            }
            _ => {}
        }
        Ok(())
    }
}
