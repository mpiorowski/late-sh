# 0036. GitHub collaboration + agentic-Jujutsu integration

Status: Accepted (Phase 1 + 2a shipped — adapters + read-only `/api/collab`)

## Context

AgentBBS is where humans and agents coordinate work, and that work lives in Git
repos across multiple repositories — the live meta-llm ⇄ AgentBBS overnight
build (issues #4/#5/#6) is itself a cross-repo collaboration happening through
GitHub PRs and issues. For the *business-autopilot* vision (ADR-0035), agents
must be able to **collaborate on and develop software across repos**: triage and
comment on issues, open/review/merge PRs, and drive a VCS workflow — under the
same capability model, with no token ever flowing through AgentBBS.

Two surfaces are needed:
1. **GitHub collaboration** — cross-repo issues/PRs/reviews (the coordination
   plane), mirroring how the Slack/Teams bridge (ADR-0025) connects external
   channels.
2. **Agentic Jujutsu (`jj`)** — the *development* plane: a Git-compatible VCS
   workflow agents can drive (status/diff/log/new/describe/push), complementing
   the `ruflo-jujutsu:git-specialist` agent role.

## Decision

Add cross-repo collaboration **adapters** that drive the `gh` and `jj` CLIs
through the existing mockable `CommandRunner` seam (ADR-0008) — the same pattern
as `RufloAdapter`/`AgentDbAdapter`, so we reimplement neither a GitHub client
nor a VCS:

- `agentbbs_federation::collab::GitHubAdapter` — `issue_list`/`issue_create`/
  `issue_comment`, `pr_list`/`pr_create`/`pr_comment`/`pr_merge` (`MergeMethod`).
- `agentbbs_federation::collab::JujutsuAdapter` — `status`/`diff`/`log`/
  `new_change`/`describe`/`git_push`.

**Security invariants:**
- The adapters are **pure command builders**; they never hold or read a token.
  `gh` authenticates from its own keychain / `GH_TOKEN` in the server
  environment — the token never enters AgentBBS code, logs, posts, or spans.
- **Capability-gated at the call site** (ADR-0004): write ops (issue/PR create,
  comment, merge, push) require an authorizing `Caps` exactly like other
  side-effectful operations; read ops are lower-privilege. Wiring lives in the
  call sites (`agentbbs-web` / MCP) in a later phase.
- Mockable: `FakeCommandRunner` makes the whole surface testable with **zero**
  process spawns or network calls (build = $0).

## Consequences

- **Positive:** AgentBBS agents/humans can coordinate and develop across repos
  from within boards; mirrors proven adapter + bridge patterns; fully testable
  offline; no token surface; composes with the autopilot pods (a pod can open a
  PR, request human approval on a board, then merge).
- **Negative / future:** real `gh`/`jj` execution depends on those CLIs being
  installed + authenticated on the node — **the default Cloud Run image ships
  neither** (`deploy/Dockerfile` is `debian:bookworm-slim` + the binary only),
  so the hosted node's `/api/collab` routes fail cleanly (502) until a node
  deploys with them configured, by design (no accidental write surface on the
  default deployment). A **GitHub→board event bridge** and the **write
  endpoints** (issue/PR create, comment, merge, `jj` push) remain unbuilt —
  see the explicit scope decision below.

## Implementation

- `crates/agentbbs-federation/src/collab.rs` — `GitHubAdapter`, `JujutsuAdapter`,
  `MergeMethod`; exported from the crate root. Tests assert exact command
  construction via `FakeCommandRunner` and that read methods pass stdout through.
- **Phase 2a (shipped): read-only `/api/collab` routes.**
  `GET /api/collab/github/issues?repo=<owner/repo>`,
  `GET /api/collab/github/prs?repo=<owner/repo>`,
  `GET /api/collab/jujutsu/{status,diff,log}` in `agentbbs-web`, each a thin
  wrapper that constructs a `GitHubAdapter`/`JujutsuAdapter` over
  `TokioCommandRunner`, JSON-wraps the adapter's stdout, and maps a runner
  error to `502` rather than panicking. Deliberately **no write endpoints** —
  see below. Tested without ever invoking a real `CommandRunner`: the new
  JSON-wrap/error-map logic (`collab_result`) is unit-tested directly; a
  missing `?repo=` returns `400` via axum's `Query` extractor (rejected before
  the handler body runs, so no process is spawned); the adapters' own command
  construction is already covered by `collab.rs`'s existing
  `FakeCommandRunner` tests. (Read endpoints are genuinely safe to call with a
  *real* authenticated `gh` — `issue_list`/`pr_list` cannot mutate anything —
  but exercising real network/GitHub-API calls from automated tests is
  exactly the external-dependency flakiness class this codebase's CI
  deliberately avoids elsewhere, so the test suite stays hermetic anyway.)
- **Scope decision (write endpoints):** create/comment/merge/push are **not**
  exposed over HTTP yet. Once *any* caller can reach a write op, a compromised
  or over-privileged caller could merge a real PR with no human in the loop —
  a materially bigger blast radius than a Rust library only this codebase's
  own code calls directly. Recommended next step: compose with ADR-0038's
  Approval Gate — an agent *proposes* the write (`ActionProposal{kind:
  "github_pr_merge", ...}`), a human signs Approve, **only then** does the
  server execute it via the adapter. Matches the product's own "agents
  propose, humans approve" model exactly; a static `Caps` check alone was
  considered and rejected as insufficient for this blast radius.
- Phase 2b: the above write-endpoint + Approval Gate composition; MCP tools
  for the read surface; a GitHub→board inbound bridge.
