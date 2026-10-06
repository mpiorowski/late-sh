// Spectating live door games: a read-only view of another player's roguelike,
// streamed as screen diffs from the door host's own mirror of it (the host
// side is `late-dcss/src/watch.rs`). `svc` follows each host's roster of live
// games for every session and resolves the watch-chat rooms; `state` and
// `proxy` hold one session's watch; `ui` draws it inside the Games hub;
// `chat` ties a session to a player's watch-chat room (the watchers' pane,
// the player's one line). See `door/dcss/CONTEXT.md` §1.
pub mod chat;
pub mod input;
pub mod proxy;
pub mod state;
pub mod svc;
pub mod ui;
