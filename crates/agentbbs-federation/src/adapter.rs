//! Thin adapters that shell out to the `ruflo` and `agentdb` Node CLIs.
//!
//! AgentBBS federation borrows the operational model of ruflo/ruv-swarm: peer
//! links and shared memory are managed by the existing `npx ruflo …` and
//! `npx agentdb …` tools. Rather than reimplement them, we drive them through
//! a [`CommandRunner`] seam. Production uses [`TokioCommandRunner`]; tests use
//! [`FakeCommandRunner`] and never spawn a real process.

use agentbbs_core::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// An abstraction over "run a program and capture its stdout".
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Run `program` with `args`, returning captured stdout on success.
    async fn run(&self, program: &str, args: &[String]) -> Result<String>;
}

/// Runs commands for real via [`tokio::process::Command`].
#[derive(Clone, Default)]
pub struct TokioCommandRunner;

impl TokioCommandRunner {
    /// A new runner.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CommandRunner for TokioCommandRunner {
    async fn run(&self, program: &str, args: &[String]) -> Result<String> {
        let output = tokio::process::Command::new(program)
            .args(args)
            .output()
            .await
            .map_err(|e| Error::Other(format!("spawn {program}: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Other(format!(
                "{program} exited with {}: {}",
                output.status, stderr
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// A scripted runner for tests: returns canned stdout and records invocations.
///
/// Construct with [`FakeCommandRunner::with_output`]; inspect what was run via
/// [`FakeCommandRunner::calls`].
#[derive(Clone, Default)]
pub struct FakeCommandRunner {
    output: String,
    calls: std::sync::Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

impl FakeCommandRunner {
    /// A runner that always returns `output` as stdout.
    pub fn with_output(output: impl Into<String>) -> Self {
        FakeCommandRunner {
            output: output.into(),
            calls: Default::default(),
        }
    }

    /// The recorded invocations, each as `[program, arg0, arg1, …]`.
    pub fn calls(&self) -> Vec<Vec<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl CommandRunner for FakeCommandRunner {
    async fn run(&self, program: &str, args: &[String]) -> Result<String> {
        let mut record = Vec::with_capacity(args.len() + 1);
        record.push(program.to_string());
        record.extend(args.iter().cloned());
        self.calls.lock().unwrap().push(record);
        Ok(self.output.clone())
    }
}

/// Drives `npx ruflo federation <sub> …` for peer-link management.
pub struct RufloAdapter<R: CommandRunner> {
    runner: R,
}

impl<R: CommandRunner> RufloAdapter<R> {
    /// Wrap a runner.
    pub fn new(runner: R) -> Self {
        RufloAdapter { runner }
    }

    async fn federation(&self, sub: &str, extra: &[&str]) -> Result<String> {
        let mut args = vec![
            "ruflo".to_string(),
            "federation".to_string(),
            sub.to_string(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        self.runner.run("npx", &args).await
    }

    /// `npx ruflo federation init` — bootstrap this node's federation state.
    pub async fn federation_init(&self) -> Result<String> {
        self.federation("init", &[]).await
    }

    /// `npx ruflo federation join <addr>` — link to a peer at `addr`.
    pub async fn federation_join(&self, addr: &str) -> Result<String> {
        self.federation("join", &[addr]).await
    }

    /// `npx ruflo federation status` — report current link state.
    pub async fn federation_status(&self) -> Result<String> {
        self.federation("status", &[]).await
    }
}

/// A typed row returned from an `agentdb` memory query.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// The memory key.
    pub key: String,
    /// The stored value.
    pub value: String,
}

/// Drives `npx agentdb …` for cross-node shared memory.
pub struct AgentDbAdapter<R: CommandRunner> {
    runner: R,
}

impl<R: CommandRunner> AgentDbAdapter<R> {
    /// Wrap a runner.
    pub fn new(runner: R) -> Self {
        AgentDbAdapter { runner }
    }

    /// `npx agentdb store <key> <value>` — persist a memory; returns stdout.
    pub async fn store_memory(&self, key: &str, value: &str) -> Result<String> {
        let args = vec![
            "agentdb".to_string(),
            "store".to_string(),
            key.to_string(),
            value.to_string(),
        ];
        self.runner.run("npx", &args).await
    }

    /// `npx agentdb query <key>` — fetch matching memories as typed records.
    /// Expects the CLI to emit a JSON array of `{key, value}` objects.
    pub async fn query_memory(&self, key: &str) -> Result<Vec<MemoryRecord>> {
        let args = vec![
            "agentdb".to_string(),
            "query".to_string(),
            key.to_string(),
        ];
        let stdout = self.runner.run("npx", &args).await?;
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(vec![]);
        }
        serde_json::from_str(trimmed).map_err(Error::from)
    }
}
