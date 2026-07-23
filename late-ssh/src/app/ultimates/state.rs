use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use late_core::models::ultimate_cooldown::UltimateCooldown;

use super::{
    effects::UltimateThemeEffect,
    manifest::{ULTIMATE_SPELLS, UltimateKind},
};
use crate::app::hub::shop::state::ShopState;

#[derive(Clone, Debug)]
pub(crate) struct UltimateCast {
    pub ultimate_id: String,
    pub seed: u64,
    pub duration_ms: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ActiveUltimateEffect {
    kind: UltimateKind,
    seed: u64,
    started_at: Instant,
    duration: Duration,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct UltimateState {
    selected_index: usize,
    active_effects: HashMap<UltimateKind, ActiveUltimateEffect>,
    cooldown_ready_at: HashMap<String, Instant>,
}

impl UltimateState {
    pub(crate) fn with_cooldowns(cooldowns: Vec<UltimateCooldown>) -> Self {
        let mut state = Self::default();
        for cooldown in cooldowns {
            state.set_cooldown(&cooldown.ultimate_id, cooldown.remaining);
        }
        state
    }

    pub(crate) fn tick(&mut self) {
        self.active_effects
            .retain(|_, effect| effect.started_at.elapsed() < effect.duration);
        let now = Instant::now();
        self.cooldown_ready_at.retain(|_, ready_at| *ready_at > now);
    }

    pub(crate) fn move_selection(&mut self, delta: isize, shop: &ShopState) {
        let len = super::owned_ultimates(shop).len();
        if len == 0 {
            self.selected_index = 0;
            return;
        }
        self.selected_index =
            (self.selected_index as isize + delta).rem_euclid(len as isize) as usize;
    }

    pub(crate) fn clamp_selection(&mut self, shop: &ShopState) {
        let len = super::owned_ultimates(shop).len();
        if len == 0 {
            self.selected_index = 0;
        } else {
            self.selected_index = self.selected_index.min(len - 1);
        }
    }

    pub(crate) fn selected_index(&self) -> usize {
        self.selected_index
    }

    pub(crate) fn selected_kind(&self, shop: &ShopState) -> Option<UltimateKind> {
        super::owned_ultimates(shop)
            .get(self.selected_index)
            .and_then(|item| UltimateKind::from_sku(&item.sku))
    }

    pub(crate) fn has_active_effect(&self) -> bool {
        self.active_effects
            .values()
            .any(|effect| effect.started_at.elapsed() < effect.duration)
    }

    pub(crate) fn apply_cast(&mut self, cast: &UltimateCast) -> Option<UltimateKind> {
        let kind = UltimateKind::from_id(&cast.ultimate_id)?;
        self.active_effects.clear();
        self.active_effects.insert(
            kind,
            ActiveUltimateEffect {
                kind,
                seed: cast.seed,
                started_at: Instant::now(),
                duration: Duration::from_millis(cast.duration_ms),
            },
        );
        Some(kind)
    }

    pub(crate) fn active_theme_effects(&self) -> Vec<UltimateThemeEffect> {
        ULTIMATE_SPELLS
            .iter()
            .filter_map(|spell| {
                let effect = self.active_effects.get(&spell.kind)?;
                Some(UltimateThemeEffect {
                    kind: effect.kind.effect_kind(),
                    seed: effect.seed,
                    elapsed_ms: effect.started_at.elapsed().as_millis() as u64,
                })
            })
            .collect()
    }

    pub(crate) fn cooldown_remaining(&self, kind: UltimateKind) -> Option<Duration> {
        self.cooldown_ready_at
            .get(kind.id())
            .map(|ready_at| ready_at.saturating_duration_since(Instant::now()))
            .filter(|remaining| !remaining.is_zero())
    }

    /// True while any spell still has cooldown time on the clock. The
    /// ultimate modal's minute-granularity label rides the per-minute
    /// global frame; tick edge-detects this flipping false to render the
    /// ready state.
    pub(crate) fn has_cooldown_running(&self) -> bool {
        let now = Instant::now();
        self.cooldown_ready_at
            .values()
            .any(|ready_at| *ready_at > now)
    }

    pub(crate) fn set_cooldown(&mut self, ultimate_id: &str, remaining: Duration) {
        if remaining.is_zero() {
            self.cooldown_ready_at.remove(ultimate_id);
            return;
        }
        self.cooldown_ready_at
            .insert(ultimate_id.to_string(), Instant::now() + remaining);
    }

    pub(crate) fn replace_cooldowns(&mut self, cooldowns: Vec<(String, Duration)>) {
        self.cooldown_ready_at.clear();
        for (ultimate_id, remaining) in cooldowns {
            self.set_cooldown(&ultimate_id, remaining);
        }
    }
}
