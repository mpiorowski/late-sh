//! Rendering — draws the current [`App`] screen in retro BBS style.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use agentbbs_core::approval::Verdict;
use agentbbs_core::caps::Caps;
use agentbbs_core::playbook::{RunStatus, StepKind};
use agentbbs_core::pod::PodStatus;

use crate::app::{App, ComposeField, Screen, MENU};
use crate::theme;

impl App {
    /// Render the whole UI into `frame`.
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title bar
                Constraint::Min(3),    // body
                Constraint::Length(1), // status bar
            ])
            .split(area);

        self.render_title(frame, rows[0]);
        match self.screen {
            Screen::Splash => self.render_splash(frame, rows[1]),
            Screen::Main => self.render_main(frame, rows[1]),
            Screen::Boards => self.render_boards(frame, rows[1]),
            Screen::Read => self.render_read(frame, rows[1]),
            Screen::Compose => self.render_compose(frame, rows[1]),
            Screen::Who => self.render_who(frame, rows[1]),
            Screen::Doors => self.render_doors(frame, rows[1]),
            Screen::Arena => self.render_arena(frame, rows[1]),
            Screen::Market => self.render_market(frame, rows[1]),
            Screen::Federation => self.render_federation(frame, rows[1]),
            Screen::Sysop => self.render_sysop(frame, rows[1]),
            Screen::Pods => self.render_pods(frame, rows[1]),
            Screen::Approvals => self.render_approvals(frame, rows[1]),
            Screen::Budget => self.render_budget(frame, rows[1]),
            Screen::Decisions => self.render_decisions(frame, rows[1]),
            Screen::Directory => self.render_directory(frame, rows[1]),
            Screen::Playbooks => self.render_playbooks(frame, rows[1]),
            Screen::Digest => self.render_digest(frame, rows[1]),
            Screen::Dm => self.render_dm(frame, rows[1]),
            Screen::Passport => self.render_passport(frame, rows[1]),
            Screen::Console => self.render_console(frame, rows[1]),
            Screen::Palette => self.render_palette(frame, rows[1]),
            Screen::Appearance => self.render_appearance(frame, rows[1]),
            Screen::Collab => self.render_collab(frame, rows[1]),
            Screen::Goodbye => self.render_goodbye(frame, rows[1]),
        }
        self.render_status(frame, rows[2]);
    }

    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let line = Line::from(vec![
            Span::styled(" AgentBBS ", theme::title(self.theme)),
            Span::raw(" "),
            Span::styled(
                format!("node {}", agentbbs_core::PROTOCOL_VERSION),
                theme::dim(self.theme),
            ),
            Span::raw("  "),
            Span::styled(
                format!("you: {}", self.session.handle),
                theme::chrome(self.theme),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let p = Paragraph::new(Line::from(vec![
            Span::styled(" ▸ ", Style::default().fg(theme::accent(self.theme))),
            Span::styled(&self.status, theme::dim(self.theme)),
        ]));
        frame.render_widget(p, area);
    }

    fn framed(&self, title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme::chrome(self.theme))
            .title(Span::styled(format!(" {title} "), theme::title(self.theme)))
    }

    fn render_splash(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = vec![Line::from("")];
        for b in theme::BANNER {
            lines.push(Line::from(Span::styled(
                *b,
                Style::default().fg(theme::accent(self.theme)).bold(),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "The first BBS made for agents and human collaboration.",
            theme::chrome(self.theme),
        )));
        lines.push(Line::from(Span::styled(
            "Anonymous · signed · federated · WASM-extensible",
            theme::dim(self.theme),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "CONNECT 57600/ARQ/V.90  —  press ENTER to log on",
            Style::default().fg(theme::green(self.theme)),
        )));
        let p = Paragraph::new(Text::from(lines))
            .alignment(Alignment::Center)
            .block(self.framed("Dial-Up"));
        frame.render_widget(p, area);
    }

    fn render_main(&self, frame: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(area);

        let items: Vec<ListItem> = MENU
            .iter()
            .enumerate()
            .map(|(i, (hot, label, _))| {
                let selected = i == self.menu_index;
                let prefix = if selected { "▶ " } else { "  " };
                let line = Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(format!("[{hot}] "), theme::hotkey(self.theme)),
                    Span::styled(*label, theme::chrome(self.theme)),
                ]);
                let style = if selected {
                    theme::lightbar(self.theme)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();
        frame.render_widget(List::new(items).block(self.framed("Main Menu")), cols[0]);

        let boards = self.boards.len();
        let msgs = self.bbs.store().message_count().unwrap_or(0);
        let info = vec![
            Line::from(Span::styled("SYSTEM STATUS", theme::hotkey(self.theme))),
            Line::from(""),
            Line::from(format!("Message bases ...... {boards}")),
            Line::from(format!("Messages on file ... {msgs}")),
            Line::from(format!("Your access ........ {:#?}", self.session.caps)),
            Line::from(""),
            Line::from(Span::styled("Your anonymous id:", theme::dim(self.theme))),
            Line::from(Span::styled(
                self.session.identity.id().to_hex(),
                Style::default().fg(theme::green(self.theme)),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Ctrl-K anywhere: command palette",
                theme::dim(self.theme),
            )),
        ];
        frame.render_widget(
            Paragraph::new(info)
                .wrap(Wrap { trim: true })
                .block(self.framed("Status")),
            cols[1],
        );
    }

    fn render_boards(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .boards
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let selected = i == self.board_index;
                let prefix = if selected { "▶ " } else { "  " };
                let lock = if b.locked { " 🔒" } else { "" };
                let fed = if b.federated { " ⇄" } else { "" };
                let unread = self.unread_for(&b.slug);
                let mut spans = vec![
                    Span::raw(prefix),
                    Span::styled(format!("{:<14}", b.slug), theme::hotkey(self.theme)),
                    Span::styled(b.title.clone(), theme::chrome(self.theme)),
                    Span::styled(format!("{lock}{fed}"), theme::dim(self.theme)),
                ];
                // Slack-style unread badge — bright and unmissable.
                if unread > 0 {
                    spans.push(Span::styled(
                        format!("  ● {unread} new"),
                        Style::default().fg(theme::green(self.theme)).bold(),
                    ));
                }
                let line = Line::from(spans);
                let style = if selected {
                    theme::lightbar(self.theme)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(self.framed("Message Bases  (ENTER read · ESC back)")),
            area,
        );
    }

    fn render_read(&self, frame: &mut Frame, area: Rect) {
        let title = self.current_board.clone().unwrap_or_else(|| "board".into());
        let hint =
            "(P post · R reply · E edit own · D delete own · [ ] switch board · ↑↓ scroll · ESC back)";
        if self.messages.is_empty() {
            let p = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "No messages yet on this base.",
                    theme::dim(self.theme),
                )),
                Line::from(Span::styled(
                    "Press [P] to post the first one.",
                    theme::chrome(self.theme),
                )),
            ])
            .alignment(Alignment::Center)
            .block(self.framed(&format!("{title}  {hint}")));
            frame.render_widget(p, area);
            return;
        }
        let mut lines: Vec<Line> = Vec::new();
        for (i, m) in self.messages.iter().enumerate() {
            let marker = if i == self.read_index { "▶" } else { " " };
            // Verified against the message *as originally fetched* (cached
            // before any edit-body substitution), never re-derived from the
            // possibly-substituted `m` here — an edit's own control message
            // is independently signed, so the composite "original metadata +
            // edited body" was never itself signed as one unit and would
            // always fail a direct `.verify()` here despite being legitimate.
            let ok = self.verified.get(&m.id.0).copied().unwrap_or(false);
            let (verified, sig_style) = if ok {
                ("✓sig", Style::default().fg(theme::green(self.theme)))
            } else {
                ("✗SIG", Style::default().fg(theme::RED))
            };
            let indent = if m.body.parent.is_some() {
                "  ↳ "
            } else {
                ""
            };
            let edited_tag = if self.edited.contains(&m.id.0) {
                " (edited)"
            } else {
                ""
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} #{} ", i + 1), theme::hotkey(self.theme)),
                Span::styled(
                    m.body.created_at.format("%Y-%m-%d %H:%M").to_string(),
                    theme::dim(self.theme),
                ),
                Span::raw("  "),
                Span::styled(
                    if m.body.handle.is_empty() {
                        m.body.author.short()
                    } else {
                        m.body.handle.clone()
                    },
                    theme::chrome(self.theme),
                ),
                Span::raw("  "),
                Span::styled(verified, sig_style),
                Span::styled(edited_tag, theme::dim(self.theme)),
            ]));
            lines.push(Line::from(vec![
                Span::raw(format!("   {indent}")),
                Span::styled(m.body.subject.clone(), Style::default().fg(theme::YELLOW)),
            ]));
            for bl in m.body.body.lines() {
                let mut spans = vec![Span::raw(format!("   {indent}"))];
                spans.extend(markdown_spans(bl));
                lines.push(Line::from(spans));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(self.framed(&format!("{title}  {hint}"))),
            area,
        );
    }

    fn render_compose(&self, frame: &mut Frame, area: Rect) {
        let reply_line = self.compose_reply_to.as_ref().map(|r| {
            Line::from(Span::styled(
                format!("  ↳ replying to {}: {}", r.handle, r.subject),
                Style::default().fg(theme::YELLOW),
            ))
        });
        let rows = if reply_line.is_some() {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Min(3),
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(0),
                    Constraint::Length(3),
                    Constraint::Min(3),
                ])
                .split(area)
        };
        if let Some(line) = reply_line {
            frame.render_widget(Paragraph::new(line), rows[0]);
        }
        let rows = &rows[1..];

        let subj_focus = self.compose_field == ComposeField::Subject;
        let subject = Paragraph::new(Line::from(format!(
            "{}{}",
            self.compose_subject,
            if subj_focus { "█" } else { "" }
        )))
        .block(self.framed("Subject").border_style(if subj_focus {
            theme::lightbar(self.theme)
        } else {
            theme::chrome(self.theme)
        }));
        frame.render_widget(subject, rows[0]);

        let body_focus = self.compose_field == ComposeField::Body;
        let body = Paragraph::new(format!(
            "{}{}",
            self.compose_body,
            if body_focus { "█" } else { "" }
        ))
        .wrap(Wrap { trim: false })
        .block(
            self.framed("Message  (TAB field · Ctrl-S send · ESC cancel)")
                .border_style(if body_focus {
                    theme::lightbar(self.theme)
                } else {
                    theme::chrome(self.theme)
                }),
        );
        frame.render_widget(body, rows[1]);
    }

    fn render_who(&self, frame: &mut Frame, area: Rect) {
        let now = self.now_ms();
        let online = self.presence.online(now);
        let mut lines = vec![
            Line::from(Span::styled(
                "NODE  WHO                                KIND    IDLE",
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, m) in online.iter().enumerate() {
            let me = m.id == self.session.identity.id();
            let idle = now.saturating_sub(m.last_seen_ms) / 1000;
            let kind = if m.agent { "agent" } else { "human" };
            let style = if me {
                theme::chrome(self.theme)
            } else {
                theme::dim(self.theme)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{:>4}  ", i + 1), theme::hotkey(self.theme)),
                Span::styled(
                    format!(
                        "{:<34}",
                        if me {
                            format!("{} (you)", m.handle)
                        } else {
                            m.handle.clone()
                        }
                    ),
                    style,
                ),
                Span::styled(format!("{kind:<7} "), theme::chrome(self.theme)),
                Span::styled(
                    format!("{:02}:{:02}", idle / 60, idle % 60),
                    theme::dim(self.theme),
                ),
            ]));
        }
        if online.is_empty() {
            lines.push(Line::from(Span::styled(
                "  (nobody online)",
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "{} online · agents join via SSH, MCP, or federation. ESC to return.",
                online.len()
            ),
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines).block(self.framed("Who's Online")),
            area,
        );
    }

    fn render_market(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "SKU            KIND        TITLE                          PRICE",
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, l) in self.market.all().iter().enumerate() {
            let selected = i == self.market_index;
            let marker = if selected { "▶ " } else { "  " };
            let owned = self.installed.contains(&l.body.sku);
            let price = if owned {
                "✓ owned".to_string()
            } else if l.body.price == 0 {
                "free".to_string()
            } else {
                format!("{} cr", l.body.price)
            };
            let sig = if l.verify().is_ok() { "✓" } else { "✗" };
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(
                Line::from(vec![
                    Span::styled(
                        format!("{marker}{:<12} ", l.body.sku),
                        theme::hotkey(self.theme),
                    ),
                    Span::styled(
                        format!("{:<11} ", format!("{:?}", l.body.kind).to_lowercase()),
                        theme::dim(self.theme),
                    ),
                    Span::styled(format!("{:<30} ", l.body.title), theme::chrome(self.theme)),
                    Span::styled(
                        format!("{price:<8} "),
                        Style::default().fg(theme::green(self.theme)),
                    ),
                    Span::styled(sig, Style::default().fg(theme::green(self.theme))),
                ])
                .style(style),
            );
            lines.push(Line::from(Span::styled(
                format!("   {}", l.body.description),
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "Every listing is signed by its seller and verifies on each node. You have {} credits.",
                self.credits
            ),
            theme::chrome(self.theme),
        )));
        lines.push(Line::from(Span::styled(
            "[N] install highlighted · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Marketplace")),
            area,
        );
    }

    fn render_pods(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "ID          DOMAIN       STATUS      ROOM              CAP",
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, p) in self.pods.iter().enumerate() {
            let selected = i == self.pod_index;
            let marker = if selected { "▶" } else { " " };
            let status = match p.status {
                PodStatus::Spawned => "spawned",
                PodStatus::Executing => "executing",
                PodStatus::Evaluating => "evaluating",
                PodStatus::Escalating => "escalating",
                PodStatus::Completed => "completed",
                PodStatus::Failed => "failed",
            };
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(
                Line::from(vec![
                    Span::styled(format!("{marker} {:<11}", p.id), theme::hotkey(self.theme)),
                    Span::styled(
                        format!("{:<12} ", p.spec.template.domain),
                        theme::chrome(self.theme),
                    ),
                    Span::styled(
                        format!("{status:<11} "),
                        Style::default().fg(theme::green(self.theme)),
                    ),
                    Span::styled(
                        format!("{:<17} ", p.spec.template.registered_room),
                        theme::dim(self.theme),
                    ),
                    Span::styled(
                        format!("${:.2}", p.spec.template.per_agent_cap_usd),
                        theme::dim(self.theme),
                    ),
                ])
                .style(style),
            );
        }
        if self.pods.is_empty() {
            lines.push(Line::from(Span::styled(
                "No pods spawned yet. Hire an agent from the Directory, or press [N] to spawn a demo pod.",
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[N] spawn a demo pod · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Pods")),
            area,
        );
    }

    fn render_approvals(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "KIND         SUMMARY                                    STATUS",
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, p) in self.proposals.iter().enumerate() {
            let selected = i == self.approval_index;
            let marker = if selected { "▶" } else { " " };
            let authorized = self.is_action_authorized(&p.action_id);
            let status = if authorized {
                Span::styled(
                    "✓ authorized",
                    Style::default().fg(theme::green(self.theme)),
                )
            } else {
                Span::styled("⧗ pending", theme::dim(self.theme))
            };
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(
                Line::from(vec![
                    Span::styled(
                        format!("{marker} {:<12}", p.kind),
                        theme::hotkey(self.theme),
                    ),
                    Span::styled(format!("{:<42} ", p.summary), theme::chrome(self.theme)),
                    status,
                ])
                .style(style),
            );
            for d in self.gate.decisions_for(&p.action_id) {
                let v = match d.verdict {
                    Verdict::Approve => {
                        Span::styled("approve", Style::default().fg(theme::green(self.theme)))
                    }
                    Verdict::Reject => Span::styled("reject", Style::default().fg(theme::RED)),
                };
                lines.push(Line::from(vec![
                    Span::raw("      ↳ "),
                    v,
                    Span::styled(
                        format!(" by @{}", d.decider.short()),
                        theme::dim(self.theme),
                    ),
                    Span::styled(
                        if d.reason.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", d.reason)
                        },
                        theme::dim(self.theme),
                    ),
                ]));
            }
        }
        if self.proposals.is_empty() {
            lines.push(Line::from(Span::styled(
                "No proposals yet. Press [N] to raise a demo proposal.",
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[N] propose · [Y] approve highlighted · [R] reject highlighted · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Approvals")),
            area,
        );
    }

    fn render_budget(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "POD          SPENT      CAP        STATUS",
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, p) in self.pods.iter().enumerate() {
            let s = self.budget_status(p);
            let selected = i == self.pod_index;
            let marker = if selected { "▶" } else { " " };
            let badge = if s.over_budget {
                Span::styled("⚠ over budget", Style::default().fg(theme::RED))
            } else {
                Span::styled(
                    format!("{:.0}%", s.pct * 100.0),
                    Style::default().fg(theme::green(self.theme)),
                )
            };
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(
                Line::from(vec![
                    Span::styled(format!("{marker} {:<11}", p.id), theme::hotkey(self.theme)),
                    Span::styled(format!("${:<9.3} ", s.spent), theme::chrome(self.theme)),
                    Span::styled(format!("${:<9.2} ", s.cap), theme::dim(self.theme)),
                    badge,
                ])
                .style(style),
            );
        }
        if self.pods.is_empty() {
            lines.push(Line::from(Span::styled(
                "No pods to budget.",
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[+] raise highlighted pod's cap by $0.10 · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Budget Guardrails")),
            area,
        );
    }

    fn render_decisions(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = Vec::new();
        for r in self.decisions.all() {
            lines.push(Line::from(vec![
                Span::styled(
                    r.decided_at.format("%Y-%m-%d %H:%M ").to_string(),
                    theme::dim(self.theme),
                ),
                Span::styled(r.title.clone(), Style::default().fg(theme::YELLOW)),
                Span::raw("  "),
                Span::styled(format!("#{}", r.board), theme::chrome(self.theme)),
            ]));
            lines.push(Line::from(format!("   {}", r.decision)));
            lines.push(Line::from(Span::styled(
                format!("   why: {}", r.rationale),
                theme::dim(self.theme),
            )));
            lines.push(Line::from(""));
        }
        if lines.is_empty() {
            lines.push(Line::from(Span::styled(
                "No decisions recorded yet.",
                theme::dim(self.theme),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(self.framed("Decision Records  (ESC back)")),
            area,
        );
    }

    fn render_directory(&self, frame: &mut Frame, area: Rect) {
        let ranking = self.reputation.ranking();
        let mut lines = vec![
            Line::from(Span::styled(
                "  #  HANDLE            SCORE   RATE    CREDENTIALS",
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, r) in ranking.iter().enumerate() {
            let selected = i == self.directory_index;
            let marker = if selected { "▶" } else { " " };
            let id = agentbbs_core::identity::AgentId::from_hex(&r.agent).ok();
            let handle = id
                .as_ref()
                .map(|id| self.directory_handle(id))
                .unwrap_or_else(|| r.agent[..8.min(r.agent.len())].to_string());
            let creds: Vec<String> = id
                .as_ref()
                .map(|id| {
                    self.credentials
                        .valid_for(id, chrono::Utc::now())
                        .iter()
                        .map(|c| format!("🎫{}", c.claim))
                        .collect()
                })
                .unwrap_or_default();
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(
                Line::from(vec![
                    Span::styled(
                        format!("{marker} {:>2}  ", i + 1),
                        theme::hotkey(self.theme),
                    ),
                    Span::styled(format!("@{:<17} ", handle), theme::chrome(self.theme)),
                    Span::styled(
                        format!("{:>5.2}  ", r.score),
                        Style::default().fg(theme::green(self.theme)),
                    ),
                    Span::styled(
                        format!("{:>4.0}%  ", r.rate * 100.0),
                        theme::dim(self.theme),
                    ),
                    Span::styled(creds.join(" "), theme::dim(self.theme)),
                ])
                .style(style),
            );
        }
        if ranking.is_empty() {
            lines.push(Line::from(Span::styled(
                "No agents observed yet.",
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[N] hire highlighted · [I] issue skill:rust credential · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Agent Directory")),
            area,
        );
    }

    fn render_playbooks(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(vec![
                Span::styled(self.playbook.name.clone(), theme::hotkey(self.theme)),
                Span::raw(" "),
                Span::styled(
                    format!("v{}", self.playbook.version),
                    theme::dim(self.theme),
                ),
            ]),
            Line::from(Span::styled(
                format!("trigger: {}", self.playbook.trigger),
                theme::dim(self.theme),
            )),
            Line::from(""),
        ];
        let current_id = self.run.as_ref().and_then(|r| r.current()).map(|s| &s.id);
        for step in &self.playbook.steps {
            let active = current_id == Some(&step.id);
            let marker = if active { "▶ " } else { "  " };
            let (kind, detail) = match &step.kind {
                StepKind::AgentTask { agent, instruction } => {
                    ("agent task", format!("@{agent}: {instruction}"))
                }
                StepKind::ApprovalGate { summary } => ("approval gate", summary.clone()),
                StepKind::Tool { tool } => ("tool", tool.clone()),
            };
            let style = if active {
                Style::default().fg(theme::YELLOW)
            } else {
                theme::chrome(self.theme)
            };
            lines.push(Line::from(vec![
                Span::raw(marker),
                Span::styled(format!("[{}] ", step.id), theme::hotkey(self.theme)),
                Span::styled(format!("{kind:<14} "), theme::dim(self.theme)),
                Span::styled(detail, style),
            ]));
        }
        lines.push(Line::from(""));
        let status_line = match &self.run {
            None => Span::styled("Not started.", theme::dim(self.theme)),
            Some(r) => match r.status() {
                RunStatus::Running => {
                    Span::styled("Running…", Style::default().fg(theme::green(self.theme)))
                }
                RunStatus::AwaitingApproval => {
                    Span::styled("⧗ Awaiting approval", Style::default().fg(theme::YELLOW))
                }
                RunStatus::Completed => {
                    Span::styled("✓ Completed", Style::default().fg(theme::green(self.theme)))
                }
                RunStatus::Failed => Span::styled("✗ Failed", Style::default().fg(theme::RED)),
            },
        };
        lines.push(Line::from(vec![
            Span::styled("Status: ", theme::hotkey(self.theme)),
            status_line,
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[R] run/advance · [Y] approve the current gate + advance · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Playbooks")),
            area,
        );
    }

    fn render_digest(&self, frame: &mut Frame, area: Rect) {
        let (count, participants) = self.digest_stats();
        let lines = vec![
            Line::from(Span::styled(
                format!("Daily Digest — {}", chrono::Utc::now().format("%Y-%m-%d")),
                theme::hotkey(self.theme),
            )),
            Line::from(""),
            Line::from("Message bases activity on #general:"),
            Line::from(vec![
                Span::styled(
                    format!("  {count} "),
                    Style::default().fg(theme::green(self.theme)),
                ),
                Span::raw("message(s) from "),
                Span::styled(
                    format!("{participants} "),
                    Style::default().fg(theme::green(self.theme)),
                ),
                Span::raw("participant(s) today."),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "[P] post this summary to #general, signed as \"digest\" · ESC back",
                theme::chrome(self.theme),
            )),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Daily Digest")),
            area,
        );
    }

    fn render_dm(&self, frame: &mut Frame, area: Rect) {
        let peers = self.dm_peers();
        let mut lines = vec![
            Line::from(Span::styled(
                "Private threads — a dm:<peer> board per peer, local-only (ADR-0037 Phase 1).",
                theme::dim(self.theme),
            )),
            Line::from(""),
        ];
        for (i, peer) in peers.iter().enumerate() {
            let selected = i == self.dm_index;
            let marker = if selected { "▶ " } else { "  " };
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(
                Line::from(vec![
                    Span::raw(marker),
                    Span::styled(format!("@{peer}"), theme::chrome(self.theme)),
                ])
                .style(style),
            );
        }
        if peers.is_empty() {
            lines.push(Line::from(Span::styled(
                "No known peers yet — hire someone from the Directory first.",
                theme::dim(self.theme),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "ENTER open the highlighted DM · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Direct Messages")),
            area,
        );
    }

    fn render_passport(&self, frame: &mut Frame, area: Rect) {
        let id = self.session.identity.id();
        let mut lines = vec![
            Line::from(Span::styled(
                "Your anonymous id (full):",
                theme::dim(self.theme),
            )),
            Line::from(Span::styled(
                id.to_hex(),
                Style::default().fg(theme::green(self.theme)),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("role ........ ", theme::hotkey(self.theme)),
                Span::styled(
                    format!("{:#?}", self.session.caps),
                    theme::chrome(self.theme),
                ),
            ]),
            Line::from(vec![
                Span::styled("handle ...... ", theme::hotkey(self.theme)),
                Span::styled(self.session.handle.clone(), theme::chrome(self.theme)),
            ]),
        ];
        if let Some(from) = &self.rotated_from {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Rotated from (dual-signed, reputation/credentials carry over):",
                theme::dim(self.theme),
            )));
            lines.push(Line::from(Span::styled(
                from.to_hex(),
                Style::default().fg(theme::YELLOW),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Your Ed25519 private key lives only in this process's memory — closing the",
            theme::dim(self.theme),
        )));
        lines.push(Line::from(Span::styled(
            "session discards it. Rotating swaps it for a fresh one, with continuity.",
            theme::dim(self.theme),
        )));
        lines.push(Line::from(""));
        let creator = self.session.caps.contains(Caps::SYSOP);
        lines.push(Line::from(vec![
            Span::styled("creator mode  ", theme::hotkey(self.theme)),
            Span::styled(
                if creator {
                    "✓ enabled"
                } else {
                    "✗ disabled"
                },
                if creator {
                    Style::default().fg(theme::green(self.theme))
                } else {
                    theme::dim(self.theme)
                },
            ),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "[R] rotate identity · [C] toggle creator mode · ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Passport")),
            area,
        );
    }

    fn render_console(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "SYSTEM DIAGNOSTICS",
                theme::hotkey(self.theme),
            )),
            Line::from(""),
        ];
        for (label, value) in self.console_diagnostics() {
            lines.push(Line::from(vec![
                Span::styled(format!("{:<17} ", label), theme::dim(self.theme)),
                Span::styled(value, theme::chrome(self.theme)),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "A point-in-time summary, not a log — see Sysop Report for the",
            theme::dim(self.theme),
        )));
        lines.push(Line::from(Span::styled(
            "chronological event stream this reads from.",
            theme::dim(self.theme),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "ESC back",
            theme::chrome(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Console")),
            area,
        );
    }

    fn render_palette(&self, frame: &mut Frame, area: Rect) {
        let entries = self.filtered_palette_entries();
        let mut lines = vec![
            Line::from(vec![
                Span::styled("> ", theme::hotkey(self.theme)),
                Span::styled(self.palette_query.clone(), theme::chrome(self.theme)),
                Span::styled("▏", theme::dim(self.theme)),
            ]),
            Line::from(""),
        ];
        if entries.is_empty() {
            lines.push(Line::from(Span::styled(
                "No matches.",
                theme::dim(self.theme),
            )));
        }
        for (i, (hot, label, _)) in entries.iter().enumerate() {
            let selected = i == self.palette_index;
            let prefix = if selected { "▶ " } else { "  " };
            let line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(format!("[{hot}] "), theme::hotkey(self.theme)),
                Span::styled(*label, theme::chrome(self.theme)),
            ]);
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(line.style(style));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "type to filter · ↑↓ select · ENTER jump · ESC cancel",
            theme::dim(self.theme),
        )));
        frame.render_widget(
            Paragraph::new(lines).block(self.framed("Command Palette (Ctrl-K)")),
            area,
        );
    }

    fn render_appearance(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled("APPEARANCE", theme::hotkey(self.theme))),
            Line::from(Span::styled(
                "Mirrors the web UI's palettes (genesis/index.html data-theme).",
                theme::dim(self.theme),
            )),
            Line::from(""),
        ];
        for (i, t) in theme::ThemeName::ALL.iter().enumerate() {
            let selected = i == self.appearance_index;
            let active = *t == self.theme;
            let prefix = if selected { "▶ " } else { "  " };
            let marker = if active { "● " } else { "○ " };
            let line = Line::from(vec![
                Span::raw(prefix),
                Span::styled(marker, Style::default().fg(theme::accent(*t))),
                Span::styled(t.label(), theme::chrome(self.theme)),
            ]);
            let style = if selected {
                theme::lightbar(self.theme)
            } else {
                Style::default()
            };
            lines.push(line.style(style));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "● current theme · ↑↓ select · ENTER apply · ESC back",
            theme::dim(self.theme),
        )));
        frame.render_widget(Paragraph::new(lines).block(self.framed("Appearance")), area);
    }

    fn render_doors(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled(
                "DOOR GAMES & AGENT TOOLS",
                theme::hotkey(self.theme),
            )),
            Line::from(""),
            Line::from("  [1] WASM Plugins ......... sandboxed agent tools (wasmi host)"),
            Line::from("  [2] Marketplace .......... trade plugins, agents, boards"),
            Line::from("  [3] MCP Bridge ........... expose boards to Claude Code & co."),
            Line::from("  [4] Memory Lane .......... RVF vector recall of past threads"),
            Line::from(""),
            Line::from(Span::styled(
                "Doors run as capability-scoped WASM modules. ESC to return.",
                theme::dim(self.theme),
            )),
        ];
        frame.render_widget(Paragraph::new(lines).block(self.framed("Doors")), area);
    }

    fn render_arena(&self, frame: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        let benches = self.arena.benchmarks();
        let items: Vec<ListItem> = benches
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let selected = i == self.arena_index;
                let prefix = if selected { "▶ " } else { "  " };
                let line = Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(format!("{:<11}", b.id.0), theme::hotkey(self.theme)),
                    Span::styled(b.name.clone(), theme::chrome(self.theme)),
                ]);
                let style = if selected {
                    theme::lightbar(self.theme)
                } else {
                    Style::default()
                };
                ListItem::new(line).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(self.framed("Benchmarks  (↑↓ · ESC back)")),
            cols[0],
        );

        let selected = benches.get(self.arena_index);
        let mut lines: Vec<Line> = Vec::new();
        if let Some(b) = selected {
            lines.push(Line::from(Span::styled(
                b.name.clone(),
                theme::hotkey(self.theme),
            )));
            lines.push(Line::from(Span::styled(
                b.description.clone(),
                theme::dim(self.theme),
            )));
            lines.push(Line::from(Span::styled(
                format!("harness: {}", b.harness),
                theme::dim(self.theme),
            )));
            lines.push(Line::from(""));
            if b.id.0 == agentbbs_arena::RETORT_BENCHMARK_ID {
                // The Retort track ranks agent+harness+model *stacks* by their
                // position on the accuracy-vs-cost PARETO FRONTIER (not raw
                // accuracy): frontier first, then accuracy within tier.
                lines.push(Line::from(Span::styled(
                    " #  PARETO  STACK (model · harness · lang)        COV    COST",
                    theme::hotkey(self.theme),
                )));
                lines.push(Line::from(
                    "──────────────────────────────────────────────────────────────────",
                ));
                let board = self.arena.retort_leaderboard();
                if board.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "No retort results ingested — `agentbbs arena retort <results.json>`.",
                        theme::dim(self.theme),
                    )));
                }
                for s in board.iter().take(12) {
                    let mark = if s.pareto_optimal {
                        "◆ front"
                    } else {
                        "✗ domin"
                    };
                    let mark_style = if s.pareto_optimal {
                        Style::default().fg(theme::green(self.theme))
                    } else {
                        theme::dim(self.theme)
                    };
                    let name = if s.is_baseline {
                        format!("{} [base]", s.stack)
                    } else {
                        s.stack.clone()
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{:>2}  ", s.rank), theme::hotkey(self.theme)),
                        Span::styled(format!("{mark:<7} "), mark_style),
                        Span::styled(format!("{name:<34}"), theme::chrome(self.theme)),
                        Span::styled(
                            format!("{:>5.1}% ", s.requirement_coverage * 100.0),
                            Style::default().fg(theme::green(self.theme)),
                        ),
                        Span::styled(format!("${:.3}", s.cost_usd), theme::dim(self.theme)),
                    ]));
                    // The cost-lever insight line.
                    lines.push(Line::from(Span::styled(
                        format!("        💡 {}", s.insight),
                        theme::dim(self.theme),
                    )));
                    if s.excluded_tooling > 0 {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "        (excluded {} TOOLING false-fail(s) — honest scoring)",
                                s.excluded_tooling
                            ),
                            theme::dim(self.theme),
                        )));
                    }
                }
                // The frontier curve (non-dominated set), cheapest first.
                let front = agentbbs_arena::frontier(&board);
                if !front.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "FRONTIER  ($/task ↑ · coverage)",
                        theme::hotkey(self.theme),
                    )));
                    for s in &front {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "  ${:<7.3} {:>5.1}%  {}",
                                s.cost_usd,
                                s.requirement_coverage * 100.0,
                                s.stack
                            ),
                            theme::dim(self.theme),
                        )));
                    }
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "RANK  AGENT                       SCORE   PASS",
                    theme::hotkey(self.theme),
                )));
                lines.push(Line::from(
                    "────────────────────────────────────────────────",
                ));
                match self.arena.leaderboard(&b.id.0) {
                    Ok(board) if !board.is_empty() => {
                        for s in board.iter().take(12) {
                            let medal = match s.rank {
                                1 => "🥇",
                                2 => "🥈",
                                3 => "🥉",
                                _ => "  ",
                            };
                            lines.push(Line::from(vec![
                                Span::styled(
                                    format!(" {:>2} {medal} ", s.rank),
                                    theme::hotkey(self.theme),
                                ),
                                Span::styled(
                                    format!("{:<26}", s.handle),
                                    theme::chrome(self.theme),
                                ),
                                Span::styled(
                                    format!("{:>5.1}%  ", s.best_score * 100.0),
                                    Style::default().fg(theme::green(self.theme)),
                                ),
                                Span::styled(
                                    format!("{}/{}", s.passed, s.total),
                                    theme::dim(self.theme),
                                ),
                            ]));
                        }
                    }
                    _ => lines.push(Line::from(Span::styled(
                        "No submissions yet — `agentbbs` agents: compete!",
                        theme::dim(self.theme),
                    ))),
                }
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Arena Leaderboard")),
            cols[1],
        );
    }

    fn render_federation(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "ZERO-TRUST FEDERATION",
                theme::hotkey(self.theme),
            )),
            Line::from(""),
            Line::from("  Identity ........ ed25519 (anonymous, per-node)"),
            Line::from("  Transport ....... signed envelopes, PII-stripped egress"),
            Line::from("  Memory .......... npm AgentDB / RVF vector sync"),
            Line::from(""),
            Line::from(Span::styled(
                "STATUS (npx ruflo federation status)",
                theme::hotkey(self.theme),
            )),
        ];
        match &self.federation_status {
            None => lines.push(Line::from(Span::styled(
                "  (not checked yet — press R; this runs a real npx subprocess)",
                theme::dim(self.theme),
            ))),
            Some(Ok(out)) if out.trim().is_empty() => lines.push(Line::from(Span::styled(
                "  (empty response)",
                theme::dim(self.theme),
            ))),
            Some(Ok(out)) => {
                for line in out.lines().take(8) {
                    lines.push(Line::from(Span::styled(
                        format!("  {line}"),
                        theme::chrome(self.theme),
                    )));
                }
            }
            Some(Err(e)) => {
                lines.push(Line::from(Span::styled(
                    format!("  ✗ {e}"),
                    Style::default().fg(theme::RED),
                )));
                lines.push(Line::from(Span::styled(
                    "  (real subprocess error — ruflo/npx isn't available here; not faked)",
                    theme::dim(self.theme),
                )));
            }
        }
        lines.push(Line::from(""));
        if self.federation_editing {
            lines.push(Line::from(vec![
                Span::styled("Peer address: ", theme::hotkey(self.theme)),
                Span::styled(self.federation_input.clone(), theme::chrome(self.theme)),
                Span::styled("▏", theme::dim(self.theme)),
            ]));
            lines.push(Line::from(Span::styled(
                "ENTER to join · ESC to cancel",
                theme::dim(self.theme),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "[J] join a peer (npx ruflo federation join <addr>) · [R] refresh · ESC back",
                theme::dim(self.theme),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Federation Hall")),
            area,
        );
    }

    fn render_collab(&self, frame: &mut Frame, area: Rect) {
        use crate::app::CollabView;
        let tab = |view: CollabView, label: &str, hot: char| {
            let active = self.collab_view == view;
            let style = if active {
                theme::lightbar(self.theme)
            } else {
                theme::dim(self.theme)
            };
            Span::styled(format!(" [{hot}] {label} "), style)
        };
        let mut lines = vec![
            Line::from(vec![
                tab(CollabView::Status, "Status", '1'),
                tab(CollabView::Diff, "Diff", '2'),
                tab(CollabView::Log, "Log", '3'),
                tab(CollabView::GithubIssues, "Issues", '4'),
                tab(CollabView::GithubPrs, "PRs", '5'),
            ]),
            Line::from(""),
        ];
        if self.collab_view.needs_repo() {
            let repo_shown = if self.collab_repo.is_empty() {
                "(none set)"
            } else {
                self.collab_repo.as_str()
            };
            lines.push(Line::from(vec![
                Span::styled("repo: ", theme::dim(self.theme)),
                Span::styled(repo_shown, theme::chrome(self.theme)),
            ]));
        }
        let cmd = match self.collab_view {
            CollabView::Status => "jj status".to_string(),
            CollabView::Diff => "jj diff".to_string(),
            CollabView::Log => "jj log -n 10".to_string(),
            CollabView::GithubIssues => format!("gh issue list --repo {}", self.collab_repo),
            CollabView::GithubPrs => format!("gh pr list --repo {}", self.collab_repo),
        };
        lines.push(Line::from(Span::styled(
            format!("$ {cmd}"),
            theme::hotkey(self.theme),
        )));
        let bin = if self.collab_view.needs_repo() {
            "gh"
        } else {
            "jj"
        };
        match self.collab_current() {
            None => lines.push(Line::from(Span::styled(
                format!("  (not checked yet — press R; this runs a real {bin} subprocess)"),
                theme::dim(self.theme),
            ))),
            Some(Ok(out)) if out.trim().is_empty() => lines.push(Line::from(Span::styled(
                "  (empty response)",
                theme::dim(self.theme),
            ))),
            Some(Ok(out)) => {
                for line in out.lines().take(14) {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        theme::chrome(self.theme),
                    )));
                }
            }
            Some(Err(e)) => {
                lines.push(Line::from(Span::styled(
                    format!("  ✗ {e}"),
                    Style::default().fg(theme::RED),
                )));
                lines.push(Line::from(Span::styled(
                    format!(
                        "  (real subprocess error — {bin} isn't available or failed; not faked)"
                    ),
                    theme::dim(self.theme),
                )));
            }
        }
        lines.push(Line::from(""));
        if self.collab_repo_editing {
            lines.push(Line::from(vec![
                Span::styled("owner/repo: ", theme::hotkey(self.theme)),
                Span::styled(self.collab_repo_input.clone(), theme::chrome(self.theme)),
                Span::styled("▏", theme::dim(self.theme)),
            ]));
            lines.push(Line::from(Span::styled(
                "ENTER to set · ESC to cancel",
                theme::dim(self.theme),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "[1-5] switch view · [E] set owner/repo · [R] refresh · ESC back",
                theme::dim(self.theme),
            )));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Collab (Jujutsu / GitHub)")),
            area,
        );
    }

    fn render_sysop(&self, frame: &mut Frame, area: Rect) {
        let events = self.reporter.snapshot();
        let mut lines = vec![
            Line::from(Span::styled(
                format!("LIVE EVENT LOG  ({} retained)", events.len()),
                theme::hotkey(self.theme),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        if !self.session.caps.contains(Caps::SYSOP) {
            lines.push(Line::from(Span::styled(
                "Read-only view — SYSOP capability required for actions (toggle creator mode on Passport).",
                theme::dim(self.theme),
            )));
        } else {
            let ranking = self.reputation.ranking();
            if let Some(entry) = ranking.get(self.directory_index) {
                let id = agentbbs_core::identity::AgentId::from_hex(&entry.agent).ok();
                let handle = id
                    .as_ref()
                    .map(|id| self.directory_handle(id))
                    .unwrap_or_else(|| entry.agent[..8.min(entry.agent.len())].to_string());
                let status = id
                    .as_ref()
                    .map(|id| self.moderation.status(id))
                    .map(|s| {
                        if s.banned {
                            "banned".to_string()
                        } else if s.muted {
                            "muted".to_string()
                        } else if s.timed_out_until.is_some() {
                            "timed out".to_string()
                        } else {
                            "none".to_string()
                        }
                    })
                    .unwrap_or_else(|| "none".to_string());
                lines.push(Line::from(vec![
                    Span::styled("Target (Directory #", theme::dim(self.theme)),
                    Span::styled(
                        format!("{}", self.directory_index + 1),
                        theme::dim(self.theme),
                    ),
                    Span::styled("): ", theme::dim(self.theme)),
                    Span::styled(format!("@{handle} "), theme::chrome(self.theme)),
                    Span::styled(format!("[{status}]"), Style::default().fg(theme::YELLOW)),
                ]));
                lines.push(Line::from(Span::styled(
                    "[M] mute · [N] ban · [L] lift · [↑↓] pick target",
                    theme::chrome(self.theme),
                )));
            }
        }
        for e in events
            .iter()
            .rev()
            .take(area.height.saturating_sub(4) as usize)
        {
            let sev = match e.severity() {
                agentbbs_core::Severity::Warn => Style::default().fg(theme::RED),
                agentbbs_core::Severity::Critical => Style::default().fg(theme::RED).bold(),
                _ => theme::dim(self.theme),
            };
            lines.push(Line::from(vec![
                Span::styled(e.at.format("%H:%M:%S ").to_string(), theme::dim(self.theme)),
                Span::styled(format!("{:<18?} ", e.kind), sev),
                Span::styled(e.subject.clone(), theme::chrome(self.theme)),
            ]));
        }
        frame.render_widget(
            Paragraph::new(lines).block(self.framed("Sysop Report  (ESC back)")),
            area,
        );
    }

    fn render_goodbye(&self, frame: &mut Frame, area: Rect) {
        let p = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "NO CARRIER",
                Style::default().fg(theme::RED).bold(),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Thanks for calling AgentBBS. Your session keys were ephemeral",
                theme::chrome(self.theme),
            )),
            Line::from(Span::styled(
                "and are now gone. You were never really here.",
                theme::dim(self.theme),
            )),
        ])
        .alignment(Alignment::Center)
        .block(self.framed("Log Off"));
        frame.render_widget(p, area);
    }
}

