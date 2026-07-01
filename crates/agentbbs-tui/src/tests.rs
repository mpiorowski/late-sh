use super::*;
use agentbbs_core::caps::Caps;
use agentbbs_core::{Message, MessageBody};
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

#[test]
fn directory_ranks_seeded_agents_by_wilson_score() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('H'))); // -> directory
    assert_eq!(app.screen, Screen::Directory);

    let ranking = app.reputation.ranking();
    assert_eq!(ranking.len(), 3);
    // script-kiddie (2/8, 25%) is unambiguously worst regardless of Wilson
    // bound specifics — must rank last.
    let last_handle = app
        .directory_handle(&agentbbs_core::identity::AgentId::from_hex(&ranking[2].agent).unwrap());
    assert_eq!(last_handle, "script-kiddie");
    let text = screen_text(&app, 120, 30);
    assert!(text.contains("Agent Directory"));
    assert!(text.contains("@graybeard"));
}

#[test]
fn hire_selected_spawns_a_pod_for_the_highlighted_agent() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('H')));
    app.on_key(press(KeyCode::Char('n'))); // hire highlighted
    assert_eq!(app.pods.len(), 1);
    assert!(app.status.starts_with("Hired"));
}

#[test]
fn issue_credential_signs_and_stores_a_claim_for_the_highlighted_agent() {
    let mut app = App::in_memory();
    let ranking = app.reputation.ranking();
    let subject = agentbbs_core::identity::AgentId::from_hex(&ranking[0].agent).unwrap();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('H')));
    app.on_key(press(KeyCode::Char('i'))); // issue skill:rust

    let valid = app.credentials.valid_for(&subject, chrono::Utc::now());
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].claim, "skill:rust");
    assert!(valid[0].verify().is_ok());
    let text = screen_text(&app, 120, 30);
    assert!(text.contains("skill:rust"));
}

#[test]
fn playbook_run_parks_at_the_gate_then_completes_on_approval() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('L'))); // -> playbooks
    assert_eq!(app.screen, Screen::Playbooks);
    assert!(app.run.is_none());

    app.on_key(press(KeyCode::Char('r'))); // start + drive to the gate
    assert_eq!(
        app.run.as_ref().unwrap().status(),
        agentbbs_core::playbook::RunStatus::AwaitingApproval
    );
    let text = screen_text(&app, 120, 30);
    assert!(text.contains("Awaiting approval"));

    let decisions_before = app.decisions.all().len();
    app.on_key(press(KeyCode::Char('y'))); // approve the gate + advance
    assert_eq!(
        app.run.as_ref().unwrap().status(),
        agentbbs_core::playbook::RunStatus::Completed
    );
    // Completion emits a signed DecisionRecord (ADR-0041 x ADR-0045).
    assert_eq!(app.decisions.all().len(), decisions_before + 1);
    let text = screen_text(&app, 120, 30);
    assert!(text.contains("Completed"));
}

#[test]
fn digest_tallies_general_and_posts_a_signed_summary() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter)); // open first board
                                       // Post one message so the digest has something to count. This may or may
                                       // not land on "general" depending on alphabetical board order — post
                                       // directly to general via Bbs to keep the test independent of that.
    let body = MessageBody {
        board: "general".into(),
        parent: None,
        subject: "hi".into(),
        body: "hello".into(),
        author: app.session.identity.id(),
        handle: app.session.handle.clone(),
        created_at: chrono::Utc::now(),
    };
    let msg = Message::sign(&app.session.identity, body).unwrap();
    app.bbs.post(app.session.caps, msg).unwrap();

    app.screen = Screen::Main;
    app.on_key(press(KeyCode::Char('I'))); // -> digest
    assert_eq!(app.screen, Screen::Digest);
    let (count, participants) = app.digest_stats();
    assert_eq!(count, 1);
    assert_eq!(participants, 1);

    let before = app.bbs.store().message_count().unwrap();
    app.on_key(press(KeyCode::Char('p'))); // post the digest
    let after = app.bbs.store().message_count().unwrap();
    assert_eq!(after, before + 1);
    let posted = app
        .bbs
        .read_board(Caps::READ, "general", 10)
        .unwrap()
        .into_iter()
        .find(|m| m.body.handle == "digest");
    assert!(posted.is_some());
    assert!(posted.unwrap().verify().is_ok());
}

#[test]
fn dm_opens_a_hidden_board_and_reuses_the_read_compose_pipeline() {
    let mut app = App::in_memory();
    let before = app.boards.len();
    app.open_dm("graybeard");
    assert_eq!(app.current_board.as_deref(), Some("dm:graybeard"));
    assert_eq!(app.screen, Screen::Read);
    assert_eq!(app.boards.len(), before + 1);

    // Posting into a DM reuses the exact same signed-post pipeline as any
    // other board.
    app.on_key(press(KeyCode::Char('P')));
    for c in "hey".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Tab));
    for c in "want to pair on the lead triage playbook?".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s'));
    assert!(app
        .messages
        .iter()
        .any(|m| m.body.body.starts_with("want to pair")));

    // Opening the same peer again reuses the board rather than duplicating it.
    app.open_dm("@GrayBeard"); // case/@ -insensitive, same peer
    assert_eq!(app.boards.len(), before + 1);
}

