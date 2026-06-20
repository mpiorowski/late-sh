use std::cell::Cell;

use chrono::{DateTime, Utc};
use late_core::models::profile::{Profile, ProfileParams, normalize_profile_tags};
use late_core::models::rss_feed::RssFeed;
use late_core::models::user::{
    RightSidebarComponentSetting, RightSidebarMode, sanitize_username_input,
};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

use crate::app::common::theme;
use crate::app::profile::svc::{IrcTokenStatus, ProfileEvent, ProfileService};
use crate::app::{
    chat::feeds::svc::{FeedEvent, FeedService, FeedSnapshot},
    common::primitives::Banner,
};

use super::data::{CountryOption, filter_countries, filter_timezones};
use super::gem::GemState;

pub(crate) const USERNAME_MAX_LEN: usize = 12;
const DELETE_CONFIRM_USERNAME_MAX_LEN: usize = late_core::models::user::USERNAME_MAX_LEN;
const LINK_CODE_MAX_LEN: usize = 16;
const LINK_CONFIRM_USERNAME_MAX_LEN: usize = late_core::models::user::USERNAME_MAX_LEN;
pub(crate) const SYSTEM_FIELD_MAX_LEN: usize = 48;
pub(crate) const FEED_URL_MAX_LEN: usize = 2000;
pub const BIO_MAX_LEN: usize = 1000;
pub const DELETE_CONFIRM_MISMATCH: &str = "Typed username does not match current username.";
pub const LINK_CONFIRM_MISMATCH: &str = "Typed username does not match the main username.";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickerKind {
    Country,
    Timezone,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    Username,
    Birthday,
    Ide,
    Terminal,
    Os,
    Langs,
    Theme,
    Country,
    Timezone,
    DirectMessages,
    Mentions,
    GameEvents,
    Bell,
    Cooldown,
    NotifyFormat,
}

