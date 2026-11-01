//! Home's named layouts: up to nine per device, one of them current, each
//! a tree with its look (`RiceLayout`). Stored on the SSH key the session
//! authenticated with (`user_ssh_keys.settings.home_layouts`, the blob
//! `Layouts` serializes to), switched with `Ctrl+F` then a digit, from the
//! layouts list (`Ctrl+F L`), or with `/layout`. Nothing switches a layout
//! on its own: not `/golive`, not a game waiting, not the screensaver.

use serde::{Deserialize, Serialize};

use super::state::RiceLayout;

/// How many layouts a device holds, one per digit key.
pub const MAX_LAYOUTS: usize = 9;
/// The longest name a layout takes; the list and the status row read it.
pub const MAX_LAYOUT_NAME_CHARS: usize = 16;
/// The two stock layouts every device starts with: Home as it always
/// drew, and Zen's page as a layout of Home.
pub const STOCK_HOME_NAME: &str = "home";
pub const STOCK_ZEN_NAME: &str = "zen";

/// One saved layout: its name, as the list and `/layout <name>` know it,
/// and the tree with its look.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedLayout {
    pub name: String,
    pub layout: RiceLayout,
}

/// A device's layouts and which one Home shows. Never empty, never past
/// `MAX_LAYOUTS`, `current` always a valid index: the constructors and
/// the parse keep it so, so the readers never check.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Layouts {
    current: usize,
    items: Vec<NamedLayout>,
}

/// The stored shape, checked into a `Layouts` on read.
#[derive(Deserialize)]
struct StoredLayouts {
    current: usize,
    items: Vec<NamedLayout>,
}

/// Why a list edit was refused; `message` is the banner that says so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutsError {
    Full,
    BlankName,
    LongName,
    NameTaken,
    NoSuchLayout,
    LastOne,
}

impl LayoutsError {
    pub fn message(self) -> &'static str {
        match self {
            LayoutsError::Full => "Nine layouts is the lot: delete one first",
            LayoutsError::BlankName => "A layout needs a name",
            LayoutsError::LongName => "A layout name stops at sixteen characters",
            LayoutsError::NameTaken => "A layout already has that name",
            LayoutsError::NoSuchLayout => "No layout by that name or number",
            LayoutsError::LastOne => "The last layout stays",
        }
    }
}

impl Layouts {
    /// A device's first set: stock Home current, Zen's page second.
    pub fn stock() -> Self {
        Self::seeded(None, None)
    }

    /// A device that arranged its pages before they had names: its Home as
    /// `home`, its Zen as `zen`, each the stock page when it never did. A
    /// stored page that no longer parses reads as its stock.
    pub fn seeded(home: Option<&serde_json::Value>, zen: Option<&serde_json::Value>) -> Self {
        Self {
            current: 0,
            items: vec![
                NamedLayout {
                    name: STOCK_HOME_NAME.to_string(),
                    layout: RiceLayout::from_json_or(home, RiceLayout::home()),
                },
                NamedLayout {
                    name: STOCK_ZEN_NAME.to_string(),
                    layout: RiceLayout::from_json_or(zen, RiceLayout::default()),
                },
            ],
        }
    }

