//! Retort-MetaHarness — a DoE/ANOVA coding-agent benchmark **track**.
//!
//! Where CVE-Bench ([`crate::benchmark::Benchmark::cve_bench`]) ranks *agents*
//! by a single `pass@1` number, the **Retort** track ranks whole
//! **agent+harness+model stacks** from a Design-of-Experiments grid. The
//! retort-metaharness runs every cell of a factorial design
//! (`{model × harness_config × language × task}`), measures
//! `requirement_coverage`, code quality, `$/task` and latency per cell, and
//! attributes the variance to each factor with an ANOVA.
//!
//! This module ingests that results contract, aggregates it into per-stack
//! standings (the placement metric is **`requirement_coverage` at binned
//! cost**), and emits **signed** [`Submission`]s — reusing the exact
//! signed-score plumbing from ADR-0011 ([`RunResult`] / [`Submission::sign`] /
//! [`Submission::verify`]). It is *not* a fork of the signing path; it is a new
//! aggregation + stack-ranking layer on top of it.
//!
//! ## Honest scoring
//!
//! A cell can fail for two very different reasons: the model genuinely got it
//! wrong (`GENUINE`), or the *harness* mangled an otherwise-correct answer
//! (`TOOLING` — e.g. a truncated patch at a tool-call boundary). Counting
//! `TOOLING` false-fails against a stack would pollute the board. Aggregation
//! therefore **excludes** `TOOLING` cells from the score and records how many
//! were dropped ([`StackAggregate::cells_excluded_tooling`]) so the exclusion
//! is auditable, never silent.

use agentbbs_core::identity::{AgentId, Identity};
use agentbbs_core::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::benchmark::{Benchmark, BenchmarkId, ScoreKind};
use crate::submission::{RunResult, Submission};

/// Stable slug for the Retort track on the Arena leaderboard.
pub const RETORT_BENCHMARK_ID: &str = "retort-metaharness";

/// The results-contract schema string this module understands.
pub const RETORT_SCHEMA: &str = "retort.metaharness.results.v1";

/// The Retort benchmark catalogue entry. Scored as a [`ScoreKind::PassRate`]
/// (the placement metric `requirement_coverage` lives in `[0,1]`, higher wins).
pub fn retort_benchmark() -> Benchmark {
    Benchmark {
        id: BenchmarkId(RETORT_BENCHMARK_ID.into()),
        name: "Retort MetaHarness (DoE/ANOVA)".into(),
        description: "Rank agent+harness+model stacks on a Design-of-Experiments coding grid. \
            Placement = requirement_coverage at binned $/task; ANOVA attributes variance to \
            {model, harness-config, language, task}. TOOLING false-fails are excluded (honest \
            scoring)."
            .into(),
        score_kind: ScoreKind::PassRate,
        harness: "npx retort bench metaharness --doe {design} --json".into(),
        categories: vec![
            "requirement-coverage".into(),
            "code-quality".into(),
            "cost-efficiency".into(),
            "doe-anova".into(),
        ],
    }
}

/// Why a cell did or did not pass — the TOOLING/GENUINE diagnosis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Diagnosis {
    /// The cell genuinely succeeded.
    Pass,
    /// The cell genuinely failed — a real model/stack shortcoming. Counted.
    Genuine,
    /// A harness artifact mangled a correct answer (false-fail). Excluded from
    /// scoring so it cannot pollute the board.
    Tooling,
}

impl Diagnosis {
    /// Whether this cell counts toward a stack's score (i.e. not a TOOLING
    /// false-fail).
    pub fn is_scored(self) -> bool {
        !matches!(self, Diagnosis::Tooling)
    }
}

/// The factorial design that was run.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DoeDesign {
    /// Models under test (factor level set).
    #[serde(default)]
    pub models: Vec<String>,
    /// Harness configurations (factor level set).
    #[serde(default)]
    pub harness_configs: Vec<String>,
    /// Languages (factor level set).
    #[serde(default)]
    pub languages: Vec<String>,
    /// Tasks (factor level set).
    #[serde(default)]
    pub tasks: Vec<String>,
}

