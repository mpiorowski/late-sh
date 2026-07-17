//! The 24h username-effect flair: the process-shared directory of live
//! effects and the pure color math that turns an effect into rendered spans.
//!
//! Distribution deliberately copies the `usernames::UsernameDirectory`
//! snapshot-swap shape instead of the SharedLobby drunk map: effects change
//! rarely (a purchase or an expiry), so readers clone an `Arc` per second
//! rather than copying a map under a mutex. Writes are event-driven — the
//! shop service seeds once at startup, writes through on a local purchase,
//! and refreshes one user from its `shop_user_changed` LISTEN/NOTIFY loop —
//! there is no polling task. Expiry is read-time only: entries carry
//! `ends_at` and consumers skip stale ones, exactly like room effects.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use late_core::{MutexRecover, models::username_effect::UsernameEffect};
use ratatui::{
    style::{Color, Style},
    text::Span,
};
use uuid::Uuid;

use super::theme;

/// One user's live effect plus its expiry; stale entries are skipped at read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameFlair {
    pub effect: UsernameEffect,
    pub ends_at: DateTime<Utc>,
}

/// Snapshot-swap directory of live username effects, shared process-wide.
pub type NameFlairDirectory = Arc<Mutex<Arc<HashMap<Uuid, NameFlair>>>>;

pub fn new_directory() -> NameFlairDirectory {
    Arc::new(Mutex::new(Arc::new(HashMap::new())))
}

/// Cheap read: clones the inner `Arc`, never the map.
pub fn snapshot(directory: &NameFlairDirectory) -> Arc<HashMap<Uuid, NameFlair>> {
    Arc::clone(&directory.lock_recover())
}

/// Wholesale replace, for the startup seed.
pub fn set_all(directory: &NameFlairDirectory, entries: Vec<(Uuid, NameFlair)>) {
    *directory.lock_recover() = Arc::new(entries.into_iter().collect());
}

/// Upsert or clear one user's flair (purchase write-through and
/// LISTEN/NOTIFY refresh).
pub fn set_user(directory: &NameFlairDirectory, user_id: Uuid, flair: Option<NameFlair>) {
    let mut guard = directory.lock_recover();
    let entries = Arc::make_mut(&mut *guard);
    match flair {
        Some(flair) => {
            entries.insert(user_id, flair);
        }
        None => {
            entries.remove(&user_id);
        }
    }
}

/// A resolved per-frame name style: what the renderers actually paint.
/// Shimmer resolves to a `TwoTone` whose endpoints move, so renderers never
/// need to know which effect produced the style.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameStyle {
    Solid(Color),
    TwoTone(Color, Color),
}

/// Shimmer advances one palette step per second (~15 world ticks).
pub const SHIMMER_PERIOD_TICKS: usize = 15;

pub fn shimmer_phase(tick: usize) -> usize {
    tick / SHIMMER_PERIOD_TICKS
}

/// The six glow anchors; shimmer walks the same cycle.
const GLOW_CYCLE: [Color; 6] = [
    Color::Rgb(255, 120, 80),  // ember
    Color::Rgb(255, 200, 80),  // gold
    Color::Rgb(170, 220, 90),  // lime
    Color::Rgb(90, 215, 200),  // aqua
    Color::Rgb(120, 180, 255), // sky
    Color::Rgb(220, 130, 255), // orchid
];

fn glow_rgb(color: late_core::models::username_effect::GlowColor) -> Color {
    use late_core::models::username_effect::GlowColor;
    match color {
        GlowColor::Ember => GLOW_CYCLE[0],
        GlowColor::Gold => GLOW_CYCLE[1],
        GlowColor::Lime => GLOW_CYCLE[2],
        GlowColor::Aqua => GLOW_CYCLE[3],
        GlowColor::Sky => GLOW_CYCLE[4],
        GlowColor::Orchid => GLOW_CYCLE[5],
    }
}

fn gradient_rgb(pair: late_core::models::username_effect::GradientPair) -> (Color, Color) {
    use late_core::models::username_effect::GradientPair;
    match pair {
        GradientPair::Sunset => (Color::Rgb(255, 120, 80), Color::Rgb(255, 210, 110)),
        GradientPair::Ocean => (Color::Rgb(80, 180, 255), Color::Rgb(90, 230, 190)),
        GradientPair::Dusk => (Color::Rgb(200, 120, 255), Color::Rgb(110, 150, 255)),
        GradientPair::Forest => (Color::Rgb(150, 220, 110), Color::Rgb(60, 180, 150)),
        GradientPair::Candy => (Color::Rgb(255, 120, 180), Color::Rgb(150, 170, 255)),
        GradientPair::Flare => (Color::Rgb(255, 90, 90), Color::Rgb(255, 220, 130)),
    }
}

/// Resolve an effect at a shimmer phase into the style renderers paint.
pub fn resolve(effect: UsernameEffect, phase: usize) -> NameStyle {
    match effect {
        UsernameEffect::Glow(color) => NameStyle::Solid(glow_rgb(color)),
        UsernameEffect::Gradient(pair) => {
            let (from, to) = gradient_rgb(pair);
            NameStyle::TwoTone(from, to)
        }
        UsernameEffect::Shimmer => NameStyle::TwoTone(
            GLOW_CYCLE[phase % GLOW_CYCLE.len()],
            GLOW_CYCLE[(phase + 1) % GLOW_CYCLE.len()],
        ),
    }
}

