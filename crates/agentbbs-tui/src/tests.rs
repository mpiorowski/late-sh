use super::*;
use crate::app::format_federation_join_status;
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
fn dm_new_peer_prompt_opens_a_dm_with_an_arbitrary_typed_handle() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('T'))); // -> dm
    assert_eq!(app.screen, Screen::Dm);
    assert!(!app.dm_peers().iter().any(|p| p == "someone-new"));

    app.on_key(press(KeyCode::Char('N')));
    assert!(app.dm_new_editing);
    for c in "someone-new".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("someone-new"));

    app.on_key(press(KeyCode::Enter));
    assert!(!app.dm_new_editing);
    assert_eq!(app.screen, Screen::Read); // open_dm switches straight into it
    assert_eq!(app.current_board.as_deref(), Some("dm:someone-new"));
    assert!(app.status.contains("someone-new"));
}

#[test]
fn dm_new_peer_prompt_esc_cancels_without_opening_anything() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('T')));
    let before = app.boards.len();

    app.on_key(press(KeyCode::Char('N')));
    for c in "throwaway".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Esc));
    assert!(!app.dm_new_editing);
    assert!(app.dm_new_input.is_empty());
    assert_eq!(app.screen, Screen::Dm); // stayed on the DM list, didn't open anything
    assert_eq!(app.boards.len(), before);
}

#[test]
fn dm_new_peer_prompt_rejects_an_empty_handle() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('T')));
    let before = app.boards.len();

    app.on_key(press(KeyCode::Char('N')));
    app.on_key(press(KeyCode::Enter)); // submit with nothing typed
    assert!(!app.dm_new_editing);
    assert_eq!(app.screen, Screen::Dm);
    assert_eq!(app.boards.len(), before);
    assert!(app.status.contains("cannot be empty"));
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

#[test]
fn console_shows_live_diagnostics_distinct_from_sysops_event_log() {
    let mut app = App::in_memory();
    app.hire("bob", "ops").unwrap();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('E'))); // -> console
    assert_eq!(app.screen, Screen::Console);

    let diag = app.console_diagnostics();
    let get = |k: &str| diag.iter().find(|(l, _)| *l == k).map(|(_, v)| v.clone());
    assert_eq!(get("boards"), Some(app.boards.len().to_string()));
    assert_eq!(get("pods"), Some("1".to_string()));
    assert_eq!(get("credits"), Some("100".to_string()));

    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Console"));
    assert!(text.contains("SYSTEM DIAGNOSTICS"));
    assert!(text.contains("point-in-time summary"));
}

fn post(app: &mut App, subject: &str, body: &str) {
    app.on_key(press(KeyCode::Char('P')));
    for c in subject.chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Tab));
    for c in body.chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s'));
}

#[test]
fn edit_own_message_replaces_its_body_via_a_signed_control_message() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    post(&mut app, "hi", "original text");
    assert_eq!(app.messages.len(), 1);

    app.on_key(press(KeyCode::Char('e'))); // edit the (only) message
    assert_eq!(app.screen, Screen::Compose);
    assert_eq!(app.compose_body, "original text"); // pre-filled
    for _ in 0.."original text".len() {
        app.on_key(press(KeyCode::Backspace));
    }
    for c in "edited text".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(ctrl('s'));

    assert_eq!(app.messages.len(), 1); // the control message is hidden
    let id = &app.messages[0].id.0;
    assert_eq!(app.messages[0].body.body, "edited text");
    assert!(app.status.contains("edited"));
    // The edited message must still show as verified — its own signature no
    // longer matches the substituted body (nobody signed "old metadata +
    // new body" as one unit), so `verified` must come from the cached
    // per-fetch flag, not a direct `.verify()` on the substituted message.
    assert_eq!(app.verified.get(id), Some(&true));
    assert!(app.edited.contains(id));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("✓sig"));
    assert!(!text.contains("✗SIG"));
    assert!(text.contains("(edited)"));
}

