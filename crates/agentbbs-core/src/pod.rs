//! Domain agent-pod templates — the AgentBBS-side control-plane primitive for
//! ADR-0035 (MetaHarness domain agent pods).
//!
//! A [`PodTemplate`] is the declarative definition of a hosted autonomous
//! worker: a domain system prompt, its (firewalled) tools, the **behavioral
//! gate** (`bench_assertions`) that every loop must pass, and the governance
//! bounds that make it runaway-proof — a Reserve-and-Commit hard cap
//! (`per_agent_cap_usd`) and a tier ceiling ([`MaxTier`]). Pods report signed
//! step-results into `registered_room` (a board slug, ADR-0003). This module
//! owns only the *type + validation*; spawning/monitoring (the `PodController`)
//! lives in `agentbbs-web`.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The model-tier ceiling a pod may escalate to (cheap-by-default, frontier on
/// hard steps — the meta-llm `cognitum-auto` dial, ADR-0034). Ordered
/// `Low < Mid < High`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaxTier {
    /// Cheapest tier only.
    Low,
    /// Up to the mid tier.
    Mid,
    /// May escalate to the frontier tier.
    High,
}

/// The lifecycle state of a spawned pod, mirroring the meta-llm Darwin Loop
/// (ADR-0035): `Spawned → Executing → Evaluating → {Escalating → Executing |
/// Completed | Failed}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PodStatus {
    /// Created, not yet running.
    Spawned,
    /// Running a loop step.
    Executing,
    /// Checking the behavioral gate (`bench_assertions`).
    Evaluating,
    /// Bumping the tier after an ambiguous/failed cheap pass.
    Escalating,
    /// Finished successfully (gate passed / goal met).
    Completed,
    /// Finished unsuccessfully (gate failed / budget exhausted / error).
    Failed,
}

impl PodStatus {
    /// Whether this is a terminal state (no further transitions).
    pub fn is_terminal(self) -> bool {
        matches!(self, PodStatus::Completed | PodStatus::Failed)
    }

    /// Whether `self → next` is a legal lifecycle transition.
    pub fn can_transition_to(self, next: PodStatus) -> bool {
        use PodStatus::*;
        matches!(
            (self, next),
            (Spawned, Executing)
                | (Executing, Evaluating)
                | (Evaluating, Escalating)
                | (Evaluating, Completed)
                | (Evaluating, Failed)
                | (Escalating, Executing)
                | (Executing, Failed)
        )
    }
}

/// The declarative definition of a domain agent pod (ADR-0035 `template_ref`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodTemplate {
    /// Stable reference, `domain/name@version` (e.g. `research/competitive-intel@1`).
    pub template_ref: String,
    /// Vertical: `research` | `coding` | `security` | `trading` | `tasks` | `business-ops`.
    pub domain: String,
    /// Domain system prompt steering the pod.
    pub system_prompt: String,
    /// Domain tools the pod may call (AgentiCow-firewalled at runtime).
    pub tools: Vec<String>,
    /// The AgentiCow behavioral pass/fail set — the per-loop gate. The heart of
    /// each vertical; required (an ungated pod is not allowed).
    pub bench_assertions: String,
    /// Reserve-and-Commit hard spend cap, USD. Must be finite and > 0.
    pub per_agent_cap_usd: f64,
    /// Cron schedule for a recurring pod; `None` for a bounded long-horizon run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_schedule: Option<String>,
    /// Tier ceiling.
    pub max_tier: MaxTier,
    /// Board slug the pod posts signed step-results into (`rooms = boards`).
    pub registered_room: String,
}

impl PodTemplate {
    /// Validate the template's invariants (ADR-0035): non-empty identity fields,
    /// a positive finite spend cap (runaway-proof), a required behavioral gate,
    /// a slug-shaped room, and a 5-field cron when scheduled. Returns
    /// [`Error::malformed`] on the first violation.
    pub fn validate(&self) -> Result<()> {
        if self.template_ref.trim().is_empty() || !self.template_ref.contains('@') {
            return Err(Error::malformed(
                "pod",
                "template_ref must be non-empty and shaped domain/name@version",
            ));
        }
        for (field, val) in [
            ("domain", &self.domain),
            ("system_prompt", &self.system_prompt),
            ("bench_assertions", &self.bench_assertions),
        ] {
            if val.trim().is_empty() {
                return Err(Error::malformed(
                    "pod",
                    format!("{field} must not be empty"),
                ));
            }
        }
        if !self.per_agent_cap_usd.is_finite() || self.per_agent_cap_usd <= 0.0 {
            return Err(Error::malformed(
                "pod",
                "per_agent_cap_usd must be finite and > 0 (a pod must be bounded)",
            ));
        }
        if !is_slug(&self.registered_room) {
            return Err(Error::malformed(
                "pod",
                "registered_room must be a board slug ([a-z0-9-]+)",
            ));
        }
        if let Some(cron) = &self.cron_schedule {
            if cron.split_whitespace().count() != 5 {
                return Err(Error::malformed("pod", "cron_schedule must have 5 fields"));
            }
        }
        Ok(())
    }
}

