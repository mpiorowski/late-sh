use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    common::theme,
    input::{ParsedInput, sanitize_paste_markers},
    rooms::{
        backend::{CreateModalAction, CreateRoomModal},
        sshattrick::svc::{SSHATTRICK_WIN_CHIP_PAYOUT, SSHATTRICK_WIN_PAYOUT_COOLDOWN},
    },
};

const DISPLAY_NAME_MAX_LEN: usize = 48;
const MODAL_WIDTH: u16 = 60;
const MODAL_HEIGHT: u16 = 12;

pub struct SshattrickCreateModal {
    display_name: String,
    error: Option<String>,
}

impl SshattrickCreateModal {
    pub fn new(default_name: impl Into<String>) -> Self {
        Self {
            display_name: default_name.into(),
            error: None,
        }
    }

    fn push_name_char(&mut self, ch: char) {
        if ch.is_control() || self.display_name.chars().count() >= DISPLAY_NAME_MAX_LEN {
            return;
        }
        self.error = None;
        self.display_name.push(ch);
    }

    fn submit(&mut self) -> CreateModalAction {
        let display_name = self.display_name.trim().to_string();
        if display_name.is_empty() {
            self.error = Some("Room name is required.".to_string());
            return CreateModalAction::Continue;
        }
        CreateModalAction::Submit {
            display_name,
            settings: serde_json::json!({}),
        }
    }
}

impl CreateRoomModal for SshattrickCreateModal {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let modal_area = centered_rect(
            area,
            MODAL_WIDTH.min(area.width),
            MODAL_HEIGHT.min(area.height),
        );
        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(" New ssHattrick Room ")
            .title_style(
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
        let inner = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let layout = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "Score the most goals in 90 seconds. Win {SSHATTRICK_WIN_CHIP_PAYOUT} chips every {} min.",
                        SSHATTRICK_WIN_PAYOUT_COOLDOWN.as_secs() / 60
                    ),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
            ])),
            layout[1],
        );

        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled("Name: ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(
                    format!("{}█", self.display_name),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
            ])),
            layout[3],
        );

        let footer = self
            .error
            .as_ref()
            .map(|message| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(message.clone(), Style::default().fg(theme::ERROR())),
                ])
            })
            .unwrap_or_else(|| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
                    Span::styled(" create  ", Style::default().fg(theme::TEXT_DIM())),
                    Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
                    Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
                ])
            });
        frame.render_widget(Paragraph::new(footer), layout[5]);
    }

    fn handle_event(&mut self, event: &ParsedInput) -> CreateModalAction {
        match event {
            ParsedInput::Byte(0x1B) => CreateModalAction::Cancel,
            ParsedInput::Byte(b'\r' | b'\n') => self.submit(),
            ParsedInput::Byte(0x08 | 0x7F) => {
                self.error = None;
                self.display_name.pop();
                CreateModalAction::Continue
            }
            ParsedInput::Byte(0x17) => {
                self.error = None;
                self.display_name.clear();
                CreateModalAction::Continue
            }
            ParsedInput::Char(ch) => {
                self.push_name_char(*ch);
                CreateModalAction::Continue
            }
            ParsedInput::Byte(byte) => {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    self.push_name_char(*byte as char);
                }
                CreateModalAction::Continue
            }
            ParsedInput::Paste(bytes) => {
                let pasted = String::from_utf8_lossy(bytes);
                for ch in sanitize_paste_markers(&pasted).chars() {
                    self.push_name_char(ch);
                }
                CreateModalAction::Continue
            }
            _ => CreateModalAction::Continue,
        }
    }
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
