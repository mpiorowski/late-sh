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
const MODAL_HEIGHT: u16 = 16;
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
            .title(" New Blackjack Room ")
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
            Constraint::Length(1), // breathing
            Constraint::Length(1), // Table heading
            Constraint::Length(1), // breathing
            Constraint::Length(1), // name row
            Constraint::Length(1), // breathing
            Constraint::Length(1), // Options heading
            Constraint::Length(1), // breathing
            Constraint::Length(1), // pace row
            Constraint::Length(1), // stake row
            Constraint::Min(0),    // flex
            Constraint::Length(1), // footer
        ])
        .split(inner);

        let width = inner.width as usize;

        frame.render_widget(Paragraph::new(section_heading("Table")), layout[1]);
        frame.render_widget(
            Paragraph::new(field_row(
                self.focus_index == FIELD_NAME,
                "Name",
                name_value_span(self.focus_index == FIELD_NAME, &self.display_name),
                width,
            )),
            layout[3],
        );

        frame.render_widget(Paragraph::new(section_heading("Options")), layout[5]);
        frame.render_widget(
            Paragraph::new(field_row(
                self.focus_index == FIELD_PACE,
                "Pace",
                option_value_span(
                    PACE_OPTIONS.iter().map(|pace| pace.label().to_string()),
                    self.pace_index,
                ),
                width,
            )),
            layout[7],
        );
        frame.render_widget(
            Paragraph::new(field_row(
                self.focus_index == FIELD_STAKE,
                "Stake",
                option_value_span(
                    STAKE_OPTIONS.iter().map(|stake| format!("{stake}")),
                    self.stake_index,
                ),
                width,
            )),
            layout[8],
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
            .unwrap_or_else(footer_line);
        frame.render_widget(Paragraph::new(footer), layout[10]);
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
            ParsedInput::Char('j' | 'J') if self.focus_index != FIELD_NAME => {
                self.move_focus(1);
                CreateModalAction::Continue
            }
            ParsedInput::Char('k' | 'K') if self.focus_index != FIELD_NAME => {
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
            ParsedInput::Char('h' | 'H') if self.focus_index != FIELD_NAME => {
                self.adjust_selection(-1);
                CreateModalAction::Continue
            }
            ParsedInput::Char('l' | 'L') if self.focus_index != FIELD_NAME => {
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

fn name_value_span(focused: bool, value: &str) -> ValueSpan {
    if focused {
        ValueSpan {
            text: format!("{value}█"),
            style: Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        }
    } else if value.trim().is_empty() {
        ValueSpan {
            text: "not set".to_string(),
            style: Style::default().fg(theme::TEXT_FAINT()),
        }
    } else {
        ValueSpan {
            text: value.to_string(),
            style: Style::default().fg(theme::TEXT_BRIGHT()),
        }
    }
}

fn option_value_span<I, S>(options: I, selected_index: usize) -> ValueSpan
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut text = String::new();
    for (index, option) in options.into_iter().enumerate() {
        if index > 0 {
            text.push_str("   ");
        }
        let option = option.into();
        if index == selected_index {
            text.push('[');
            text.push_str(&option);
            text.push(']');
        } else {
            text.push(' ');
            text.push_str(&option);
            text.push(' ');
        }
    }
    ValueSpan {
        text,
        style: Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    }
}

fn footer_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled("Tab ↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" field  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cycle  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" create  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ])
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
        Span::styled("  ── ", Style::default().fg(theme::BORDER())),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ──", Style::default().fg(theme::BORDER())),
    ])
}

struct ValueSpan {
    text: String,
    style: Style,
}

fn field_row(focused: bool, label: &str, value: ValueSpan, width: usize) -> Line<'static> {
    let marker = if focused { "›" } else { " " };
    let prefix_style = if focused {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if focused {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if focused {
        value.style.bg(theme::BG_SELECTION())
    } else {
        value.style
    };
    let trailing_style = if focused {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let label_text = format!("{label:<LABEL_WIDTH$}");
    let used = prefix.chars().count() + label_text.chars().count() + value.text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value.text, value_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn cycle_index(index: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    (index as isize + delta).rem_euclid(len as isize) as usize
}