/// A spawn request for a pod (the AgentBBS-side shape behind `/v1/pods/spawn`,
/// ADR-0035): the template plus an optional tier override (bounded by the
/// template's `max_tier`) and an idempotency key to dedupe spawns. Per-tenant
/// attribution is the gateway's job (via the `cog_` key → accountId), so it is
/// deliberately not carried here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PodSpec {
    /// The pod definition.
    pub template: PodTemplate,
    /// Requested starting tier; must be `<= template.max_tier`. `None` = the
    /// gateway's auto dial up to `max_tier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<MaxTier>,
    /// Optional client key to make a spawn idempotent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

impl PodSpec {
    /// Validate the template and that any requested `tier` respects the
    /// template's `max_tier` ceiling.
    pub fn validate(&self) -> Result<()> {
        self.template.validate()?;
        if let Some(tier) = self.tier {
            if tier > self.template.max_tier {
                return Err(Error::malformed(
                    "pod",
                    "requested tier exceeds the template max_tier",
                ));
            }
        }
        Ok(())
    }

    /// The tier the pod starts at: the requested override, else `max_tier`.
    pub fn effective_tier(&self) -> MaxTier {
        self.tier.unwrap_or(self.template.max_tier)
    }

    /// Map this spec onto the **frozen meta-llm `POST /v1/pods/spawn` request**
    /// (ADR-0035 / issue #6 contract). This is the AgentBBS-side serialization
    /// the `PodController` sends through the `cog_` gateway — keeping it here (and
    /// tested) makes flipping the spawn stub to the live call a config change, not
    /// a reshape. Per-tenant attribution stays the gateway's job (cog_ key →
    /// accountId), so it is intentionally absent.
    pub fn to_spawn_request(&self) -> SpawnRequest {
        let t = &self.template;
        SpawnRequest {
            pod_name: t.template_ref.clone(),
            domain: t.domain.clone(),
            template_ref: t.template_ref.clone(),
            cron_schedule: t.cron_schedule.clone(),
            per_agent_cap_usd: t.per_agent_cap_usd,
            shard_count: 1,
            bench_config: SpawnBenchConfig {
                tier: self.effective_tier(),
                pass_tier: None,
                max_output_tokens: None,
                max_steps: None,
            },
            registered_room: Some(t.registered_room.clone()),
        }
    }
}

/// The `bench_config` of a spawn request (frozen meta-llm `BenchConfig`): the
/// starting `tier`, an optional `pass_tier`, and runaway backstops.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpawnBenchConfig {
    /// Starting model tier (`low` | `mid` | `high`).
    pub tier: MaxTier,
    /// Optional tier the gate must pass at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_tier: Option<MaxTier>,
    /// Optional per-step output-token cap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Optional max loop steps (runaway backstop).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_steps: Option<u32>,
}

/// The frozen meta-llm `POST /v1/pods/spawn` request body (`SpawnPodInput`,
/// issue #6 contract). Field names/types match the runtime exactly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpawnRequest {
    /// Human label for the pod.
    pub pod_name: String,
    /// Vertical.
    pub domain: String,
    /// Stable template reference.
    pub template_ref: String,
    /// Cron schedule for a recurring pod.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cron_schedule: Option<String>,
    /// Reserve-and-Commit hard spend cap, USD.
    pub per_agent_cap_usd: f64,
    /// Number of shards to allocate (>= 1).
    pub shard_count: u32,
    /// The behavioral gate config.
    pub bench_config: SpawnBenchConfig,
    /// Board slug to route result posts to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registered_room: Option<String>,
}