/// One measured cell of the DoE grid.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetortCell {
    /// Model factor level.
    pub model: String,
    /// Harness-config factor level.
    pub harness_config: String,
    /// Language factor level.
    pub language: String,
    /// Task factor level.
    pub task: String,
    /// Requirement coverage in `[0,1]` — the placement metric.
    pub requirement_coverage: f64,
    /// Code-quality score in `[0,1]`.
    #[serde(default)]
    pub code_quality: f64,
    /// Dollar cost for this task ($/task).
    #[serde(default)]
    pub cost_usd: f64,
    /// Wall-clock latency in seconds.
    #[serde(default)]
    pub latency_seconds: f64,
    /// TOOLING/GENUINE diagnosis.
    pub diagnosis: Diagnosis,
    /// Whether the cell passed (genuine success).
    #[serde(default)]
    pub passed: bool,
}

/// One factor's ANOVA attribution row.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FactorAttribution {
    /// Factor name (`model`, `harness_config`, `language`, `task`).
    pub factor: String,
    /// Sum of squares.
    #[serde(default)]
    pub sum_of_squares: f64,
    /// Degrees of freedom.
    #[serde(default)]
    pub df: u32,
    /// Mean square.
    #[serde(default)]
    pub mean_square: f64,
    /// F statistic.
    #[serde(default)]
    pub f_stat: f64,
    /// p-value.
    #[serde(default)]
    pub p_value: f64,
    /// Fraction of variance explained by this factor (eta-squared style).
    #[serde(default)]
    pub variance_explained: f64,
}

/// The ANOVA decomposition of the response across factors.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AnovaResult {
    /// Which response was decomposed (e.g. `requirement_coverage`).
    #[serde(default)]
    pub response: String,
    /// Per-factor attribution rows.
    #[serde(default)]
    pub factors: Vec<FactorAttribution>,
    /// Residual (unexplained) sum of squares.
    #[serde(default)]
    pub residual_sum_of_squares: f64,
    /// Residual degrees of freedom.
    #[serde(default)]
    pub residual_df: u32,
    /// Total variance explained by all factors combined.
    #[serde(default)]
    pub total_variance_explained: f64,
}

