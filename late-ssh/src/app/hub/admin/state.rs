use late_core::models::{
    marketplace::{MarketplaceAdminRow, MarketplaceAdminUpdate},
    quest::{RewardTemplateAdminRow, RewardTemplateAdminUpdate},
};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::{
    common::primitives::Banner,
    hub::{dailies::svc::QuestService, shop::svc::ShopService},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminCategory {
    DailyQuests,
    WeeklyQuests,
    PuzzleRewards,
    GameRewards,
    ShopItems,
    All,
}

impl AdminCategory {
    pub const ALL: [Self; 6] = [
        Self::DailyQuests,
        Self::WeeklyQuests,
        Self::PuzzleRewards,
        Self::GameRewards,
        Self::ShopItems,
        Self::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::DailyQuests => "Daily quests",
            Self::WeeklyQuests => "Weekly quests",
            Self::PuzzleRewards => "Puzzle rewards",
            Self::GameRewards => "Game rewards",
            Self::ShopItems => "Shop items",
            Self::All => "All",
        }
    }

    fn matches_reward(self, row: &RewardTemplateAdminRow) -> bool {
        match self {
            Self::DailyQuests => row.is_quest && row.cadence.as_deref() == Some("daily"),
            Self::WeeklyQuests => row.is_quest && row.cadence.as_deref() == Some("weekly"),
            Self::PuzzleRewards => !row.is_quest && row.domain == "puzzle",
            Self::GameRewards => !row.is_quest && row.domain != "puzzle",
            Self::ShopItems => false,
            Self::All => true,
        }
    }

    fn matches_shop(self, _row: &MarketplaceAdminRow) -> bool {
        matches!(self, Self::ShopItems | Self::All)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminField {
    Title,
    Description,
    Target,
    Reward,
    Weight,
    Active,
}

impl AdminField {
    const REWARD_FIELDS: [Self; 6] = [
        Self::Title,
        Self::Description,
        Self::Target,
        Self::Reward,
        Self::Weight,
        Self::Active,
    ];
    const SHOP_FIELDS: [Self; 5] = [
        Self::Title,
        Self::Description,
        Self::Reward,
        Self::Weight,
        Self::Active,
    ];

    pub fn label(self, draft_kind: AdminDraftKind) -> &'static str {
        match (draft_kind, self) {
            (AdminDraftKind::Shop, Self::Title) => "name",
            (AdminDraftKind::Shop, Self::Reward) => "price",
            (AdminDraftKind::Shop, Self::Weight) => "sort",
            (_, Self::Title) => "title",
            (_, Self::Description) => "desc",
            (_, Self::Target) => "req",
            (_, Self::Reward) => "reward",
            (_, Self::Weight) => "weight",
            (_, Self::Active) => "active",
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self, Self::Title | Self::Description)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminDraftKind {
    Reward,
    Shop,
}

#[derive(Clone, Debug)]
pub enum AdminEntryRef<'a> {
    Reward(&'a RewardTemplateAdminRow),
    Shop(&'a MarketplaceAdminRow),
}

impl AdminEntryRef<'_> {
    pub fn title(&self) -> &str {
        match self {
            Self::Reward(row) => &row.title,
            Self::Shop(row) => &row.name,
        }
    }
}

#[derive(Clone, Debug)]
pub enum AdminDraft {
    Reward(RewardAdminDraft),
    Shop(ShopAdminDraft),
}

impl AdminDraft {
    fn from_reward(row: &RewardTemplateAdminRow) -> Self {
        Self::Reward(RewardAdminDraft {
            id: row.id,
            title: row.title.clone(),
            description: row.description.clone(),
            target: row.target,
            reward_chips: row.reward_chips,
            weight: row.weight,
            active: row.active,
        })
    }

    fn from_shop(row: &MarketplaceAdminRow) -> Self {
        Self::Shop(ShopAdminDraft {
            id: row.id,
            name: row.name.clone(),
            description: row.description.clone(),
            price_chips: row.price_chips,
            sort_order: row.sort_order,
            active: row.active,
        })
    }

    pub fn kind(&self) -> AdminDraftKind {
        match self {
            Self::Reward(_) => AdminDraftKind::Reward,
            Self::Shop(_) => AdminDraftKind::Shop,
        }
    }

    fn fields(&self) -> &'static [AdminField] {
        match self {
            Self::Reward(_) => &AdminField::REWARD_FIELDS,
            Self::Shop(_) => &AdminField::SHOP_FIELDS,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RewardAdminDraft {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub target: i32,
    pub reward_chips: i64,
    pub weight: i32,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct ShopAdminDraft {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub price_chips: i64,
    pub sort_order: i32,
    pub active: bool,
}

#[derive(Clone, Debug)]
enum AdminAsyncResult {
    Loaded {
        templates: Vec<RewardTemplateAdminRow>,
        shop_items: Vec<MarketplaceAdminRow>,
    },
    SavedReward(Box<RewardTemplateAdminRow>),
    SavedShop(Box<MarketplaceAdminRow>),
    Failed(String),
}

pub struct AdminState {
    quest_service: QuestService,
    shop_service: ShopService,
    templates: Vec<RewardTemplateAdminRow>,
    shop_items: Vec<MarketplaceAdminRow>,
    category_index: usize,
    selected_index: usize,
    field_index: usize,
    draft: Option<AdminDraft>,
    dirty: bool,
    editing: bool,
    edit_buffer: String,
    edit_cursor: usize,
    loading: bool,
    saving: bool,
    loaded_once: bool,
    tx: mpsc::UnboundedSender<AdminAsyncResult>,
    rx: mpsc::UnboundedReceiver<AdminAsyncResult>,
}

pub struct AdminTick {
    pub banner: Option<Banner>,
}

impl AdminState {
    pub fn new(quest_service: QuestService, shop_service: ShopService) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            quest_service,
            shop_service,
            templates: Vec::new(),
            shop_items: Vec::new(),
            category_index: 0,
            selected_index: 0,
            field_index: 0,
            draft: None,
            dirty: false,
            editing: false,
            edit_buffer: String::new(),
            edit_cursor: 0,
            loading: false,
            saving: false,
            loaded_once: false,
            tx,
            rx,
        }
    }

    pub fn tick(&mut self, is_admin: bool) -> AdminTick {
        if is_admin && !self.loaded_once && !self.loading {
            self.reload(true);
        }

        let mut banner = None;
        while let Ok(result) = self.rx.try_recv() {
            match result {
                AdminAsyncResult::Loaded {
                    templates,
                    shop_items,
                } => {
                    self.loading = false;
                    self.loaded_once = true;
                    self.templates = templates;
                    self.shop_items = shop_items;
                    self.clamp_selection();
                    self.sync_draft_to_selected();
                }
                AdminAsyncResult::SavedReward(row) => {
                    self.saving = false;
                    if let Some(existing) = self.templates.iter_mut().find(|item| item.id == row.id)
                    {
                        *existing = *row;
                    }
                    self.sync_draft_to_selected();
                    self.dirty = false;
                    banner = Some(Banner::success("Saved reward template"));
                }
                AdminAsyncResult::SavedShop(row) => {
                    self.saving = false;
                    if let Some(existing) =
                        self.shop_items.iter_mut().find(|item| item.id == row.id)
                    {
                        *existing = *row;
                    }
                    self.sync_draft_to_selected();
                    self.dirty = false;
                    banner = Some(Banner::success("Saved shop item"));
                }
                AdminAsyncResult::Failed(message) => {
                    self.loading = false;
                    self.saving = false;
                    banner = Some(Banner::error(&message));
                }
            }
        }

        AdminTick { banner }
    }

    pub fn templates(&self) -> &[RewardTemplateAdminRow] {
        &self.templates
    }

    pub fn shop_items(&self) -> &[MarketplaceAdminRow] {
        &self.shop_items
    }

    pub fn visible_entries(&self) -> Vec<AdminEntryRef<'_>> {
        let category = self.selected_category();
        let mut rows = self
            .templates
            .iter()
            .filter(|row| category.matches_reward(row))
            .map(AdminEntryRef::Reward)
            .collect::<Vec<_>>();
        rows.extend(
            self.shop_items
                .iter()
                .filter(|row| category.matches_shop(row))
                .map(AdminEntryRef::Shop),
        );
        rows
    }

    pub fn selected_entry(&self) -> Option<AdminEntryRef<'_>> {
        self.visible_entries().get(self.selected_index).cloned()
    }

    pub fn selected_category(&self) -> AdminCategory {
        AdminCategory::ALL[self.category_index.min(AdminCategory::ALL.len() - 1)]
    }

    pub fn selected_category_index(&self) -> usize {
        self.category_index
    }

    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub fn selected_field(&self) -> AdminField {
        self.available_fields()[self
            .field_index
            .min(self.available_fields().len().saturating_sub(1))]
    }

    pub fn selected_field_index(&self) -> usize {
        self.field_index
            .min(self.available_fields().len().saturating_sub(1))
    }

    pub fn available_fields(&self) -> &'static [AdminField] {
        self.draft
            .as_ref()
            .map(AdminDraft::fields)
            .unwrap_or(&AdminField::REWARD_FIELDS)
    }

    pub fn draft(&self) -> Option<&AdminDraft> {
        self.draft.as_ref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn is_editing(&self) -> bool {
        self.editing
    }

    pub fn edit_buffer(&self) -> &str {
        &self.edit_buffer
    }

    pub fn edit_cursor(&self) -> usize {
        self.edit_cursor.min(char_count(&self.edit_buffer))
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn is_saving(&self) -> bool {
        self.saving
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.editing {
            return;
        }
        let len = self.visible_entries().len();
        if len == 0 {
            self.selected_index = 0;
            self.draft = None;
            return;
        }
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
        self.sync_draft_to_selected();
    }

    pub fn select_next_category(&mut self) {
        if self.editing {
            return;
        }
        self.category_index = (self.category_index + 1) % AdminCategory::ALL.len();
        self.selected_index = 0;
        self.sync_draft_to_selected();
    }

    pub fn select_previous_category(&mut self) {
        if self.editing {
            return;
        }
        self.category_index =
            (self.category_index + AdminCategory::ALL.len() - 1) % AdminCategory::ALL.len();
        self.selected_index = 0;
        self.sync_draft_to_selected();
    }

    pub fn select_next_field(&mut self) {
        if self.editing {
            return;
        }
        self.field_index = (self.field_index + 1) % self.available_fields().len();
    }

    pub fn select_previous_field(&mut self) {
        if self.editing {
            return;
        }
        let len = self.available_fields().len();
        self.field_index = (self.field_index + len - 1) % len;
    }

    pub fn begin_edit(&mut self) -> Option<Banner> {
        let field = self.selected_field();
        if !field.is_text() {
            return self.adjust_or_toggle(1);
        }
        let draft = self.draft.as_ref()?;
        self.edit_buffer = match (field, draft) {
            (AdminField::Title, AdminDraft::Reward(draft)) => draft.title.clone(),
            (AdminField::Title, AdminDraft::Shop(draft)) => draft.name.clone(),
            (AdminField::Description, AdminDraft::Reward(draft)) => draft.description.clone(),
            (AdminField::Description, AdminDraft::Shop(draft)) => draft.description.clone(),
            _ => String::new(),
        };
        self.edit_cursor = char_count(&self.edit_buffer);
        self.editing = true;
        Some(Banner::success(&format!(
            "Editing {}",
            field.label(draft.kind())
        )))
    }

    pub fn cancel_edit(&mut self) -> Option<Banner> {
        if !self.editing {
            return None;
        }
        self.editing = false;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
        Some(Banner::success("Edit cancelled"))
    }

    pub fn commit_edit(&mut self) -> Option<Banner> {
        if !self.editing {
            return None;
        }
        let value = self.edit_buffer.trim().to_string();
        if value.is_empty() {
            return Some(Banner::error("Value cannot be empty"));
        }
        let field = self.selected_field();
        let draft = self.draft.as_mut()?;
        match (field, draft) {
            (AdminField::Title, AdminDraft::Reward(draft)) => draft.title = value,
            (AdminField::Title, AdminDraft::Shop(draft)) => draft.name = value,
            (AdminField::Description, AdminDraft::Reward(draft)) => draft.description = value,
            (AdminField::Description, AdminDraft::Shop(draft)) => draft.description = value,
            _ => {}
        }
        self.editing = false;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
        self.dirty = true;
        Some(Banner::success("Draft updated"))
    }

    pub fn push_edit_char(&mut self, ch: char) {
        if !self.editing || ch.is_control() {
            return;
        }
        let index = byte_index_for_char(&self.edit_buffer, self.edit_cursor);
        self.edit_buffer.insert(index, ch);
        self.edit_cursor += 1;
    }

    pub fn backspace_edit(&mut self) {
        if !self.editing || self.edit_cursor == 0 {
            return;
        }
        self.edit_cursor -= 1;
        let start = byte_index_for_char(&self.edit_buffer, self.edit_cursor);
        let end = byte_index_for_char(&self.edit_buffer, self.edit_cursor + 1);
        self.edit_buffer.replace_range(start..end, "");
    }

    pub fn delete_edit(&mut self) {
        if !self.editing || self.edit_cursor >= char_count(&self.edit_buffer) {
            return;
        }
        let start = byte_index_for_char(&self.edit_buffer, self.edit_cursor);
        let end = byte_index_for_char(&self.edit_buffer, self.edit_cursor + 1);
        self.edit_buffer.replace_range(start..end, "");
    }

    pub fn clear_edit(&mut self) {
        if self.editing {
            self.edit_buffer.clear();
            self.edit_cursor = 0;
        }
    }

    pub fn move_edit_cursor(&mut self, delta: isize) {
        if !self.editing {
            return;
        }
        let len = char_count(&self.edit_buffer) as isize;
        self.edit_cursor = (self.edit_cursor as isize + delta).clamp(0, len) as usize;
    }

    pub fn edit_cursor_home(&mut self) {
        if self.editing {
            self.edit_cursor = 0;
        }
    }

    pub fn edit_cursor_end(&mut self) {
        if self.editing {
            self.edit_cursor = char_count(&self.edit_buffer);
        }
    }

    pub fn adjust_or_toggle(&mut self, direction: i32) -> Option<Banner> {
        let field = self.selected_field();
        let draft = self.draft.as_mut()?;
        match draft {
            AdminDraft::Reward(draft) => match field {
                AdminField::Target => {
                    let step = target_step(draft.target);
                    draft.target = (draft.target + direction.signum() * step).max(1);
                }
                AdminField::Reward => {
                    let step = chip_step(draft.reward_chips);
                    draft.reward_chips =
                        (draft.reward_chips + i64::from(direction.signum()) * step).max(0);
                }
                AdminField::Weight => {
                    draft.weight = (draft.weight + direction.signum() * 10).max(1);
                }
                AdminField::Active => {
                    draft.active = !draft.active;
                }
                AdminField::Title | AdminField::Description => return self.begin_edit(),
            },
            AdminDraft::Shop(draft) => match field {
                AdminField::Reward => {
                    let step = chip_step(draft.price_chips);
                    draft.price_chips =
                        (draft.price_chips + i64::from(direction.signum()) * step).max(0);
                }
                AdminField::Weight => {
                    draft.sort_order += direction.signum() * 10;
                }
                AdminField::Active => {
                    draft.active = !draft.active;
                }
                AdminField::Title | AdminField::Description => return self.begin_edit(),
                AdminField::Target => {}
            },
        }
        self.dirty = true;
        Some(Banner::success("Draft updated"))
    }

    pub fn reload(&mut self, is_admin: bool) {
        if !is_admin || self.loading {
            return;
        }
        self.loading = true;
        let quest_service = self.quest_service.clone();
        let shop_service = self.shop_service.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = async {
                let templates = quest_service.list_reward_templates_for_admin(true).await?;
                let shop_items = shop_service.list_marketplace_items_for_admin(true).await?;
                Ok::<_, anyhow::Error>((templates, shop_items))
            }
            .await;
            let result = match result {
                Ok((templates, shop_items)) => AdminAsyncResult::Loaded {
                    templates,
                    shop_items,
                },
                Err(error) => AdminAsyncResult::Failed(format!("Admin load failed: {error:#}")),
            };
            let _ = tx.send(result);
        });
    }

    pub fn save(&mut self, is_admin: bool) -> Option<Banner> {
        if !is_admin {
            return Some(Banner::error("Admin access required"));
        }
        if self.saving {
            return Some(Banner::success("Save already in progress"));
        }
        let draft = self.draft.clone()?;
        let saving_message = match draft.kind() {
            AdminDraftKind::Reward => "Saving reward template",
            AdminDraftKind::Shop => "Saving shop item",
        };
        self.saving = true;
        let quest_service = self.quest_service.clone();
        let shop_service = self.shop_service.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = match draft {
                AdminDraft::Reward(draft) => quest_service
                    .update_reward_template_for_admin(
                        true,
                        RewardTemplateAdminUpdate {
                            id: draft.id,
                            title: draft.title,
                            description: draft.description,
                            target: draft.target,
                            reward_chips: draft.reward_chips,
                            weight: draft.weight,
                            active: draft.active,
                        },
                    )
                    .await
                    .map(|row| AdminAsyncResult::SavedReward(Box::new(row))),
                AdminDraft::Shop(draft) => shop_service
                    .update_marketplace_item_for_admin(
                        true,
                        MarketplaceAdminUpdate {
                            id: draft.id,
                            name: draft.name,
                            description: draft.description,
                            price_chips: draft.price_chips,
                            active: draft.active,
                            sort_order: draft.sort_order,
                        },
                    )
                    .await
                    .map(|row| AdminAsyncResult::SavedShop(Box::new(row))),
            };
            let result = match result {
                Ok(result) => result,
                Err(error) => AdminAsyncResult::Failed(format!("Admin save failed: {error:#}")),
            };
            let _ = tx.send(result);
        });
        Some(Banner::success(saving_message))
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_entries().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
        self.clamp_field();
    }

    fn clamp_field(&mut self) {
        let len = self.available_fields().len();
        if len > 0 {
            self.field_index = self.field_index.min(len - 1);
        }
    }

    fn sync_draft_to_selected(&mut self) {
        self.draft = self.selected_entry().map(|entry| match entry {
            AdminEntryRef::Reward(row) => AdminDraft::from_reward(row),
            AdminEntryRef::Shop(row) => AdminDraft::from_shop(row),
        });
        self.clamp_field();
        self.dirty = false;
        self.editing = false;
        self.edit_buffer.clear();
        self.edit_cursor = 0;
    }
}

fn target_step(current: i32) -> i32 {
    if current >= 10_000 {
        1_000
    } else if current >= 1_000 {
        100
    } else if current >= 100 {
        10
    } else {
        1
    }
}

fn chip_step(current: i64) -> i64 {
    if current >= 1_000 { 100 } else { 25 }
}

fn char_count(value: &str) -> usize {
    value.chars().count()
}

fn byte_index_for_char(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .map(|(index, _)| index)
        .nth(char_index)
        .unwrap_or(value.len())
}
