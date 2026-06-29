//! Meta-harness adapters that actually run benchmarks.
//!
//! Agents compete by running a benchmark through a *meta-harness* — `ruflo`
//! (the npm/npx Claude meta-harness) — which in turn drives the benchmark's
//! own harness. The flagship target is CVE-Bench (`ruvnet/cve-bench`), which
//! runs each CVE in a Docker sandbox.
//!
//! All process execution goes through the [`HarnessRunner`] trait, so the
//! arena logic is fully testable with a [`FakeHarnessRunner`] and never shells
//! out during unit tests.

use agentbbs_core::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Abstraction over running an external command, so harness invocation is
/// injectable and mockable.
#[async_trait]
pub trait HarnessRunner: Send + Sync {
    /// Run `program` with `args`, returning combined stdout on success.
    async fn run(&self, program: &str, args: &[String]) -> Result<String>;
}

/// Runs commands for real via `tokio::process::Command`.
pub struct TokioHarnessRunner;

#[async_trait]
impl HarnessRunner for TokioHarnessRunner {
    async fn run(&self, program: &str, args: &[String]) -> Result<String> {
        let output = tokio::process::Command::new(program)
            .args(args)
            .output()
            .await
            .map_err(|e| Error::Other(format!("spawn {program}: {e}")))?;
        if !output.status.success() {
            return Err(Error::Other(format!(
                "{program} exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

/// A canned runner for tests: returns preset stdout for any invocation, and
/// records the calls it received.
pub struct FakeHarnessRunner {
    stdout: String,
    /// Recorded `(program, args)` invocations.
    pub calls: std::sync::Mutex<Vec<(String, Vec<String>)>>,
}

impl FakeHarnessRunner {
    /// Build a fake that always returns `stdout`.
    pub fn new(stdout: impl Into<String>) -> Self {
        FakeHarnessRunner {
            stdout: stdout.into(),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl HarnessRunner for FakeHarnessRunner {
    async fn run(&self, program: &str, args: &[String]) -> Result<String> {
        self.calls
            .lock()
            .unwrap()
            .push((program.to_string(), args.to_vec()));
        Ok(self.stdout.clone())
    }
}

/// The JSON report shape we expect a benchmark harness to emit on stdout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HarnessReport {
    /// Benchmark slug.
    pub benchmark: String,
    /// Fraction in `[0,1]` (pass rate) or raw score, per the benchmark kind.
    pub score: f64,
    /// Tasks passed.
    pub passed: u32,
    /// Tasks attempted.
    pub total: u32,
    /// Meta-harness version string.
    #[serde(default)]
    pub harness: String,
    /// Optional per-task or per-CVE detail.
    #[serde(default)]
    pub detail: serde_json::Value,
}

/// Drives benchmarks through the `ruflo` meta-harness (`npx ruflo bench …`).
pub struct MetaHarness<R: HarnessRunner> {
    runner: R,
    /// The npx package to invoke (default `ruflo`).
    pub package: String,
}

impl<R: HarnessRunner> MetaHarness<R> {
    /// Build over a runner, using `ruflo` as the meta-harness package.
    pub fn new(runner: R) -> Self {
        MetaHarness {
            runner,
            package: "ruflo".into(),
        }
    }

    /// Run an arbitrary benchmark by slug for `agent`, parsing the JSON report.
    pub async fn run_benchmark(&self, benchmark: &str, agent: &str) -> Result<HarnessReport> {
        let args = [
            self.package.clone(),
            "bench".into(),
            benchmark.into(),
            "--agent".into(),
            agent.into(),
            "--json".into(),
        ]
        .to_vec();
        let out = self.runner.run("npx", &args).await?;
        parse_report(&out)
    }

    /// Run CVE-Bench (`ruvnet/cve-bench`) for `agent`.
    pub async fn run_cve_bench(&self, agent: &str) -> Result<HarnessReport> {
        self.run_benchmark("cve-bench", agent).await
    }
}

/// Parse a [`HarnessReport`] from harness stdout. Tolerates leading log lines
/// by scanning for the last line that parses as the report JSON.
pub fn parse_report(stdout: &str) -> Result<HarnessReport> {
    // Prefer a whole-string parse; fall back to the last JSON-looking line.
    if let Ok(r) = serde_json::from_str::<HarnessReport>(stdout.trim()) {
        return Ok(r);
    }
    for line in stdout.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') {
            if let Ok(r) = serde_json::from_str::<HarnessReport>(line) {
                return Ok(r);
            }
        }
    }
    Err(Error::malformed(
        "harness report",
        "no parseable JSON report on stdout",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn meta_harness_invokes_npx_ruflo() {
        let report = HarnessReport {
            benchmark: "cve-bench".into(),
            score: 0.325,
            passed: 13,
            total: 40,
            harness: "ruflo@3.5".into(),
            detail: serde_json::json!({"rce": 5}),
        };
        let json = serde_json::to_string(&report).unwrap();
        let runner = FakeHarnessRunner::new(json);
        let mh = MetaHarness::new(runner);
        let got = mh.run_cve_bench("claude-opus").await.unwrap();
        assert_eq!(got.passed, 13);
        assert_eq!(got.total, 40);
        // Verify we actually shelled `npx ruflo bench cve-bench --agent ...`.
        let calls = mh.runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "npx");
        assert!(calls[0].1.contains(&"ruflo".to_string()));
        assert!(calls[0].1.contains(&"cve-bench".to_string()));
        assert!(calls[0].1.contains(&"claude-opus".to_string()));
    }

    #[test]
    fn parse_report_skips_log_lines() {
        let stdout = "booting docker sandbox...\nrunning CVE-2024-1234\n{\"benchmark\":\"cve-bench\",\"score\":0.5,\"passed\":20,\"total\":40}\n";
        let r = parse_report(stdout).unwrap();
        assert_eq!(r.passed, 20);
    }

    #[test]
    fn parse_report_errors_on_garbage() {
        assert!(parse_report("no json here").is_err());
    }
}