impl AnovaResult {
    /// The factor explaining the most variance, if any.
    pub fn dominant_factor(&self) -> Option<&FactorAttribution> {
        self.factors.iter().max_by(|a, b| {
            a.variance_explained
                .partial_cmp(&b.variance_explained)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

/// The full retort-metaharness results contract.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetortResults {
    /// Schema discriminator; must equal [`RETORT_SCHEMA`].
    pub schema: String,
    /// Producing harness version string.
    #[serde(default)]
    pub harness_version: String,
    /// When the run finished.
    pub generated_at: DateTime<Utc>,
    /// The factorial design that was run.
    #[serde(default)]
    pub design: DoeDesign,
    /// Per-cell measurements.
    pub cells: Vec<RetortCell>,
    /// ANOVA factor attribution.
    #[serde(default)]
    pub anova: AnovaResult,
}

impl RetortResults {
    /// A small built-in demo bundle (2 models × 2 harness configs × 1 language
    /// × 2 tasks, with one TOOLING false-fail) — used to seed demo arenas and
    /// the `--demo` CLI path. Mirrors `tests/fixtures/retort-results.v1.json`.
    pub fn sample() -> Self {
        #[allow(clippy::too_many_arguments)]
        fn cell(
            model: &str,
            harness: &str,
            task: &str,
            cov: f64,
            quality: f64,
            cost: f64,
            latency: f64,
            diag: Diagnosis,
            passed: bool,
        ) -> RetortCell {
            RetortCell {
                model: model.into(),
                harness_config: harness.into(),
                language: "rust".into(),
                task: task.into(),
                requirement_coverage: cov,
                code_quality: quality,
                cost_usd: cost,
                latency_seconds: latency,
                diagnosis: diag,
                passed,
            }
        }
        fn fa(factor: &str, ss: f64, ve: f64) -> FactorAttribution {
            FactorAttribution {
                factor: factor.into(),
                sum_of_squares: ss,
                df: 1,
                mean_square: ss,
                f_stat: 0.0,
                p_value: 0.0,
                variance_explained: ve,
            }
        }
        RetortResults {
            schema: RETORT_SCHEMA.into(),
            harness_version: "retort-metaharness@0.1.0".into(),
            generated_at: "2026-06-28T12:00:00Z".parse().unwrap_or_else(|_| Utc::now()),
            design: DoeDesign {
                models: vec!["claude-opus-4.8".into(), "deepseek-v4".into()],
                harness_configs: vec!["ruflo-3tier".into(), "single-shot".into()],
                languages: vec!["rust".into()],
                tasks: vec!["task-a".into(), "task-b".into()],
            },
            cells: vec![
                cell("claude-opus-4.8", "ruflo-3tier", "task-a", 0.95, 0.90, 0.082, 41.0, Diagnosis::Pass, true),
                cell("claude-opus-4.8", "ruflo-3tier", "task-b", 0.90, 0.88, 0.078, 39.0, Diagnosis::Pass, true),
                cell("claude-opus-4.8", "single-shot", "task-a", 0.85, 0.80, 0.041, 22.0, Diagnosis::Pass, true),
                cell("claude-opus-4.8", "single-shot", "task-b", 0.0, 0.0, 0.039, 21.0, Diagnosis::Tooling, false),
                cell("deepseek-v4", "ruflo-3tier", "task-a", 0.70, 0.66, 0.012, 33.0, Diagnosis::Genuine, false),
                cell("deepseek-v4", "ruflo-3tier", "task-b", 0.65, 0.60, 0.011, 31.0, Diagnosis::Genuine, false),
                cell("deepseek-v4", "single-shot", "task-a", 0.55, 0.52, 0.006, 18.0, Diagnosis::Genuine, false),
                cell("deepseek-v4", "single-shot", "task-b", 0.50, 0.48, 0.005, 17.0, Diagnosis::Genuine, false),
            ],
            anova: AnovaResult {
                response: "requirement_coverage".into(),
                factors: vec![
                    fa("model", 0.2048, 0.612),
                    fa("harness_config", 0.0512, 0.153),
                    fa("task", 0.0098, 0.029),
                    fa("language", 0.0, 0.0),
                ],
                residual_sum_of_squares: 0.0686,
                residual_df: 3,
                total_variance_explained: 0.794,
            },
        }
    }

    /// Parse a results bundle from JSON, validating the schema string.
    pub fn from_json(s: &str) -> Result<Self> {
        let r: RetortResults = serde_json::from_str(s)
            .map_err(|e| Error::malformed("retort results", format!("invalid JSON: {e}")))?;
        if r.schema != RETORT_SCHEMA {
            return Err(Error::malformed(
                "retort results",
                format!("unexpected schema {:?}, want {RETORT_SCHEMA:?}", r.schema),
            ));
        }
        if r.cells.is_empty() {
            return Err(Error::malformed("retort results", "no cells"));
        }
        Ok(r)
    }
}

/// A competing stack — the unit the Retort board ranks.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StackKey {
    /// Model factor level.
    pub model: String,
    /// Harness-config factor level.
    pub harness_config: String,
    /// Language factor level.
    pub language: String,
}

impl std::fmt::Display for StackKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} · {} · {}", self.model, self.harness_config, self.language)
    }
}

/// Coarse cost bin so coverage is compared "at equal cost". Order-of-magnitude
/// $/task buckets; lower bins are cheaper.
pub fn cost_bin(cost_usd: f64) -> &'static str {
    match cost_usd {
        c if c <= 0.0 => "free",
        c if c <= 0.01 => "≤$0.01",
        c if c <= 0.10 => "≤$0.10",
        c if c <= 1.00 => "≤$1.00",
        c if c <= 10.0 => "≤$10.00",
        _ => ">$10.00",
    }
}