/// Resolve every live entry in a directory snapshot into paintable styles,
/// skipping expired ones. Runs once per second per session in the tick loop.
pub fn resolve_all(
    entries: &HashMap<Uuid, NameFlair>,
    phase: usize,
    now: DateTime<Utc>,
) -> HashMap<Uuid, NameStyle> {
    entries
        .iter()
        .filter(|(_, flair)| flair.ends_at > now)
        .map(|(user_id, flair)| (*user_id, resolve(flair.effect, phase)))
        .collect()
}

/// The fg color for character `index` of a `len`-character name.
pub fn char_color(style: NameStyle, index: usize, len: usize) -> Color {
    match style {
        NameStyle::Solid(color) => color,
        NameStyle::TwoTone(from, to) => {
            let t = index as f32 / len.saturating_sub(1).max(1) as f32;
            theme::blend_toward(from, to, t)
        }
    }
}

/// The name as per-character spans with the effect fg over `base`. The base
/// style's bg (drunk tint) and modifiers (BOLD) survive; only fg is replaced,
/// which is what lets an effect override the friend/own author colors.
pub fn styled_name_spans(name: &str, style: NameStyle, base: Style) -> Vec<Span<'static>> {
    let len = name.chars().count();
    name.chars()
        .enumerate()
        .map(|(index, ch)| Span::styled(ch.to_string(), base.fg(char_color(style, index, len))))
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use late_core::models::username_effect::{GlowColor, GradientPair};

    use super::*;

    #[test]
    fn char_color_hits_gradient_endpoints() {
        let style = NameStyle::TwoTone(Color::Rgb(0, 0, 0), Color::Rgb(200, 100, 50));
        assert_eq!(char_color(style, 0, 8), Color::Rgb(0, 0, 0));
        assert_eq!(char_color(style, 7, 8), Color::Rgb(200, 100, 50));
    }

    #[test]
    fn char_color_single_char_does_not_divide_by_zero() {
        let style = NameStyle::TwoTone(Color::Rgb(10, 20, 30), Color::Rgb(200, 100, 50));
        assert_eq!(char_color(style, 0, 1), Color::Rgb(10, 20, 30));
        let solid = NameStyle::Solid(Color::Rgb(1, 2, 3));
        assert_eq!(char_color(solid, 0, 1), Color::Rgb(1, 2, 3));
    }

    #[test]
    fn shimmer_cycles_with_period_six_and_moving_endpoints() {
        for phase in 0..12 {
            let NameStyle::TwoTone(from, to) = resolve(UsernameEffect::Shimmer, phase) else {
                panic!("shimmer must resolve to a two-tone style");
            };
            let NameStyle::TwoTone(next_from, _) = resolve(UsernameEffect::Shimmer, phase + 1)
            else {
                panic!("shimmer must resolve to a two-tone style");
            };
            // The trailing endpoint becomes the next phase's leading one.
            assert_eq!(to, next_from);
            assert_eq!(
                resolve(UsernameEffect::Shimmer, phase + 6),
                NameStyle::TwoTone(from, to)
            );
        }
    }

    #[test]
    fn glow_and_gradient_resolve_ignore_phase() {
        let glow = UsernameEffect::Glow(GlowColor::Sky);
        assert_eq!(resolve(glow, 0), resolve(glow, 99));
        let gradient = UsernameEffect::Gradient(GradientPair::Ocean);
        assert_eq!(resolve(gradient, 0), resolve(gradient, 99));
    }

    #[test]
    fn directory_set_expire_replace() {
        let directory = new_directory();
        let user = Uuid::now_v7();
        let other = Uuid::now_v7();
        let now = Utc::now();

        set_user(
            &directory,
            user,
            Some(NameFlair {
                effect: UsernameEffect::Shimmer,
                ends_at: now + Duration::hours(24),
            }),
        );
        set_user(
            &directory,
            other,
            Some(NameFlair {
                effect: UsernameEffect::Glow(GlowColor::Ember),
                ends_at: now - Duration::seconds(1),
            }),
        );

        let resolved = resolve_all(&snapshot(&directory), 0, now);
        assert!(resolved.contains_key(&user));
        assert!(
            !resolved.contains_key(&other),
            "expired flair must be skipped"
        );

        set_user(&directory, user, None);
        assert!(snapshot(&directory).is_empty() || !snapshot(&directory).contains_key(&user));

        set_all(
            &directory,
            vec![(
                other,
                NameFlair {
                    effect: UsernameEffect::Gradient(GradientPair::Candy),
                    ends_at: now + Duration::hours(1),
                },
            )],
        );
        let entries = snapshot(&directory);
        assert_eq!(entries.len(), 1);
        assert!(entries.contains_key(&other));
    }

    #[test]
    fn styled_name_spans_keeps_base_bg_and_modifiers() {
        use ratatui::style::Modifier;
        let base = Style::default()
            .bg(Color::Rgb(40, 40, 40))
            .add_modifier(Modifier::BOLD);
        let spans = styled_name_spans("mat", NameStyle::Solid(Color::Rgb(255, 200, 80)), base);
        assert_eq!(spans.len(), 3);
        for span in &spans {
            assert_eq!(span.style.bg, Some(Color::Rgb(40, 40, 40)));
            assert!(span.style.add_modifier.contains(Modifier::BOLD));
            assert_eq!(span.style.fg, Some(Color::Rgb(255, 200, 80)));
        }
    }
}
