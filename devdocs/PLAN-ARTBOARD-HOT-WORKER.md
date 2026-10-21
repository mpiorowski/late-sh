# Plan: one Artboard session must not cost a tokio worker

Status: investigated, not implemented. As the work lands, move shipped behavior into `late-ssh/src/app/artboard/CONTEXT.md` (op limits, snapshot shape), `late-ssh/CONTEXT.md` (render loop input budget, pass metric) and `SCALE.md` Pain Point 11 (what the pass metric adds to the wire row), then delete this file. Another agent is editing docs at the moment: land code first, hand them the doc lines, or wait.

## Goal

A scripted Artboard session (a bot typing rows of glyphs for hours) costs the fleet a few millicores, like any other session, and a slow render pass anywhere is visible in Grafana within a minute instead of after a two-hour hunt.

## What was measured (2026-10-20, prod, one `service-ssh` pod)

- CPU per session sat at 22 to 31 millicores for two weeks. It doubled to 55 at 18:40, on the pod deployed two days earlier, so the `#724` deploy at 21:10 is not the cause. Draws per second did not change.
- Inside the pod, one of the 8 core tokio workers (TIDs 7 to 19; the other 10 `tokio-rt-worker` threads are the blocking pool) sat at a full core. The other 17 threads summed to 0.34 cores: 24 millicores per session, the old normal.
- The hot thread stayed the same TID across samples and, over 3 s, made zero voluntary context switches and zero read/write syscalls in state R. One task never yields.
- The hot worker started within 30 s of spoon connecting and entering the Artboard (18:39:34 old pod, 21:26:08 new pod) and stopped within 30 s of spoon's old sessions closing (21:29:41 to 21:30:01). spoon runs a script that writes and clears the board row by row (265 `artboard paint region` lines per hour; ops under 50 cells are not logged, so the real op rate is unknown). The only `dartboard-<user_id>` client thread on the pod is spoon's.
- Kube is green throughout: CPU limit is 8 cores, no throttling, node at 30%. That is why nothing alerted.
- Nagle is off on both ends (`nodelay: true` in `late-ssh/src/ssh.rs` and `late-cli/src/ssh.rs`); the chat composer does nothing per keystroke beyond rendering. The typing hitches people feel are consistent with wake-ups landing on the pinned worker's queue or LIFO slot and waiting out its poll.

## What the code says each Artboard op costs

The canvas (`dartboard_core::Canvas`) is two `HashMap`s of about 42k entries; the session's `DartboardSnapshot` (`late-ssh/src/app/artboard/svc.rs`) adds `ArtboardProvenance`, a third map with a `String` per cell.

- Viewer and writer alike: every op comes back as `OpBroadcast`, the client thread runs `send_modify` on the watch, and the session's `State::tick` (`state.rs`) clones the whole snapshot: three 42k-entry maps and 42k Strings, roughly 10 ms. Coalesced per tick, so about 15% of a core per session on the page at the hot cadence. Real waste, but not a full core on its own.
- A typed glyph (`paint_char`) is already O(1): `submit_single_op` with a `PaintCell`.
- Backspace, floating-block stamps and region edits go through `edit_canvas`: clone the canvas and the provenance, apply, compare the full canvas, then `diff_canvas_op` over the full canvas. Four full passes per edit.
- Provenance is applied twice per op: once to the session copy, once to the process-shared `SharedArtboardProvenance` under a std mutex.

None of these adds up to a pinned worker by arithmetic. Something in that session's poll loops or runs far longer than the tick. The profile in step 0 says which; steps 1 and 2 remove the waste we can see; steps 3 to 5 make the fleet safe whatever the profile says.

**Acceptance for the whole plan:** with spoon's script running, `kubectl top pod` for `service-ssh` reads about 25 millicores per session, no tokio worker thread above 0.2 cores (sample with the `/proc/1/task/*/stat` loop below), and your own typing in the lounge shows no hitches.

Per-thread sample, no tooling needed:

```
POD=$(kubectl -n default get pod -l app=service-ssh -o jsonpath='{.items[0].metadata.name}')
kubectl -n default exec $POD -- sh -c 'for t in /proc/1/task/*; do printf "%s %s %s\n" "${t##*/}" "$(cat $t/comm)" "$(awk "{print \$14+\$15}" $t/stat)"; done' > /tmp/t1; sleep 5
kubectl -n default exec $POD -- sh -c 'for t in /proc/1/task/*; do printf "%s %s %s\n" "${t##*/}" "$(cat $t/comm)" "$(awk "{print \$14+\$15}" $t/stat)"; done' > /tmp/t2
join <(sort /tmp/t1) <(sort /tmp/t2) | awk '{printf "%s %s %.2f cores\n", $1, $2, ($5-$3)/500}' | sort -k3 -rn | head
```

## Step 0: profile the hot worker first (15 seconds, prerequisite)

Do this before touching code, while the script is running. It installs nothing on the node; the package lands in a throwaway debug pod that is deleted at the end.

```
CID=$(kubectl -n default get pod -l app=service-ssh -o jsonpath='{.items[0].status.containerStatuses[0].containerID}' | sed 's#containerd://##' | cut -c1-12)
kubectl debug node/server-1 -it --image=debian:trixie-slim --profile=sysadmin -- bash -c "
apt-get update -qq && apt-get install -y -qq linux-perf >/dev/null
PID=\$(for p in \$(pgrep -x late-ssh); do grep -q $CID /proc/\$p/cgroup && echo \$p; done)
perf record -F 249 -g -p \$PID -o /tmp/p.data -- sleep 15 >/dev/null 2>&1
perf report -i /tmp/p.data --stdio --no-children --symfs=/proc/\$PID/root --sort tid,sym | head -120"
kubectl -n default get pods | grep node-debugger   # then delete that pod
```

The release profile keeps symbols (`strip = "debuginfo"` in `Cargo.toml`), so Rust frames resolve. Read the hottest `tid`'s top symbols. Expected candidates: `State::tick` / snapshot clone (step 2), `diff_canvas_op` or `edit_canvas` (step 1), `handle_input` draining a key flood (step 4), or something not on this list, which changes the order below.

## Step 1: edits cost O(edit), not O(canvas)

Where: `late-ssh/src/app/artboard/state.rs`, the 7 `edit_canvas` callers and `diff_canvas_op`.