#[test]
fn edit_is_author_only() {
    use agentbbs_core::{MemoryStore, Presence};
    use std::sync::Arc;
    let presence = Arc::new(Presence::default());
    let store: Arc<dyn agentbbs_core::Store> = Arc::new(MemoryStore::new());
    let mut a = App::with_presence(store.clone(), presence.clone());
    let b = App::with_presence(store.clone(), presence.clone());

    a.on_key(press(KeyCode::Enter));
    a.on_key(press(KeyCode::Char('M')));
    a.on_key(press(KeyCode::Enter));
    post(&mut a, "hi", "a's message");
    let slug = a.current_board.clone().unwrap();

    // Forge an edit control message signed by b, targeting a's message.
    let target = a.messages[0].id.clone();
    let forged = agentbbs_core::MessageBody {
        board: slug.clone(),
        parent: None,
        subject: format!("agentbbs/ctl:edit:{}", target.0),
        body: "forged edit".into(),
        author: b.session.identity.id(),
        handle: "you".into(),
        created_at: chrono::Utc::now(),
    };
    let signed = Message::sign(&b.session.identity, forged).unwrap();
    b.bbs.post(b.session.caps, signed).unwrap();

    // a re-reads the board — the forged edit must NOT apply (author mismatch).
    a.open_selected_board();
    assert_eq!(a.messages.len(), 1);
    assert_eq!(a.messages[0].body.body, "a's message");
}

#[test]
fn delete_own_message_hides_it_and_is_author_only() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    post(&mut app, "hi", "to be deleted");
    assert_eq!(app.messages.len(), 1);

    app.on_key(press(KeyCode::Char('d')));
    assert_eq!(app.messages.len(), 0);
    assert!(app.status.contains("deleted"));

    // Store-level: the original message and the retract control message
    // both still exist (append-only), but the filtered view hides it.
    assert_eq!(app.bbs.store().message_count().unwrap(), 2);
}

#[test]
fn markdown_bold_and_code_markers_are_stripped_when_rendered() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    post(
        &mut app,
        "md",
        "run **cargo test** then check `agentbbs-tui` builds",
    );

    let text = screen_text(&app, 120, 30);
    // The literal markers must be gone from the rendered output...
    assert!(!text.contains("**cargo test**"));
    assert!(!text.contains("`agentbbs-tui`"));
    // ...but the enclosed content must still be there.
    assert!(text.contains("cargo test"));
    assert!(text.contains("agentbbs-tui"));
    assert!(text.contains("run"));
    assert!(text.contains("then check"));
}

#[test]
fn command_palette_opens_via_ctrl_k_filters_and_jumps_to_a_screen() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // splash -> main
    assert_eq!(app.screen, Screen::Main);

    app.on_key(ctrl('k'));
    assert_eq!(app.screen, Screen::Palette);
    assert_eq!(app.palette_return, Screen::Main);
    // Unfiltered, every MENU entry is a candidate.
    assert_eq!(app.filtered_palette_entries().len(), MENU.len());

    for c in "pod".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    let matches = app.filtered_palette_entries();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].1, "Pods");

    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Command Palette"));
    assert!(text.contains("pod"));
    assert!(text.contains("Pods"));

    app.on_key(press(KeyCode::Enter));
    assert_eq!(app.screen, Screen::Pods);
}

#[test]
fn command_palette_esc_returns_to_the_screen_it_was_opened_from() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // splash -> main
    app.on_key(press(KeyCode::Char('A'))); // main -> arena
    assert_eq!(app.screen, Screen::Arena);

    app.on_key(ctrl('k'));
    assert_eq!(app.screen, Screen::Palette);
    assert_eq!(app.palette_return, Screen::Arena);

    app.on_key(press(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Arena);
}

#[test]
fn command_palette_query_with_no_matches_shows_a_message_and_enter_is_a_no_op() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(ctrl('k'));
    for c in "zzzznomatch".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    assert!(app.filtered_palette_entries().is_empty());
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("No matches."));

    app.on_key(press(KeyCode::Enter));
    // Enter on an empty match list bails back to where the palette opened.
    assert_eq!(app.screen, Screen::Main);
}

