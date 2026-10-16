# Bastion Redux Review

Date: 2026-07-07
Branch: `mevanlc--bastion-redux`
Baseline: `upstream/main` at `v0.37.4`
Merge reviewed: `9c75425 Merge branch 'main' into mevanlc--bastion-redux`

## Scope

This review looked for stale-branch integration problems outside the direct
conflict-resolution path. The focus was on the new bastion service, the
`late-ssh` tunnel endpoint, deployment wiring, and Terraform/Kubernetes
boundaries.

## Findings

### High: `build_bastion` is not safe for manual deploys

Status: fixed in this branch.

Original issue: `build_ssh` had been updated to handle both `release` and
`workflow_dispatch` deploys, but `build_bastion` still used release-only
expressions:

- The job condition checked `github.ref` directly.
- `image_tag` came only from `github.event.release.tag_name`.
- `environment` came only from release prerelease state.
- `source_ref` was not passed to the reusable build workflow, whose fallback is
  `inputs.source_ref || github.ref`.
- Terraform consumes `needs.build_bastion.outputs.image_tag` as
  `bastion_image_tag`, so a bad bastion build output reaches apply.

On `workflow_dispatch`, `github.event.release.tag_name` is empty. That means the
bastion build can produce an invalid or empty image tag, and the build workflow
can check out the dispatcher ref instead of `inputs.release_tag`. If bastion is
enabled, Terraform then receives the wrong bastion image input or the deploy
fails before apply.

Current fix:

- `.github/workflows/deploy.yml:57-74` makes `build_bastion` mirror the
  `build_ssh` condition, including the
  `workflow_dispatch` path and the `-nethack` / `-dopewars` exclusions.
- `image_tag` is now
  `${{ github.event_name == 'workflow_dispatch' && inputs.release_tag ||
  github.event.release.tag_name }}`.
- `environment` now uses the same expression used by `build_ssh`.
- `source_ref` is now passed as `${{ github.event_name == 'workflow_dispatch' &&
  inputs.release_tag || github.ref }}`.

### Medium: Deploy waits for `service-ssh` but not `service-bastion`

Status: fixed in this branch.

Original issue: the deploy workflow applied Terraform and then waited only for
the legacy SSH deployment:

- It ran `kubectl rollout status deployment service-ssh -n default
  --timeout=180s`.
- It did not wait for `service-bastion`.

The bastion is a separate deployment with its own image, config, secret mounts,
network policy, and readiness behavior. A deploy can therefore report success
while the bastion rollout is still pending or failing. This is especially risky
when dogfooding or switching traffic to the bastion path.

Current fix:

- `.github/workflows/deploy.yml:159-179` renames the job to
  `wait_for_rollouts`, still waits for `service-ssh`, and conditionally waits
  for `service-bastion` when `BASTION_ENABLED == '1'`.

### Medium: Bastion likely reintroduces the 3-second initial auth delay

Status: fixed in this branch.

The main SSH server explicitly avoids delaying the initial `none` auth probe:

- `late-ssh/src/ssh.rs:139-143` sets `auth_rejection_time` to 3 seconds and
  `auth_rejection_time_initial` to zero.

Original issue: the new bastion server set the 3-second rejection delay but did
not set the initial exception:

- The bastion `russh::server::Config` set `auth_rejection_time` but left
  `auth_rejection_time_initial` at the `russh` default.

OpenSSH commonly starts with a `none` auth probe before attempting public-key
auth. Without the zero initial rejection time, the bastion path can add a
3-second delay to every successful connection even though the backend SSH path
already fixed this behavior.

Current fix:

- `late-bastion/src/ssh.rs:622-635` now copies the
  `auth_rejection_time_initial: Some(Duration::ZERO)` setting from `late-ssh`
  into the bastion `russh::server::Config`.

## Checked and not flagged

- The tunnel server lifetime intentionally follows `session_shutdown`, not only
  accept shutdown. `late-ssh/src/main.rs:489-499` runs the tunnel server under
  `session_shutdown`, and `late-ssh/src/tunnel.rs:268-274` rejects new tunnel
  requests once backend draining begins.
- The network-policy shape looks intentional: `service-ssh` keeps public
  ingress on `2222` and `4000`, while `/tunnel` on `4001` is limited to pods
  labeled `app = service-bastion` (`infra/network-policies.tf:21-65`).
- Bastion egress is tightly scoped to the backend tunnel port plus DNS
  (`infra/network-policies.tf:68-111`). That matches the current bastion role.
- The default `BASTION_TUNNEL_TRUSTED_CIDRS` is documented as matching the
  bastion pod CIDR (`infra/variables.tf:215-219`). This should be verified
  against the real cluster CIDR before enabling bastion in a non-default
  cluster.
- The bastion host key secret is mounted read-only (`infra/service-bastion.tf:146-164`).
  That is fine for production secret-backed operation, but the generate-on-miss
  path in local code will not be usable against that mounted secret path.

## Validation

Focused post-merge checks run during the initial review:

```bash
NEXTEST_TEST_THREADS=4 cargo nextest run -p late-bastion
NEXTEST_TEST_THREADS=4 cargo nextest run -p late-core proxy_protocol tunnel_protocol
NEXTEST_TEST_THREADS=4 cargo nextest run -p late-ssh tunnel::
```

Results:

- `late-bastion`: 27 passed.
- `late-core` proxy/tunnel protocol filters: 24 passed, 201 skipped.
- `late-ssh` tunnel filter: 14 passed, 1677 skipped.

After implementing the three fixes, these checks also passed:

```bash
cargo fmt --check
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/deploy.yml"); puts "deploy.yml parsed"'
actionlint .github/workflows/deploy.yml
NEXTEST_TEST_THREADS=4 cargo nextest run -p late-bastion
```

The post-fix `late-bastion` run passed all 27 tests. Earlier merge validation
also reached `cargo fmt`, `cargo check`, and the commit hook's rustfmt/clippy
gate. The full `late-ssh` nextest package run was blocked only by DB integration
tests requiring `TEST_DATABASE_URL`; the focused tunnel filter above passed
without the DB-dependent cases.

## Remaining operational check

Before production enablement, confirm `BASTION_TUNNEL_TRUSTED_CIDRS` matches the
cluster pod CIDR and that the ingress TCP path sends PROXY protocol only when
`LATE_BASTION_PROXY_PROTOCOL` is enabled.
