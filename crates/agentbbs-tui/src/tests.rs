use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn screen_text(app: &App, w: u16, h: u16) -> String {
    let backend = TestBackend::new(w, h);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    buffer
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<Vec<_>>()
        .join("")
}

#[test]
fn seeds_default_boards() {
    let app = App::in_memory();
    assert_eq!(app.boards.len(), 4);
    assert!(app.boards.iter().any(|b| b.slug == "general"));
}

#[test]
fn splash_renders_banner() {
    let app = App::in_memory();
    let text = screen_text(&app, 100, 30);
    assert!(text.contains("AgentBBS"));
    assert!(text.contains("CONNECT"));
}

#[test]
fn navigate_to_boards_and_back() {
    let mut app = App::in_memory();
    assert_eq!(app.screen, Screen::Splash);
    app.on_key(press(KeyCode::Enter)); // splash -> main
    assert_eq!(app.screen, Screen::Main);
    app.on_key(press(KeyCode::Char('m'))); // hotkey -> boards
    assert_eq!(app.screen, Screen::Boards);
    app.on_key(press(KeyCode::Esc)); // back to main
    assert_eq!(app.screen, Screen::Main);
}

#[test]
fn compose_and_post_flow() {
    let mut app = App::in_memory();
    let before = app.bbs.store().message_count().unwrap();

    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('M'))); // -> boards
    app.on_key(press(KeyCode::Enter)); // open first board -> read
    assert_eq!(app.screen, Screen::Read);
    app.on_key(press(KeyCode::Char('P'))); // -> compose
    assert_eq!(app.screen, Screen::Compose);

    for c in "Hello".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Tab)); // -> body
    for c in "first post".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s')); // send

    let after = app.bbs.store().message_count().unwrap();
    assert_eq!(after, before + 1);
    assert_eq!(app.screen, Screen::Read);
    assert!(app.messages.iter().any(|m| m.body.body == "first post"));
    // Posted message must verify.
    assert!(app.messages.last().unwrap().verify().is_ok());
}

#[test]
fn empty_message_is_rejected() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('P')));
    let before = app.bbs.store().message_count().unwrap();
    app.on_key(ctrl('s')); // send with empty body
    assert_eq!(app.bbs.store().message_count().unwrap(), before);
    assert!(app.status.contains("empty"));
}

#[test]
fn goodbye_quits() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('G'))); // -> goodbye screen
    assert_eq!(app.screen, Screen::Goodbye);
    let ctl = app.on_key(press(KeyCode::Enter));
    assert_eq!(ctl, Control::Quit);
    assert!(app.should_quit);
}

#[test]
fn arena_shows_leaderboard() {
    let mut app = App::in_memory();
    assert!(app.arena.submission_count() >= 3);
    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('A'))); // -> arena
    assert_eq!(app.screen, Screen::Arena);
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Arena Leaderboard"));
    assert!(text.contains("CVE-Bench"));
    app.on_key(press(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Main);
}

#[test]
fn sysop_panel_renders() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('S'))); // sysop panel
    let text = screen_text(&app, 100, 30);
    assert!(text.contains("Sysop Report"));
}

#[test]
fn shared_presence_sees_other_sessions() {
    use agentbbs_core::{MemoryStore, Presence};
    use std::sync::Arc;
    let presence = Arc::new(Presence::default());
    let store: Arc<dyn agentbbs_core::Store> = Arc::new(MemoryStore::new());
    let a = App::with_presence(store.clone(), presence.clone());
    let b = App::with_presence(store.clone(), presence.clone());
    let (aid, bid) = (a.session.identity.id(), b.session.identity.id());
    let online = presence.online(10);
    assert!(online.len() >= 2);
    assert!(online.iter().any(|m| m.id == aid));
    assert!(online.iter().any(|m| m.id == bid));
    // Dropping a session leaves the registry.
    drop(b);
    assert!(presence.online(10).iter().all(|m| m.id != bid));
    let _ = a; // keep a alive until here
}

#[test]
fn who_panel_renders_real_presence() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('W'))); // who's online
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Who's Online"));
    assert!(text.contains("(you)")); // our own session is listed
    assert!(text.contains("online"));
}

#[test]
fn marketplace_renders_signed_listings() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('K'))); // marketplace
    assert_eq!(app.screen, Screen::Market);
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Marketplace"));
    assert!(text.contains("Echo Door"));
}

#[test]
fn unread_badge_appears_when_another_session_posts_and_clears_on_open() {
    use agentbbs_core::{MemoryStore, Presence};
    use std::sync::Arc;
    let presence = Arc::new(Presence::default());
    let store: Arc<dyn agentbbs_core::Store> = Arc::new(MemoryStore::new());
    let mut a = App::with_presence(store.clone(), presence.clone());
    let mut b = App::with_presence(store.clone(), presence.clone());

    // `a` opens the first board (list order is alphabetical by slug,
    // not seed order) — this marks it seen at 0 messages.
    a.on_key(press(KeyCode::Enter));
    a.on_key(press(KeyCode::Char('M')));
    a.on_key(press(KeyCode::Enter));
    let slug = a.current_board.clone().unwrap();
    assert_eq!(a.unread_for(&slug), 0);

    // `b` posts to that same shared board.
    b.on_key(press(KeyCode::Enter));
    b.on_key(press(KeyCode::Char('M')));
    b.on_key(press(KeyCode::Enter));
    assert_eq!(b.current_board.as_deref(), Some(slug.as_str()));
    b.on_key(press(KeyCode::Char('P')));
    for c in "from b".chars() {
        b.on_key(press(KeyCode::Char(c)));
    }
    b.on_key(press(KeyCode::Tab));
    for c in "hello a".chars() {
        b.on_key(press(KeyCode::Char(c)));
    }
    b.on_key(ctrl('s'));

    // `a` hasn't re-opened the board — unread_for recomputes live against
    // the shared store, so it must reflect b's post without a refresh.
    assert_eq!(a.unread_for(&slug), 1);
    let boards_text = {
        a.screen = Screen::Boards;
        screen_text(&a, 110, 30)
    };
    assert!(boards_text.contains("1 new"));

    // Re-opening the board marks it seen again.
    a.board_index = 0;
    a.open_selected_board();
    assert_eq!(a.unread_for(&slug), 0);
}

