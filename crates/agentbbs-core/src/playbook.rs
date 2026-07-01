//! Playbooks — versioned, signed business workflows (ADR-0041).
//!
//! A [`Playbook`] is a content-addressed, ordered sequence of typed steps —
//! agent tasks, human approval gates (ADR-0038), and tool/door runs (ADR-0009).
//! It is the declarative process the autopilot runs; this module owns the
//! reviewable *definition* (type + validation + content hash). The runner that
//! walks steps and blocks at approval gates is Phase 2.

use serde::{Deserialize, Serialize};

use crate::approval::ApprovalGate;
use crate::error::{Error, Result};
use crate::identity::AgentId;

/// What a playbook step does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepKind {
    /// Assign work to an agent (or a pod hosting it).
    AgentTask {
        /// The agent handle to run the task.
        agent: String,
        /// What the agent should do.
        instruction: String,
    },
    /// Require a human sign-off before continuing (ADR-0038).
    ApprovalGate {
        /// What the human is approving.
        summary: String,
    },
    /// Run a tool / door (ADR-0009).
    Tool {
        /// The tool/door key.
        tool: String,
    },
}

impl StepKind {
    /// Canonical bytes for content addressing.
    fn tag_bytes(&self) -> Vec<u8> {
        match self {
            StepKind::AgentTask { agent, instruction } => {
                format!("agent_task\u{1f}{agent}\u{1f}{instruction}").into_bytes()
            }
            StepKind::ApprovalGate { summary } => {
                format!("approval_gate\u{1f}{summary}").into_bytes()
            }
            StepKind::Tool { tool } => format!("tool\u{1f}{tool}").into_bytes(),
        }
    }

    fn validate(&self) -> Result<()> {
        let ok = match self {
            StepKind::AgentTask { agent, instruction } => {
                !agent.trim().is_empty() && !instruction.trim().is_empty()
            }
            StepKind::ApprovalGate { summary } => !summary.trim().is_empty(),
            StepKind::Tool { tool } => !tool.trim().is_empty(),
        };
        if ok {
            Ok(())
        } else {
            Err(Error::malformed(
                "playbook",
                "step has an empty required field",
            ))
        }
    }
}

/// One step in a playbook.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaybookStep {
    /// Unique step id within the playbook.
    pub id: String,
    /// What the step does.
    #[serde(flatten)]
    pub kind: StepKind,
}

impl PlaybookStep {
    /// A one-line, human-readable summary of what this step does — used as the
    /// body of the `Step` progress message a runner posts as it advances
    /// (ADR-0052 Phase 2).
    pub fn progress_summary(&self) -> String {
        match &self.kind {
            StepKind::AgentTask { agent, instruction } => {
                format!("agent task — @{agent}: {instruction}")
            }
            StepKind::ApprovalGate { summary } => format!("approval gate — {summary}"),
            StepKind::Tool { tool } => format!("tool — {tool}"),
        }
    }
}

/// A versioned, content-addressed workflow definition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playbook {
    /// BLAKE3 content hash of the definition.
    pub playbook_id: String,
    /// Human-readable name.
    pub name: String,
    /// Version string (e.g. `1`, `2025.06`).
    pub version: String,
    /// What kicks the playbook off (opaque for now: cron, event, manual…).
    pub trigger: String,
    /// The ordered steps.
    pub steps: Vec<PlaybookStep>,
}

impl Playbook {
    /// Build a playbook, computing its content-addressed `playbook_id`.
    pub fn new(
        name: impl Into<String>,
        version: impl Into<String>,
        trigger: impl Into<String>,
        steps: Vec<PlaybookStep>,
    ) -> Self {
        let name = name.into();
        let version = version.into();
        let trigger = trigger.into();
        let mut buf = Vec::new();
        buf.extend_from_slice(b"agentbbs.playbook.v1\n");
        for part in [name.as_bytes(), version.as_bytes(), trigger.as_bytes()] {
            buf.extend_from_slice(format!("{}:", part.len()).as_bytes());
            buf.extend_from_slice(part);
            buf.push(b'\n');
        }
        for s in &steps {
            let tag = s.kind.tag_bytes();
            buf.extend_from_slice(format!("{}:{}\u{1f}", s.id.len(), s.id).as_bytes());
            buf.extend_from_slice(format!("{}:", tag.len()).as_bytes());
            buf.extend_from_slice(&tag);
            buf.push(b'\n');
        }
        let playbook_id = blake3::hash(&buf).to_hex().to_string();
        Playbook {
            playbook_id,
            name,
            version,
            trigger,
            steps,
        }
    }

