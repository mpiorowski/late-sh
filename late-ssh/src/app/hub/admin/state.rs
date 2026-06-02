use late_core::models::quest::{RewardTemplateAdminRow, RewardTemplateAdminUpdate};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::app::{common::primitives::Banner, hub::dailies::svc::QuestService};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminCategory {
    DailyQuests,
    WeeklyQuests,
    PuzzleRewards,
    GameRewards,
    All,
}

impl AdminCategory {
    pub const ALL: [Self; 5] = [
        Self::DailyQuests,
        Self::WeeklyQuests,
        Self::PuzzleRewards,
        Self::GameRewards,
        Self::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::DailyQuests => "Daily quests",
            Self::WeeklyQuests => "Weekly quests",
            Self::PuzzleRewards => "Puzzle rewards",
            Self::GameRewards => "Game rewards",
            Self::All => "All",
        }
    }

    fn matches(self, row: &RewardTemplateAdminRow) -> bool {
        match self {
            Self::DailyQuests => row.is_quest && row.cadence.as_deref() == Some("daily"),
            Self::WeeklyQuests => row.is_quest && row.cadence.as_deref() == Some("weekly"),
            Self::PuzzleRewards => !row.is_quest && row.domain == "puzzle",
            Self::GameRewards => !row.is_quest && row.domain != "puzzle",
            Self::All => true,
        }
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
    pub const ALL: [Self; 6] = [
        Self::Title,
        Self::Description,
        Self::Target,
        Self::Reward,
        Self::Weight,
        Self::Active,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Description => "desc",
            Self::Target => "req",
            Self::Reward => "reward",
            Self::Weight => "weight",
            Self::Active => "active",
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self, Self::Title | Self::Description)
    }
}

#[derive(Clone, Debug)]
pub struct AdminDraft {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    pub target: i32,
    pub reward_chips: i64,
    pub weight: i32,
    pub active: bool,
}

impl AdminDraft {
    fn from_row(row: &RewardTemplateAdminRow) -> Self {
        Self {
            id: row.id,
            title: row.title.clone(),
            description: row.description.clone(),
            target: row.target,
            reward_chips: row.reward_chips,
            weight: row.weight,
            active: row.active,
        }
    }

    fn update(&self) -> RewardTemplateAdminUpdate {
        RewardTemplateAdminUpdate {
            id: self.id,
            title: self.title.clone(),
            description: self.description.clone(),
            target: self.target,
            reward_chips: self.reward_chips,
            weight: self.weight,
            active: self.active,
        }
    }
}

#[derive(Clone, Debug)]
enum AdminAsyncResult {
    Loaded(Vec<RewardTemplateAdminRow>),
    Saved(Box<RewardTemplateAdminRow>),
    Failed(String),
}

pub struct AdminState {
    service: QuestService,
    templates: Vec<RewardTemplateAdminRow>,
    category_index: usize,
    selected_index: usize,
    field_index: usize,
    draft: Option<AdminDraft>,
    dirty: bool,
    editing: bool,
    edit_buffer: String,
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
    pub fn new(service: QuestService) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            service,
            templates: Vec::new(),
            category_index: 0,
            selected_index: 0,
            field_index: 0,
            draft: None,
            dirty: false,
            editing: false,
            edit_buffer: String::new(),
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
                AdminAsyncResult::Loaded(rows) => {
                    self.loading = false;
                    self.loaded_once = true;
                    self.templates = rows;
                    self.clamp_selection();
                    self.sync_draft_to_selected();
                }
                AdminAsyncResult::Saved(row) => {
                    self.saving = false;
                    if let Some(existing) = self.templates.iter_mut().find(|item| item.id == row.id)
                    {
                        *existing = *row;
                    }
                    self.sync_draft_to_selected();
                    self.dirty = false;
                    banner = Some(Banner::success("Saved reward template"));
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

    pub fn visible_templates(&self) -> Vec<&RewardTemplateAdminRow> {
        let category = self.selected_category();
        self.templates
            .iter()
            .filter(|row| category.matches(row))
            .collect()
    }

    pub fn selected_template(&self) -> Option<&RewardTemplateAdminRow> {
        self.visible_templates().get(self.selected_index).copied()
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
        AdminField::ALL[self.field_index.min(AdminField::ALL.len() - 1)]
    }

    pub fn selected_field_index(&self) -> usize {
        self.field_index
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
        let len = self.visible_templates().len();
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
        self.field_index = (self.field_index + 1) % AdminField::ALL.len();
    }

    pub fn select_previous_field(&mut self) {
        if self.editing {
            return;
        }
        self.field_index = (self.field_index + AdminField::ALL.len() - 1) % AdminField::ALL.len();
    }

    pub fn begin_edit(&mut self) -> Option<Banner> {
        let field = self.selected_field();
        if !field.is_text() {
            return self.adjust_or_toggle(1);
        }
        let draft = self.draft.as_ref()?;
        self.edit_buffer = match field {
            AdminField::Title => draft.title.clone(),
            AdminField::Description => draft.description.clone(),
            _ => String::new(),
        };
        self.editing = true;
        Some(Banner::success(&format!("Editing {}", field.label())))
    }

    pub fn cancel_edit(&mut self) -> Option<Banner> {
        if !self.editing {
            return None;
        }
        self.editing = false;
        self.edit_buffer.clear();
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
        match field {
            AdminField::Title => draft.title = value,
            AdminField::Description => draft.description = value,
            _ => {}
        }
        self.editing = false;
        self.edit_buffer.clear();
        self.dirty = true;
        Some(Banner::success("Draft updated"))
    }

    pub fn push_edit_char(&mut self, ch: char) {
        if self.editing && !ch.is_control() {
            self.edit_buffer.push(ch);
        }
    }

    pub fn backspace_edit(&mut self) {
        if self.editing {
            self.edit_buffer.pop();
        }
    }

    pub fn clear_edit(&mut self) {
        if self.editing {
            self.edit_buffer.clear();
        }
    }

    pub fn adjust_or_toggle(&mut self, direction: i32) -> Option<Banner> {
        let field = self.selected_field();
        let draft = self.draft.as_mut()?;
        match field {
            AdminField::Target => {
                let step = target_step(draft.target);
                draft.target = (draft.target + direction.signum() * step).max(1);
            }
            AdminField::Reward => {
                let step = reward_step(draft.reward_chips);
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
        }
        self.dirty = true;
        Some(Banner::success("Draft updated"))
    }

    pub fn reload(&mut self, is_admin: bool) {
        if !is_admin || self.loading {
            return;
        }
        self.loading = true;
        let service = self.service.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = match service.list_reward_templates_for_admin(true).await {
                Ok(rows) => AdminAsyncResult::Loaded(rows),
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
        self.saving = true;
        let service = self.service.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = match service
                .update_reward_template_for_admin(true, draft.update())
                .await
            {
                Ok(row) => AdminAsyncResult::Saved(Box::new(row)),
                Err(error) => AdminAsyncResult::Failed(format!("Admin save failed: {error:#}")),
            };
            let _ = tx.send(result);
        });
        Some(Banner::success("Saving reward template"))
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_templates().len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    fn sync_draft_to_selected(&mut self) {
        self.draft = self.selected_template().map(AdminDraft::from_row);
        self.dirty = false;
        self.editing = false;
        self.edit_buffer.clear();
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

fn reward_step(current: i64) -> i64 {
    if current >= 1_000 { 100 } else { 25 }
}
