//! What a click in Settings can land on, and the panes the wheel scrolls.
//! The bookkeeping itself is `app/common/mouse.rs`.
use super::state::{AccountRow, IrcTokenFocus, Row, Tab, TweakRow};

pub(crate) type MouseState = crate::app::common::mouse::MouseState<Target, Pane>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    Username,
    System,
    Feed,
    InviteCode,
    LinkCode,
    LinkConfirm,
    DeleteConfirm,
}

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
    Caret(Field, usize),
    AddInviteCode,
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