#[test]
fn reply_threads_the_post_and_shows_indicator() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('P')));
    for c in "root".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Tab));
    for c in "the original message".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s'));
    let root_id = app.messages.last().unwrap().id.clone();

    // Reply to the highlighted (only) message.
    app.on_key(press(KeyCode::Char('r')));
    assert_eq!(app.screen, Screen::Compose);
    assert!(app.compose_subject.starts_with("Re: "));
    assert!(app.compose_reply_to.is_some());
    for c in "a threaded reply".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s'));

    assert!(app.compose_reply_to.is_none()); // cleared after send
    let reply = app.messages.last().unwrap();
    assert_eq!(reply.body.parent.as_ref(), Some(&root_id));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains('↳')); // thread indicator rendered
}

#[test]
fn quick_switch_jumps_boards_while_reading() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter)); // opens board_index 0
    let slugs: Vec<String> = app.boards.iter().map(|b| b.slug.clone()).collect();
    assert_eq!(app.current_board.as_deref(), Some(slugs[0].as_str()));

    app.on_key(press(KeyCode::Char(']'))); // next board
    assert_eq!(app.screen, Screen::Read); // stays in Read, no trip back to Boards
    assert_eq!(app.current_board.as_deref(), Some(slugs[1].as_str()));

    app.on_key(press(KeyCode::Char('['))); // back
    assert_eq!(app.current_board.as_deref(), Some(slugs[0].as_str()));

    app.on_key(press(KeyCode::Char('['))); // wraps to the last board
    assert_eq!(
        app.current_board.as_deref(),
        Some(slugs[slugs.len() - 1].as_str())
    );
}

#[test]
fn pods_screen_spawns_and_renders() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('P'))); // -> pods
    assert_eq!(app.screen, Screen::Pods);
    assert!(app.pods.is_empty());

    app.on_key(press(KeyCode::Char('n'))); // spawn a demo pod
    assert_eq!(app.pods.len(), 1);
    assert_eq!(app.pods[0].spec.template.domain, "ops");
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Pods"));
    assert!(text.contains("pod-0000"));
    assert!(text.contains("spawned"));
}

#[test]
fn hire_produces_a_pod_matching_the_web_adapters_defaults() {
    let mut app = App::in_memory();
    let p = app.hire("@Alice", "research").unwrap();
    assert_eq!(p.id, "pod-0000");
    assert_eq!(p.spec.template.template_ref, "research/hired-alice@1");
    assert_eq!(p.spec.template.registered_room, "research-ops");
    assert!((p.spec.template.per_agent_cap_usd - 0.25).abs() < 1e-9);
    assert_eq!(p.spec.template.max_tier, agentbbs_core::pod::MaxTier::Mid);
}

#[test]
fn approvals_propose_approve_and_reject_flow() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('V'))); // -> approvals
    assert_eq!(app.screen, Screen::Approvals);

    app.on_key(press(KeyCode::Char('n'))); // propose
    assert_eq!(app.proposals.len(), 1);
    assert!(!app.is_action_authorized(&app.proposals[0].action_id));

    app.on_key(press(KeyCode::Char('y'))); // approve
    assert!(app.is_action_authorized(&app.proposals[0].action_id));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("authorized"));
    assert!(text.contains("approve"));

    app.on_key(press(KeyCode::Char('r'))); // then reject — veto wins (fail-closed)
    assert!(!app.is_action_authorized(&app.proposals[0].action_id));
}

#[test]
fn budget_screen_shows_pod_spend_and_topup_raises_cap() {
    let mut app = App::in_memory();
    app.hire("bob", "ops").unwrap();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('B'))); // -> budget
    assert_eq!(app.screen, Screen::Budget);

    let before = app.budget_status(&app.pods[0].clone());
    assert!((before.cap - 0.25).abs() < 1e-9);

    app.on_key(press(KeyCode::Char('+'))); // top up
    let after = app.budget_status(&app.pods[0].clone());
    assert!((after.cap - 0.35).abs() < 1e-9);
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Budget Guardrails"));
}

#[test]
fn decisions_screen_shows_the_seeded_records() {
    let mut app = App::in_memory();
    assert_eq!(app.decisions.all().len(), 2);
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('C'))); // -> decisions
    assert_eq!(app.screen, Screen::Decisions);
    let text = screen_text(&app, 120, 30);
    assert!(text.contains("Decision Records"));
    assert!(text.contains("Adopt the meta-llm gateway"));
    assert!(text.contains("Human approval for spend"));
}
