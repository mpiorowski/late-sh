//! The customizable status bar painted on the app frame's bottom border.
//!
//! This module owns only the *persisted* model: the component roster, the
//! per-component dials, and the parse/normalize rules. Building spans,
//! fitting them to the available columns, and click hit-testing all live in
//! `late-ssh/src/app/statusline/`, which is where the terminal and the frame
//! data are.
//!
//! Shape deliberately mirrors `RightSidebarComponent` (closed enum, string
//! keys, `normalize_*` backfill) so the two editors read the same way, with
//! two divergences called out at their definitions: `backfill_existing` is a
//! per-component decision rather than a blanket rule, and `low_priority` is a
//! user-settable drop tier that the sidebar has no equivalent for.

use serde_json::Value;

pub const STATUS_COMPONENT_COUNT: usize = 11;

/// A segment the user can place on the status bar. Order in the stored list is
/// the paint order, left to right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusComponent {
    Shortcuts,
    Time,
    Chips,
    Mentions,
    Pot,
    Users,
    Turns,
    Station,
    Quests,
    Invites,
    Voice,
}

impl StatusComponent {
    /// Default paint order, left to right. `ALL` is also the backfill order for
    /// components missing from a stored list.
    ///
    /// Keyboard shortcuts retain the bottom-left frame hint by default. Status
    /// readouts are opt-in here because the fixed top bar already carries the
    /// upstream HUD; users can add whichever duplicate or supplemental readings
    /// they want along the bottom border.
    pub const ALL: [StatusComponent; STATUS_COMPONENT_COUNT] = [
        Self::Shortcuts,
        Self::Voice,
        Self::Mentions,
        Self::Pot,
        Self::Chips,
        Self::Turns,
        Self::Invites,
        Self::Quests,
        Self::Station,
        Self::Users,
        Self::Time,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shortcuts => "shortcuts",
            Self::Time => "time",
            Self::Chips => "chips",
            Self::Mentions => "mentions",
            Self::Pot => "pot",
            Self::Users => "users",
            Self::Turns => "turns",
            Self::Station => "station",
            Self::Quests => "quests",
            Self::Invites => "invites",
            Self::Voice => "voice",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "shortcuts" => Some(Self::Shortcuts),
            "time" => Some(Self::Time),
            "chips" => Some(Self::Chips),
            "mentions" => Some(Self::Mentions),
            "pot" => Some(Self::Pot),
            "users" => Some(Self::Users),
            "turns" => Some(Self::Turns),
            "station" => Some(Self::Station),
            "quests" => Some(Self::Quests),
            "invites" => Some(Self::Invites),
            "voice" => Some(Self::Voice),
            _ => None,
        }
    }

    /// Name shown in the customizer's component list.
    pub fn label(self) -> &'static str {
        match self {
            Self::Shortcuts => "Keyhints",
            Self::Time => "Time",
            Self::Chips => "Chips",
            Self::Mentions => "Mentions",
            Self::Pot => "Pot",
            Self::Users => "Users online",
            Self::Turns => "Your move",
            Self::Station => "Station",
            Self::Quests => "Quests",
            Self::Invites => "Invites",
            Self::Voice => "Voice",
        }
    }

    /// Short explanation shown beside the selected component in the customizer.
    pub fn description(self) -> &'static str {
        match self {
            Self::Shortcuts => "Keyboard shortcuts for navigation and common actions.",
            Self::Time => "Current time in your chosen timezone.",
            Self::Chips => "Your chip balance.",
            Self::Mentions => "Unread mentions, optionally including direct messages.",
            Self::Pot => "Raffle pot size and time until the next draw.",
            Self::Users => "People online, excluding bots.",
            Self::Turns => "Correspondence games waiting for your move.",
            Self::Station => "Your selected audio source or its current track.",
            Self::Quests => "Unfinished daily quests, optionally including weekly quests.",
            Self::Invites => "Game challenges awaiting your response.",
            Self::Voice => "Your voice channel: speaking, listening, muted or deafened.",
        }
    }

    /// The word painted on the bar under `LabelMode::Text`. Shorter than
    /// `label()`, which only has to be legible in the editor's list.
    pub fn text_label(self) -> &'static str {
        match self {
            Self::Shortcuts => "",
            Self::Time => "",
            Self::Chips => "chips",
            Self::Mentions => "unread",
            Self::Pot => "pot",
            Self::Users => "online",
            Self::Turns => "your move",
            Self::Station => "on air",
            Self::Quests => "quests",
            Self::Invites => "invites",
            Self::Voice => "mic",
        }
    }

    /// The glyph painted under `LabelMode::Icon`.
    ///
    /// Every icon here is Emoji_Presentation, i.e. unambiguously two cells
    /// wide. That is a hard requirement, not a style preference: the bar is
    /// right-aligned and its click rects are derived from measured widths, so
    /// a glyph the terminal paints at a width `unicode-width` disagrees about
    /// both slides every hit rect and lets the bar overrun the page tabs.
    /// Text-default glyphs that only become emoji via VS16 (♟️, ✉️, ☎️) must
    /// not be used here. `Time` returns the empty string because its icon is
    /// hour-dependent and comes from `clock_icon`; Keyhints is a pre-styled
    /// frame hint rather than an icon/value pair and also returns empty.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Shortcuts => "",
            Self::Time => "",
            Self::Chips => "🪙",
            Self::Mentions => "📩",
            Self::Pot => "🍯",
            Self::Users => "🌐",
            Self::Turns => "🎲",
            Self::Station => "🎵",
            Self::Quests => "❕",
            Self::Invites => "❔",
            Self::Voice => "🔊",
        }
    }

    /// Whether this component has an "inactive" reading at all, and so whether
    /// the customizer offers it an auto-hide switch. Keyhints, Time, and Users
    /// always have something to say; the rest can read zero/idle.
    pub fn can_auto_hide(self) -> bool {
        !matches!(self, Self::Shortcuts | Self::Time | Self::Users)
    }

    /// Whether the component starts enabled for a user with no stored list.
    ///
    /// The bottom-left keyboard hint is the only shipped segment. Status
    /// readouts remain discoverable in the customizer rather than duplicating
    /// the fixed top bar until a user asks for them.
    pub fn default_enabled(self) -> bool {
        self == Self::Shortcuts
    }

    pub fn default_label_mode(self) -> LabelMode {
        match self {
            // The clock reads as a clock; a label would only cost columns.
            Self::Shortcuts | Self::Time => LabelMode::None,
            _ => LabelMode::Text,
        }
    }

    pub fn default_auto_hide(self) -> bool {
        self.can_auto_hide()
    }

    /// Whether the component starts in the low-priority drop tier. The existing
    /// keyboard hint keeps normal priority; every opt-in status reading starts
    /// low priority until the user promotes it.
    pub fn default_low_priority(self) -> bool {
        !self.default_enabled()
    }

    /// What happens when this component is added to the roster *after* a user
    /// has already saved a bar. `false` (the default for anything cosmetic or
    /// niche) backfills it disabled, leaving a customized bar untouched;
    /// `true` forces it on, and is reserved for the keyboard hint so an existing
    /// saved component list does not make the longstanding bottom-left help
    /// disappear. Diverges on purpose from
    /// `normalize_right_sidebar_components`, which backfills everything
    /// enabled — a sidebar panel that appears costs a user rows in a rail
    /// built to hold panels, while a bar segment that appears costs horizontal
    /// frame space.
    pub fn backfill_existing(self) -> bool {
        self == Self::Shortcuts
    }

    /// The component's one extra dial, or `&[]` when it has none. The first
    /// entry is the default, and is what an absent or unrecognized stored
    /// variant resolves to.
    pub fn variants(self) -> &'static [StatusVariant] {
        match self {
            Self::Time => &[StatusVariant::Clock24, StatusVariant::ClockAmPm],
            Self::Mentions => &[StatusVariant::MentionsOnly, StatusVariant::MentionsAndDms],
            Self::Quests => &[StatusVariant::QuestsDaily, StatusVariant::QuestsDailyWeekly],
            Self::Station => &[StatusVariant::StationName, StatusVariant::StationTrack],
            _ => &[],
        }
    }

    /// Heading for this component's dial in the customizer's detail pane.
    pub fn variant_title(self) -> Option<&'static str> {
        match self {
            Self::Time => Some("Clock"),
            Self::Mentions => Some("Count"),
            Self::Quests => Some("Count"),
            Self::Station => Some("Show"),
            _ => None,
        }
    }

    fn default_variant(self) -> Option<StatusVariant> {
        self.variants().first().copied()
    }
}