impl Row {
    pub const ALL: [Row; 15] = [
        Row::Username,
        Row::Country,
        Row::Timezone,
        Row::Birthday,
        Row::Theme,
        Row::Ide,
        Row::Terminal,
        Row::Os,
        Row::Langs,
        Row::DirectMessages,
        Row::Mentions,
        Row::GameEvents,
        Row::Bell,
        Row::Cooldown,
        Row::NotifyFormat,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountRow {
    LinkAccounts,
    IrcToken,
    DeleteAccount,
}

impl AccountRow {
    pub const ALL: [AccountRow; 3] = [
        AccountRow::LinkAccounts,
        AccountRow::IrcToken,
        AccountRow::DeleteAccount,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TweakRow {
    // Appearance group.
    BackgroundColor,
    RightSidebar,
    RoomListSidebar,
    LoungeInfo,
    // Compose / Music / Display / Other groups.
    ComposerKeepFocused,
    StartWithMusicMuted,
    FlagFallback,
    ShowSettingsOnConnect,
}

impl TweakRow {
    pub const ALL: [TweakRow; 8] = [
        TweakRow::BackgroundColor,
        TweakRow::RightSidebar,
        TweakRow::RoomListSidebar,
        TweakRow::LoungeInfo,
        TweakRow::ComposerKeepFocused,
        TweakRow::StartWithMusicMuted,
        TweakRow::FlagFallback,
        TweakRow::ShowSettingsOnConnect,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkAccountStep {
    EnterCode,
    Confirm,
    Pending,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkAccountEnterCodeFocus {
    GenerateCode,
    PeerCode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SystemField {
    Birthday,
    Ide,
    Terminal,
    Os,
    Langs,
}

impl SystemField {
    pub(crate) fn from_row(row: Row) -> Option<Self> {
        match row {
            Row::Birthday => Some(Self::Birthday),
            Row::Ide => Some(Self::Ide),
            Row::Terminal => Some(Self::Terminal),
            Row::Os => Some(Self::Os),
            Row::Langs => Some(Self::Langs),
            _ => None,
        }
    }

    fn value(self, profile: &Profile) -> Option<String> {
        match self {
            Self::Birthday => profile.birthday.clone(),
            Self::Ide => profile.ide.clone(),
            Self::Terminal => profile.terminal.clone(),
            Self::Os => profile.os.clone(),
            Self::Langs => (!profile.langs.is_empty()).then(|| profile.langs.join(", ")),
        }
    }

    fn set_value(self, profile: &mut Profile, text: String) {
        match self {
            Self::Birthday => {
                profile.birthday = late_core::models::birthday::normalize_birthday(&text);
            }
            Self::Ide => profile.ide = normalize_optional_text(&text),
            Self::Terminal => profile.terminal = normalize_optional_text(&text),
            Self::Os => profile.os = normalize_optional_text(&text),
            Self::Langs => {
                profile.langs = normalize_profile_tags([text.as_str()]);
            }
        }
    }
}

/// Top-level tab in the settings modal. `Settings` holds every compact row
/// (identity/appearance/location/notifications); `Themes` is a fast browser
/// for the expanded theme catalog; `Bio` is a separate full-width pane with
/// the markdown editor + preview; `Tweaks` holds power-user toggles and the
/// gem easter egg.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tab {
    Settings,
    Tweaks,
    Bio,
    Themes,
    Account,
    Feeds,
}

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Settings,
        Tab::Bio,
        Tab::Themes,
        Tab::Tweaks,
        Tab::Account,
        Tab::Feeds,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Tab::Settings => "Settings",
            Tab::Tweaks => "Tweaks",
            Tab::Bio => "Bio",
            Tab::Themes => "Themes",
            Tab::Account => "Account",
            Tab::Feeds => "RSS",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeTreeRow {
    Group {
        group: theme::ThemeGroup,
        collapsed: bool,
    },
    Theme {
        option_index: usize,
        last_in_group: bool,
    },
}

#[derive(Default)]
pub struct PickerState {
    pub kind: Option<PickerKind>,
    pub query: String,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub visible_height: Cell<usize>,
}

pub struct DeleteAccountDialogState {
    open: bool,
    input: TextArea<'static>,
    status: Option<String>,
    pending: bool,
}

impl DeleteAccountDialogState {
    fn new() -> Self {
        Self {
            open: false,
            input: new_short_textarea(false),
            status: None,
            pending: false,
        }
    }

    pub fn open(&self) -> bool {
        self.open
    }

    pub fn input(&self) -> &TextArea<'static> {
        &self.input
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn pending(&self) -> bool {
        self.pending
    }
}

/// Which action button is focused in the IRC token dialog. `Reset` and
/// `Revoke` are only reachable when a token currently exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrcTokenFocus {
    /// Create (no token yet) or Reset (token exists) — same slot.
    Primary,
    Revoke,
}

/// Settings → Account IRC token dialog. Drives mint/reset/revoke and shows a
/// freshly minted token exactly once. See devdocs/FRD-IRCD.md §5.
pub struct IrcTokenDialogState {
    open: bool,
    /// `None` while the status load is in flight; `Some(None)` = no token;
    /// `Some(Some(_))` = an active token with metadata.
    status: Option<Option<IrcTokenStatus>>,
    focus: IrcTokenFocus,
    /// Plaintext token to display exactly once, right after minting.
    revealed_token: Option<String>,
    /// True once the user has armed the (destructive) revoke and must confirm.
    confirming_revoke: bool,
    pending: bool,
    message: Option<String>,
}

impl IrcTokenDialogState {
    fn new() -> Self {
        Self {
            open: false,
            status: None,
            focus: IrcTokenFocus::Primary,
            revealed_token: None,
            confirming_revoke: false,
            pending: false,
            message: None,
        }
    }

    pub fn open(&self) -> bool {
        self.open
    }

    /// `None` while loading, otherwise the current token status.
    pub fn status(&self) -> Option<&Option<IrcTokenStatus>> {
        self.status.as_ref()
    }

    pub fn has_token(&self) -> bool {
        matches!(self.status, Some(Some(_)))
    }

    pub fn focus(&self) -> IrcTokenFocus {
        self.focus
    }

    pub fn revealed_token(&self) -> Option<&str> {
        self.revealed_token.as_deref()
    }

    pub fn confirming_revoke(&self) -> bool {
        self.confirming_revoke
    }

    pub fn pending(&self) -> bool {
        self.pending
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

pub struct LinkAccountDialogState {
    open: bool,
    step: LinkAccountStep,
    own_code: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    enter_code_focus: LinkAccountEnterCodeFocus,
    code_input: TextArea<'static>,
    peer_user_id: Option<Uuid>,
    peer_username: Option<String>,
    peer_created: Option<DateTime<Utc>>,
    keep_current: bool,
    confirm_input: TextArea<'static>,
    status: Option<String>,
    pending: bool,
}

impl LinkAccountDialogState {
    fn new() -> Self {
        Self {
            open: false,
            step: LinkAccountStep::EnterCode,
            own_code: None,
            expires_at: None,
            enter_code_focus: LinkAccountEnterCodeFocus::GenerateCode,
            code_input: new_short_textarea(false),
            peer_user_id: None,
            peer_username: None,
            peer_created: None,
            keep_current: true,
            confirm_input: new_short_textarea(false),
            status: None,
            pending: false,
        }
    }

    pub fn open(&self) -> bool {
        self.open
    }

    pub fn step(&self) -> LinkAccountStep {
        self.step
    }

    pub fn own_code(&self) -> Option<&str> {
        self.own_code.as_deref()
    }

    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.expires_at.as_ref().cloned()
    }

    pub fn enter_code_focus(&self) -> LinkAccountEnterCodeFocus {
        self.enter_code_focus
    }

    pub fn code_input(&self) -> &TextArea<'static> {
        &self.code_input
    }

    pub fn peer_username(&self) -> Option<&str> {
        self.peer_username.as_deref()
    }

    pub fn peer_created(&self) -> Option<DateTime<Utc>> {
        self.peer_created.as_ref().cloned()
    }

    pub fn keep_current(&self) -> bool {
        self.keep_current
    }

    pub fn confirm_input(&self) -> &TextArea<'static> {
        &self.confirm_input
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    pub fn pending(&self) -> bool {
        self.pending
    }
}

pub struct SettingsModalState {
    profile_service: ProfileService,
    feed_service: FeedService,
    user_id: Uuid,
    draft: Profile,
    selected_tab: Tab,
    row_index: usize,
    account_row_index: usize,
    tweak_row_index: usize,
    theme_index: usize,
    theme_selected_row: usize,
    theme_scroll_offset: usize,
    theme_visible_height: Cell<usize>,
    theme_collapsed_groups: u32,
    editing_username: bool,
    username_input: TextArea<'static>,
    editing_system_field: Option<SystemField>,
    system_input: TextArea<'static>,
    editing_bio: bool,
    bio_input: TextArea<'static>,
    picker: PickerState,
    link_account: LinkAccountDialogState,
    delete_account: DeleteAccountDialogState,
    irc_token: IrcTokenDialogState,
    right_sidebar_components_open: bool,
    right_sidebar_components_index: usize,
    feeds: Vec<RssFeed>,
    feed_index: usize,
    editing_feed_url: bool,
    feed_url_input: TextArea<'static>,
    feed_snapshot_rx: watch::Receiver<FeedSnapshot>,
    feed_event_rx: broadcast::Receiver<FeedEvent>,
    profile_event_rx: broadcast::Receiver<ProfileEvent>,
    /// Per-session gem easter egg on the Special tab. Persists across modal
    /// open/close cycles for the lifetime of the SSH session.
    gem: GemState,
    /// On-screen rects for each tab in the strip, indexed by the tab's
    /// position in `Tab::ALL`. `None` if the tab is currently hidden (e.g.
    /// the Special tab before it's unlocked). Populated by the renderer
    /// each frame.
    tab_rects: Cell<[Option<Rect>; Tab::ALL.len()]>,
    /// Bounds of the body area (whichever tab is showing). Used to gate
    /// scroll-wheel events to the body, so the wheel doesn't move the
    /// row cursor when the pointer is hovering over the tab strip or footer.
    body_area: Cell<Rect>,
}

impl SettingsModalState {
    pub fn new(profile_service: ProfileService, feed_service: FeedService, user_id: Uuid) -> Self {
        let feed_snapshot_rx = feed_service.subscribe_snapshot();
        let feed_event_rx = feed_service.subscribe_events();
        let profile_event_rx = profile_service.subscribe_events();
        feed_service.list_task(user_id);
        Self {
            profile_service,
            feed_service,
            user_id,
            draft: Profile::default(),
            selected_tab: Tab::Settings,
            row_index: 0,
            account_row_index: 0,
            tweak_row_index: 0,
            theme_index: 0,
            theme_selected_row: 0,
            theme_scroll_offset: 0,
            theme_visible_height: Cell::new(1),
            theme_collapsed_groups: 0,
            editing_username: false,
            username_input: new_short_textarea(false),
            editing_system_field: None,
            system_input: new_short_textarea(false),
            editing_bio: false,
            bio_input: new_bio_textarea(false),
            picker: PickerState::default(),
            link_account: LinkAccountDialogState::new(),
            delete_account: DeleteAccountDialogState::new(),
            irc_token: IrcTokenDialogState::new(),
            right_sidebar_components_open: false,
            right_sidebar_components_index: 0,
            feeds: Vec::new(),
            feed_index: 0,
            editing_feed_url: false,
            feed_url_input: new_short_textarea(false),
            feed_snapshot_rx,
            feed_event_rx,
            profile_event_rx,
            gem: GemState::new(),
            tab_rects: Cell::new([None; Tab::ALL.len()]),
            body_area: Cell::new(Rect::new(0, 0, 0, 0)),
        }
    }

    pub fn gem(&self) -> &GemState {
        &self.gem
    }

    pub fn gem_mut(&mut self) -> &mut GemState {
        &mut self.gem
    }

    pub fn open_from_profile(&mut self, profile: &Profile) {
        self.draft = profile.clone();
        self.selected_tab = Tab::Settings;
        self.row_index = 0;
        self.account_row_index = 0;
        self.tweak_row_index = 0;
        self.sync_theme_index_to_draft();
        self.editing_username = false;
        self.username_input = new_short_textarea(false);
        self.editing_system_field = None;
        self.system_input = new_short_textarea(false);
        self.editing_bio = false;
        self.bio_input = bio_textarea_for_readonly_text(&self.draft.bio);
        self.picker = PickerState::default();
        self.link_account = LinkAccountDialogState::new();
        self.delete_account = DeleteAccountDialogState::new();
        self.irc_token = IrcTokenDialogState::new();
        self.right_sidebar_components_open = false;
        self.right_sidebar_components_index = 0;
        self.feed_service.list_task(self.user_id);
    }

    pub fn tick(&mut self) -> Option<Banner> {
        self.drain_feed_snapshot();
        let mut banner = self.drain_profile_events();
        if let Some(feed_banner) = self.drain_feed_events() {
            banner = Some(feed_banner);
        }
        banner
    }

    pub fn selected_tab(&self) -> Tab {
        self.selected_tab
    }

    /// Switch to the neighboring tab. Auto-saves + ends any in-flight bio
    /// edit when leaving the Bio tab so the preview reflects the draft.
    /// Skips the Special tab while it's hidden (no bio/country/timezone).
    pub fn cycle_tab(&mut self, forward: bool) {
        let visible = self.visible_tabs();
        let idx = visible
            .iter()
            .position(|t| *t == self.selected_tab)
            .unwrap_or(0);
        let next_idx = if forward {
            (idx + 1) % visible.len()
        } else {
            (idx + visible.len() - 1) % visible.len()
        };
        self.switch_tab(visible[next_idx]);
    }

    /// Jump directly to a specific tab (e.g. via a mouse click on the tab
    /// strip), running the same auto-save / edit-cleanup logic as `cycle_tab`.
    /// Ignored if the tab isn't currently visible (e.g. clicking a stale
    /// rect for the Special tab after it was hidden again).
    pub fn select_tab(&mut self, next: Tab) {
        if !self.visible_tabs().contains(&next) || next == self.selected_tab {
            return;
        }
        self.switch_tab(next);
    }

    fn switch_tab(&mut self, next: Tab) {
        if self.selected_tab == Tab::Bio && next != Tab::Bio && self.editing_bio {
            self.stop_bio_edit();
            self.save();
        }
        if self.selected_tab == Tab::Settings && self.editing_username {
            // Leaving the Settings tab mid-username-edit → commit what's typed.
            self.submit_username();
            self.save();
        }
        if self.selected_tab == Tab::Settings && self.editing_system_field.is_some() {
            self.submit_system_field();
            self.save();
        }
        if self.selected_tab == Tab::Feeds && self.editing_feed_url {
            self.cancel_feed_url_edit();
        }
        if next == Tab::Themes {
            self.sync_theme_index_to_draft();
        }
        self.selected_tab = next;
    }

    pub fn set_tab_rects(&self, rects: [Option<Rect>; Tab::ALL.len()]) {
        self.tab_rects.set(rects);
    }

    pub fn set_body_area(&self, area: Rect) {
        self.body_area.set(area);
    }

    /// Hit-test the tab strip. Returns the tab whose cell contains the
    /// (0-based ratatui) point, if any.
    pub fn tab_at_point(&self, x: u16, y: u16) -> Option<Tab> {
        let rects = self.tab_rects.get();
        Tab::ALL
            .iter()
            .copied()
            .zip(rects.iter())
            .find_map(|(tab, slot)| slot.filter(|rect| rect_contains(*rect, x, y)).map(|_| tab))
    }

    pub fn body_contains(&self, x: u16, y: u16) -> bool {
        rect_contains(self.body_area.get(), x, y)
    }

    /// Tabs to show in the tab strip in display order. All tabs are always
    /// visible — there is no unlock gating.
    pub fn visible_tabs(&self) -> Vec<Tab> {
        Tab::ALL.to_vec()
    }

    pub fn set_modal_width(&mut self, _modal_width: u16) {
        // TextArea wraps internally at render time; nothing to sync here.
    }

    pub fn draft(&self) -> &Profile {
        &self.draft
    }

    pub fn selected_row(&self) -> Row {
        Row::ALL[self.row_index]
    }

    pub fn right_sidebar_components_open(&self) -> bool {
        self.right_sidebar_components_open
    }

    pub fn open_right_sidebar_components(&mut self) {
        self.right_sidebar_components_open = true;
        self.right_sidebar_components_index = 0;
    }

    pub fn close_right_sidebar_components(&mut self) {
        self.right_sidebar_components_open = false;
    }

    pub fn right_sidebar_components_index(&self) -> usize {
        self.right_sidebar_components_index
    }

    pub fn right_sidebar_components(&self) -> &[RightSidebarComponentSetting] {
        &self.draft.right_sidebar_components
    }

    pub fn move_right_sidebar_components_cursor(&mut self, delta: isize) {
        let last = self
            .draft
            .right_sidebar_components
            .len()
            .saturating_sub(1) as isize;
        self.right_sidebar_components_index =
            (self.right_sidebar_components_index as isize + delta).clamp(0, last) as usize;
    }

    /// Toggle the on/off state of the selected component.
    pub fn toggle_right_sidebar_component(&mut self) {
        if let Some(setting) = self
            .draft
            .right_sidebar_components
            .get_mut(self.right_sidebar_components_index)
        {
            setting.enabled ^= true;
            self.save();
        }
    }

    /// Move the selected component up or down in the render order, keeping the
    /// cursor on the moved row.
    pub fn move_right_sidebar_component(&mut self, delta: isize) {
        let len = self.draft.right_sidebar_components.len();
        if len == 0 {
            return;
        }
        let from = self.right_sidebar_components_index;
        let to = (from as isize + delta).clamp(0, len as isize - 1) as usize;
        if to == from {
            return;
        }
        let setting = self.draft.right_sidebar_components.remove(from);
        self.draft.right_sidebar_components.insert(to, setting);
        self.right_sidebar_components_index = to;
        self.save();
    }

    pub fn selected_account_row(&self) -> AccountRow {
        AccountRow::ALL[self.account_row_index]
    }

    pub fn move_account_row(&mut self, delta: isize) {
        let last = AccountRow::ALL.len().saturating_sub(1) as isize;
        self.account_row_index = (self.account_row_index as isize + delta).clamp(0, last) as usize;
    }

    pub fn selected_tweak_row(&self) -> TweakRow {
        TweakRow::ALL[self.tweak_row_index]
    }

    pub fn move_tweak_row(&mut self, delta: isize) {
        let last = TweakRow::ALL.len().saturating_sub(1) as isize;
        self.tweak_row_index = (self.tweak_row_index as isize + delta).clamp(0, last) as usize;
    }

    pub fn toggle_selected_tweak(&mut self) {
        match self.selected_tweak_row() {
            TweakRow::BackgroundColor => {
                self.draft.enable_background_color ^= true;
            }
            TweakRow::RightSidebar => {
                self.draft.right_sidebar_mode = self.draft.right_sidebar_mode.cycle(true);
                self.draft.show_right_sidebar =
                    self.draft.right_sidebar_mode != RightSidebarMode::Off;
            }
            TweakRow::RoomListSidebar => {
                self.draft.show_room_list_sidebar ^= true;
            }
            TweakRow::LoungeInfo => {
                self.draft.show_dashboard_header ^= true;
            }
            TweakRow::ComposerKeepFocused => {
                self.draft.keep_composer_focused ^= true;
            }
            TweakRow::StartWithMusicMuted => {
                self.draft.start_with_music_muted ^= true;
            }
            TweakRow::FlagFallback => {
                self.draft.show_flag_fallback ^= true;
            }
            TweakRow::ShowSettingsOnConnect => {
                self.draft.show_settings_on_connect ^= true;
            }
        }
        self.save();
    }

    pub fn link_account_dialog(&self) -> &LinkAccountDialogState {
        &self.link_account
    }

    pub fn open_link_account_dialog(&mut self) {
        self.link_account = LinkAccountDialogState {
            open: true,
            step: LinkAccountStep::EnterCode,
            own_code: None,
            expires_at: None,
            enter_code_focus: LinkAccountEnterCodeFocus::GenerateCode,
            code_input: new_short_textarea(false),
            peer_user_id: None,
            peer_username: None,
            peer_created: None,
            keep_current: true,
            confirm_input: new_short_textarea(false),
            status: None,
            pending: false,
        };
    }

    pub fn close_link_account_dialog(&mut self) {
        self.link_account = LinkAccountDialogState::new();
    }

    pub fn generate_link_account_code(&mut self) {
        if self.link_account.pending {
            return;
        }
        self.link_account.pending = true;
        self.link_account.status = Some("Creating link code...".to_string());
        self.profile_service.create_account_link_code(self.user_id);
    }

    pub fn move_link_account_enter_code_focus(&mut self, focus: LinkAccountEnterCodeFocus) {
        if self.link_account.step != LinkAccountStep::EnterCode {
            return;
        }
        self.link_account.enter_code_focus = focus;
        set_short_textarea_cursor_visible(
            &mut self.link_account.code_input,
            focus == LinkAccountEnterCodeFocus::PeerCode,
        );
    }

    pub fn activate_link_account_enter_code(&mut self) {
        match self.link_account.enter_code_focus {
            LinkAccountEnterCodeFocus::GenerateCode => self.generate_link_account_code(),
            LinkAccountEnterCodeFocus::PeerCode => self.submit_link_account_code(),
        }
    }

    fn submit_link_account_code(&mut self) {
        if self.link_account.pending {
            return;
        }
        let code = self.link_account_code_text();
        if code.trim().is_empty() {
            self.link_account.status = Some("Enter the other account's code.".to_string());
            return;
        }
        self.link_account.pending = true;
        self.link_account.status = Some("Checking code...".to_string());
        self.profile_service
            .preview_account_link_code(self.user_id, code);
    }

    pub fn select_link_account_main(&mut self, keep_current: bool) {
        if self.link_account.keep_current != keep_current {
            self.link_account.keep_current = keep_current;
            self.link_account.confirm_input = new_short_textarea(true);
            self.link_account.status = None;
        }
    }

    pub fn submit_link_account_confirmation(&mut self) {
        if self.link_account.pending || self.link_account.step != LinkAccountStep::Confirm {
            return;
        }
        let Some(peer_user_id) = self.link_account.peer_user_id else {
            self.link_account.status = Some("Enter the other account's code first.".to_string());
            self.link_account.step = LinkAccountStep::EnterCode;
            return;
        };
        let Some(kept_username) = self.link_account_kept_username() else {
            self.link_account.status = Some("Choose the main account to keep.".to_string());
            return;
        };
        let typed = self.link_account_confirm_text();
        if typed != kept_username {
            self.link_account.status = Some(LINK_CONFIRM_MISMATCH.to_string());
            return;
        }
        let kept_user_id = if self.link_account.keep_current {
            self.user_id
        } else {
            peer_user_id
        };
        let code = self.link_account_code_text();
        self.link_account.pending = true;
        self.link_account.step = LinkAccountStep::Pending;
        self.link_account.status = Some("Linking accounts...".to_string());
        self.profile_service
            .complete_account_link(self.user_id, peer_user_id, code, kept_user_id);
    }

    pub fn link_account_kept_username(&self) -> Option<String> {
        if self.link_account.keep_current {
            Some(self.draft.username.clone())
        } else {
            self.link_account.peer_username.clone()
        }
    }

    pub fn link_account_push(&mut self, ch: char) {
        match self.link_account.step {
            LinkAccountStep::EnterCode => {
                if self.link_account.enter_code_focus != LinkAccountEnterCodeFocus::PeerCode {
                    self.move_link_account_enter_code_focus(LinkAccountEnterCodeFocus::PeerCode);
                }
                if single_line_char_count(&self.link_account.code_input) < LINK_CODE_MAX_LEN
                    && ch.is_ascii_alphanumeric()
                {
                    self.link_account
                        .code_input
                        .insert_char(ch.to_ascii_uppercase());
                    self.link_account.status = None;
                }
            }
            LinkAccountStep::Confirm => {
                if single_line_char_count(&self.link_account.confirm_input)
                    < LINK_CONFIRM_USERNAME_MAX_LEN
                    && !ch.is_control()
                    && ch != '\n'
                    && ch != '\r'
                {
                    self.link_account.confirm_input.insert_char(ch);
                    self.link_account.status = None;
                }
            }
            LinkAccountStep::Pending => {}
        }
    }

    pub fn link_account_backspace(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.delete_char();
            self.link_account.status = None;
        }
    }

    pub fn link_account_delete_right(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.delete_next_char();
            self.link_account.status = None;
        }
    }

    pub fn link_account_delete_word_left(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.delete_word();
            self.link_account.status = None;
        }
    }

    pub fn link_account_delete_word_right(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.delete_next_word();
            self.link_account.status = None;
        }
    }

    pub fn link_account_cursor_left(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.move_cursor(CursorMove::Back);
        }
    }

    pub fn link_account_cursor_right(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.move_cursor(CursorMove::Forward);
        }
    }