#[test]
fn appearance_screen_lists_all_themes_and_enter_applies_the_selection() {
    let mut app = App::in_memory();
    assert_eq!(app.theme, crate::theme::ThemeName::Retro);

    app.on_key(press(KeyCode::Enter)); // splash -> main
    app.on_key(press(KeyCode::Char('O'))); // main -> appearance
    assert_eq!(app.screen, Screen::Appearance);
    // Opening the picker highlights the currently-active theme (index 0 = Retro).
    assert_eq!(app.appearance_index, 0);

    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Appearance"));
    for t in crate::theme::ThemeName::ALL {
        assert!(text.contains(t.label()), "missing theme label {t:?}");
    }

    app.on_key(press(KeyCode::Down));
    assert_eq!(app.appearance_index, 1);
    app.on_key(press(KeyCode::Enter));
    assert_eq!(app.theme, crate::theme::ThemeName::Dark);

    // Re-opening the picker now highlights the newly active theme, not 0.
    app.on_key(press(KeyCode::Esc));
    app.on_key(press(KeyCode::Char('O')));
    assert_eq!(app.appearance_index, 1);
}

#[test]
fn appearance_esc_returns_to_main_without_changing_the_theme() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('O')));
    app.on_key(press(KeyCode::Down));
    app.on_key(press(KeyCode::Esc));
    assert_eq!(app.screen, Screen::Main);
    assert_eq!(app.theme, crate::theme::ThemeName::Retro);
}

#[test]
fn theme_style_functions_resolve_distinct_accent_colours() {
    use crate::theme::{self, ThemeName};
    use ratatui::style::Color;
    assert_eq!(theme::chrome(ThemeName::Retro).fg, Some(Color::Cyan));
    assert_eq!(
        theme::chrome(ThemeName::Terminal).fg,
        Some(Color::Rgb(0xff, 0xb0, 0x00))
    );
    assert_ne!(
        theme::chrome(ThemeName::Retro).fg,
        theme::chrome(ThemeName::Nord).fg
    );
}

/// Regression test for a real bug caught during live verification: `chrome()`
/// and `lightbar()` both used to derive from the same per-theme `accent()`
/// colour, so a selected row's accent-coloured label text rendered as
/// accent-on-accent — genuinely invisible — for every theme. Selects each
/// theme's own row in the Appearance picker (so its label is rendered
/// highlighted) and asserts the label's fg/bg never match.
#[test]
fn selected_row_text_is_never_the_same_colour_as_its_own_highlight() {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    for (i, t) in crate::theme::ThemeName::ALL.iter().enumerate() {
        let mut app = App::in_memory();
        app.theme = *t;
        app.screen = Screen::Appearance;
        app.appearance_index = i;
        let backend = TestBackend::new(110, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| app.render(f)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let label = t.label();
        let first_char: String = label.chars().next().unwrap().to_string();
        let mut found = false;
        for y in 0..buffer.area.height {
            let row: String = (0..buffer.area.width)
                .map(|x| buffer.cell((x, y)).unwrap().symbol())
                .collect();
            if !row.contains(label) {
                continue;
            }
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, y)).unwrap();
                if cell.symbol() == first_char {
                    found = true;
                    assert_ne!(
                        cell.fg, cell.bg,
                        "{t:?} selected-row text is invisible (fg==bg)"
                    );
                }
            }
        }
        assert!(found, "did not find highlighted label cell for {t:?}");
    }
}

// ADR-0051: Federation screen gets a real `npx ruflo federation join/status`
// action instead of a hardcoded "no peers linked" panel. None of these tests
// invoke the real subprocess (`federation_join`/`federation_refresh_status`
// are never called) — matching the established rule that automated tests
// never spawn a real CommandRunner. `format_federation_join_status` is a
// pure function tested directly with synthetic results, exactly like the
// web's `collab_result`.