/// How a component identifies itself on the bar. The component's *value* is
/// always painted; this picks what sits next to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelMode {
    Text,
    Icon,
    None,
}

impl LabelMode {
    pub const ALL: [LabelMode; 3] = [Self::Text, Self::Icon, Self::None];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Icon => "icon",
            Self::None => "none",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "text" => Some(Self::Text),
            "icon" => Some(Self::Icon),
            "none" => Some(Self::None),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Icon => "Icon",
            Self::None => "None",
        }
    }

    pub fn cycle(self, forward: bool) -> Self {
        let idx = Self::ALL.iter().position(|m| *m == self).unwrap_or(0);
        let len = Self::ALL.len();
        let next = if forward {
            (idx + 1) % len
        } else {
            (idx + len - 1) % len
        };
        Self::ALL[next]
    }
}

/// One extra per-component dial.
///
/// Deliberately one flat closed enum rather than an associated type per
/// component: the customizer renders any component's dial straight from
/// `StatusComponent::variants()` with no match arm per component, and parsing
/// stays a single `from_key`. Stored by key, never by index, so reordering a
/// component's `variants()` later cannot silently repoint saved settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusVariant {
    Clock24,
    ClockAmPm,
    MentionsOnly,
    MentionsAndDms,
    QuestsDaily,
    QuestsDailyWeekly,
    StationName,
    StationTrack,
}