    pub fn link_account_cursor_word_left(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.move_cursor(CursorMove::WordBack);
        }
    }

    pub fn link_account_cursor_word_right(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.move_cursor(CursorMove::WordForward);
        }
    }

    pub fn link_account_cursor_home(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.move_cursor(CursorMove::Head);
        }
    }

    pub fn link_account_cursor_end(&mut self) {
        if let Some(input) = self.link_account_active_input_mut() {
            input.move_cursor(CursorMove::End);
        }
    }

    pub fn clear_link_account_input(&mut self) {
        match self.link_account.step {
            LinkAccountStep::EnterCode => {
                self.link_account.enter_code_focus = LinkAccountEnterCodeFocus::PeerCode;
                self.link_account.code_input = new_short_textarea(true);
            }
            LinkAccountStep::Confirm => {
                self.link_account.confirm_input = new_short_textarea(true);
            }
            LinkAccountStep::Pending => {}
        }
        self.link_account.status = None;
    }

    fn link_account_active_input_mut(&mut self) -> Option<&mut TextArea<'static>> {
        match self.link_account.step {
            LinkAccountStep::EnterCode
                if self.link_account.enter_code_focus == LinkAccountEnterCodeFocus::PeerCode =>
            {
                Some(&mut self.link_account.code_input)
            }
            LinkAccountStep::Confirm => Some(&mut self.link_account.confirm_input),
            LinkAccountStep::EnterCode | LinkAccountStep::Pending => None,
        }
    }

    fn link_account_code_text(&self) -> String {
        self.link_account.code_input.lines().join("")
    }

    fn link_account_confirm_text(&self) -> String {
        self.link_account.confirm_input.lines().join("")
    }

    pub fn delete_account_dialog(&self) -> &DeleteAccountDialogState {
        &self.delete_account
    }

    pub fn open_delete_account_dialog(&mut self) {
        self.delete_account.open = true;
        self.delete_account.input = new_short_textarea(true);
        self.delete_account.status = None;
        self.delete_account.pending = false;
    }

    pub fn close_delete_account_dialog(&mut self) {
        self.delete_account = DeleteAccountDialogState::new();
    }

    pub fn submit_delete_account_confirmation(&mut self) {
        if self.delete_account.pending {
            return;
        }
        let typed = self.delete_account_text();
        if typed != self.draft.username {
            self.delete_account.status = Some(DELETE_CONFIRM_MISMATCH.to_string());
            return;
        }
        self.delete_account.pending = true;
        self.delete_account.status = Some("Deleting account...".to_string());
        self.profile_service.delete_account(self.user_id);
    }

    pub fn delete_account_push(&mut self, ch: char) {
        if single_line_char_count(&self.delete_account.input) < DELETE_CONFIRM_USERNAME_MAX_LEN {
            self.delete_account.input.insert_char(ch);
            self.delete_account.status = None;
        }
    }

    pub fn delete_account_backspace(&mut self) {
        self.delete_account.input.delete_char();
        self.delete_account.status = None;
    }

    pub fn delete_account_delete_right(&mut self) {
        self.delete_account.input.delete_next_char();
        self.delete_account.status = None;
    }

    pub fn delete_account_delete_word_left(&mut self) {
        self.delete_account.input.delete_word();
        self.delete_account.status = None;
    }

    pub fn delete_account_delete_word_right(&mut self) {
        self.delete_account.input.delete_next_word();
        self.delete_account.status = None;
    }

    pub fn delete_account_cursor_left(&mut self) {
        self.delete_account.input.move_cursor(CursorMove::Back);
    }

    pub fn delete_account_cursor_right(&mut self) {
        self.delete_account.input.move_cursor(CursorMove::Forward);
    }

    pub fn delete_account_cursor_word_left(&mut self) {
        self.delete_account.input.move_cursor(CursorMove::WordBack);
    }

    pub fn delete_account_cursor_word_right(&mut self) {
        self.delete_account
            .input
            .move_cursor(CursorMove::WordForward);
    }

    pub fn delete_account_cursor_home(&mut self) {
        self.delete_account.input.move_cursor(CursorMove::Head);
    }

    pub fn delete_account_cursor_end(&mut self) {
        self.delete_account.input.move_cursor(CursorMove::End);
    }

    pub fn clear_delete_account_confirmation(&mut self) {
        self.delete_account.input = new_short_textarea(true);
        self.delete_account.status = None;
    }

    pub fn delete_account_text(&self) -> String {
        self.delete_account.input.lines().join("")
    }

    pub fn irc_token_dialog(&self) -> &IrcTokenDialogState {
        &self.irc_token
    }

    pub fn open_irc_token_dialog(&mut self) {
        self.irc_token = IrcTokenDialogState::new();
        self.irc_token.open = true;
        // status stays `None` (loading) until the service replies.
        self.profile_service.load_irc_token_status(self.user_id);
    }

    pub fn close_irc_token_dialog(&mut self) {
        self.irc_token = IrcTokenDialogState::new();
    }

    /// Move focus between the IRC token action buttons. Only meaningful while a
    /// token exists (Create-only state has a single button).
    pub fn move_irc_token_focus(&mut self, focus: IrcTokenFocus) {
        if self.irc_token.revealed_token.is_some() || !self.irc_token.has_token() {
            return;
        }
        self.irc_token.focus = focus;
        self.irc_token.confirming_revoke = false;
        self.irc_token.message = None;
    }

    /// Dismiss the one-time token reveal and reload the (now-active) status.
    pub fn dismiss_irc_token_reveal(&mut self) {
        if self.irc_token.revealed_token.take().is_some() {
            self.irc_token.message = None;
            self.irc_token.status = None;
            self.irc_token.focus = IrcTokenFocus::Primary;
            self.profile_service.load_irc_token_status(self.user_id);
        }
    }

    /// Activate the focused IRC token action (Enter). Mints/resets or arms and
    /// then performs a revoke. No-op while a request is in flight.
    pub fn activate_irc_token_focus(&mut self) {
        if self.irc_token.revealed_token.is_some() {
            self.dismiss_irc_token_reveal();
            return;
        }
        if self.irc_token.pending || self.irc_token.status.is_none() {
            return;
        }
        match self.irc_token.focus {
            IrcTokenFocus::Primary => {
                self.irc_token.pending = true;
                self.irc_token.confirming_revoke = false;
                self.irc_token.message = Some(if self.irc_token.has_token() {
                    "Resetting token...".to_string()
                } else {
                    "Creating token...".to_string()
                });
                self.profile_service.mint_irc_token(self.user_id);
            }
            IrcTokenFocus::Revoke => {
                if !self.irc_token.has_token() {
                    return;
                }
                if !self.irc_token.confirming_revoke {
                    self.irc_token.confirming_revoke = true;
                    self.irc_token.message = Some(
                        "Revoke token? Connected IRC clients will be disconnected. \
                         Press Enter again to confirm."
                            .to_string(),
                    );
                    return;
                }
                self.irc_token.pending = true;
                self.irc_token.message = Some("Revoking token...".to_string());
                self.profile_service.revoke_irc_token(self.user_id);
            }
        }
    }

    pub fn move_row(&mut self, delta: isize) {
        let last = Row::ALL.len().saturating_sub(1) as isize;
        self.row_index = (self.row_index as isize + delta).clamp(0, last) as usize;
    }

    pub fn theme_selected_row(&self) -> usize {
        self.theme_selected_row
    }

    pub fn theme_scroll_offset(&self) -> usize {
        self.theme_scroll_offset
    }

    pub fn set_theme_visible_height(&self, height: usize) {
        self.theme_visible_height.set(height.max(1));
    }

    pub fn move_theme_cursor(&mut self, delta: isize) {
        let rows = self.theme_tree_rows();
        let last = rows.len().saturating_sub(1) as isize;
        self.theme_selected_row =
            (self.theme_selected_row as isize + delta).clamp(0, last) as usize;
        if let Some(ThemeTreeRow::Theme { option_index, .. }) =
            rows.get(self.theme_selected_row).copied()
        {
            self.apply_theme_index(option_index);
        }
        self.keep_theme_cursor_visible();
    }

    pub fn theme_cursor_left(&mut self) {
        let rows = self.theme_tree_rows();
        match rows.get(self.theme_selected_row).copied() {
            Some(ThemeTreeRow::Group {
                group,
                collapsed: false,
            }) => self.collapse_theme_group(group),
            Some(ThemeTreeRow::Theme { option_index, .. }) => {
                self.collapse_theme_group(theme::OPTIONS[option_index].group);
            }
            _ => {}
        }
    }

    pub fn theme_cursor_right(&mut self) {
        let rows = self.theme_tree_rows();
        match rows.get(self.theme_selected_row).copied() {
            Some(ThemeTreeRow::Group {
                group,
                collapsed: true,
            }) => self.expand_theme_group(group),
            Some(ThemeTreeRow::Group {
                group,
                collapsed: false,
            }) => {
                if let Some(row) = self.first_theme_row_for_group(group) {
                    self.theme_selected_row = row;
                    if let Some(ThemeTreeRow::Theme { option_index, .. }) =
                        self.theme_tree_rows().get(row).copied()
                    {
                        self.apply_theme_index(option_index);
                    }
                    self.keep_theme_cursor_visible();
                }
            }
            _ => {}
        }
    }

    pub fn toggle_theme_tree_row(&mut self) {
        let rows = self.theme_tree_rows();
        if let Some(row) = rows.get(self.theme_selected_row).copied() {
            match row {
                ThemeTreeRow::Group { group, collapsed } => {
                    if collapsed {
                        self.expand_theme_group(group);
                    } else {
                        self.collapse_theme_group(group);
                    }
                }
                ThemeTreeRow::Theme { option_index, .. } => self.select_theme_index(option_index),
            }
        }
    }

    pub fn select_theme_index(&mut self, index: usize) {
        let clamped = index.min(theme::OPTIONS.len().saturating_sub(1));
        self.expand_theme_group(theme::OPTIONS[clamped].group);
        self.theme_index = clamped;
        self.theme_selected_row = self
            .theme_row_for_option(clamped)
            .unwrap_or(self.theme_selected_row);
        self.apply_theme_index(clamped);
        self.keep_theme_cursor_visible();
    }

    fn apply_theme_index(&mut self, index: usize) {
        if let Some(option) = theme::OPTIONS.get(index) {
            self.theme_index = index;
            let current = self
                .draft
                .theme_id
                .as_deref()
                .map(theme::normalize_id)
                .unwrap_or(theme::DEFAULT_ID);
            let changed = current != option.id;
            self.draft.theme_id = Some(option.id.to_string());
            self.keep_theme_cursor_visible();
            if changed {
                self.save();
            }
        }
    }

    pub fn theme_tree_rows(&self) -> Vec<ThemeTreeRow> {
        let mut rows = Vec::new();
        for group in theme::ThemeGroup::ALL {
            let collapsed = self.theme_group_collapsed(group);
            rows.push(ThemeTreeRow::Group { group, collapsed });
            if collapsed {
                continue;
            }

            let option_indices: Vec<usize> = theme::OPTIONS
                .iter()
                .enumerate()
                .filter_map(|(idx, option)| (option.group == group).then_some(idx))
                .collect();
            let last_option_idx = option_indices.len().saturating_sub(1);
            for (idx, option_index) in option_indices.into_iter().enumerate() {
                rows.push(ThemeTreeRow::Theme {
                    option_index,
                    last_in_group: idx == last_option_idx,
                });
            }
        }
        rows
    }

    fn sync_theme_index_to_draft(&mut self) {
        let current = self
            .draft
            .theme_id
            .as_deref()
            .unwrap_or_else(|| theme::normalize_id(""));
        let normalized = theme::normalize_id(current);
        self.theme_index = theme::OPTIONS
            .iter()
            .position(|option| option.id == normalized)
            .unwrap_or(0);
        self.expand_theme_group(theme::OPTIONS[self.theme_index].group);
        self.theme_selected_row = self.theme_row_for_option(self.theme_index).unwrap_or(0);
        self.keep_theme_cursor_visible();
    }

    fn keep_theme_cursor_visible(&mut self) {
        let visible = self.theme_visible_height.get().max(1);
        let max_scroll = self.theme_tree_rows().len().saturating_sub(visible);
        if self.theme_selected_row < self.theme_scroll_offset {
            self.theme_scroll_offset = self.theme_selected_row;
        } else if self.theme_selected_row >= self.theme_scroll_offset + visible {
            self.theme_scroll_offset = self.theme_selected_row.saturating_sub(visible - 1);
        }
        self.theme_scroll_offset = self.theme_scroll_offset.min(max_scroll);
    }

    fn theme_group_collapsed(&self, group: theme::ThemeGroup) -> bool {
        self.theme_collapsed_groups & group.bit() != 0
    }

    fn expand_theme_group(&mut self, group: theme::ThemeGroup) {
        self.theme_collapsed_groups &= !group.bit();
        self.keep_theme_cursor_visible();
    }

    fn collapse_theme_group(&mut self, group: theme::ThemeGroup) {
        self.theme_collapsed_groups |= group.bit();
        self.theme_selected_row = self.theme_group_row(group).unwrap_or_else(|| {
            self.theme_selected_row
                .min(self.theme_tree_rows().len().saturating_sub(1))
        });
        self.keep_theme_cursor_visible();
    }

    fn theme_group_row(&self, group: theme::ThemeGroup) -> Option<usize> {
        self.theme_tree_rows()
            .iter()
            .position(|row| matches!(row, ThemeTreeRow::Group { group: row_group, .. } if *row_group == group))
    }

    fn theme_row_for_option(&self, option_index: usize) -> Option<usize> {
        self.theme_tree_rows().iter().position(
            |row| matches!(row, ThemeTreeRow::Theme { option_index: row_index, .. } if *row_index == option_index),
        )
    }

    fn first_theme_row_for_group(&self, group: theme::ThemeGroup) -> Option<usize> {
        self.theme_tree_rows().iter().position(|row| {
            matches!(
                row,
                ThemeTreeRow::Theme { option_index, .. }
                    if theme::OPTIONS[*option_index].group == group
            )
        })
    }

    pub fn editing_username(&self) -> bool {
        self.editing_username
    }

    pub fn editing_system_field(&self) -> Option<SystemField> {
        self.editing_system_field
    }

    pub fn editing_system_row(&self, row: Row) -> bool {
        self.editing_system_field == SystemField::from_row(row)
    }

    pub fn editing_bio(&self) -> bool {
        self.editing_bio
    }

    pub fn username_input(&self) -> &TextArea<'static> {
        &self.username_input
    }

    pub(crate) fn username_input_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.username_input
    }

    pub(crate) fn system_input_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.system_input
    }

    pub(crate) fn bio_input_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.bio_input
    }

    pub(crate) fn feed_url_input_mut(&mut self) -> &mut TextArea<'static> {
        &mut self.feed_url_input
    }

    fn username_text(&self) -> String {
        self.username_input.lines().join("")
    }

    pub fn system_input(&self) -> &TextArea<'static> {
        &self.system_input
    }

    fn system_text(&self) -> String {
        self.system_input.lines().join("")
    }

    pub fn bio_input(&self) -> &TextArea<'static> {
        &self.bio_input
    }

    pub fn feeds(&self) -> &[RssFeed] {
        &self.feeds
    }

    pub fn feed_index(&self) -> usize {
        self.feed_index
    }

    pub fn editing_feed_url(&self) -> bool {
        self.editing_feed_url
    }

    pub fn feed_url_input(&self) -> &TextArea<'static> {
        &self.feed_url_input
    }

    fn bio_text(&self) -> String {
        self.bio_input.lines().join("\n")
    }

    pub fn picker(&self) -> &PickerState {
        &self.picker
    }

    pub fn picker_open(&self) -> bool {
        self.picker.kind.is_some()
    }

    pub fn open_picker(&mut self, kind: PickerKind) {
        self.picker.kind = Some(kind);
        self.picker.query.clear();
        self.picker.selected_index = 0;
        self.picker.scroll_offset = 0;
    }

    pub fn close_picker(&mut self) {
        self.picker = PickerState::default();
    }

    pub fn filtered_countries(&self) -> Vec<&'static CountryOption> {
        filter_countries(&self.picker.query)
    }

    pub fn filtered_timezones(&self) -> Vec<&'static str> {
        filter_timezones(&self.picker.query)
    }

    pub fn picker_len(&self) -> usize {
        match self.picker.kind {
            Some(PickerKind::Country) => self.filtered_countries().len(),
            Some(PickerKind::Timezone) => self.filtered_timezones().len(),
            None => 0,
        }
    }

    pub fn picker_move(&mut self, delta: isize) {
        let len = self.picker_len();
        if len == 0 {
            self.picker.selected_index = 0;
            self.picker.scroll_offset = 0;
            return;
        }
        let next = (self.picker.selected_index as isize + delta).clamp(0, len as isize - 1);
        self.picker.selected_index = next as usize;
        let visible = self.picker.visible_height.get().max(1);
        if self.picker.selected_index < self.picker.scroll_offset {
            self.picker.scroll_offset = self.picker.selected_index;
        } else if self.picker.selected_index >= self.picker.scroll_offset + visible {
            self.picker.scroll_offset = self.picker.selected_index.saturating_sub(visible - 1);
        }
    }

    pub fn picker_push(&mut self, ch: char) {
        self.picker.query.push(ch);
        self.picker.selected_index = 0;
        self.picker.scroll_offset = 0;
    }

    pub fn picker_backspace(&mut self) {
        self.picker.query.pop();
        self.picker.selected_index = 0;
        self.picker.scroll_offset = 0;
    }

    pub fn apply_picker_selection(&mut self) {
        let mut mutated = false;
        match self.picker.kind {
            Some(PickerKind::Country) => {
                let options = self.filtered_countries();
                if let Some(country) = options.get(self.picker.selected_index) {
                    self.draft.country = Some(country.code.to_string());
                    mutated = true;
                }
            }
            Some(PickerKind::Timezone) => {
                let options = self.filtered_timezones();
                if let Some(timezone) = options.get(self.picker.selected_index) {
                    self.draft.timezone = Some((*timezone).to_string());
                    mutated = true;
                }
            }
            None => {}
        }
        self.close_picker();
        if mutated {
            self.save();
        }
    }

    pub fn start_username_edit(&mut self) {
        self.editing_system_field = None;
        self.editing_username = true;
        self.username_input = new_short_textarea(true);
        self.username_input.insert_str(&self.draft.username);
    }

    pub fn cancel_username_edit(&mut self) {
        self.editing_username = false;
        self.username_input = new_short_textarea(false);
    }

    pub fn submit_username(&mut self) {
        self.editing_username = false;
        let normalized = sanitize_username_input(self.username_text().trim());
        self.username_input = new_short_textarea(false);
        self.draft.username = normalized;
        self.save();
    }

    pub fn start_system_field_edit(&mut self, field: SystemField) {
        self.editing_username = false;
        self.editing_system_field = Some(field);
        self.system_input = new_short_textarea(true);
        if let Some(value) = field.value(&self.draft) {
            self.system_input.insert_str(&value);
        }
    }

    pub fn cancel_system_field_edit(&mut self) {
        self.editing_system_field = None;
        self.system_input = new_short_textarea(false);
    }

    pub fn submit_system_field(&mut self) {
        let Some(field) = self.editing_system_field.take() else {
            return;
        };
        let text = self.system_text();
        self.system_input = new_short_textarea(false);
        field.set_value(&mut self.draft, text);
        self.save();
    }

    pub fn start_bio_edit(&mut self) {
        self.editing_bio = true;
        move_bio_cursor_to_end(&mut self.bio_input);
        set_bio_cursor_visible(&mut self.bio_input, true);
    }

    pub fn stop_bio_edit(&mut self) {
        self.editing_bio = false;
        self.draft.bio = self.bio_text().trim_end().to_string();
        reset_bio_view_to_top(&mut self.bio_input);
        set_bio_cursor_visible(&mut self.bio_input, false);
        self.save();
    }

    pub fn move_feed_cursor(&mut self, delta: isize) {
        let len = self.feed_slot_count();
        if len == 0 {
            self.feed_index = 0;
            return;
        }
        self.feed_index = (self.feed_index as isize + delta).clamp(0, len as isize - 1) as usize;
    }

    pub fn feed_slot_count(&self) -> usize {
        self.feeds.len() + 1
    }

    pub fn feed_index_is_add_row(&self) -> bool {
        self.feed_index == self.feeds.len()
    }

    pub fn start_feed_url_edit(&mut self) {
        self.editing_feed_url = true;
        self.feed_url_input = new_short_textarea(true);
    }

    pub fn cancel_feed_url_edit(&mut self) {
        self.editing_feed_url = false;
        self.feed_url_input = new_short_textarea(false);
    }

    pub fn submit_feed_url(&mut self) {
        let url = self.feed_url_input.lines().join("").trim().to_string();
        self.cancel_feed_url_edit();
        if url.is_empty() {
            return;
        }
        self.feed_service.add_feed_task(self.user_id, url);
    }

    pub fn remove_selected_feed(&mut self) {
        if self.feed_index_is_add_row() {
            return;
        }
        let Some(feed) = self.feeds.get(self.feed_index) else {
            return;
        };
        self.feed_service.delete_feed_task(self.user_id, feed.id);
    }

    pub fn refresh_feeds(&self) {
        self.feed_service.poll_once_task();
        self.feed_service.list_task(self.user_id);
    }

    fn drain_feed_snapshot(&mut self) {
        if let Ok(true) = self.feed_snapshot_rx.has_changed() {
            let snapshot = self.feed_snapshot_rx.borrow_and_update().clone();
            if snapshot.user_id == Some(self.user_id) {
                self.feeds = snapshot.feeds;
                self.feed_index = self
                    .feed_index
                    .min(self.feed_slot_count().saturating_sub(1));
            }
        }
    }

    fn drain_feed_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.feed_event_rx.try_recv() {
                Ok(FeedEvent::FeedAdded { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("RSS connected."));
                }
                Ok(FeedEvent::FeedDeleted { user_id }) if user_id == self.user_id => {
                    banner = Some(Banner::success("RSS removed."));
                }
                Ok(FeedEvent::FeedFailed { user_id, error }) if user_id == self.user_id => {
                    banner = Some(Banner::error(&format!("RSS failed: {error}")));
                }
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive settings feed event");
                    break;
                }
            }
        }
        banner
    }

    fn drain_profile_events(&mut self) -> Option<Banner> {
        let mut banner = None;
        loop {
            match self.profile_event_rx.try_recv() {
                Ok(ProfileEvent::AccountLinkCodeCreated {
                    user_id,
                    code,
                    expires_at,
                }) if user_id == self.user_id => {
                    self.link_account.own_code = Some(code);
                    self.link_account.expires_at = Some(expires_at);
                    self.link_account.pending = false;
                    if self.link_account.step == LinkAccountStep::EnterCode {
                        self.link_account.status = Some("Link code ready.".to_string());
                        self.move_link_account_enter_code_focus(
                            LinkAccountEnterCodeFocus::PeerCode,
                        );
                    }
                }
                Ok(ProfileEvent::AccountLinkPeerLoaded {
                    user_id,
                    peer_user_id,
                    peer_username,
                    peer_created,
                }) if user_id == self.user_id => {
                    self.link_account.peer_user_id = Some(peer_user_id);
                    self.link_account.peer_username = Some(peer_username);
                    self.link_account.peer_created = Some(peer_created);
                    self.link_account.keep_current = true;
                    self.link_account.confirm_input = new_short_textarea(true);
                    self.link_account.step = LinkAccountStep::Confirm;
                    self.link_account.pending = false;
                    self.link_account.status = None;
                }
                Ok(ProfileEvent::AccountLinked {
                    kept_user_id,
                    abandoned_user_id,
                    kept_username,
                    abandoned_username: _,
                }) if kept_user_id == self.user_id || abandoned_user_id == self.user_id => {
                    self.link_account = LinkAccountDialogState::new();
                    if kept_user_id == self.user_id {
                        self.draft.username = kept_username.clone();
                    }
                    banner = Some(Banner::success(&format!(
                        "Linked accounts. Both SSH keys now open {kept_username}."
                    )));
                }
                Ok(ProfileEvent::IrcTokenStatus { user_id, status }) if user_id == self.user_id => {
                    if self.irc_token.open && self.irc_token.revealed_token.is_none() {
                        let had_token = status.is_some();
                        self.irc_token.status = Some(status);
                        self.irc_token.pending = false;
                        if !had_token {
                            self.irc_token.focus = IrcTokenFocus::Primary;
                            self.irc_token.confirming_revoke = false;
                        }
                    }
                }
                Ok(ProfileEvent::IrcTokenMinted { user_id, token }) if user_id == self.user_id => {
                    if self.irc_token.open {
                        self.irc_token.revealed_token = Some(token);
                        self.irc_token.pending = false;
                        self.irc_token.confirming_revoke = false;
                        self.irc_token.message =
                            Some("Save this token now — it will not be shown again.".to_string());
                    }
                }
                Ok(ProfileEvent::IrcTokenRevoked { user_id }) if user_id == self.user_id => {
                    if self.irc_token.open {
                        self.irc_token.status = Some(None);
                        self.irc_token.revealed_token = None;
                        self.irc_token.confirming_revoke = false;
                        self.irc_token.pending = false;
                        self.irc_token.focus = IrcTokenFocus::Primary;
                        self.irc_token.message = Some("Token revoked.".to_string());
                    }
                }
                Ok(ProfileEvent::Error { user_id, message }) if user_id == self.user_id => {
                    if self.irc_token.open {
                        self.irc_token.pending = false;
                        self.irc_token.confirming_revoke = false;
                        self.irc_token.message = Some(message.clone());
                    }
                    if self.link_account.open {
                        self.link_account.pending = false;
                        if self.link_account.step == LinkAccountStep::Pending {
                            self.link_account.step = LinkAccountStep::Confirm;
                        }
                        self.link_account.status = Some(message);
                    }
                }
                Ok(_) => {}
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(e) => {
                    tracing::error!(%e, "failed to receive settings profile event");
                    break;
                }
            }
        }
        banner
    }

    /// Cycle the value of the currently selected row and auto-persist.
    /// Username/Country/Timezone don't cycle here (they open editors/pickers);
    /// this only fires for the toggle/enum rows.
    pub fn cycle_setting(&mut self, forward: bool) {
        let mutated = match self.selected_row() {
            Row::Theme => {
                let current = self
                    .draft
                    .theme_id
                    .as_deref()
                    .unwrap_or_else(|| theme::normalize_id(""));
                self.draft.theme_id = Some(theme::cycle_id(current, forward).to_string());
                self.sync_theme_index_to_draft();
                true
            }
            Row::DirectMessages => {
                toggle_kind(&mut self.draft.notify_kinds, "dms");
                true
            }
            Row::Mentions => {
                toggle_kind(&mut self.draft.notify_kinds, "mentions");
                true
            }
            Row::GameEvents => {
                toggle_kind(&mut self.draft.notify_kinds, "game_events");
                true
            }
            Row::Bell => {
                self.draft.notify_bell ^= true;
                true
            }
            Row::Cooldown => {
                self.draft.notify_cooldown_mins =
                    cycle_cooldown_value(self.draft.notify_cooldown_mins, forward);
                true
            }
            Row::NotifyFormat => {
                self.draft.notify_format = Some(
                    cycle_notify_format(self.draft.notify_format.as_deref(), forward).to_string(),
                );
                true
            }
            Row::Birthday | Row::Ide | Row::Terminal | Row::Os | Row::Langs => false,
            _ => false,
        };
        if mutated {
            self.save();
        }
    }

    pub fn save(&self) {
        self.profile_service.edit_profile(
            self.user_id,
            ProfileParams {
                username: self.draft.username.clone(),
                bio: self.draft.bio.clone(),
                country: self.draft.country.clone(),
                timezone: self.draft.timezone.clone(),
                ide: self.draft.ide.clone(),
                terminal: self.draft.terminal.clone(),
                os: self.draft.os.clone(),
                langs: self.draft.langs.clone(),
                notify_kinds: self.draft.notify_kinds.clone(),
                notify_bell: self.draft.notify_bell,
                notify_cooldown_mins: self.draft.notify_cooldown_mins,
                notify_format: self.draft.notify_format.clone(),
                theme_id: Some(
                    self.draft
                        .theme_id
                        .clone()
                        .unwrap_or_else(|| theme::DEFAULT_ID.to_string()),
                ),
                enable_background_color: self.draft.enable_background_color,
                show_dashboard_header: self.draft.show_dashboard_header,
                show_right_sidebar: self.draft.show_right_sidebar,
                right_sidebar_mode: self.draft.right_sidebar_mode,
                right_sidebar_components: self.draft.right_sidebar_components.clone(),
                show_room_list_sidebar: self.draft.show_room_list_sidebar,
                show_settings_on_connect: self.draft.show_settings_on_connect,
                keep_composer_focused: self.draft.keep_composer_focused,
                start_with_music_muted: self.draft.start_with_music_muted,
                show_flag_fallback: self.draft.show_flag_fallback,
                favorite_room_ids: self.draft.favorite_room_ids.clone(),
                birthday: self.draft.birthday.clone(),
            },
        );
    }
}

