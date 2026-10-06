// The door log pipe (contract in `app/leaderboard/CONTEXT.md`): late-ssh tails the
// roguelike door hosts' append-only xlog files over a reserved `late_stats`
// SSH session and lands runs/milestones in the DB, which feeds the door
// boards, badge payouts, and #lounge feed events. One connect-with-retry task
// per enabled door, spawned from main.rs; the hosts stay stateless (no
// cursors, no parsing, no DB), so parser fixes never need a door redeploy.
pub mod award;
pub mod brogue;
#[cfg(test)]
mod brogue_test;
pub mod dcss;
#[cfg(test)]
mod dcss_test;
pub mod nethack;
#[cfg(test)]
mod nethack_test;
pub mod stream;
#[cfg(test)]
mod stream_test;
pub mod svc;
#[cfg(test)]
mod svc_test;
