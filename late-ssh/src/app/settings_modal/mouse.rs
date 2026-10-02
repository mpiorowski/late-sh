//! Geometry belongs to the last painted foreground surface. Offsets belong to
//! panes, independently of keyboard selection and editable text.
use std::cell::{Cell, RefCell};

use ratatui::layout::Rect;

use super::state::{AccountRow, IrcTokenFocus, Row, Tab, TweakRow};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Username,
    System,
    Bio,
    Feed,
    LinkCode,
    LinkConfirm,
    DeleteConfirm,
}

#[cfg(test)]
#[path = "mouse_test.rs"]
mod mouse_test;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
    Close,
    Tab(Tab),
    Row(Row),
    RowCycle(Row, bool),
    Tweak(TweakRow),
    TweakCycle(TweakRow, bool),
    SidebarMode,
    SidebarPanels,
    Account(AccountRow),
    Theme(usize),
    Star(usize),
    Search,
    Status(usize),
    StatusToggle(usize),
    StatusMove(usize, isize),
    Dial(usize),
    DialCycle(usize, bool),
    Sidebar(usize),
    SidebarMove(usize, isize),
    Badge(usize),
    Pick(usize),
    Bio,
    Feed(uuid::Uuid),
    AddFeed,
    RemoveFeed,
    RefreshFeeds,
    Submit,
    Cancel,
    Caret(Field, usize, usize),
    GenerateCode,
    LookupCode,
    KeepAccount(bool),
    ConfirmLink,
    ConfirmDelete,
    Irc(IrcTokenFocus),
    DismissToken,
    Gem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pane {
    Settings,
    Tweaks,
    Account,
    Feeds,
    Themes,
    Bio,
    StatusList,
    StatusDetail,
    Picker,
    Sidebar,
    Badges,
    Link,
    Delete,
    Irc,
}
const PANES: usize = 14;

#[derive(Default)]
pub(crate) struct MouseState {
    pub(crate) hits: RefCell<Vec<(Rect, Target)>>,
    panes: RefCell<Vec<(Rect, Pane, usize)>>,
    offsets: Cell<[usize; PANES]>,
    reveal: Cell<bool>,
    size: Cell<(u16, u16)>,
    valid: Cell<bool>,
}

impl MouseState {
    pub(crate) fn begin(&self, size: (u16, u16)) {
        self.clear_surface();
        if self.size.replace(size) != size {
            self.reveal.set(true);
        }
        self.valid.set(true);
    }

    pub(crate) fn clear_surface(&self) {
        self.hits.borrow_mut().clear();
        self.panes.borrow_mut().clear();
    }

    pub(crate) fn invalidate(&self) {
        self.valid.set(false);
    }

    pub(crate) fn reveal_selection(&self) {
        self.reveal.set(true);
        self.invalidate();
    }

    pub(crate) fn reset_pane(&self, pane: Pane) {
        let mut offsets = self.offsets.get();
        offsets[pane as usize] = 0;
        self.offsets.set(offsets);
        self.invalidate();
    }

    pub(crate) fn finish(&self) {
        self.reveal.set(false);
    }

    pub(crate) fn hit(&self, rect: Rect, target: Target) {
        if !rect.is_empty() {
            self.hits.borrow_mut().push((rect, target));
        }
    }

    pub(crate) fn target(&self, x: u16, y: u16, size: (u16, u16)) -> Option<Target> {
        if !self.valid.get() || self.size.get() != size {
            return None;
        }
        self.hits
            .borrow()
            .iter()
            .rev()
            .find_map(|(rect, target)| rect.contains((x, y).into()).then_some(*target))
    }

    pub(crate) fn pane(&self, area: Rect, pane: Pane, rows: usize, focus: usize) -> usize {
        let max = rows.saturating_sub(area.height as usize);
        let mut offsets = self.offsets.get();
        let offset = &mut offsets[pane as usize];
        *offset = (*offset).min(max);
        if self.reveal.get() && area.height > 0 {
            if focus < *offset {
                *offset = focus;
            } else if focus >= *offset + area.height as usize {
                *offset = focus + 1 - area.height as usize;
            }
            *offset = (*offset).min(max);
        }
        let result = *offset;
        self.offsets.set(offsets);
        self.panes.borrow_mut().push((area, pane, max));
        result
    }

    pub(crate) fn scroll(&self, x: u16, y: u16, delta: isize, size: (u16, u16)) {
        if !self.valid.get() || self.size.get() != size {
            return;
        }
        if let Some((_, pane, max)) = self
            .panes
            .borrow()
            .iter()
            .rev()
            .find(|(area, _, _)| area.contains((x, y).into()))
        {
            let mut offsets = self.offsets.get();
            let offset = &mut offsets[*pane as usize];
            *offset = offset.saturating_add_signed(delta).min(*max);
            self.offsets.set(offsets);
            self.hits.borrow_mut().clear();
        }
    }

    /// Translate virtual-body hits into the visible viewport, dropping clipped
    /// cells so an invisible control can never be activated.
    pub(crate) fn translate(&self, from: usize, area: Rect, offset: usize) {
        let mut hits = self.hits.borrow_mut();
        let local = hits.split_off(from);
        for (rect, target) in local {
            let top = usize::from(rect.y).max(offset);
            let bottom = usize::from(rect.bottom()).min(offset + usize::from(area.height));
            if bottom > top && rect.x < area.width {
                hits.push((
                    Rect::new(
                        area.x + rect.x,
                        area.y + (top - offset) as u16,
                        rect.width.min(area.width - rect.x),
                        (bottom - top) as u16,
                    ),
                    target,
                ));
            }
        }
    }
}
