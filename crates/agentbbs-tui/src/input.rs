//! Keyboard handling — maps [`crossterm`] key events to state transitions.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, ComposeField, Control, Screen, MENU};

impl App {
    /// Handle a single key event, returning whether to keep running.
    pub fn on_key(&mut self, key: KeyEvent) -> Control {
        // Any activity refreshes our entry in the shared presence registry.
        self.heartbeat();

        // Ctrl-C always quits, from anywhere.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Control::Quit;
        }

        match self.screen {
            Screen::Splash => self.key_splash(key),
            Screen::Main => self.key_main(key),
            Screen::Boards => self.key_boards(key),
            Screen::Read => self.key_read(key),
            Screen::Compose => self.key_compose(key),
            Screen::Arena => self.key_arena(key),
            Screen::Pods => self.key_pods(key),
            Screen::Approvals => self.key_approvals(key),
            Screen::Budget => self.key_budget(key),
            Screen::Directory => self.key_directory(key),
            Screen::Playbooks => self.key_playbooks(key),
            Screen::Digest => self.key_digest(key),
            Screen::Dm => self.key_dm(key),
            Screen::Passport => self.key_passport(key),
            Screen::Market => self.key_market(key),
            Screen::Sysop => self.key_sysop(key),
            Screen::Who | Screen::Doors | Screen::Federation | Screen::Decisions => {
                self.key_panel(key)
            }
            Screen::Goodbye => {
                self.should_quit = true;
                Control::Quit
            }
        }
    }

    fn key_splash(&mut self, _key: KeyEvent) -> Control {
        self.screen = Screen::Main;
        self.status = "Main menu — use arrows or hotkeys.".into();
        Control::Continue
    }

    fn key_main(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu_index = self.menu_index.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.menu_index + 1 < MENU.len() {
                    self.menu_index += 1;
                }
            }
            KeyCode::Enter => {
                let target = MENU[self.menu_index].2;
                self.goto(target);
            }
            KeyCode::Char(c) => {
                let up = c.to_ascii_uppercase();
                if let Some((_, _, target)) = MENU.iter().find(|(h, _, _)| *h == up) {
                    self.goto(*target);
                } else if up == 'Q' {
                    self.should_quit = true;
                    return Control::Quit;
                }
            }
            KeyCode::Esc => {
                self.should_quit = true;
                return Control::Quit;
            }
            _ => {}
        }
        Control::Continue
    }

    fn goto(&mut self, target: Screen) {
        match target {
            Screen::Boards => self.refresh_boards(),
            Screen::Goodbye => {}
            _ => {}
        }
        self.screen = target;
    }

    fn key_boards(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.board_index = self.board_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.board_index + 1 < self.boards.len() {
                    self.board_index += 1;
                }
            }
            KeyCode::Enter => self.open_selected_board(),
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_read(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.read_index = self.read_index.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if self.read_index + 1 < self.messages.len() {
                    self.read_index += 1;
                }
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                self.compose_reply_to = None;
                self.compose_field = ComposeField::Subject;
                self.status = "Compose — TAB switches field, Ctrl-S sends, ESC cancels.".into();
                self.screen = Screen::Compose;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => self.begin_reply(),
            // Slack-style quick channel switch — jump boards without
            // returning to the Boards list first.
            KeyCode::Char('[') => self.switch_board(-1),
            KeyCode::Char(']') => self.switch_board(1),
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Boards,
            _ => {}
        }
        Control::Continue
    }

    fn key_compose(&mut self, key: KeyEvent) -> Control {
        // Ctrl-S sends.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.submit_compose();
            return Control::Continue;
        }
        match key.code {
            KeyCode::Esc => {
                self.compose_reply_to = None;
                self.status = "Compose cancelled.".into();
                self.screen = Screen::Read;
            }
            KeyCode::Tab => {
                self.compose_field = match self.compose_field {
                    ComposeField::Subject => ComposeField::Body,
                    ComposeField::Body => ComposeField::Subject,
                };
            }
            KeyCode::Enter => match self.compose_field {
                ComposeField::Subject => self.compose_field = ComposeField::Body,
                ComposeField::Body => self.compose_body.push('\n'),
            },
            KeyCode::Backspace => {
                match self.compose_field {
                    ComposeField::Subject => self.compose_subject.pop(),
                    ComposeField::Body => self.compose_body.pop(),
                };
            }
            KeyCode::Char(c) => match self.compose_field {
                ComposeField::Subject => self.compose_subject.push(c),
                ComposeField::Body => self.compose_body.push(c),
            },
            _ => {}
        }
        Control::Continue
    }

    fn key_arena(&mut self, key: KeyEvent) -> Control {
        let count = self.arena.benchmarks().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.arena_index = self.arena_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.arena_index + 1 < count {
                    self.arena_index += 1;
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_pods(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.pod_index = self.pod_index.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if self.pod_index + 1 < self.pods.len() {
                    self.pod_index += 1;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => match self.hire("demo", "ops") {
                Ok(p) => {
                    self.status =
                        format!("Spawned {} in #{}", p.id, p.spec.template.registered_room)
                }
                Err(e) => self.status = format!("Spawn failed: {e}"),
            },
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_approvals(&mut self, key: KeyEvent) -> Control {
        use agentbbs_core::approval::Verdict;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.approval_index = self.approval_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.approval_index + 1 < self.proposals.len() {
                    self.approval_index += 1;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let p = self.propose_action("spend", "buy 1 GPU-hr for a pod run", "general");
                let short = &p.action_id[..p.action_id.len().min(8)];
                self.status = format!("Proposed {} ({short})", p.kind);
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(p) = self.proposals.get(self.approval_index).cloned() {
                    match self.decide_action(&p.action_id, Verdict::Approve, "looks fine") {
                        Ok(()) => self.status = "Approved.".into(),
                        Err(e) => self.status = format!("Decision failed: {e}"),
                    }
                }
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if let Some(p) = self.proposals.get(self.approval_index).cloned() {
                    match self.decide_action(&p.action_id, Verdict::Reject, "vetoed") {
                        Ok(()) => self.status = "Rejected.".into(),
                        Err(e) => self.status = format!("Decision failed: {e}"),
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_budget(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.pod_index = self.pod_index.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if self.pod_index + 1 < self.pods.len() {
                    self.pod_index += 1;
                }
            }
            KeyCode::Char('+') => {
                self.topup_selected_pod(0.10);
                self.status = "Raised cap by $0.10.".into();
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_directory(&mut self, key: KeyEvent) -> Control {
        let count = self.reputation.ranking().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.directory_index = self.directory_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.directory_index + 1 < count {
                    self.directory_index += 1;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => match self.hire_selected() {
                Ok(p) => {
                    self.status = format!(
                        "Hired — spawned {} in #{}",
                        p.id, p.spec.template.registered_room
                    )
                }
                Err(e) => self.status = format!("Hire failed: {e}"),
            },
            KeyCode::Char('i') | KeyCode::Char('I') => match self.issue_credential("skill:rust") {
                Ok(c) => self.status = format!("Issued {} to the highlighted agent.", c.claim),
                Err(e) => self.status = format!("Issue failed: {e}"),
            },
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_playbooks(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => {
                if self.run.is_none() {
                    self.run_playbook();
                } else {
                    self.advance_run();
                }
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => self.approve_current_gate("looks fine"),
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_digest(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Char('p') | KeyCode::Char('P') => match self.post_digest() {
                Ok(()) => self.status = "Digest posted to #general.".into(),
                Err(e) => self.status = format!("Digest post failed: {e}"),
            },
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_dm(&mut self, key: KeyEvent) -> Control {
        let count = self.dm_peers().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.dm_index = self.dm_index.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if self.dm_index + 1 < count {
                    self.dm_index += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(peer) = self.dm_peers().get(self.dm_index).cloned() {
                    self.open_dm(&peer);
                    self.status = format!("Opened DM with @{peer}.");
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_passport(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Char('r') | KeyCode::Char('R') => match self.rotate_identity() {
                Ok(()) => self.status = "Identity rotated — continuity preserved.".into(),
                Err(e) => self.status = format!("Rotation failed: {e}"),
            },
            KeyCode::Char('c') | KeyCode::Char('C') => {
                self.toggle_creator_mode();
                let on = self.session.caps.contains(agentbbs_core::caps::Caps::SYSOP);
                self.status = if on {
                    "Creator mode enabled — Sysop actions unlocked.".into()
                } else {
                    "Creator mode disabled.".into()
                };
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_market(&mut self, key: KeyEvent) -> Control {
        let count = self.market.all().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.market_index = self.market_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.market_index + 1 < count {
                    self.market_index += 1;
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let sku = self
                    .market
                    .all()
                    .get(self.market_index)
                    .map(|l| l.body.sku.clone());
                if let Some(sku) = sku {
                    match self.install_listing(&sku) {
                        Ok(()) => {
                            self.status = format!("Installed {sku} — {} credits left", self.credits)
                        }
                        Err(e) => self.status = format!("Install failed: {e}"),
                    }
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_sysop(&mut self, key: KeyEvent) -> Control {
        use agentbbs_core::moderation::Sanction;
        let count = self.reputation.ranking().len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.directory_index = self.directory_index.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.directory_index + 1 < count {
                    self.directory_index += 1;
                }
            }
            KeyCode::Char('m') | KeyCode::Char('M') => match self.moderate_selected(Sanction::Mute)
            {
                Ok(()) => self.status = "Target muted.".into(),
                Err(e) => self.status = format!("Moderation failed: {e}"),
            },
            KeyCode::Char('n') | KeyCode::Char('N') => {
                match self.moderate_selected(Sanction::Ban) {
                    Ok(()) => self.status = "Target banned.".into(),
                    Err(e) => self.status = format!("Moderation failed: {e}"),
                }
            }
            KeyCode::Char('l') | KeyCode::Char('L') => match self.moderate_selected(Sanction::Lift)
            {
                Ok(()) => self.status = "Sanction lifted.".into(),
                Err(e) => self.status = format!("Moderation failed: {e}"),
            },
            KeyCode::Esc | KeyCode::Char('q') => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }

    fn key_panel(&mut self, key: KeyEvent) -> Control {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => self.screen = Screen::Main,
            _ => {}
        }
        Control::Continue
    }
}