#[test]
fn dm_peers_lists_directory_agents() {
    let app = App::in_memory();
    let peers = app.dm_peers();
    assert!(peers.contains(&"graybeard".to_string()));
    assert!(peers.contains(&"night-owl".to_string()));
    assert!(peers.contains(&"script-kiddie".to_string()));
}

#[test]
fn rotate_identity_preserves_reputation_continuity() {
    let mut app = App::in_memory();
    let old_id = app.session.identity.id();
    // Give the old identity some reputation to carry over.
    app.reputation
        .record(agentbbs_core::reputation::OutcomeRecord {
            agent: old_id,
            success: true,
            weight: 1.0,
            source: "test".into(),
        });

    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('X'))); // -> passport
    assert_eq!(app.screen, Screen::Passport);
    app.on_key(press(KeyCode::Char('r'))); // rotate

    let new_id = app.session.identity.id();
    assert_ne!(old_id, new_id);
    assert_eq!(app.rotated_from, Some(old_id));
    // The rotation link resolves the old key to the new one.
    assert_eq!(app.rotation.resolve(&old_id), new_id);
    // Reputation recorded under the old key is reachable via score_via.
    let carried = app.reputation.score_via(&new_id, &app.rotation);
    assert!(carried.total > 0.0);

    let text = screen_text(&app, 110, 30);
    assert!(text.contains(&new_id.to_hex()));
    assert!(text.contains("Rotated from"));
}

#[test]
fn marketplace_install_debits_credits_and_is_idempotent() {
    let mut app = App::in_memory();
    assert_eq!(app.credits, 100);
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('K'))); // marketplace
                                           // graybeard (agent listing) is the second seeded item, price 25.
    app.market_index = 1;
    assert_eq!(app.market.all()[1].body.sku, "graybeard");

    app.on_key(press(KeyCode::Char('n'))); // install
    assert_eq!(app.credits, 75);
    assert!(app.installed.contains("graybeard"));

    app.on_key(press(KeyCode::Char('n'))); // installing again doesn't double-charge
    assert_eq!(app.credits, 75);
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("owned"));
    assert!(text.contains("75 credits"));
}

#[test]
fn creator_mode_toggle_gates_sysop_screen() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('S'))); // sysop
    assert!(screen_text(&app, 110, 30).contains("Read-only view"));

    app.screen = Screen::Main;
    app.on_key(press(KeyCode::Char('X'))); // passport
    app.on_key(press(KeyCode::Char('c'))); // toggle creator mode
    assert!(app.session.caps.contains(Caps::SYSOP));

    app.screen = Screen::Main;
    app.on_key(press(KeyCode::Char('S'))); // sysop again
    assert!(!screen_text(&app, 110, 30).contains("Read-only view"));
}

#[test]
fn sysop_mute_blocks_posting_and_lift_restores_it() {
    let mut app = App::in_memory();
    let target_id = app.session.identity.id();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('X'))); // passport
    app.on_key(press(KeyCode::Char('c'))); // enable creator mode

    // Point Directory's ranking at the session's own identity so the sysop
    // action targets something whose posting behavior we can observe.
    app.reputation
        .record(agentbbs_core::reputation::OutcomeRecord {
            agent: target_id,
            success: true,
            weight: 1.0,
            source: "test".into(),
        });
    let ranking = app.reputation.ranking();
    app.directory_index = ranking
        .iter()
        .position(|r| r.agent == target_id.to_hex())
        .unwrap();

    app.screen = Screen::Main;
    app.on_key(press(KeyCode::Char('S'))); // sysop
    app.on_key(press(KeyCode::Char('m'))); // mute the target (self)
    assert!(!app.moderation.can_post(&target_id, chrono::Utc::now()));

    // Posting must now actually fail — moderation is enforced at the post
    // path, not just displayed.
    app.screen = Screen::Main;
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    let before = app.bbs.store().message_count().unwrap();
    app.on_key(press(KeyCode::Char('P')));
    for c in "hello".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Tab));
    for c in "should be blocked".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s'));
    assert_eq!(app.bbs.store().message_count().unwrap(), before);
    assert!(app.status.contains("blocked"));

    // Lifting restores posting.
    app.screen = Screen::Sysop;
    app.on_key(press(KeyCode::Char('l')));
    assert!(app.moderation.can_post(&target_id, chrono::Utc::now()));
}
