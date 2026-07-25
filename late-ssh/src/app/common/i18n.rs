//! TUI string translation.
//!
//! Mirrors `theme`'s thread-local "current" pattern so each SSH session
//! (thread) renders in its own language without affecting concurrent sessions.
//! The active language is set once at the top of `App::render` from the user's
//! profile, exactly like `theme::set_current_by_id`.
//!
//! Translation tables live in `locales/*.toml` and are compiled in via
//! `include_str!`, parsed once on first use. English is the source language and
//! always-present fallback: a missing key in another language falls back to
//! English, and a missing English key falls back to the key itself (visible
//! during development so gaps are obvious).

use std::cell::Cell;
use std::collections::HashMap;

use std::sync::LazyLock;

use late_core::models::language::{self, Language};

thread_local! {
    static CURRENT_LOCALE: Cell<Language> = const { Cell::new(Language::En) };
}

/// Set the active language for the current thread/session. Call at the top of
/// `App::render` (mirrors `theme::set_current_by_id`).
pub fn set_current_by_id(id: &str) {
    CURRENT_LOCALE.with(|current| current.set(language::from_id(id)));
}

/// The active language id for the current thread/session.
pub fn current_id() -> &'static str {
    CURRENT_LOCALE.with(|current| current.get().id())
}

/// Translate a key for the current language. Falls back to English, then to
/// the key itself if English is also missing.
///
/// `key` must be a `'static` string literal so the fallback can return it
/// without allocation. All TUI shell keys are literals, so this is no hardship
/// in practice.
pub fn tr(key: &'static str) -> &'static str {
    let locale = CURRENT_LOCALE.with(|current| current.get());
    let hit = match locale {
        Language::ZhHans => ZH_HANS.get(key).copied().or_else(|| EN.get(key).copied()),
        Language::En => EN.get(key).copied(),
    };
    hit.unwrap_or(key)
}

/// Translate a key with `{placeholder}` substitution. Placeholders are written
/// `{name}` in the locale file and supplied as `&[("name", value)]`.
pub fn trf(key: &'static str, args: &[(&str, &str)]) -> String {
    let mut out = tr(key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

// --- translation tables (compiled in, parsed once) -------------------------

static EN: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| build(include_str!("../../../locales/en.toml")));
static ZH_HANS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| build(include_str!("../../../locales/zh-hans.toml")));

/// Parse a TOML locale file into a flat `section.key -> value` map. Table
/// sections are joined with `.`; only string values are kept. Strings are
/// leaked to `'static` once at first use - the catalog is bounded and lives
/// for the process.
fn build(toml_text: &'static str) -> HashMap<&'static str, &'static str> {
    let table: toml::Table =
        toml::from_str(toml_text).unwrap_or_else(|e| panic!("failed to parse locale toml: {e}"));
    let mut out = HashMap::new();
    flatten(&table, String::new(), &mut out);
    out
}

fn flatten(table: &toml::Table, prefix: String, out: &mut HashMap<&'static str, &'static str>) {
    for (key, value) in table {
        let full = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::String(s) => {
                let k: &'static str = Box::leak(full.into_boxed_str());
                let v: &'static str = Box::leak(s.clone().into_boxed_str());
                out.insert(k, v);
            }
            toml::Value::Table(t) => flatten(t, full, out),
            _ => {}
        }
    }
}
