//! Rendering — draws the current [`App`] screen in retro BBS style.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use agentbbs_core::caps::Caps;

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
            Screen::Goodbye => self.render_goodbye(frame, rows[1]),
        }
        self.render_status(frame, rows[2]);
    }

    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let line = Line::from(vec![
            Span::styled(" AgentBBS ", theme::title()),
            Span::raw(" "),
            Span::styled(
                format!("node {}", agentbbs_core::PROTOCOL_VERSION),
                theme::dim(),
            ),
            Span::raw("  "),
            Span::styled(format!("you: {}", self.session.handle), theme::chrome()),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let p = Paragraph::new(Line::from(vec![
            Span::styled(" ▸ ", Style::default().fg(theme::MAGENTA)),
            Span::styled(&self.status, theme::dim()),
        ]));
        frame.render_widget(p, area);
    }

    fn framed(&self, title: &str) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme::chrome())
            .title(Span::styled(format!(" {title} "), theme::title()))
    }

    fn render_splash(&self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = vec![Line::from("")];
        for b in theme::BANNER {
            lines.push(Line::from(Span::styled(
                *b,
                Style::default().fg(theme::MAGENTA).bold(),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "The first BBS made for agents and human collaboration.",
            theme::chrome(),
        )));
        lines.push(Line::from(Span::styled(
            "Anonymous · signed · federated · WASM-extensible",
            theme::dim(),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "CONNECT 57600/ARQ/V.90  —  press ENTER to log on",
            Style::default().fg(theme::GREEN),
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
                    Span::styled(format!("[{hot}] "), theme::hotkey()),
                    Span::styled(*label, theme::chrome()),
                ]);
                let style = if selected { theme::lightbar() } else { Style::default() };
                ListItem::new(line).style(style)
            })
            .collect();
        frame.render_widget(List::new(items).block(self.framed("Main Menu")), cols[0]);

        let boards = self.boards.len();
        let msgs = self.bbs.store().message_count().unwrap_or(0);
        let info = vec![
            Line::from(Span::styled("SYSTEM STATUS", theme::hotkey())),
            Line::from(""),
            Line::from(format!("Message bases ...... {boards}")),
            Line::from(format!("Messages on file ... {msgs}")),
            Line::from(format!("Your access ........ {:#?}", self.session.caps)),
            Line::from(""),
            Line::from(Span::styled("Your anonymous id:", theme::dim())),
            Line::from(Span::styled(
                self.session.identity.id().to_hex(),
                Style::default().fg(theme::GREEN),
            )),
        ];
        frame.render_widget(
            Paragraph::new(info).wrap(Wrap { trim: true }).block(self.framed("Status")),
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
                let line = Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(format!("{:<14}", b.slug), theme::hotkey()),
                    Span::styled(b.title.clone(), theme::chrome()),
                    Span::styled(format!("{lock}{fed}"), theme::dim()),
                ]);
                let style = if selected { theme::lightbar() } else { Style::default() };
                ListItem::new(line).style(style)
            })
            .collect();
        frame.render_widget(
            List::new(items).block(self.framed("Message Bases  (ENTER read · ESC back)")),
            area,
        );
    }

    fn render_read(&self, frame: &mut Frame, area: Rect) {
        let title = self
            .current_board
            .clone()
            .unwrap_or_else(|| "board".into());
        if self.messages.is_empty() {
            let p = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("No messages yet on this base.", theme::dim())),
                Line::from(Span::styled("Press [P] to post the first one.", theme::chrome())),
            ])
            .alignment(Alignment::Center)
            .block(self.framed(&format!("{title}  (P post · ESC back)")));
            frame.render_widget(p, area);
            return;
        }
        let mut lines: Vec<Line> = Vec::new();
        for (i, m) in self.messages.iter().enumerate() {
            let marker = if i == self.read_index { "▶" } else { " " };
            let verified = if m.verify().is_ok() { "✓sig" } else { "✗SIG" };
            lines.push(Line::from(vec![
                Span::styled(format!("{marker} #{} ", i + 1), theme::hotkey()),
                Span::styled(
                    m.body.created_at.format("%Y-%m-%d %H:%M").to_string(),
                    theme::dim(),
                ),
                Span::raw("  "),
                Span::styled(
                    if m.body.handle.is_empty() {
                        m.body.author.short()
                    } else {
                        m.body.handle.clone()
                    },
                    theme::chrome(),
                ),
                Span::raw("  "),
                Span::styled(verified, Style::default().fg(theme::GREEN)),
            ]));
            lines.push(Line::from(Span::styled(
                format!("   {}", m.body.subject),
                Style::default().fg(theme::YELLOW),
            )));
            for bl in m.body.body.lines() {
                lines.push(Line::from(format!("   {bl}")));
            }
            lines.push(Line::from(""));
        }
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .block(self.framed(&format!("{title}  (P post · ↑↓ scroll · ESC back)"))),
            area,
        );
    }

    fn render_compose(&self, frame: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(3)])
            .split(area);

        let subj_focus = self.compose_field == ComposeField::Subject;
        let subject = Paragraph::new(Line::from(format!(
            "{}{}",
            self.compose_subject,
            if subj_focus { "█" } else { "" }
        )))
        .block(
            self.framed("Subject")
                .border_style(if subj_focus { theme::lightbar() } else { theme::chrome() }),
        );
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
                .border_style(if body_focus { theme::lightbar() } else { theme::chrome() }),
        );
        frame.render_widget(body, rows[1]);
    }

    fn render_who(&self, frame: &mut Frame, area: Rect) {
        let now = self.now_ms();
        let online = self.presence.online(now);
        let mut lines = vec![
            Line::from(Span::styled(
                "NODE  WHO                                KIND    IDLE",
                theme::hotkey(),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for (i, m) in online.iter().enumerate() {
            let me = m.id == self.session.identity.id();
            let idle = now.saturating_sub(m.last_seen_ms) / 1000;
            let kind = if m.agent { "agent" } else { "human" };
            let style = if me { theme::chrome() } else { theme::dim() };
            lines.push(Line::from(vec![
                Span::styled(format!("{:>4}  ", i + 1), theme::hotkey()),
                Span::styled(
                    format!("{:<34}", if me { format!("{} (you)", m.handle) } else { m.handle.clone() }),
                    style,
                ),
                Span::styled(format!("{kind:<7} "), theme::chrome()),
                Span::styled(format!("{:02}:{:02}", idle / 60, idle % 60), theme::dim()),
            ]));
        }
        if online.is_empty() {
            lines.push(Line::from(Span::styled("  (nobody online)", theme::dim())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!(
                "{} online · agents join via SSH, MCP, or federation. ESC to return.",
                online.len()
            ),
            theme::chrome(),
        )));
        frame.render_widget(Paragraph::new(lines).block(self.framed("Who's Online")), area);
    }

    fn render_market(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(Span::styled(
                "SKU            KIND        TITLE                          PRICE",
                theme::hotkey(),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        for l in self.market.all() {
            let price = if l.body.price == 0 {
                "free".to_string()
            } else {
                format!("{} cr", l.body.price)
            };
            let sig = if l.verify().is_ok() { "✓" } else { "✗" };
            lines.push(Line::from(vec![
                Span::styled(format!("{:<14} ", l.body.sku), theme::hotkey()),
                Span::styled(format!("{:<11} ", format!("{:?}", l.body.kind).to_lowercase()), theme::dim()),
                Span::styled(format!("{:<30} ", l.body.title), theme::chrome()),
                Span::styled(format!("{price:<6} "), Style::default().fg(theme::GREEN)),
                Span::styled(sig, Style::default().fg(theme::GREEN)),
            ]));
            lines.push(Line::from(Span::styled(
                format!("   {}", l.body.description),
                theme::dim(),
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "Every listing is signed by its seller and verifies on each node. ESC to return.",
            theme::chrome(),
        )));
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .block(self.framed("Marketplace")),
            area,
        );
    }

    fn render_doors(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled("DOOR GAMES & AGENT TOOLS", theme::hotkey())),
            Line::from(""),
            Line::from("  [1] WASM Plugins ......... sandboxed agent tools (wasmi host)"),
            Line::from("  [2] Marketplace .......... trade plugins, agents, boards"),
            Line::from("  [3] MCP Bridge ........... expose boards to Claude Code & co."),
            Line::from("  [4] Memory Lane .......... RVF vector recall of past threads"),
            Line::from(""),
            Line::from(Span::styled(
                "Doors run as capability-scoped WASM modules. ESC to return.",
                theme::dim(),
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
                    Span::styled(format!("{:<11}", b.id.0), theme::hotkey()),
                    Span::styled(b.name.clone(), theme::chrome()),
                ]);
                let style = if selected { theme::lightbar() } else { Style::default() };
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
            lines.push(Line::from(Span::styled(b.name.clone(), theme::hotkey())));
            lines.push(Line::from(Span::styled(b.description.clone(), theme::dim())));
            lines.push(Line::from(Span::styled(
                format!("harness: {}", b.harness),
                theme::dim(),
            )));
            lines.push(Line::from(""));
            if b.id.0 == agentbbs_arena::RETORT_BENCHMARK_ID {
                // The Retort track ranks agent+harness+model *stacks* by their
                // position on the accuracy-vs-cost PARETO FRONTIER (not raw
                // accuracy): frontier first, then accuracy within tier.
                lines.push(Line::from(Span::styled(
                    " #  PARETO  STACK (model · harness · lang)        COV    COST",
                    theme::hotkey(),
                )));
                lines.push(Line::from(
                    "──────────────────────────────────────────────────────────────────",
                ));
                let board = self.arena.retort_leaderboard();
                if board.is_empty() {
                    lines.push(Line::from(Span::styled(
                        "No retort results ingested — `agentbbs arena retort <results.json>`.",
                        theme::dim(),
                    )));
                }
                for s in board.iter().take(12) {
                    let mark = if s.pareto_optimal { "◆ front" } else { "✗ domin" };
                    let mark_style = if s.pareto_optimal {
                        Style::default().fg(theme::GREEN)
                    } else {
                        theme::dim()
                    };
                    let name = if s.is_baseline {
                        format!("{} [base]", s.stack)
                    } else {
                        s.stack.clone()
                    };
                    lines.push(Line::from(vec![
                        Span::styled(format!("{:>2}  ", s.rank), theme::hotkey()),
                        Span::styled(format!("{mark:<7} "), mark_style),
                        Span::styled(format!("{name:<34}"), theme::chrome()),
                        Span::styled(
                            format!("{:>5.1}% ", s.requirement_coverage * 100.0),
                            Style::default().fg(theme::GREEN),
                        ),
                        Span::styled(format!("${:.3}", s.cost_usd), theme::dim()),
                    ]));
                    // The cost-lever insight line.
                    lines.push(Line::from(Span::styled(
                        format!("        💡 {}", s.insight),
                        theme::dim(),
                    )));
                    if s.excluded_tooling > 0 {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "        (excluded {} TOOLING false-fail(s) — honest scoring)",
                                s.excluded_tooling
                            ),
                            theme::dim(),
                        )));
                    }
                }
                // The frontier curve (non-dominated set), cheapest first.
                let front = agentbbs_arena::frontier(&board);
                if !front.is_empty() {
                    lines.push(Line::from(""));
                    lines.push(Line::from(Span::styled(
                        "FRONTIER  ($/task ↑ · coverage)",
                        theme::hotkey(),
                    )));
                    for s in &front {
                        lines.push(Line::from(Span::styled(
                            format!(
                                "  ${:<7.3} {:>5.1}%  {}",
                                s.cost_usd,
                                s.requirement_coverage * 100.0,
                                s.stack
                            ),
                            theme::dim(),
                        )));
                    }
                }
            } else {
                lines.push(Line::from(Span::styled(
                    "RANK  AGENT                       SCORE   PASS",
                    theme::hotkey(),
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
                                Span::styled(format!(" {:>2} {medal} ", s.rank), theme::hotkey()),
                                Span::styled(format!("{:<26}", s.handle), theme::chrome()),
                                Span::styled(
                                    format!("{:>5.1}%  ", s.best_score * 100.0),
                                    Style::default().fg(theme::GREEN),
                                ),
                                Span::styled(format!("{}/{}", s.passed, s.total), theme::dim()),
                            ]));
                        }
                    }
                    _ => lines.push(Line::from(Span::styled(
                        "No submissions yet — `agentbbs` agents: compete!",
                        theme::dim(),
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
        let lines = vec![
            Line::from(Span::styled("ZERO-TRUST FEDERATION", theme::hotkey())),
            Line::from(""),
            Line::from("  Identity ........ ed25519 (anonymous, per-node)"),
            Line::from("  Transport ....... signed envelopes, PII-stripped egress"),
            Line::from("  Peering ......... npx ruflo federation join <addr>"),
            Line::from("  Memory .......... npm AgentDB / RVF vector sync"),
            Line::from(""),
            Line::from(Span::styled("PEERS", theme::hotkey())),
            Line::from(Span::styled("  (no peers linked — this is a leaf node)", theme::dim())),
            Line::from(""),
            Line::from(Span::styled("ESC to return.", theme::chrome())),
        ];
        frame.render_widget(Paragraph::new(lines).block(self.framed("Federation Hall")), area);
    }

    fn render_sysop(&self, frame: &mut Frame, area: Rect) {
        let events = self.reporter.snapshot();
        let mut lines = vec![
            Line::from(Span::styled(
                format!("LIVE EVENT LOG  ({} retained)", events.len()),
                theme::hotkey(),
            )),
            Line::from("──────────────────────────────────────────────────────────"),
        ];
        if !self.session.caps.contains(Caps::SYSOP) {
            lines.push(Line::from(Span::styled(
                "Read-only view — SYSOP capability required for actions.",
                theme::dim(),
            )));
        }
        for e in events.iter().rev().take(area.height.saturating_sub(4) as usize) {
            let sev = match e.severity() {
                agentbbs_core::Severity::Warn => Style::default().fg(theme::RED),
                agentbbs_core::Severity::Critical => Style::default().fg(theme::RED).bold(),
                _ => theme::dim(),
            };
            lines.push(Line::from(vec![
                Span::styled(e.at.format("%H:%M:%S ").to_string(), theme::dim()),
                Span::styled(format!("{:<18?} ", e.kind), sev),
                Span::styled(e.subject.clone(), theme::chrome()),
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
            Line::from(Span::styled("NO CARRIER", Style::default().fg(theme::RED).bold())),
            Line::from(""),
            Line::from(Span::styled(
                "Thanks for calling AgentBBS. Your session keys were ephemeral",
                theme::chrome(),
            )),
            Line::from(Span::styled(
                "and are now gone. You were never really here.",
                theme::dim(),
            )),
        ])
        .alignment(Alignment::Center)
        .block(self.framed("Log Off"));
        frame.render_widget(p, area);
    }
}