#[test]
fn federation_join_status_formats_ok_and_err_results() {
    assert_eq!(
        format_federation_join_status("100.1.2.3:7443", &Ok("linked ok".to_string())),
        "Joined peer 100.1.2.3:7443: linked ok"
    );
    assert_eq!(
        format_federation_join_status("100.1.2.3:7443", &Ok("  \n".to_string())),
        "Joined peer 100.1.2.3:7443."
    );
    assert_eq!(
        format_federation_join_status(
            "100.1.2.3:7443",
            &Err("spawn npx: No such file or directory".to_string())
        ),
        "Federation join failed: spawn npx: No such file or directory"
    );
}

#[test]
fn federation_screen_entry_does_not_touch_status_and_j_opens_editing() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // splash -> main
    app.on_key(press(KeyCode::Char('F'))); // main -> federation
    assert_eq!(app.screen, Screen::Federation);
    // Entering the screen must never trigger the real subprocess call.
    assert!(app.federation_status.is_none());

    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Federation Hall"));
    assert!(text.contains("not checked yet"));
    assert!(!text.contains("no peers linked")); // the old hardcoded panel is gone

    app.on_key(press(KeyCode::Char('J')));
    assert!(app.federation_editing);
    for c in "100.1.2.3:7443".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    assert_eq!(app.federation_input, "100.1.2.3:7443");
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("100.1.2.3:7443"));

    // Esc cancels — does not call federation_join, clears the input.
    app.on_key(press(KeyCode::Esc));
    assert!(!app.federation_editing);
    assert!(app.federation_input.is_empty());
    assert!(app.federation_status.is_none());
}

#[test]
fn federation_panel_renders_a_real_error_honestly_when_status_is_set() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('F')));
    // Simulate what a real failed `npx ruflo federation status` call would
    // leave behind, without actually spawning a process.
    app.federation_status = Some(Err("spawn npx: No such file or directory".to_string()));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("spawn npx: No such file or directory"));
    assert!(text.contains("real subprocess error"));
}

#[test]
fn federation_panel_renders_real_status_output_when_set() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('F')));
    app.federation_status = Some(Ok("mode: leaf\npeers: 0".to_string()));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("mode: leaf"));
    assert!(text.contains("peers: 0"));
}

// ADR-0051/0036: Collab screen (Jujutsu half — status/diff/log). GitHub
// issues/PRs are a separate slice needing a repo-input prompt. None of these
// tests invoke the real `jj` subprocess — `collab_jj_refresh` is never
// called, matching the established rule.

#[test]
fn collab_screen_defaults_to_status_view_and_shows_the_real_command() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // splash -> main
    app.on_key(press(KeyCode::Char('U'))); // main -> collab
    assert_eq!(app.screen, Screen::Collab);
    assert_eq!(app.collab_view, crate::app::CollabView::Status);
    assert!(app.collab_jj_status.is_none());

    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Collab"));
    assert!(text.contains("jj status"));
    assert!(text.contains("not checked yet"));
}

#[test]
fn collab_view_switches_between_status_diff_and_log_independently() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('U')));

    app.on_key(press(KeyCode::Char('2')));
    assert_eq!(app.collab_view, crate::app::CollabView::Diff);
    assert!(screen_text(&app, 110, 30).contains("jj diff"));

    app.on_key(press(KeyCode::Char('3')));
    assert_eq!(app.collab_view, crate::app::CollabView::Log);
    assert!(screen_text(&app, 110, 30).contains("jj log -n 10"));

    app.on_key(press(KeyCode::Char('1')));
    assert_eq!(app.collab_view, crate::app::CollabView::Status);
}

