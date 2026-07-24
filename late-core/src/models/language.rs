//! UI language identifiers for the SSH TUI shell.
//!
//! Mirrors the shape of the theme registry in
//! `late-ssh::app::common::theme`: a small set of supported options, each with
//! a stable id and a human-readable label, plus normalize/cycle/label helpers.
//! Living in `late-core` lets `Profile` normalize a stored language string at
//! the DB boundary, while `late-ssh::app::common::i18n` adds the thread-local
//! translation lookup on top.

/// A language the TUI shell can render in. Add new variants here and append
/// them to [`OPTIONS`]; the rest of the system picks them up automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// English - the source language and always-present fallback.
    En,
    /// Simplified Chinese (简体中文).
    ZhHans,
}

impl Language {
    /// Stable identifier persisted in `users.settings -> language` and used as
    /// the locale key for translation lookups.
    pub fn id(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhHans => "zh-hans",
        }
    }

    /// Human-readable label shown in the settings picker.
    pub fn label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::ZhHans => "中文(简体)",
        }
    }
}

/// Default language id, used when a user has no `language` setting yet.
pub const DEFAULT_ID: &str = "en";

/// Every supported language, in cycle order. Append new languages here.
pub const OPTIONS: &[Language] = &[Language::En, Language::ZhHans];

/// Resolve a stored/raw language string to a supported id, accepting common
/// aliases (`"zh"`, `"zh-CN"`, `"zh-cn"`, `"zh-hans"`, `"chinese"`) and falling
/// back to [`DEFAULT_ID`] for anything unrecognized or empty.
pub fn normalize_id(id: &str) -> &'static str {
    from_id(id).id()
}

/// Resolve a stored/raw language string to a [`Language`].
pub fn from_id(id: &str) -> Language {
    let lower = id.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return Language::En;
    }
    for lang in OPTIONS {
        if lang.id() == lower.as_str() {
            return *lang;
        }
    }
    // Aliases for Simplified Chinese.
    if matches!(
        lower.as_str(),
        "zh" | "zh-cn" | "zh_cn" | "zh-hans-cn" | "chinese"
    ) {
        return Language::ZhHans;
    }
    Language::En
}

/// Human-readable label for a stored id (falls back to the default label).
pub fn label_for_id(id: &str) -> &'static str {
    from_id(id).label()
}

/// Cycle forward/backward through [`OPTIONS`], used by the settings picker.
pub fn cycle_id(current_id: &str, forward: bool) -> &'static str {
    let current = from_id(current_id);
    let idx = OPTIONS
        .iter()
        .position(|lang| *lang == current)
        .unwrap_or(0);
    let next = if forward {
        (idx + 1) % OPTIONS.len()
    } else {
        (idx + OPTIONS.len() - 1) % OPTIONS.len()
    };
    OPTIONS[next].id()
}