fn cycle_notify_format(current: Option<&str>, forward: bool) -> &'static str {
    const OPTIONS: &[&str] = &["both", "osc777", "osc9"];
    let idx = OPTIONS
        .iter()
        .position(|value| Some(*value) == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % OPTIONS.len()
    } else {
        (idx + OPTIONS.len() - 1) % OPTIONS.len()
    };
    OPTIONS[next]
}

fn toggle_kind(kinds: &mut Vec<String>, kind: &str) {
    if let Some(idx) = kinds.iter().position(|value| value == kind) {
        kinds.remove(idx);
    } else {
        kinds.push(kind.to_string());
    }
}

fn cycle_cooldown_value(current: i32, forward: bool) -> i32 {
    const OPTIONS: &[i32] = &[0, 1, 2, 5, 10, 15, 30, 60, 120, 240];
    let idx = OPTIONS
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % OPTIONS.len()
    } else {
        (idx + OPTIONS.len() - 1) % OPTIONS.len()
    };
    OPTIONS[next]
}

fn single_line_char_count(input: &TextArea<'static>) -> usize {
    input.lines().iter().map(|l| l.chars().count()).sum()
}

fn normalize_optional_text(text: &str) -> Option<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

fn reset_bio_view_to_top(input: &mut TextArea<'static>) {
    input.move_cursor(CursorMove::Top);
    input.move_cursor(CursorMove::Head);
}

