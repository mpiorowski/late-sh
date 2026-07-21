use std::{collections::HashSet, time::Duration};

pub(crate) mod effects;
mod manifest;
mod service;
mod state;
mod ui;

pub(crate) use effects::apply_ultimate_postprocess;
pub(crate) use manifest::{ULTIMATE_CAST_COOLDOWN, UltimateKind};
pub use service::UltimateService;
pub(crate) use state::{UltimateCast, UltimateState};
pub(crate) use ui::{draw, handle_input, open_ultimate_modal};

use crate::app::{
    common::primitives::Banner,
    hub::shop::{state::ShopState, svc::ShopCatalogItem},
    state::App,
};

pub fn owned_ultimates(shop: &ShopState) -> Vec<&ShopCatalogItem> {
    shop.all_items()
        .iter()
        .filter(|item| item.owned && item.is_ultimate_spell())
        .collect()
}

impl App {
    pub(crate) fn refresh_ultimate_cooldowns(&mut self) {
        let service = self.ultimate_service.clone();
        let user_id = self.user_id;
        let token = self.session_token.clone();
        let Some(registry) = self.session_registry.clone() else {
            return;
        };
        tokio::spawn(async move {
            match service.list_cooldowns(user_id).await {
                Ok(cooldowns) => {
                    let cooldowns = cooldowns
                        .into_iter()
                        .map(|cooldown| {
                            (
                                cooldown.ultimate_id,
                                duration_millis_u64(cooldown.remaining),
                            )
                        })
                        .collect();
                    let _ = registry
                        .send_message(
                            &token,
                            crate::session::SessionMessage::UltimateCooldownDbRereadOk {
                                cooldowns,
                            },
                        )
                        .await;
                }
                Err(error) => {
                    tracing::warn!(?error, user_id = %user_id, "failed to refresh ultimate cooldowns");
                }
            }
        });
    }

    pub(crate) fn cast_ultimate(&mut self, kind: UltimateKind) {
        if self.ultimate_state.has_active_effect() {
            self.banner = Some(Banner::error("An ultimate is already active"));
            return;
        }
        if let Some(remaining) = self.ultimate_state.cooldown_remaining(kind) {
            self.banner = Some(Banner::error(&format!(
                "{} is cooling down ({})",
                kind.name(),
                format_cooldown(remaining)
            )));
            return;
        }
        let service = self.ultimate_service.clone();
        let user_id = self.user_id;
        let all_tokens = self.active_session_tokens();
        let user_tokens = self.current_user_session_tokens();
        let fallback_token = self.session_token.clone();
        let Some(registry) = self.session_registry.clone() else {
            return;
        };
        let cast = UltimateCast {
            ultimate_id: kind.id().to_string(),
            seed: uuid::Uuid::now_v7().as_u128() as u64,
            duration_ms: kind.duration_ms(),
        };
        self.ultimate_state
            .set_cooldown(kind.id(), ULTIMATE_CAST_COOLDOWN);
        self.banner = Some(Banner::success(&format!("Casting {}", kind.name())));
        tokio::spawn(async move {
            match service.try_claim_cast(user_id, kind).await {
                Ok(claim) if claim.allowed => {
                    let remaining_ms = duration_millis_u64(claim.remaining);
                    send_to_tokens(
                        &registry,
                        user_tokens,
                        crate::session::SessionMessage::UltimateCooldownUpdated {
                            ultimate_id: cast.ultimate_id.clone(),
                            remaining_ms,
                        },
                    )
                    .await;
                    send_to_tokens(
                        &registry,
                        all_tokens,
                        crate::session::SessionMessage::UltimateCast {
                            ultimate_id: cast.ultimate_id,
                            seed: cast.seed,
                            duration_ms: cast.duration_ms,
                        },
                    )
                    .await;
                }
                Ok(claim) => {
                    let remaining_ms = duration_millis_u64(claim.remaining);
                    send_to_tokens(
                        &registry,
                        vec![fallback_token],
                        crate::session::SessionMessage::UltimateCastRejected {
                            ultimate_id: cast.ultimate_id,
                            remaining_ms,
                        },
                    )
                    .await;
                }
                Err(error) => {
                    tracing::error!(?error, user_id = %user_id, ultimate = kind.id(), "failed to cast ultimate");
                    send_to_tokens(
                        &registry,
                        vec![fallback_token],
                        crate::session::SessionMessage::UltimateCastRejected {
                            ultimate_id: cast.ultimate_id,
                            remaining_ms: 0,
                        },
                    )
                    .await;
                }
            }
        });
    }

    fn active_session_tokens(&self) -> Vec<String> {
        self.active_users
            .as_ref()
            .map(|active_users| {
                let guard = active_users
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                guard
                    .values()
                    .flat_map(|user| user.sessions.iter().map(|session| session.token.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![self.session_token.clone()])
    }

    fn current_user_session_tokens(&self) -> Vec<String> {
        self.active_users
            .as_ref()
            .and_then(|active_users| {
                let guard = active_users
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner());
                guard.get(&self.user_id).map(|user| {
                    user.sessions
                        .iter()
                        .map(|session| session.token.clone())
                        .collect::<Vec<_>>()
                })
            })
            .filter(|tokens| !tokens.is_empty())
            .unwrap_or_else(|| vec![self.session_token.clone()])
    }
}

async fn send_to_tokens(
    registry: &crate::session::SessionRegistry,
    tokens: Vec<String>,
    message: crate::session::SessionMessage,
) {
    let mut seen = HashSet::new();
    for token in tokens {
        if seen.insert(token.clone()) {
            let _ = registry.send_message(&token, message.clone()).await;
        }
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

pub fn format_cooldown(duration: Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {mins}m")
    } else if mins > 0 {
        format!("{mins}m")
    } else {
        format!("{}s", secs.max(1))
    }
}

#[cfg(test)]
mod ultimates_test;