impl StatusVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clock24 => "clock_24",
            Self::ClockAmPm => "clock_ampm",
            Self::MentionsOnly => "mentions_only",
            Self::MentionsAndDms => "mentions_and_dms",
            Self::QuestsDaily => "quests_daily",
            Self::QuestsDailyWeekly => "quests_daily_weekly",
            Self::StationName => "station_name",
            Self::StationTrack => "station_track",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim() {
            "clock_24" => Some(Self::Clock24),
            "clock_ampm" => Some(Self::ClockAmPm),
            "mentions_only" => Some(Self::MentionsOnly),
            "mentions_and_dms" => Some(Self::MentionsAndDms),
            "quests_daily" => Some(Self::QuestsDaily),
            "quests_daily_weekly" => Some(Self::QuestsDailyWeekly),
            "station_name" => Some(Self::StationName),
            "station_track" => Some(Self::StationTrack),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Clock24 => "24-hour",
            Self::ClockAmPm => "AM/PM",
            Self::MentionsOnly => "Mentions",
            Self::MentionsAndDms => "Mentions + DMs",
            Self::QuestsDaily => "Daily",
            Self::QuestsDailyWeekly => "Daily + weekly",
            Self::StationName => "Station",
            Self::StationTrack => "Track",
        }
    }
}

/// One entry in the ordered status bar list: a component plus every dial the
/// customizer exposes for it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusComponentSetting {
    pub component: StatusComponent,
    pub enabled: bool,
    /// Use the compact glyph hints. Only offered for Keyhints.
    pub brief: bool,
    pub label: LabelMode,
    /// Drop the segment entirely while the component reads inactive/zero.
    /// Meaningless, and not offered, when `!component.can_auto_hide()`.
    pub auto_hide: bool,
    /// Drop tier. The bottom bar shares its row with the sponsor title, so when
    /// space runs out every low-priority segment is given up before any
    /// normal-priority one yields; within a tier the rightmost segment goes
    /// first.
    pub low_priority: bool,
    /// Resolved against `component.variants()`; `None` for components with no
    /// dial. A stored value that is absent or foreign resolves to the first
    /// variant rather than disabling the component.
    pub variant: Option<StatusVariant>,
}

impl StatusComponentSetting {
    /// The component at its shipped defaults.
    pub fn new(component: StatusComponent) -> Self {
        Self {
            component,
            enabled: component.default_enabled(),
            brief: false,
            label: component.default_label_mode(),
            auto_hide: component.default_auto_hide(),
            low_priority: component.default_low_priority(),
            variant: component.default_variant(),
        }
    }

    /// The same component, enabled or not, with every other dial defaulted.
    /// Used to backfill a component missing from a stored list.
    fn backfilled(component: StatusComponent) -> Self {
        Self {
            enabled: component.backfill_existing(),
            ..Self::new(component)
        }
    }
}

/// Default bar: every component, in default order, at its shipped state.
pub fn default_statusline_components() -> Vec<StatusComponentSetting> {
    StatusComponent::ALL
        .into_iter()
        .map(StatusComponentSetting::new)
        .collect()
}