    /// Validate: non-empty name/version, at least one step, unique step ids, and
    /// every step's required fields populated.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            return Err(Error::malformed(
                "playbook",
                "name and version are required",
            ));
        }
        if self.steps.is_empty() {
            return Err(Error::malformed(
                "playbook",
                "a playbook needs at least one step",
            ));
        }
        let mut seen = std::collections::HashSet::new();
        for s in &self.steps {
            if s.id.trim().is_empty() || !seen.insert(&s.id) {
                return Err(Error::malformed(
                    "playbook",
                    "step ids must be non-empty and unique",
                ));
            }
            s.kind.validate()?;
        }
        Ok(())
    }
}

/// The state of a [`PlaybookRun`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Ready to advance the current step.
    Running,
    /// The current step is an approval gate awaiting a human sign-off.
    AwaitingApproval,
    /// All steps done.
    Completed,
    /// Aborted.
    Failed,
}

/// A stateful walk through a [`Playbook`]. `advance` processes the current step
/// and moves to the next; an `ApprovalGate` step only advances when the
/// [`ApprovalGate`] authorizes its action (otherwise the run parks in
/// [`RunStatus::AwaitingApproval`] — fail-closed). Dispatching the actual work
/// (spawning a pod, running a tool) is the caller's job; this is the state
/// machine that sequences and gates it.
#[derive(Clone, Debug)]
pub struct PlaybookRun {
    playbook: Playbook,
    cursor: usize,
    status: RunStatus,
}

impl PlaybookRun {
    /// Start a run over a validated playbook.
    pub fn start(playbook: Playbook) -> Result<Self> {
        playbook.validate()?;
        Ok(PlaybookRun {
            playbook,
            cursor: 0,
            status: RunStatus::Running,
        })
    }

    /// The current status.
    pub fn status(&self) -> RunStatus {
        self.status
    }

    /// The playbook this run is executing.
    pub fn playbook(&self) -> &Playbook {
        &self.playbook
    }

    /// The step at the cursor, if any.
    pub fn current(&self) -> Option<&PlaybookStep> {
        self.playbook.steps.get(self.cursor)
    }

    /// How many steps have been advanced past (i.e. completed) so far. A runner
    /// posts one `Step` progress message per completed step (ADR-0052 Phase 2);
    /// this is the cursor, exposed read-only.
    pub fn steps_completed(&self) -> usize {
        self.cursor
    }

    /// The deterministic approval action-id for the current `ApprovalGate` step
    /// (what a human signs a decision over), or `None` if the current step is
    /// not a gate.
    pub fn gate_action_id(&self) -> Option<String> {
        match self.current()?.kind {
            StepKind::ApprovalGate { .. } => Some(format!(
                "playbook:{}:{}",
                self.playbook.playbook_id,
                self.current()?.id
            )),
            _ => None,
        }
    }

