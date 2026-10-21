# Parked: advisory-lock audio leader

**Status: parked.** The reconciliation contract (`late-ssh/src/app/audio/CONTEXT.md` §19) is the active fix and covers the realistic failure modes for a single-replica deployment. This is the next-step option *if* rolling-deploy WS overlap becomes user-visible OR we scale `service-ssh` past one replica.

## Idea

`AudioService` acquires a Postgres session-level advisory lock on a fixed key at startup. Only the lock-holder is the audio leader: it owns timer scheduling, queue mutation, and WS broadcasting. Followers (other replicas, the draining-out pod) keep serving SSH sessions but reject every audio-mutating call with a typed `NotLeader` error that surfaces as "audio is moving — reconnect" in the booth/sidebar.

```text
pod with lock     = audio leader, can mutate queue/timers, broadcasts ws events
pod without lock  = read-only follower; submit/skip/vote/advance return NotLeader
draining old pod  = releases lock + cancels timers in begin_drain()
new pod           = acquires lock + runs resume_from_db
old user sessions = stay connected but audio actions are rejected until they
                    reconnect to the new pod (k8s service routes new WS to leader)
```

## Sketch

- Pin a single pool connection for the lock. `pg_advisory_lock` is session-scoped; if the connection dies the lock releases, which is exactly the recovery behavior we want.
- Expose leader status as `watch::Receiver<bool>` so the sidebar/booth react to transitions, not just poll at action time. UI banner can react proactively rather than waiting for the user to press a key.
- A `LeaderGuard` zero-cost token returned by `acquire_for_mutation()`, required by every mutating method's signature. The compiler enforces the check instead of human discipline; trivial to miss otherwise given the surface (see below).
- `begin_drain()` releases the lock and cancels timers, letting the new pod take leadership before the old pod finishes draining its SSH sessions.

## Mutation surface that has to honor the gate

`submit_url`, `submit_trusted_url`, `submit_video`, `set_trusted_youtube_fallback`, `force_skip`, `cast_skip_vote`, `cast_vote`, `clear_vote`, `delete_queue_item`, `toggle_unskippable`, `report_player_state`, `finish_item`, plus all the `*_task` spawners that call them. A `LeaderGuard` parameter on the inner sync methods catches this at compile time.

## Why not yet

- The prod incident was single-pod state drift, not multi-pod contention. The reconciliation contract is the minimum viable fix; the lock is layered safety, not the bug fix.
- Big audit surface (above). Easy to miss one, and the failure mode of missing one is a hard-to-debug "this one path bypasses leader" inconsistency. Worth doing only when we know we need it.
- Leader-handover UX during rolling deploys needs design work (banner copy, reconnect timing, what the booth modal does when the lock moves mid-modal). Premature without the demand.

## Reactivation criteria

- The reconciliation contract is in place and stable.
- We see real WS-broadcast overlap symptoms (browsers double-loading items during rollouts) OR we want to scale `service-ssh` past one replica.
- We have a story for the "audio is moving" UX that's not just a banner the user sees mid-action.

Until then: the contract holds; one replica is the deploy.