/// Drop duplicates, repair dials that no longer make sense, and backfill any
/// missing component so the list always covers every component exactly once,
/// preserving stored order. The keyboard hint is the one positional exception:
/// when newly backfilled it takes its longstanding bottom-left slot at the
/// front; other additions append.
///
/// Backfilled components take `StatusComponent::backfill_existing()` rather
/// than a blanket `true`: see that method for why this diverges from the
/// sidebar's rule.
pub fn normalize_statusline_components(
    components: &[StatusComponentSetting],
) -> Vec<StatusComponentSetting> {
    let mut result: Vec<StatusComponentSetting> = Vec::new();
    for setting in components {
        if result.iter().any(|s| s.component == setting.component) {
            continue;
        }
        let component = setting.component;
        // A variant stored for a component that has since lost its dial (or
        // one belonging to a different component entirely) is dropped rather
        // than trusted, and a component that has since *gained* a dial picks
        // up its default.
        let variant = setting
            .variant
            .filter(|v| component.variants().contains(v))
            .or_else(|| component.default_variant());
        result.push(StatusComponentSetting {
            component,
            enabled: setting.enabled,
            brief: setting.brief && component == StatusComponent::Shortcuts,
            label: setting.label,
            auto_hide: setting.auto_hide && component.can_auto_hide(),
            low_priority: setting.low_priority,
            variant,
        });
    }
    for component in StatusComponent::ALL {
        if !result.iter().any(|s| s.component == component) {
            let setting = StatusComponentSetting::backfilled(component);
            if component == StatusComponent::Shortcuts {
                result.insert(0, setting);
            } else {
                result.push(setting);
            }
        }
    }
    result
}

/// Parse the stored `statusline_components` array. Unknown keys are skipped,
/// then `normalize_statusline_components` fills the gaps.
pub fn parse_statusline_components(values: &[Value]) -> Vec<StatusComponentSetting> {
    // Fold the former brief component into Keyhints, keeping the active entry's
    // position and options. If both were enabled, the full Keyhints entry wins.
    fn key(value: &Value) -> Option<&str> {
        value.get("key").and_then(Value::as_str).map(str::trim)
    }
    let full = values
        .iter()
        .position(|value| key(value) == Some("shortcuts"));
    let brief = values
        .iter()
        .position(|value| key(value) == Some("keyhints_brief"));
    let selected_keyhints = brief
        .filter(|&index| {
            values[index].get("enabled").and_then(Value::as_bool) == Some(true)
                && full.is_none_or(|index| {
                    values[index].get("enabled").and_then(Value::as_bool) == Some(false)
                })
        })
        .or(full)
        .or(brief);
    let mut parsed: Vec<StatusComponentSetting> = Vec::new();
    for (index, value) in values.iter().enumerate() {
        let legacy_brief = key(value) == Some("keyhints_brief");
        let component = if legacy_brief {
            Some(StatusComponent::Shortcuts)
        } else {
            key(value).and_then(StatusComponent::from_key)
        };
        let Some(component) = component else {
            continue;
        };
        if component == StatusComponent::Shortcuts && Some(index) != selected_keyhints {
            continue;
        }
        parsed.push(StatusComponentSetting {
            component,
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| !legacy_brief && component.default_enabled()),
            brief: legacy_brief || value.get("brief").and_then(Value::as_bool).unwrap_or(false),
            label: value
                .get("label")
                .and_then(Value::as_str)
                .and_then(LabelMode::from_key)
                .unwrap_or_else(|| component.default_label_mode()),
            auto_hide: value
                .get("auto_hide")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| component.default_auto_hide()),
            low_priority: value
                .get("low_priority")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| component.default_low_priority()),
            variant: value
                .get("variant")
                .and_then(Value::as_str)
                .and_then(StatusVariant::from_key),
        });
    }
    normalize_statusline_components(&parsed)
}

/// Render the list back to the stored JSON shape.
pub fn statusline_components_json(components: &[StatusComponentSetting]) -> Value {
    Value::Array(
        normalize_statusline_components(components)
            .into_iter()
            .map(|setting| {
                serde_json::json!({
                    "key": setting.component.as_str(),
                    "enabled": setting.enabled,
                    "brief": setting.brief,
                    "label": setting.label.as_str(),
                    "auto_hide": setting.auto_hide,
                    "low_priority": setting.low_priority,
                    "variant": setting.variant.map(StatusVariant::as_str),
                })
            })
            .collect(),
    )
}
