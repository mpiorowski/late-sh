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
        blackjack::settings::{BlackjackTableSettings, PACE_OPTIONS, STAKE_OPTIONS},
    },
};

const DISPLAY_NAME_MAX_LEN: usize = 48;
const MODAL_WIDTH: u16 = 64;
const MODAL_HEIGHT: u16 = 14;
const LABEL_WIDTH: usize = 10;
const FIELD_NAME: usize = 0;
const FIELD_PACE: usize = 1;
const FIELD_STAKE: usize = 2;
const FIELD_COUNT: usize = 3;

pub struct BlackjackCreateModal {
    display_name: String,
    focus_index: usize,
    pace_index: usize,
    stake_index: usize,
    error: Option<String>,
}

impl BlackjackCreateModal {
    pub fn new(default_name: impl Into<String>) -> Self {
        Self {
            display_name: default_name.into(),
            focus_index: FIELD_NAME,
            pace_index: 1,
            stake_index: 0,
            error: None,
        }
    }

    fn move_focus(&mut self, delta: isize) {
        self.focus_index = cycle_index(self.focus_index, FIELD_COUNT, delta);
    }

    fn adjust_selection(&mut self, delta: isize) {
        match self.focus_index {
            FIELD_PACE => {
                self.pace_index = cycle_index(self.pace_index, PACE_OPTIONS.len(), delta);
            }
            FIELD_STAKE => {
                self.stake_index = cycle_index(self.stake_index, STAKE_OPTIONS.len(), delta);
            }
            _ => {}
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
            self.error = Some("Table name is required.".to_string());
            self.focus_index = FIELD_NAME;
            return CreateModalAction::Continue;
        }

        let settings = BlackjackTableSettings {
            pace: PACE_OPTIONS
                .get(self.pace_index)
                .copied()
                .unwrap_or_default(),
            stake: STAKE_OPTIONS
                .get(self.stake_index)
                .copied()
                .unwrap_or(STAKE_OPTIONS[0]),
        }
        .normalized()
        .to_json();

        CreateModalAction::Submit {
            display_name,
            settings,
        }
    }
}

impl CreateRoomModal for BlackjackCreateModal {
    fn draw(&self, frame: &mut Frame, area: Rect) {
        let modal_area = centered_rect(
            area,
            MODAL_WIDTH.min(area.width),
            MODAL_HEIGHT.min(area.height),
        );
        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .title(" Blackjack Room ")
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
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(inner);

        frame.render_widget(Paragraph::new(section_heading("Table")), layout[1]);
        frame.render_widget(
            Paragraph::new(name_row(
                self.focus_index == FIELD_NAME,
                &self.display_name,
                inner.width as usize,
            )),
            layout[2],
        );
        frame.render_widget(Paragraph::new(section_heading("Options")), layout[4]);
        frame.render_widget(
            Paragraph::new(option_row(
                self.focus_index == FIELD_PACE,
                "Pace",
                PACE_OPTIONS.iter().map(|pace| pace.label()).collect(),
                self.pace_index,
                inner.width as usize,
            )),
            layout[5],
        );
        frame.render_widget(
            Paragraph::new(option_row(
                self.focus_index == FIELD_STAKE,
                "Stake",
                STAKE_OPTIONS
                    .iter()
                    .map(|stake| format!("{stake}"))
                    .collect(),
                self.stake_index,
                inner.width as usize,
            )),
            layout[6],
        );

        let footer = self
            .error
            .as_ref()
            .map(|message| {
                Line::from(Span::styled(
                    message.clone(),
                    Style::default().fg(theme::ERROR()),
                ))
            })
            .unwrap_or_else(footer_line);
        frame.render_widget(Paragraph::new(footer), layout[8]);
    }

    fn handle_event(&mut self, event: &ParsedInput) -> CreateModalAction {
        match event {
            ParsedInput::Byte(0x1B) => CreateModalAction::Cancel,
            ParsedInput::Byte(b'\r' | b'\n') => self.submit(),
            ParsedInput::Byte(b'\t') | ParsedInput::Arrow(b'B') => {
                self.move_focus(1);
                CreateModalAction::Continue
            }
            ParsedInput::BackTab | ParsedInput::Arrow(b'A') => {
                self.move_focus(-1);
                CreateModalAction::Continue
            }
            ParsedInput::Arrow(b'D') => {
                self.adjust_selection(-1);
                CreateModalAction::Continue
            }
            ParsedInput::Arrow(b'C') => {
                self.adjust_selection(1);
                CreateModalAction::Continue
            }
            ParsedInput::Char('a' | 'A') if self.focus_index != FIELD_NAME => {
                self.adjust_selection(-1);
                CreateModalAction::Continue
            }
            ParsedInput::Char('d' | 'D') if self.focus_index != FIELD_NAME => {
                self.adjust_selection(1);
                CreateModalAction::Continue
            }
            ParsedInput::Byte(0x08 | 0x7F) if self.focus_index == FIELD_NAME => {
                self.error = None;
                self.display_name.pop();
                CreateModalAction::Continue
            }
            ParsedInput::Byte(0x17) if self.focus_index == FIELD_NAME => {
                self.error = None;
                self.display_name.clear();
                CreateModalAction::Continue
            }
            ParsedInput::Char(ch) if self.focus_index == FIELD_NAME => {
                self.push_name_char(*ch);
                CreateModalAction::Continue
            }
            ParsedInput::Byte(byte) if self.focus_index == FIELD_NAME => {
                if byte.is_ascii_graphic() || *byte == b' ' {
                    self.push_name_char(*byte as char);
                }
                CreateModalAction::Continue
            }
            ParsedInput::Paste(bytes) if self.focus_index == FIELD_NAME => {
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

fn section_heading(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  -- ", Style::default().fg(theme::BORDER())),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" --", Style::default().fg(theme::BORDER())),
    ])
}

fn name_row(focused: bool, value: &str, width: usize) -> Line<'static> {
    let text = if focused {
        format!("{value}█")
    } else if value.trim().is_empty() {
        "not set".to_string()
    } else {
        value.to_string()
    };
    row_with_value(focused, "Name", text, width)
}

fn option_row(
    focused: bool,
    label: &str,
    options: Vec<impl Into<String>>,
    selected_index: usize,
    width: usize,
) -> Line<'static> {
    let mut value = String::new();
    for (index, option) in options.into_iter().enumerate() {
        if index > 0 {
            value.push_str("  ");
        }
        let option = option.into();
        if index == selected_index {
            value.push('[');
            value.push_str(&option);
            value.push(']');
        } else {
            value.push_str(&option);
        }
    }
    row_with_value(focused, label, value, width)
}

fn row_with_value(focused: bool, label: &str, value: String, width: usize) -> Line<'static> {
    let style = if focused {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let marker = if focused { "›" } else { " " };
    let label = format!("{:<LABEL_WIDTH$}", label);
    let used = 3 + label.chars().count() + value.chars().count();
    let padding = width.saturating_sub(used);
    Line::from(vec![
        Span::styled(format!(" {marker} "), Style::default().fg(theme::AMBER())),
        Span::styled(label, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(value, style),
        Span::raw(" ".repeat(padding)),
    ])
}

fn footer_line() -> Line<'static> {
    Line::from(vec![
        Span::styled("Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" field  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("a/d", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" create  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn cycle_index(index: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).rem_euclid(len as isize) as usize
}