fn move_bio_cursor_to_end(input: &mut TextArea<'static>) {
    input.move_cursor(CursorMove::Bottom);
    input.move_cursor(CursorMove::End);
}

fn bio_textarea_for_readonly_text(text: &str) -> TextArea<'static> {
    let mut input = new_bio_textarea(false);
    input.insert_str(text);
    reset_bio_view_to_top(&mut input);
    input
}

fn new_bio_textarea(editing: bool) -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_wrap_mode(WrapMode::Word);
    set_bio_cursor_visible(&mut ta, editing);
    ta
}

fn set_bio_cursor_visible(ta: &mut TextArea<'static>, visible: bool) {
    let style = if visible {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    ta.set_cursor_style(style);
}

fn new_short_textarea(editing: bool) -> TextArea<'static> {
    let mut ta = TextArea::default();
    ta.set_cursor_line_style(Style::default());
    ta.set_wrap_mode(WrapMode::None);
    set_short_textarea_cursor_visible(&mut ta, editing);
    ta
}

fn set_short_textarea_cursor_visible(ta: &mut TextArea<'static>, editing: bool) {
    let style = if editing {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };
    ta.set_cursor_style(style);
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_optional_text_trims_and_collapses_blank() {
        assert_eq!(
            normalize_optional_text("  VS   Code  ").as_deref(),
            Some("VS Code")
        );
        assert_eq!(normalize_optional_text("   "), None);
    }

    #[test]
    fn readonly_bio_textarea_resets_cursor_to_top() {
        let input = bio_textarea_for_readonly_text("first line\nsecond line\nthird line");
        assert_eq!(input.cursor(), (0usize, 0usize));
    }

    #[test]
    fn move_bio_cursor_to_end_goes_to_last_line_end() {
        let mut input = bio_textarea_for_readonly_text("first line\nsecond line\nthird line");

        move_bio_cursor_to_end(&mut input);

        assert_eq!(input.cursor(), (2usize, "third line".chars().count()));
    }
}