/// A board-slug check: non-empty, lowercase alphanumerics and hyphens only.
fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn research_pod() -> PodTemplate {
        PodTemplate {
            template_ref: "research/competitive-intel@1".into(),
            domain: "research".into(),
            system_prompt: "You are a competitive-intelligence analyst.".into(),
            tools: vec!["web.search".into(), "rvf.memory".into()],
            bench_assertions: "every briefing claim has >=2 independent dated sources".into(),
            per_agent_cap_usd: 0.10,
            cron_schedule: Some("0 * * * *".into()),
            max_tier: MaxTier::Mid,
            registered_room: "research-intel".into(),
        }
    }

    #[test]
    fn valid_template_passes_and_roundtrips() {
        let p = research_pod();
        assert!(p.validate().is_ok());
        // Serde shape matches ADR-0035.
        let v: serde_json::Value = serde_json::to_value(&p).unwrap();
        assert_eq!(v["template_ref"], "research/competitive-intel@1");
        assert_eq!(v["max_tier"], "mid");
        let back: PodTemplate = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn long_horizon_pod_omits_cron() {
        let mut p = research_pod();
        p.cron_schedule = None;
        assert!(p.validate().is_ok());
        let v = serde_json::to_value(&p).unwrap();
        assert!(v.get("cron_schedule").is_none()); // skipped when None
    }

    #[test]
    fn unbounded_cap_is_rejected() {
        let mut p = research_pod();
        p.per_agent_cap_usd = 0.0;
        assert!(p.validate().is_err());
        p.per_agent_cap_usd = -1.0;
        assert!(p.validate().is_err());
        p.per_agent_cap_usd = f64::INFINITY;
        assert!(p.validate().is_err());
    }

    #[test]
    fn ungated_or_malformed_fields_rejected() {
        let mut p = research_pod();
        p.bench_assertions = "  ".into();
        assert!(p.validate().is_err()); // the behavioral gate is required

        let mut p = research_pod();
        p.registered_room = "Research Intel".into();
        assert!(p.validate().is_err()); // not a slug

        let mut p = research_pod();
        p.template_ref = "no-version".into();
        assert!(p.validate().is_err());

        let mut p = research_pod();
        p.cron_schedule = Some("0 *".into());
        assert!(p.validate().is_err()); // not 5 fields
    }

    #[test]
    fn spawn_request_matches_frozen_contract() {
        // A Low-tier override on a Mid-ceiling research pod.
        let spec = PodSpec {
            template: research_pod(),
            tier: Some(MaxTier::Low),
            idempotency_key: None,
        };
        let v = serde_json::to_value(spec.to_spawn_request()).unwrap();
        // Field names + types match the frozen /v1/pods/spawn SpawnPodInput.
        assert_eq!(v["pod_name"], "research/competitive-intel@1");
        assert_eq!(v["domain"], "research");
        assert_eq!(v["template_ref"], "research/competitive-intel@1");
        assert_eq!(v["per_agent_cap_usd"], 0.10);
        assert_eq!(v["shard_count"], 1);
        assert_eq!(v["bench_config"]["tier"], "low"); // effective (override) tier
        assert_eq!(v["registered_room"], "research-intel");
        assert_eq!(v["cron_schedule"], "0 * * * *");
        // Optional bench fields are omitted, not null.
        assert!(v["bench_config"].get("pass_tier").is_none());
    }

    #[test]
    fn spawn_request_defaults_to_template_ceiling_tier() {
        let spec = PodSpec {
            template: research_pod(), // max_tier = Mid
            tier: None,
            idempotency_key: None,
        };
        let v = serde_json::to_value(spec.to_spawn_request()).unwrap();
        assert_eq!(v["bench_config"]["tier"], "mid");
    }

    #[test]
    fn tier_ordering() {
        assert!(MaxTier::Low < MaxTier::Mid && MaxTier::Mid < MaxTier::High);
    }

    #[test]
    fn pod_status_lifecycle() {
        use PodStatus::*;
        assert!(Spawned.can_transition_to(Executing));
        assert!(Executing.can_transition_to(Evaluating));
        assert!(Evaluating.can_transition_to(Escalating));
        assert!(Escalating.can_transition_to(Executing));
        assert!(Evaluating.can_transition_to(Completed));
        assert!(Executing.can_transition_to(Failed));
        // Illegal jumps.
        assert!(!Spawned.can_transition_to(Completed));
        assert!(!Completed.can_transition_to(Executing));
        assert!(Completed.is_terminal() && Failed.is_terminal());
        assert!(!Executing.is_terminal());
        assert_eq!(serde_json::to_value(Escalating).unwrap(), "escalating");
    }

    #[test]
    fn pod_spec_tier_ceiling() {
        let mut spec = PodSpec {
            template: research_pod(), // max_tier = Mid
            tier: Some(MaxTier::Low),
            idempotency_key: Some("k1".into()),
        };
        assert!(spec.validate().is_ok());
        assert_eq!(spec.effective_tier(), MaxTier::Low);

        // Requested tier above the ceiling is rejected.
        spec.tier = Some(MaxTier::High);
        assert!(spec.validate().is_err());

        // No override → effective tier is the template ceiling.
        spec.tier = None;
        assert!(spec.validate().is_ok());
        assert_eq!(spec.effective_tier(), MaxTier::Mid);

        // An invalid template propagates.
        spec.template.per_agent_cap_usd = 0.0;
        assert!(spec.validate().is_err());
    }
}