- The editor crate mutates the canvas in 122 places and is third party (`dartboard-*` 0.1.4, mevanlc's repo). Do not fork it to record writes.
- Instead, every `edit_canvas` caller knows the area it can touch: the cursor row for backspace, the floating rect for a stamp, the selection bounds for region edits. Pass that `Bounds` in, snapshot only those cells (value, colour, provenance) before the edit, and diff only inside it. Wide glyphs can spill one cell past the bounds; extend by one column, and keep the existing wide-insert test green.
- Provenance: drop the full `before_provenance` clone; `apply_op` only needs the before canvas for shift and replace ops, which already clone the canvas on the broadcast path.
- Tests: `state_internal_test.rs` already pins `diff_canvas_op` on wide inserts and floating moves. Add one per edit kind asserting the op produced from the bounded diff equals the op from the full diff, and one that a 384x192 backspace touches at most one row.

## Step 2: no per-session copy of the board

Where: `late-ssh/src/app/artboard/svc.rs` (`DartboardSnapshot`, `handle_server_msg`), `state.rs` (16 sites touch `.provenance`), `ui.rs` through `owner_username` and the ownership overlay.

- Provenance is already process-shared. Remove the `provenance` field from `DartboardSnapshot`; `username_at` for the cursor, hover and the ownership overlay reads the shared map under its mutex (one lock and at most a viewport's worth of lookups per frame). The overlay cache stays; invalidate it on snapshot change as today.
- Wrap `canvas` in `Arc<Canvas>`; the client thread mutates through `Arc::make_mut` inside `send_modify`, so a session that is mid-draw holds the old pointer and the writer copies once, not every viewer every tick.
- Lock order today is watch write lock, then shared provenance mutex (inside `send_modify`). Keep that order everywhere; never take the watch lock while holding the mutex.
- Tests: `svc_test.rs` for the broadcast path (op applied once, provenance applied once), and a `state_internal_test.rs` case that a tick after an op does not reallocate the canvas (compare `Arc::ptr_eq` before and after when the op was a no-op, and inequality when it changed).
- Doc: the snapshot shape in `artboard/CONTEXT.md` (Shared state list).

## Step 3: a per-user op rate limit

Where: the `SubmitOp` arm of `run_client_loop` in `late-ssh/src/app/artboard/svc.rs`, before `client.submit_op`.

- Token bucket per connection: refill 20 ops/s, burst 60. Over budget, do not submit; send `DartboardEvent::Reject { reason }` with a lowercase reason such as `"painting too fast, slow down"` so it surfaces as the existing private notice. No log per rejection; count it.
- Numbers are a starting point: a human cannot exceed 20 ops/s, a story script at 20 rows a second still reads as animation.
- Telemetry: `record_artboard_op(ArtboardOpOutcome::{Submitted, RateLimited})` in `late-ssh/src/metrics.rs`, closed enum, with the no-op twin. Dashboard: a small panel next to the existing Artboard gallery metrics.
- Tests: `svc_test.rs`, drive 100 ops in one instant, assert 60 submitted and 40 rejected with the notice, then one more after a 50 ms wait succeeds.
- Doc: `artboard/CONTEXT.md` gets one line under the server section.

## Step 4: a render pass cannot own a worker

Where: `render_once` in `late-ssh/src/ssh.rs`, the `while let Ok(data) = input_rx.try_recv()` drain.

- Budget the drain: stop draining after about 4 ms of `handle_input` wall time or 64 messages, whichever first, render what was applied, set `dirty` so the next pass picks up the rest at once. A key flood then costs that session a frame of lag, never the worker.
- This is the one change outside the Artboard domain; it is root glue and stays three lines plus the constant.
- Telemetry: the pass already counts drops; add `late_ssh_render_input_budget_hits_total` (counter, no labels) so a session hitting the budget is visible.
- Tests: `ssh_internal_test.rs` has the render action tests; add one feeding 200 queued inputs and asserting the pass renders after the budget with the remainder still queued and `dirty` set.
- Doc: `late-ssh/CONTEXT.md` render loop section, one sentence.

## Step 5: see slow passes before users do

Where: `render_once` in `late-ssh/src/ssh.rs` (timing), `late-ssh/src/metrics.rs` (metric), `monitoring/dashboards/observability.json` (panel in the Wire & Latency row).

- `late_ssh_render_pass_seconds{reason,screen}` histogram around the locked section of `render_once` (input drain, tick, render). `screen` from a closed enum mapped from `App::screen()` (chat, artboard, hub, door, ...), never a free string. Boundaries from 1 ms to 1 s.
- One `warn!` per session per minute at most when a pass exceeds the hot tick (66 ms), with `username`, `screen` and the pass time, so `_msg:"slow render pass"` in VictoriaLogs names the user. Rate-limit it in the render loop state, not in the log call.
- This is the metric this incident lacked: Grafana showed green CPU because the fleet average hides one pinned core. Pair it with a dashboard alert: p99 pass time over 66 ms for 5 minutes.
- Doc: `SCALE.md` Pain Point 11 "What is measured" gets the pass histogram; the Observability bullet in the 1000-user section already lists the wire row.

## Landing order

1. Step 0, profile, 15 minutes including the debug pod.
2. Steps 3, 4, 5 together: small, contained, protect the fleet tonight, and 5 proves whatever 1 and 2 later change.
3. Step 2, then step 1, in the order the profile ranks them. If the profile names something not on this list, add it as step 6 above both.

## Out of scope, reported separately

- A session whose client stops reading is never cut: `render_once` records a dropped frame when `handle.data` times out but never adds those bytes to `OutputBudget`, so the stall disconnect in the render loop never fires. The draining pod ran one such session at 655 drops per minute for two hours. Fix: count timed-out bytes against the budget, or disconnect after N consecutive drops.
- A paired CLI retrying the pair WebSocket every 3 s for 20 hours with a dead token (`ws pair rejected: no live session for token`, 279 in a day). Harmless; a backoff in `late-cli/src/ws.rs` would quiet it.
- The lounge preview and chat images are not involved; the 706 KB/s output peak seen on one session is unexplained and the summary line cannot say which screen did it. A `screen` label on `late_ssh_frame_bytes` would.