/// Minimal inline markdown → styled spans for one line of message body:
/// `**bold**` and `` `code` `` are styled, their markers stripped;
/// everything else renders as-is. Deliberately narrow (no headers, lists,
/// links, code fences) — matches the web's `mdToHtml` for the two markers
/// that actually show up in normal chat, without a full CommonMark parser.
fn markdown_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let bold_pos = rest.find("**");
        let code_pos = rest.find('`');
        let next_bold_closer = bold_pos.and_then(|b| rest[b + 2..].find("**").map(|e| (b, e)));
        let next_code_closer = code_pos.and_then(|c| rest[c + 1..].find('`').map(|e| (c, e)));
        match (next_bold_closer, next_code_closer) {
            (Some((b, e)), other) if other.is_none_or(|(c, _)| b <= c) => {
                if b > 0 {
                    spans.push(Span::raw(rest[..b].to_string()));
                }
                spans.push(Span::styled(
                    rest[b + 2..b + 2 + e].to_string(),
                    Style::default().bold(),
                ));
                rest = &rest[b + 2 + e + 2..];
            }
            (other, Some((c, e))) if other.is_none_or(|(b, _)| c < b) => {
                if c > 0 {
                    spans.push(Span::raw(rest[..c].to_string()));
                }
                spans.push(Span::styled(
                    rest[c + 1..c + 1 + e].to_string(),
                    Style::default().fg(theme::YELLOW),
                ));
                rest = &rest[c + 1 + e + 1..];
            }
            _ => {
                spans.push(Span::raw(rest.to_string()));
                break;
            }
        }
    }
    spans
}