#[test]
fn collab_panel_renders_real_output_and_errors_honestly_per_view() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('U')));

    // Simulate a successful `jj status` without spawning a real process.
    app.collab_jj_status = Some(Ok("Working copy : abc123 (no changes)".to_string()));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Working copy : abc123"));

    // Switching to Diff shows Diff's own (unset) state, not Status's cache.
    app.on_key(press(KeyCode::Char('2')));
    assert!(app.collab_jj_diff.is_none());
    assert!(screen_text(&app, 110, 30).contains("not checked yet"));

    // Simulate a real failure (e.g. `jj` not installed).
    app.collab_jj_diff = Some(Err("spawn jj: No such file or directory".to_string()));
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("spawn jj: No such file or directory"));
    assert!(text.contains("real subprocess error"));

    // Status's cached result is untouched by Diff's state.
    app.on_key(press(KeyCode::Char('1')));
    assert!(screen_text(&app, 110, 30).contains("Working copy : abc123"));
}

#[test]
fn collab_github_tabs_prompt_for_repo_via_e_and_persist_it() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('U')));

    app.on_key(press(KeyCode::Char('4'))); // -> Issues tab
    assert_eq!(app.collab_view, crate::app::CollabView::GithubIssues);
    assert!(app.collab_view.needs_repo());
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("(none set)"));
    assert!(text.contains("gh issue list"));

    app.on_key(press(KeyCode::Char('E')));
    assert!(app.collab_repo_editing);
    for c in "ruvnet/AgentBBS".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Enter));
    assert!(!app.collab_repo_editing);
    assert_eq!(app.collab_repo, "ruvnet/AgentBBS");
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("ruvnet/AgentBBS"));
    assert!(text.contains("gh issue list --repo ruvnet/AgentBBS"));

    // The repo persists across tabs and screens.
    app.on_key(press(KeyCode::Char('5'))); // -> PRs tab
    assert!(screen_text(&app, 110, 30).contains("gh pr list --repo ruvnet/AgentBBS"));
}

#[test]
fn collab_github_refresh_without_a_repo_is_a_clear_local_error_not_a_subprocess_call() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('U')));
    app.on_key(press(KeyCode::Char('4')));
    assert!(app.collab_repo.is_empty());

    // R with no repo set must not spawn `gh` — it's a local validation error.
    app.on_key(press(KeyCode::Char('R')));
    assert_eq!(
        app.collab_gh_issues,
        Some(Err(
            "no repo set — press E to set owner/repo first".to_string()
        ))
    );
    let text = screen_text(&app, 110, 30);
    assert!(text.contains("no repo set"));
}

#[test]
fn collab_github_issues_and_prs_cache_independently() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('U')));
    app.collab_repo = "ruvnet/AgentBBS".to_string();

    app.on_key(press(KeyCode::Char('4')));
    app.collab_gh_issues = Some(Ok("#6  open  checkpoint issue".to_string()));
    assert!(screen_text(&app, 110, 30).contains("checkpoint issue"));

    app.on_key(press(KeyCode::Char('5')));
    assert!(app.collab_gh_prs.is_none());
    assert!(screen_text(&app, 110, 30).contains("not checked yet"));

    app.on_key(press(KeyCode::Char('4')));
    assert!(screen_text(&app, 110, 30).contains("checkpoint issue"));
}

#[test]
fn collab_repo_editing_esc_cancels_without_changing_the_stored_repo() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('U')));
    app.collab_repo = "already/set".to_string();

    app.on_key(press(KeyCode::Char('E')));
    for c in "throwaway/value".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Esc));
    assert!(!app.collab_repo_editing);
    assert_eq!(app.collab_repo, "already/set");
}

// ADR-0049: Agent Inbox — human-confirmed agent-drafted replies. The TUI has
// no HTTP client, so drafting always uses the offline scripted responder
// (ported verbatim from the web's own fallback) — never a live LLM call.

