// Spectating live door games (DCSS, NetHack, Brogue): a read-only view of
// another player's roguelike, streamed as screen diffs from the door host's
// own mirror of it (the host side is `src/watch.rs`, one copy in each of
// `late-dcss`, `late-nethack` and `late-brogue`). `svc` follows each host's
// roster of live games for every session and resolves the watch-chat rooms;
// `state` and `proxy` hold one session's watch; `ui` draws it, as a preview
// beside the Games hub's rail (whose live rows are how a watch is picked) or
// opened across the page with its chat; `chat` ties a session to a player's
// watch-chat room (the watchers' pane, the player's read-only one); `live`
// puts a game that just started on the live strip (`app/live`), drawn in its
// own door's ASCII. See `door/dcss/CONTEXT.md` §1.
pub mod chat;
pub mod input;
pub mod live;
pub mod proxy;
pub mod state;
pub mod svc;
pub mod ui;