    /// Parse a stored set; anything unreadable, empty, over the cap or
    /// pointing past its items falls back to `fallback`, so a bad row never
    /// locks someone out of Home.
    pub fn from_json_or(value: Option<&serde_json::Value>, fallback: Layouts) -> Self {
        let Some(value) = value else {
            return fallback;
        };
        let stored: StoredLayouts = match serde_json::from_value(value.clone()) {
            Ok(stored) => stored,
            Err(_) => return fallback,
        };
        let sound = !stored.items.is_empty()
            && stored.items.len() <= MAX_LAYOUTS
            && stored.current < stored.items.len();
        match sound {
            true => Self {
                current: stored.current,
                items: stored.items,
            },
            false => fallback,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_full(&self) -> bool {
        self.items.len() >= MAX_LAYOUTS
    }

    pub fn items(&self) -> &[NamedLayout] {
        &self.items
    }

    pub fn get(&self, index: usize) -> Option<&NamedLayout> {
        self.items.get(index)
    }

    pub fn current_layout(&self) -> &RiceLayout {
        &self.items[self.current].layout
    }

    pub fn current_name(&self) -> &str {
        &self.items[self.current].name
    }

    /// What `R` puts back: Zen's page for the layout named `zen`, stock
    /// Home for every other name.
    pub fn stock_of(name: &str) -> RiceLayout {
        match name == STOCK_ZEN_NAME {
            true => RiceLayout::default(),
            false => RiceLayout::home(),
        }
    }

    pub fn current_stock(&self) -> RiceLayout {
        Self::stock_of(self.current_name())
    }

    /// The page as it is now, written back into the current slot. Called
    /// before the set is stored or another layout is loaded, so the slot
    /// never lags the page.
    pub fn store_current(&mut self, layout: RiceLayout) {
        self.items[self.current].layout = layout;
    }

    /// Make `index` current. False past the end; the caller loads the
    /// layout onto the page itself.
    pub fn switch(&mut self, index: usize) -> bool {
        match index < self.items.len() {
            true => {
                self.current = index;
                true
            }
            false => false,
        }
    }

    /// The slot a name points at, case and surrounding space aside.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        let wanted = name.trim().to_lowercase();
        self.items
            .iter()
            .position(|item| item.name.to_lowercase() == wanted)
    }

    /// Add `layout` under `name` and make it current. Refused when the set
    /// is full, the name is blank, too long or taken.
    pub fn save_new(&mut self, name: &str, layout: RiceLayout) -> Result<usize, LayoutsError> {
        if self.is_full() {
            return Err(LayoutsError::Full);
        }
        let name = self.free_name(name)?;
        self.items.push(NamedLayout { name, layout });
        self.current = self.items.len() - 1;
        Ok(self.current)
    }

    /// Give the layout at `index` a new name, under the same rules as
    /// `save_new`; its own name is not taken.
    pub fn rename(&mut self, index: usize, name: &str) -> Result<(), LayoutsError> {
        if index >= self.items.len() {
            return Err(LayoutsError::NoSuchLayout);
        }
        let cleaned = clean_name(name)?;
        let taken = self.index_of(&cleaned).is_some_and(|other| other != index);
        if taken {
            return Err(LayoutsError::NameTaken);
        }
        self.items[index].name = cleaned;
        Ok(())
    }

    /// Drop the layout at `index`, never the last one. Deleting the current
    /// one makes its neighbor current (the one before it, else the first);
    /// the caller loads it onto the page.
    pub fn delete(&mut self, index: usize) -> Result<(), LayoutsError> {
        if index >= self.items.len() {
            return Err(LayoutsError::NoSuchLayout);
        }
        if self.items.len() == 1 {
            return Err(LayoutsError::LastOne);
        }
        self.items.remove(index);
        if self.current > index || self.current == self.items.len() {
            self.current -= 1;
        }
        Ok(())
    }

    /// Put the layout at `index` back to its stock (`stock_of` its name).
    /// False past the end.
    pub fn reset(&mut self, index: usize) -> bool {
        match self.items.get_mut(index) {
            Some(item) => {
                item.layout = Self::stock_of(&item.name);
                true
            }
            None => false,
        }
    }

    fn free_name(&self, name: &str) -> Result<String, LayoutsError> {
        let cleaned = clean_name(name)?;
        match self.index_of(&cleaned) {
            Some(_) => Err(LayoutsError::NameTaken),
            None => Ok(cleaned),
        }
    }
}

/// A name as stored: trimmed, at least one character, at most
/// `MAX_LAYOUT_NAME_CHARS`.
fn clean_name(name: &str) -> Result<String, LayoutsError> {
    let cleaned = name.trim();
    if cleaned.is_empty() {
        return Err(LayoutsError::BlankName);
    }
    if cleaned.chars().count() > MAX_LAYOUT_NAME_CHARS {
        return Err(LayoutsError::LongName);
    }
    Ok(cleaned.to_string())
}

/// What `/layout` asked for, parsed by the composer and run by Home:
/// `/layout` alone lists, `/layout <name or digit>` loads, `/layout save
/// <name>` saves the page as a new layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LayoutCommand {
    List,
    Load(String),
    Save(String),
}

impl LayoutCommand {
    /// The words after `/layout`, or `None` for a body that is not the
    /// command (`/layouts`, say, is not).
    pub fn parse(body: &str) -> Option<Self> {
        let rest = body.trim().strip_prefix("/layout")?;
        if !(rest.is_empty() || rest.starts_with(' ')) {
            return None;
        }
        let rest = rest.trim();
        if rest.is_empty() {
            return Some(LayoutCommand::List);
        }
        match rest.strip_prefix("save") {
            Some(name) if name.is_empty() || name.starts_with(' ') => {
                Some(LayoutCommand::Save(name.trim().to_string()))
            }
            Some(_) | None => Some(LayoutCommand::Load(rest.to_string())),
        }
    }
}

/// The layouts list `L` opens in arrange mode: the selected row, and the
/// name being typed when `n` (save the page as new) or `r` (rename the
/// selected) is under way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayoutList {
    pub selected: usize,
    pub editing: Option<NameEdit>,
}

impl LayoutList {
    pub fn open(selected: usize) -> Self {
        Self {
            selected,
            editing: None,
        }
    }

    /// Move the selection by `delta`, clamped to `len` rows.
    pub fn move_by(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let last = len as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, last) as usize;
    }
}

/// A name under edit in the list and what it is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameEdit {
    pub purpose: NamePurpose,
    pub text: String,
}

impl NameEdit {
    pub fn new(purpose: NamePurpose, text: String) -> Self {
        Self { purpose, text }
    }

    /// Type one character; a name past the cap takes no more.
    pub fn push(&mut self, ch: char) {
        if self.text.chars().count() < MAX_LAYOUT_NAME_CHARS {
            self.text.push(ch);
        }
    }

    pub fn pop(&mut self) {
        self.text.pop();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamePurpose {
    /// Save the page as a new layout under the typed name.
    SaveNew,
    /// Rename the layout at this index.
    Rename(usize),
}

#[cfg(test)]
#[path = "presets_test.rs"]
mod presets_test;