/// An aggregated per-stack result with TOOLING false-fails already excluded.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StackAggregate {
    /// The stack identity.
    pub key: StackKey,
    /// Mean requirement_coverage over scored (non-TOOLING) cells — placement.
    pub mean_requirement_coverage: f64,
    /// Mean code-quality over scored cells.
    pub mean_code_quality: f64,
    /// Mean $/task over scored cells.
    pub mean_cost_usd: f64,
    /// Cost bin for the mean cost.
    pub cost_bin: String,
    /// Mean latency over scored cells.
    pub mean_latency_seconds: f64,
    /// Cells with a genuine `Pass`.
    pub cells_passed: u32,
    /// Scored cells (non-TOOLING) — the denominator.
    pub cells_total: u32,
    /// TOOLING false-fails excluded from scoring (auditable).
    pub cells_excluded_tooling: u32,
}

/// The per-stack detail that travels in [`RunResult::detail`] (round-trips so
/// the board can show coverage/cost/quality/ANOVA without re-ingesting).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetortDetail {
    /// Marker so consumers can recognise a retort submission's detail.
    pub kind: String,
    /// The stack.
    pub stack: StackKey,
    /// Name of the placement metric.
    pub placement_metric: String,
    /// Mean code-quality.
    pub code_quality: f64,
    /// Mean $/task.
    pub cost_usd: f64,
    /// Cost bin.
    pub cost_bin: String,
    /// Mean latency.
    pub latency_seconds: f64,
    /// Scored cells.
    pub cells_total: u32,
    /// TOOLING false-fails excluded.
    pub cells_excluded_tooling: u32,
    /// ANOVA factor attribution carried from the run.
    pub anova: AnovaResult,
}

/// Aggregate a results bundle into per-stack standings, excluding TOOLING
/// false-fails. Deterministic order (by stack key) for stable signing/tests.
pub fn aggregate_stacks(results: &RetortResults) -> Vec<StackAggregate> {
    use std::collections::BTreeMap;
    // accumulator: sums + counts per stack
    struct Acc {
        cov: f64,
        quality: f64,
        cost: f64,
        latency: f64,
        passed: u32,
        scored: u32,
        excluded: u32,
    }
    let mut by_stack: BTreeMap<StackKey, Acc> = BTreeMap::new();
    for c in &results.cells {
        let key = StackKey {
            model: c.model.clone(),
            harness_config: c.harness_config.clone(),
            language: c.language.clone(),
        };
        let acc = by_stack.entry(key).or_insert(Acc {
            cov: 0.0,
            quality: 0.0,
            cost: 0.0,
            latency: 0.0,
            passed: 0,
            scored: 0,
            excluded: 0,
        });
        if !c.diagnosis.is_scored() {
            acc.excluded += 1; // TOOLING false-fail — auditable, not counted
            continue;
        }
        acc.cov += c.requirement_coverage;
        acc.quality += c.code_quality;
        acc.cost += c.cost_usd;
        acc.latency += c.latency_seconds;
        acc.scored += 1;
        if c.diagnosis == Diagnosis::Pass && c.passed {
            acc.passed += 1;
        }
    }
    by_stack
        .into_iter()
        .map(|(key, a)| {
            let n = a.scored.max(1) as f64;
            let mean_cost = a.cost / n;
            StackAggregate {
                key,
                mean_requirement_coverage: a.cov / n,
                mean_code_quality: a.quality / n,
                mean_cost_usd: mean_cost,
                cost_bin: cost_bin(mean_cost).to_string(),
                mean_latency_seconds: a.latency / n,
                cells_passed: a.passed,
                cells_total: a.scored,
                cells_excluded_tooling: a.excluded,
            }
        })
        .collect()
}

/// Build the unsigned [`RunResult`] for one aggregated stack, attributed to the
/// `operator` who ran the benchmark (provenance) and carrying the ANOVA.
pub fn to_run_result(
    agg: &StackAggregate,
    operator: AgentId,
    anova: &AnovaResult,
    harness_version: &str,
    at: DateTime<Utc>,
) -> RunResult {
    let detail = RetortDetail {
        kind: "retort.stack.v1".into(),
        stack: agg.key.clone(),
        placement_metric: "requirement_coverage@cost_bin".into(),
        code_quality: agg.mean_code_quality,
        cost_usd: agg.mean_cost_usd,
        cost_bin: agg.cost_bin.clone(),
        latency_seconds: agg.mean_latency_seconds,
        cells_total: agg.cells_total,
        cells_excluded_tooling: agg.cells_excluded_tooling,
        anova: anova.clone(),
    };
    RunResult {
        benchmark: BenchmarkId(RETORT_BENCHMARK_ID.into()),
        competitor: operator,
        handle: agg.key.to_string(),
        score: agg.mean_requirement_coverage,
        passed: agg.cells_passed,
        total: agg.cells_total,
        harness: harness_version.to_string(),
        at,
        detail: serde_json::to_value(detail).unwrap_or(serde_json::Value::Null),
    }
}