#[test]
fn begin_draft_composes_a_pending_draft_from_the_selected_message() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter)); // -> main
    app.on_key(press(KeyCode::Char('M'))); // -> boards
    app.on_key(press(KeyCode::Enter)); // open first board -> read
    post(&mut app, "dinner?", "can we meet for dinner Tuesday");
    assert_eq!(app.screen, Screen::Read);

    app.on_key(press(KeyCode::Char('A')));
    assert_eq!(app.drafts.pending().len(), 1);
    let d = app.drafts.pending()[0].clone();
    assert_eq!(d.agent, "claude");
    assert!(d.body.contains("Tuesday 7:30pm")); // scripted_reply's "dinner" branch
    assert!(d.in_reply_to.is_some());
    assert!(app.status.contains("Agent Inbox"));

    let text = screen_text(&app, 110, 30);
    assert!(!text.is_empty()); // Read screen still shown; Drafts is a separate screen
}

#[test]
fn drafts_screen_lists_pending_and_shows_the_full_body_for_the_selected_one() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    post(&mut app, "bug report", "found a bug, can you review");
    app.on_key(press(KeyCode::Char('A')));

    app.on_key(press(KeyCode::Esc)); // read -> boards
    app.on_key(press(KeyCode::Esc)); // boards -> main
    app.on_key(press(KeyCode::Char('N'))); // main -> drafts
    assert_eq!(app.screen, Screen::Drafts);

    let text = screen_text(&app, 110, 30);
    assert!(text.contains("Agent Inbox"));
    assert!(text.contains("@claude"));
    assert!(text.contains("suggested fix")); // scripted_reply's "bug" branch, full body shown
}

#[test]
fn draft_send_signs_under_the_reviewers_own_key_and_removes_it_from_pending() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    let slug = app.current_board.clone().unwrap();
    post(&mut app, "hello", "just saying hi");
    app.on_key(press(KeyCode::Char('A')));
    assert_eq!(app.drafts.pending().len(), 1);

    app.screen = Screen::Drafts;
    app.draft_index = 0;
    let before = app.bbs.store().message_count().unwrap();
    app.on_key(press(KeyCode::Char('S')));

    assert_eq!(
        app.drafts.pending().len(),
        0,
        "sent draft leaves the pending queue"
    );
    let after = app.bbs.store().message_count().unwrap();
    assert_eq!(after, before + 1);
    let sent = app
        .bbs
        .read_board(agentbbs_core::caps::Caps::READ, &slug, 10)
        .unwrap();
    let posted = sent.last().unwrap();
    // Signed under the reviewer's own key, never the drafting agent's identity.
    assert_eq!(posted.body.author, app.session.identity.id());
    assert!(posted.verify().is_ok());
}

#[test]
fn draft_discard_removes_it_without_posting_anything() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    post(&mut app, "hi", "checking the arena cve-bench score");
    app.on_key(press(KeyCode::Char('A')));

    app.screen = Screen::Drafts;
    app.draft_index = 0;
    let before = app.bbs.store().message_count().unwrap();
    app.on_key(press(KeyCode::Char('D')));

    assert_eq!(app.drafts.pending().len(), 0);
    assert_eq!(app.bbs.store().message_count().unwrap(), before);
}

#[test]
fn draft_edit_flow_revises_the_body_and_marks_it_edited() {
    let mut app = App::in_memory();
    app.on_key(press(KeyCode::Enter));
    app.on_key(press(KeyCode::Char('M')));
    app.on_key(press(KeyCode::Enter));
    post(&mut app, "hi", "generic question here");
    app.on_key(press(KeyCode::Char('A')));

    app.screen = Screen::Drafts;
    app.draft_index = 0;
    app.on_key(press(KeyCode::Char('E')));
    assert!(app.draft_editing);
    assert!(!app.draft_edit_input.is_empty()); // pre-filled with the current body

    // Replace it entirely.
    app.draft_edit_input.clear();
    for c in "a hand-revised reply".chars() {
        app.on_key(press(KeyCode::Char(c)));
    }
    app.on_key(press(KeyCode::Enter));

    assert!(!app.draft_editing);
    let d = app.drafts.pending()[0].clone();
    assert_eq!(d.body, "a hand-revised reply");
    assert_eq!(d.status, agentbbs_core::DraftStatus::Edited);
}