    /// Process the current step and advance. `AgentTask`/`Tool` steps advance
    /// unconditionally (the caller has run them); an `ApprovalGate` advances only
    /// if `gate` authorizes its action for an `allowed` decider — otherwise the
    /// run parks in `AwaitingApproval` and the cursor does not move.
    pub fn advance(&mut self, gate: &ApprovalGate, allowed: &[AgentId]) -> RunStatus {
        if matches!(self.status, RunStatus::Completed | RunStatus::Failed) {
            return self.status;
        }
        let is_gate = matches!(
            self.current().map(|s| &s.kind),
            Some(StepKind::ApprovalGate { .. })
        );
        if is_gate {
            let aid = self.gate_action_id().unwrap();
            if !gate.is_authorized(&aid, allowed) {
                self.status = RunStatus::AwaitingApproval;
                return self.status;
            }
        }
        self.cursor += 1;
        self.status = if self.cursor >= self.playbook.steps.len() {
            RunStatus::Completed
        } else {
            RunStatus::Running
        };
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Playbook {
        Playbook::new(
            "triage-inbound-lead",
            "1",
            "event:lead.created",
            vec![
                PlaybookStep {
                    id: "research".into(),
                    kind: StepKind::AgentTask {
                        agent: "claude".into(),
                        instruction: "enrich the lead from public sources".into(),
                    },
                },
                PlaybookStep {
                    id: "approve-spend".into(),
                    kind: StepKind::ApprovalGate {
                        summary: "approve $5 enrichment spend".into(),
                    },
                },
                PlaybookStep {
                    id: "crm".into(),
                    kind: StepKind::Tool {
                        tool: "crm.upsert".into(),
                    },
                },
            ],
        )
    }

    #[test]
    fn content_addressed_and_roundtrips() {
        let p = sample();
        assert!(p.validate().is_ok());
        assert_eq!(p.playbook_id, sample().playbook_id); // deterministic
                                                         // serde roundtrip across all three step kinds.
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["steps"][0]["kind"], "agent_task");
        assert_eq!(v["steps"][1]["kind"], "approval_gate");
        assert_eq!(v["steps"][2]["kind"], "tool");
        let back: Playbook = serde_json::from_value(v).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn id_is_content_bound() {
        let p = sample();
        let mut steps = p.steps.clone();
        steps.push(PlaybookStep {
            id: "extra".into(),
            kind: StepKind::Tool {
                tool: "noop".into(),
            },
        });
        let p2 = Playbook::new("triage-inbound-lead", "1", "event:lead.created", steps);
        assert_ne!(p.playbook_id, p2.playbook_id);
    }

    #[test]
    fn run_blocks_at_gate_until_approved() {
        use crate::approval::{ApprovalGate, SignedDecision, Verdict};
        use crate::identity::Identity;
        let human = Identity::generate();
        let mut run = PlaybookRun::start(sample()).unwrap();
        let mut gate = ApprovalGate::new();
        let empty: Vec<crate::identity::AgentId> = vec![];

        // Step 0 (AgentTask) advances unconditionally → now at the gate.
        assert_eq!(run.advance(&gate, &empty), RunStatus::Running);
        assert!(matches!(
            run.current().unwrap().kind,
            StepKind::ApprovalGate { .. }
        ));

        // Without an approval, the run parks (cursor unchanged).
        assert_eq!(
            run.advance(&gate, &[human.id()]),
            RunStatus::AwaitingApproval
        );
        assert!(matches!(
            run.current().unwrap().kind,
            StepKind::ApprovalGate { .. }
        ));

        // Human signs an Approve over the gate's action id → it advances.
        let aid = run.gate_action_id().unwrap();
        let when = chrono::DateTime::parse_from_rfc3339("2026-06-30T05:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        gate.record(SignedDecision::sign(
            &human,
            aid,
            Verdict::Approve,
            "ok",
            when,
        ))
        .unwrap();
        assert_eq!(run.advance(&gate, &[human.id()]), RunStatus::Running); // past the gate → at Tool

        // Final Tool step completes the run.
        assert_eq!(run.advance(&gate, &[human.id()]), RunStatus::Completed);
        assert!(run.current().is_none());
    }

    #[test]
    fn progress_summary_and_steps_completed_track_the_run() {
        use crate::approval::ApprovalGate;
        let p = sample();
        // Per-kind human summaries (ADR-0052 Phase 2 Step bodies).
        assert!(p.steps[0]
            .progress_summary()
            .starts_with("agent task — @claude:"));
        assert!(p.steps[1].progress_summary().starts_with("approval gate —"));
        assert_eq!(p.steps[2].progress_summary(), "tool — crm.upsert");

        // steps_completed advances with the cursor and stops at the gate.
        let mut run = PlaybookRun::start(p).unwrap();
        let gate = ApprovalGate::new();
        let empty: Vec<crate::identity::AgentId> = vec![];
        assert_eq!(run.steps_completed(), 0);
        run.advance(&gate, &empty); // past AgentTask → parked at the gate
        assert_eq!(run.steps_completed(), 1);
        run.advance(&gate, &empty); // gate not approved → cursor unchanged
        assert_eq!(run.steps_completed(), 1);
    }

    #[test]
    fn validation_rejects_bad_definitions() {
        // empty name
        let mut p = sample();
        p.name = "".into();
        assert!(p.validate().is_err());
        // no steps
        let p = Playbook::new("x", "1", "manual", vec![]);
        assert!(p.validate().is_err());
        // duplicate step ids
        let dup = Playbook::new(
            "x",
            "1",
            "manual",
            vec![
                PlaybookStep {
                    id: "a".into(),
                    kind: StepKind::Tool { tool: "t".into() },
                },
                PlaybookStep {
                    id: "a".into(),
                    kind: StepKind::Tool { tool: "u".into() },
                },
            ],
        );
        assert!(dup.validate().is_err());
        // empty step field
        let empty = Playbook::new(
            "x",
            "1",
            "manual",
            vec![PlaybookStep {
                id: "a".into(),
                kind: StepKind::Tool { tool: "  ".into() },
            }],
        );
        assert!(empty.validate().is_err());
    }
}