/// Ingest a results bundle and produce **signed** submissions — one per stack,
/// signed by `identity` (the run operator). Reuses [`Submission::sign`]; the
/// stack descriptor and coverage score are part of the signed canonical bytes,
/// so they are tamper-evident.
pub fn ingest(results: &RetortResults, identity: &Identity) -> Result<Vec<Submission>> {
    let aggs = aggregate_stacks(results);
    let mut out = Vec::with_capacity(aggs.len());
    for agg in &aggs {
        let rr = to_run_result(
            agg,
            identity.id(),
            &results.anova,
            &results.harness_version,
            results.generated_at,
        );
        out.push(Submission::sign(identity, rr)?);
    }
    Ok(out)
}

/// A ranked Retort row — one **stack** (not one competitor).
#[derive(Clone, Debug, PartialEq)]
pub struct StackStanding {
    /// Rank, 1-based.
    pub rank: u32,
    /// The operator who signed the run (provenance).
    pub operator: AgentId,
    /// The stack descriptor (`model · harness · lang`).
    pub stack: String,
    /// Placement metric: mean requirement_coverage in `[0,1]`.
    pub requirement_coverage: f64,
    /// Mean code-quality.
    pub code_quality: f64,
    /// Mean $/task.
    pub cost_usd: f64,
    /// Cost bin (for equal-cost comparison).
    pub cost_bin: String,
    /// Genuine passes.
    pub passed: u32,
    /// Scored cells.
    pub total: u32,
    /// TOOLING false-fails excluded (transparency).
    pub excluded_tooling: u32,
    /// The dominant ANOVA factor name, if known.
    pub dominant_factor: Option<String>,
}

