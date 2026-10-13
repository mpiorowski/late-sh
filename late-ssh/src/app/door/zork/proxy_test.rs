use super::identity::derive_client_key;
use super::protocol::*;
use super::state::{Mode, State, StateConfig};
use anyhow::Result;
use russh::keys::PublicKey;
use russh::server::{Auth, Handler, Msg, Server as _, Session};
use russh::{Channel, ChannelId, Sig};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;

#[derive(Clone)]
struct Stub {
    requests: Arc<Mutex<Vec<String>>>,
    busy: bool,
}
struct StubHandler {
    stub: Stub,
    game: String,
}
impl russh::server::Server for Stub {
    type Handler = StubHandler;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> StubHandler {
        StubHandler {
            stub: self.clone(),
            game: String::new(),
        }
    }
}
impl Handler for StubHandler {
    type Error = anyhow::Error;
    async fn auth_publickey(&mut self, user: &str, _: &PublicKey) -> Result<Auth> {
        assert_eq!(user, "late_000000000000000000000001");
        Ok(Auth::Accept)
    }
    async fn channel_open_session(&mut self, _: Channel<Msg>, _: &mut Session) -> Result<bool> {
        Ok(true)
    }
    async fn pty_request(
        &mut self,
        id: ChannelId,
        _: &str,
        _: u32,
        _: u32,
        _: u32,
        _: u32,
        _: &[(russh::Pty, u32)],
        s: &mut Session,
    ) -> Result<()> {
        s.channel_success(id)?;
        Ok(())
    }
    async fn exec_request(&mut self, id: ChannelId, command: &[u8], s: &mut Session) -> Result<()> {
        let command = std::str::from_utf8(command)?.to_string();
        self.stub.requests.lock().unwrap().push(command.clone());
        s.channel_success(id)?;
        if command == "list" {
            let catalogue = Catalogue {
                editions: Edition::ALL
                    .into_iter()
                    .map(|edition| EditionSlots {
                        edition,
                        autosave: Slot {
                            status: Availability::Ready,
                            saved_at: Some(100),
                            description: String::new(),
                        },
                        manual: Slot {
                            status: Availability::Ready,
                            saved_at: Some(99),
                            description: "before the maze".into(),
                        },
                    })
                    .collect(),
                active: None,
            };
            s.data(id, serde_json::to_vec(&catalogue)?)?;
            s.exit_status_request(id, 0)?;
            s.eof(id)?;
            s.close(id)?;
        } else if self.stub.busy {
            s.extended_data(
                id,
                1,
                b"Zork is already active for this account.\n".to_vec(),
            )?;
            s.exit_status_request(id, 75)?;
            s.eof(id)?;
            s.close(id)?;
        } else {
            self.game = command;
            s.data(id, format!("\x1b[2J\x1b[HZORK {}", self.game).into_bytes())?;
        }
        Ok(())
    }
    async fn data(&mut self, id: ChannelId, bytes: &[u8], s: &mut Session) -> Result<()> {
        s.data(id, bytes.to_vec())?;
        Ok(())
    }
    async fn signal(&mut self, id: ChannelId, signal: Sig, s: &mut Session) -> Result<()> {
        assert!(matches!(signal, Sig::USR1));
        self.stub
            .requests
            .lock()
            .unwrap()
            .push(format!("saved {}", self.game));
        s.exit_status_request(id, 20)?;
        s.eof(id)?;
        s.close(id)?;
        Ok(())
    }
}
async fn fixture(busy: bool) -> (u16, Arc<Mutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let mut server = Stub {
        requests: requests.clone(),
        busy,
    };
    let config = Arc::new(russh::server::Config {
        keys: vec![derive_client_key("host-key")],
        auth_rejection_time: Duration::ZERO,
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let handler = server.new_client(None);
            let config = config.clone();
            tokio::spawn(async move {
                if let Ok(session) = russh::server::run_stream(config, stream, handler).await {
                    let _ = session.await;
                }
            });
        }
    });
    (port, requests, task)
}
fn state(port: u16) -> State {
    State::new(StateConfig {
        user_id: uuid::Uuid::from_u128(1),
        host: "127.0.0.1".into(),
        port,
        secret: "test".into(),
        term: "xterm-256color".into(),
        enabled: true,
        repaint: None,
    })
}
async fn wait(state: &mut State, predicate: impl Fn(&State) -> bool) {
    timeout(Duration::from_secs(3), async {
        loop {
            state.tick();
            if predicate(state) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn menus_switch_only_after_checkpoint_exit_and_keep_one_live_destination() {
    let (port, requests, task) = fixture(false).await;
    let mut state = state(port);
    state.open_menu();
    wait(&mut state, |s| !s.loading()).await;
    state.menu_key(b'\r');
    assert!(state.available(Action::Auto));
    state.menu_key(b'\r');
    wait(&mut state, |s| {
        s.proxy()
            .is_some_and(|p| p.with_screen(|screen| screen.contents().contains("ZORK")))
    })
    .await;
    state.forward_input(b"look\r\x13");
    state.open_menu();
    state.step(true);
    state.menu_key(b'\r');
    state.menu_key(b'\r');
    wait(&mut state, |s| {
        s.live() == Some(Edition::Zork2)
            && s.proxy()
                .is_some_and(|p| p.with_screen(|screen| screen.contents().contains("ZORK")))
    })
    .await;
    assert_eq!(state.mode(), Mode::Running);
    let commands = requests.lock().unwrap().clone();
    let saved = commands
        .iter()
        .position(|c| c == "saved play zork1 auto")
        .unwrap();
    let launched = commands
        .iter()
        .position(|c| c == "play zork2 auto")
        .unwrap();
    assert!(saved < launched, "{commands:?}");
    drop(state);
    task.abort();
}
#[tokio::test]
async fn backtick_detaches_and_resumes_the_same_game_through_the_workspace() {
    use crate::app::common::primitives::Screen;

    let (port, requests, task) = fixture(false).await;
    let mut state = state(port);
    state.open_menu();
    wait(&mut state, |s| !s.loading()).await;
    state.menu_key(b'\r');
    state.menu_key(b'\r');
    wait(&mut state, |s| {
        s.proxy()
            .is_some_and(|p| p.with_screen(|screen| screen.contents().contains("ZORK")))
    })
    .await;
    let db = late_core::db::Db::new(&late_core::db::DbConfig {
        port: 1,
        ..Default::default()
    })
    .unwrap();
    let mut app = crate::test_helpers::make_app(db, uuid::Uuid::from_u128(1), "zork-workspace");
    app.zork_state = Some(state);
    app.set_screen(Screen::Zork);
    app.handle_input(b"\x13look\r");
    assert_eq!(app.screen, Screen::Zork);
    wait(app.zork_state.as_mut().unwrap(), |s| {
        s.proxy()
            .is_some_and(|p| p.with_screen(|screen| screen.contents().contains("look")))
    })
    .await;

    for _ in 0..2 {
        app.handle_input(b"`");
        assert_eq!(app.screen, Screen::Dashboard);
        assert!(app.zork_state.as_ref().unwrap().is_running());
        app.handle_input(b"`");
        assert_eq!(app.screen, Screen::Zork);
        assert!(app.zork_state.as_ref().unwrap().game_visible());
    }
    assert_eq!(
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|command| command.starts_with("play "))
            .count(),
        1
    );
    drop(app);
    task.abort();
}

#[tokio::test]
async fn busy_host_is_visible_in_menu_and_does_not_look_like_a_successful_game() {
    let (port, _, task) = fixture(true).await;
    let mut state = state(port);
    state.open_menu();
    wait(&mut state, |s| !s.loading()).await;
    state.menu_key(b'\r');
    state.menu_key(b'\r');
    wait(&mut state, |s| !s.is_running()).await;
    assert_eq!(state.mode(), Mode::Actions);
    assert!(state.message().contains("already active"));
    drop(state);
    task.abort();
}

/// Run against the built runtime-zork container with scripts/test_zork_host.sh.
#[tokio::test]
#[ignore = "requires the separately built Frotz host"]
async fn real_frotz_trilogy_roundtrip() {
    let port = std::env::var("LATE_ZORK_TEST_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let mut state = State::new(StateConfig {
        user_id: uuid::Uuid::from_u128(0x1234),
        host: "127.0.0.1".into(),
        port,
        secret: "zork-integration-test".into(),
        term: "xterm-256color".into(),
        enabled: true,
        repaint: None,
    });
    state.set_viewport(ratatui::layout::Rect::new(0, 0, 100, 30));
    let mut screens = Vec::new();
    for (index, edition) in Edition::ALL.into_iter().enumerate() {
        state.open_menu();
        wait(&mut state, |s| !s.loading()).await;
        while state.selected() != index {
            state.step(true);
        }
        state.menu_key(b'\r');
        state.step(true);
        state.step(true);
        state.menu_key(b'\r');
        state.menu_key(b'y');
        wait(&mut state, |s| {
            s.live() == Some(edition)
                && s.proxy().is_some_and(|p| {
                    p.with_screen(|screen| screen.contents().trim_end().ends_with('>'))
                })
        })
        .await;
        state.forward_input(b"look\r");
        wait(&mut state, |s| {
            s.proxy()
                .unwrap()
                .with_screen(|screen| screen.contents().contains("Moves: 1"))
        })
        .await;
        state.forward_input(b"save\r");
        wait(&mut state, |s| {
            s.proxy()
                .unwrap()
                .with_screen(|screen| screen.contents().contains("One manual slot"))
        })
        .await;
        state.forward_input(b"../cosmetic fallback\r");
        wait(&mut state, |s| {
            s.proxy()
                .unwrap()
                .with_screen(|screen| screen.contents().trim_end().ends_with('>'))
        })
        .await;
        screens.push(
            state
                .proxy()
                .unwrap()
                .with_screen(|screen| screen.contents()),
        );
    }
    for (index, edition) in Edition::ALL.into_iter().enumerate() {
        state.open_menu();
        wait(&mut state, |s| !s.loading()).await;
        {
            let slots = state.slots(edition).expect("edition metadata");
            assert_eq!(slots.autosave.status, Availability::Ready);
            assert_eq!(slots.manual.status, Availability::Ready);
            assert_eq!(slots.manual.description, "../cosmetic fallback");
        }
        while state.selected() != index {
            state.step(true);
        }
        state.menu_key(b'\r');
        state.menu_key(b'\r');
        wait(&mut state, |s| {
            s.live() == Some(edition)
                && s.proxy().is_some_and(|p| {
                    p.with_screen(|screen| screen.contents().trim_end().ends_with('>'))
                })
        })
        .await;
        assert_eq!(
            state
                .proxy()
                .unwrap()
                .with_screen(|screen| screen.contents()),
            screens[index]
        );
    }
    // RESTORE's default cancellation leaves the pending READ intact.
    state.forward_input(b"restore ignored\r");
    wait(&mut state, |s| {
        s.proxy()
            .unwrap()
            .with_screen(|screen| screen.contents().contains("Return to this Zork's menu?"))
    })
    .await;
    state.forward_input(b"n");
    wait(&mut state, |s| {
        s.proxy()
            .unwrap()
            .with_screen(|screen| !screen.contents().contains("Return to this Zork's menu?"))
    })
    .await;
    assert_eq!(
        state
            .proxy()
            .unwrap()
            .with_screen(|screen| screen.contents()),
        screens[2]
    );
    state.forward_input(b"restore\r");
    wait(&mut state, |s| {
        s.proxy()
            .unwrap()
            .with_screen(|screen| screen.contents().contains("Return to this Zork's menu?"))
    })
    .await;
    state.forward_input(b"y");
    wait(&mut state, |s| !s.is_running()).await;
    assert_eq!(state.mode(), Mode::Actions);
}

/// The script runs this after creating six slots, against the same named volume.
#[tokio::test]
#[ignore = "requires the separately built Frotz host and Docker"]
async fn real_frotz_host_restart_preserves_six_slots() {
    let port = std::env::var("LATE_ZORK_TEST_PORT")
        .unwrap()
        .parse()
        .unwrap();
    let container = std::env::var("LATE_ZORK_TEST_CONTAINER").unwrap();
    let config = |port| StateConfig {
        user_id: uuid::Uuid::from_u128(0x1234),
        host: "127.0.0.1".into(),
        port,
        secret: "zork-integration-test".into(),
        term: "xterm-256color".into(),
        enabled: true,
        repaint: None,
    };
    let mut state = State::new(config(port));
    state.set_viewport(ratatui::layout::Rect::new(0, 0, 100, 30));
    state.open_menu();
    wait(&mut state, |s| !s.loading()).await;
    state.menu_key(b'\r');
    state.menu_key(b'\r');
    wait(&mut state, |s| {
        s.proxy()
            .is_some_and(|p| p.with_screen(|screen| screen.contents().trim_end().ends_with('>')))
    })
    .await;
    let before = state
        .proxy()
        .unwrap()
        .with_screen(|screen| screen.contents());
    // Stop the actual host with a live READ pending. Its SIGTERM path must
    // hang up/reap the child within Docker's grace and release the old lease.
    assert!(
        tokio::process::Command::new("docker")
            .args(["restart", "--time", "15", &container])
            .stdout(std::process::Stdio::null())
            .status()
            .await
            .unwrap()
            .success()
    );
    wait(&mut state, |s| !s.is_running()).await;
    drop(state);
    // Docker can reassign an ephemeral published port across restart, and
    // `restart` returns before the listener has necessarily bound.
    let published = tokio::process::Command::new("docker")
        .args(["port", &container, "2331/tcp"])
        .output()
        .await
        .unwrap();
    assert!(published.status.success());
    let port = String::from_utf8(published.stdout)
        .unwrap()
        .trim()
        .rsplit(':')
        .next()
        .unwrap()
        .parse::<u16>()
        .unwrap();
    timeout(Duration::from_secs(5), async {
        // A Docker port forward can accept TCP before its new SSH backend is
        // ready. Require a successful authenticated request, not just TCP.
        loop {
            let result = super::proxy::catalogue(super::proxy::Connection {
                host: "127.0.0.1".into(),
                port,
                secret: "zork-integration-test".into(),
                user_id: uuid::Uuid::from_u128(0x1234),
                repaint: None,
            })
            .await;
            if result.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("restarted host SSH catalogue");
    let mut resumed = State::new(config(port));
    resumed.set_viewport(ratatui::layout::Rect::new(0, 0, 100, 30));
    resumed.open_menu();
    wait(&mut resumed, |s| !s.loading()).await;
    for edition in Edition::ALL {
        let slots = resumed.slots(edition).expect("persistent edition metadata");
        assert_eq!(slots.autosave.status, Availability::Ready);
        assert_eq!(slots.manual.status, Availability::Ready);
        assert_eq!(slots.manual.description, "../cosmetic fallback");
    }
    resumed.menu_key(b'\r');
    resumed.menu_key(b'\r');
    wait(&mut resumed, |s| {
        s.proxy()
            .is_some_and(|p| p.with_screen(|screen| screen.contents().trim_end().ends_with('>')))
    })
    .await;
    assert_eq!(
        resumed
            .proxy()
            .unwrap()
            .with_screen(|screen| screen.contents()),
        before
    );
    resumed.forward_input(b"north\r");
    wait(&mut resumed, |s| {
        s.proxy()
            .unwrap()
            .with_screen(|screen| screen.contents().contains("Moves: 2"))
    })
    .await;
}
