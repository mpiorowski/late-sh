# Plan: one Artboard session must not cost a tokio worker

Status: partly built. Bounded edits (`EditReach`, `late-ssh/src/app/artboard/CONTEXT.md`), the render input budget and the render pass metric (`late-ssh/CONTEXT.md`, `SCALE.md`) have shipped. What is left is below. As each step lands, move its behavior into `artboard/CONTEXT.md` and delete it here; delete the file when nothing is left.

## Goal

A scripted Artboard session (a bot typing rows of glyphs for hours) costs the fleet a few millicores, like any other session.

The trigger (prod, 2026-10-20): one Artboard script pinned a full tokio worker for hours while kube CPU stayed green, and the typing hitches other users felt matched wake-ups waiting on that worker.

## Step 2: no per-session copy of the board

Every op comes back as `OpBroadcast`, the client thread runs `send_modify` on the watch, and each session's `State::tick` (`state.rs`) clones the whole `DartboardSnapshot`: the canvas's two 42k-entry maps plus `ArtboardProvenance`, a third map with a `String` per cell. Roughly 10 ms per clone, coalesced per tick, so about 15% of a core per session on the page at the hot cadence.

Where: `late-ssh/src/app/artboard/svc.rs` (`DartboardSnapshot`, `handle_server_msg`), `state.rs` (the sites that touch `.provenance`), `ui.rs` through `owner_username` and the ownership overlay.

- Provenance is already process-shared. Remove the `provenance` field from `DartboardSnapshot`; `username_at` for the cursor, hover and the ownership overlay reads the shared map under its mutex (one lock and at most a viewport's worth of lookups per frame). The overlay cache stays; invalidate it on snapshot change as today.
- Wrap `canvas` in `Arc<Canvas>`; the client thread mutates through `Arc::make_mut` inside `send_modify`, so a session that is mid-draw holds the old pointer and the writer copies once, not every viewer every tick.
- Lock order today is watch write lock, then shared provenance mutex (inside `send_modify`). Keep that order everywhere; never take the watch lock while holding the mutex.
- Tests: `svc_test.rs` for the broadcast path (op applied once, provenance applied once), and a `state_internal_test.rs` case that a tick after an op does not reallocate the canvas (`Arc::ptr_eq` before and after when the op was a no-op, inequality when it changed).
- Doc: the snapshot shape in `artboard/CONTEXT.md` (Shared state list).

## Step 3: a per-user op rate limit

Where: the `SubmitOp` arm of `run_client_loop` in `late-ssh/src/app/artboard/svc.rs`, before `client.submit_op`.

- Token bucket per connection: refill 20 ops/s, burst 60. Over budget, do not submit; send `DartboardEvent::Reject { reason }` with a lowercase reason such as `"painting too fast, slow down"` so it surfaces as the existing private notice. No log per rejection; count it.
- Numbers are a starting point: a human cannot exceed 20 ops/s, a story script at 20 rows a second still reads as animation.
- Telemetry: `record_artboard_op(ArtboardOpOutcome::{Submitted, RateLimited})` in `late-ssh/src/metrics.rs`, closed enum, with the no-op twin. Dashboard: a small panel next to the existing Artboard gallery metrics.
- Tests: `svc_test.rs`, drive 100 ops in one instant, assert 60 submitted and 40 rejected with the notice, then one more after a 50 ms wait succeeds.
- Doc: `artboard/CONTEXT.md` gets one line under the server section.

**Acceptance:** with a painting script running, `kubectl top pod` for `service-ssh` reads about 25 millicores per session and no tokio worker thread sits above 0.2 cores. Per-thread sample, no tooling needed:

```
POD=$(kubectl -n default get pod -l app=service-ssh -o jsonpath='{.items[0].metadata.name}')
kubectl -n default exec $POD -- sh -c 'for t in /proc/1/task/*; do printf "%s %s %s\n" "${t##*/}" "$(cat $t/comm)" "$(awk "{print \$14+\$15}" $t/stat)"; done' > /tmp/t1; sleep 5
kubectl -n default exec $POD -- sh -c 'for t in /proc/1/task/*; do printf "%s %s %s\n" "${t##*/}" "$(cat $t/comm)" "$(awk "{print \$14+\$15}" $t/stat)"; done' > /tmp/t2
join <(sort /tmp/t1) <(sort /tmp/t2) | awk '{printf "%s %s %.2f cores\n", $1, $2, ($5-$3)/500}' | sort -k3 -rn | head
```

## Out of scope, reported separately

- A session whose client stops reading is never cut: `render_once` records a dropped frame when `handle.data` times out but never adds those bytes to `OutputBudget`, so the stall disconnect never fires. Retries now settle to the idle cadence (`retry_after_frame_drop`), which caps the cost but still keeps the session. Fix: count timed-out bytes against the budget, or disconnect after N consecutive drops.
- A paired CLI retrying the pair WebSocket every 3 s for 20 hours with a dead token (`ws pair rejected: no live session for token`, 279 in a day). Harmless; a backoff in `late-cli/src/ws.rs` would quiet it.
- A 706 KB/s output peak on one session is unexplained, and `late_ssh_frame_bytes` cannot say which screen did it. A `screen` label on it would.