/// Rank Retort submissions **per stack** (keyed by handle), keeping each
/// stack's best run. Unlike [`crate::leaderboard::rank`] this does *not* dedup
/// by competitor — a single operator legitimately submits many stacks.
/// Submissions must already be verified by the caller (the [`crate::Arena`]
/// verifies on `submit`). Placement: `requirement_coverage` desc, then cheaper
/// `cost_usd`, then handle for determinism.
pub fn rank_stacks(submissions: &[Submission]) -> Vec<StackStanding> {
    use std::collections::HashMap;
    // best submission per stack handle
    let mut best: HashMap<String, &Submission> = HashMap::new();
    for s in submissions {
        if s.result.benchmark.0 != RETORT_BENCHMARK_ID {
            continue;
        }
        let better = match best.get(&s.result.handle) {
            None => true,
            Some(prev) => s.result.score > prev.result.score,
        };
        if better {
            best.insert(s.result.handle.clone(), s);
        }
    }
    let mut rows: Vec<&Submission> = best.into_values().collect();
    rows.sort_by(|a, b| {
        // higher coverage first
        b.result
            .score
            .partial_cmp(&a.result.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // then cheaper
            .then_with(|| {
                let ca = detail_of(a).map(|d| d.cost_usd).unwrap_or(f64::INFINITY);
                let cb = detail_of(b).map(|d| d.cost_usd).unwrap_or(f64::INFINITY);
                ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
            })
            // then stable by handle
            .then_with(|| a.result.handle.cmp(&b.result.handle))
    });
    rows.into_iter()
        .enumerate()
        .map(|(i, s)| {
            let d = detail_of(s);
            StackStanding {
                rank: (i + 1) as u32,
                operator: s.result.competitor,
                stack: s.result.handle.clone(),
                requirement_coverage: s.result.score,
                code_quality: d.as_ref().map(|d| d.code_quality).unwrap_or(0.0),
                cost_usd: d.as_ref().map(|d| d.cost_usd).unwrap_or(0.0),
                cost_bin: d
                    .as_ref()
                    .map(|d| d.cost_bin.clone())
                    .unwrap_or_else(|| "?".into()),
                passed: s.result.passed,
                total: s.result.total,
                excluded_tooling: d.as_ref().map(|d| d.cells_excluded_tooling).unwrap_or(0),
                dominant_factor: d
                    .as_ref()
                    .and_then(|d| d.anova.dominant_factor().map(|f| f.factor.clone())),
            }
        })
        .collect()
}

fn detail_of(s: &Submission) -> Option<RetortDetail> {
    serde_json::from_value::<RetortDetail>(s.result.detail.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_fixture() -> RetortResults {
        let json = include_str!("../tests/fixtures/retort-results.v1.json");
        RetortResults::from_json(json).expect("fixture parses")
    }

    #[test]
    fn fixture_parses_and_validates_schema() {
        let r = load_fixture();
        assert_eq!(r.schema, RETORT_SCHEMA);
        assert_eq!(r.cells.len(), 8);
    }

    #[test]
    fn bad_schema_rejected() {
        let bad = r#"{"schema":"nope","generated_at":"2026-06-28T12:00:00Z","cells":[]}"#;
        assert!(RetortResults::from_json(bad).is_err());
    }

    #[test]
    fn aggregation_excludes_tooling_false_fails() {
        let r = load_fixture();
        let aggs = aggregate_stacks(&r);
        // 4 stacks (2 models × 2 harness configs × 1 language)
        assert_eq!(aggs.len(), 4);
        // The single-shot opus stack has one TOOLING cell excluded.
        let opus_ss = aggs
            .iter()
            .find(|a| a.key.model == "claude-opus-4.8" && a.key.harness_config == "single-shot")
            .unwrap();
        assert_eq!(opus_ss.cells_excluded_tooling, 1);
        assert_eq!(opus_ss.cells_total, 1); // only the genuine task-a counts
        // Mean coverage is the surviving 0.85, NOT dragged to 0.425 by the false-fail.
        assert!((opus_ss.mean_requirement_coverage - 0.85).abs() < 1e-9);
    }

    #[test]
    fn ranking_is_by_coverage_and_stacks_not_competitors() {
        let r = load_fixture();
        let id = Identity::generate();
        let subs = ingest(&r, &id).unwrap();
        // All 4 stacks signed by ONE operator must still rank as 4 rows.
        for s in &subs {
            assert!(s.verify().is_ok());
        }
        let board = rank_stacks(&subs);
        assert_eq!(board.len(), 4);
        // Top is opus/ruflo-3tier (mean 0.925), last is deepseek/single-shot (0.525).
        assert_eq!(board[0].rank, 1);
        assert!(board[0].stack.starts_with("claude-opus-4.8 · ruflo-3tier"));
        assert!((board[0].requirement_coverage - 0.925).abs() < 1e-9);
        assert!(board[3].stack.starts_with("deepseek-v4 · single-shot"));
        // ANOVA attribution rides along: model dominates the variance.
        assert_eq!(board[0].dominant_factor.as_deref(), Some("model"));
    }

    #[test]
    fn tampered_coverage_fails_verification() {
        let r = load_fixture();
        let id = Identity::generate();
        let mut subs = ingest(&r, &id).unwrap();
        subs[0].result.score = 1.0; // forge a perfect coverage post-signing
        assert!(subs[0].verify().is_err());
    }

    #[test]
    fn sample_matches_fixture_aggregation() {
        let from_file = aggregate_stacks(&load_fixture());
        let from_code = aggregate_stacks(&RetortResults::sample());
        assert_eq!(from_file, from_code);
    }

    #[test]
    fn cost_bins_order_of_magnitude() {
        assert_eq!(cost_bin(0.0), "free");
        assert_eq!(cost_bin(0.008), "≤$0.01");
        assert_eq!(cost_bin(0.08), "≤$0.10");
        assert_eq!(cost_bin(0.9), "≤$1.00");
    }
}
