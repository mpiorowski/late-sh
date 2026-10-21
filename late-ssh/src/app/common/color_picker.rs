//! The paint colour picker: a pure state machine over one working colour,
//! and the popup that shows it. Shared by every surface that paints a
//! `dartboard_core` canvas (the Artboard, PRINTS in the undercity).
//!
//! The picker edits a copy; the host decides what Enter does with the
//! working colour (the Artboard makes it the paint colour). Rows are the
//! three channels, a hex field, and the preset strip; the keys move between
//! rows and nudge the focused one. Key routing stays with each host: open,
//! close, apply, and anything like an eyedropper are its decisions. This
//! module draws the popup (`draw`) and says what a click on it hit (`hit`).

use dartboard_core::RgbColor;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;

/// The preset strip: sixteen colours that read on a dark board.
pub(crate) const PAINT_PALETTE: [RgbColor; 16] = [
    RgbColor::new(255, 110, 64),
    RgbColor::new(255, 236, 96),
    RgbColor::new(255, 214, 102),
    RgbColor::new(145, 226, 88),
    RgbColor::new(188, 255, 128),
    RgbColor::new(72, 220, 170),
    RgbColor::new(86, 245, 214),
    RgbColor::new(84, 196, 255),
    RgbColor::new(96, 225, 255),
    RgbColor::new(128, 163, 255),
    RgbColor::new(164, 146, 255),
    RgbColor::new(192, 132, 255),
    RgbColor::new(224, 116, 255),
    RgbColor::new(255, 124, 196),
    RgbColor::new(255, 142, 158),
    RgbColor::new(238, 242, 255),
];

/// Six hex digits make a colour; fewer is a field still being typed.
pub const HEX_LEN: usize = 6;
/// Shift+arrow step on a channel.
pub const COARSE_STEP: i16 = 16;

const POPUP_WIDTH: u16 = 54;
const POPUP_HEIGHT: u16 = 9;
const LABEL_WIDTH: u16 = 9;
const BAR_WIDTH: u16 = 24;
const PRESET_WIDTH: u16 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Channel {
    Red,
    Green,
    Blue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickerRow {
    Channel(Channel),
    Hex,
    Presets,
}

impl PickerRow {
    pub const ALL: [PickerRow; 5] = [
        PickerRow::Channel(Channel::Red),
        PickerRow::Channel(Channel::Green),
        PickerRow::Channel(Channel::Blue),
        PickerRow::Hex,
        PickerRow::Presets,
    ];

    fn index(self) -> usize {
        match self {
            PickerRow::Channel(Channel::Red) => 0,
            PickerRow::Channel(Channel::Green) => 1,
            PickerRow::Channel(Channel::Blue) => 2,
            PickerRow::Hex => 3,
            PickerRow::Presets => 4,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Edge {
    Min,
    Max,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColorPicker {
    /// The working colour; what Enter applies.
    pub color: RgbColor,
    pub row: PickerRow,
    /// The hex field as typed, up to `HEX_LEN` digits. Empty means the
    /// field mirrors `color`; six digits have already been folded into it.
    pub hex: String,
    /// The preset strip's cursor.
    pub preset: usize,
}

impl ColorPicker {
    pub fn open(color: RgbColor) -> Self {
        Self {
            color,
            row: PickerRow::Channel(Channel::Red),
            hex: String::new(),
            preset: preset_index(color).unwrap_or(0),
        }
    }

    pub fn channel_value(&self, channel: Channel) -> u8 {
        match channel {
            Channel::Red => self.color.r,
            Channel::Green => self.color.g,
            Channel::Blue => self.color.b,
        }
    }

    /// The hex field's text: the digits typed so far, or the working colour
    /// when nothing is being typed.
    pub fn hex_text(&self) -> String {
        if self.hex.is_empty() {
            hex_of(self.color)
        } else {
            self.hex.clone()
        }
    }

    /// Up / Down: the focused row, clamped at both ends.
    pub fn move_row(&mut self, delta: isize) {
        let last = PickerRow::ALL.len() as isize - 1;
        let next = (self.row.index() as isize + delta).clamp(0, last);
        self.row = PickerRow::ALL[next as usize];
    }

    /// Left / Right on the focused row: nudge the channel, or step the
    /// preset cursor (wrapping) and take that preset as the working colour.
    /// The hex row has nothing to nudge.
    pub fn adjust(&mut self, delta: i16) {
        match self.row {
            PickerRow::Channel(channel) => {
                let value = (self.channel_value(channel) as i16 + delta).clamp(0, 255) as u8;
                self.set_channel(channel, value);
            }
            PickerRow::Hex => {}
            PickerRow::Presets => {
                let len = PAINT_PALETTE.len() as isize;
                let next = (self.preset as isize + delta as isize).rem_euclid(len) as usize;
                self.select_preset(next);
            }
        }
    }

    /// Home / End on the focused row.
    pub fn jump(&mut self, edge: Edge) {
        match (self.row, edge) {
            (PickerRow::Channel(channel), Edge::Min) => self.set_channel(channel, 0),
            (PickerRow::Channel(channel), Edge::Max) => self.set_channel(channel, 255),
            (PickerRow::Hex, Edge::Min | Edge::Max) => {}
            (PickerRow::Presets, Edge::Min) => self.select_preset(0),
            (PickerRow::Presets, Edge::Max) => self.select_preset(PAINT_PALETTE.len() - 1),
        }
    }

    /// A whole colour at once (the eyedropper).
    pub fn set_color(&mut self, color: RgbColor) {
        self.color = color;
        self.hex.clear();
        self.sync_preset_cursor();
    }

    pub fn set_channel(&mut self, channel: Channel, value: u8) {
        match channel {
            Channel::Red => self.color.r = value,
            Channel::Green => self.color.g = value,
            Channel::Blue => self.color.b = value,
        }
        self.hex.clear();
        self.sync_preset_cursor();
    }

    pub fn select_preset(&mut self, index: usize) {
        self.row = PickerRow::Presets;
        self.preset = index;
        self.color = PAINT_PALETTE[index];
        self.hex.clear();
    }

    /// A typed hex digit focuses the hex field and appends; the sixth digit
    /// folds the field into the working colour. Returns false for anything
    /// that is not a hex digit, so the caller can pass the key on.
    pub fn type_hex(&mut self, ch: char) -> bool {
        if !ch.is_ascii_hexdigit() {
            return false;
        }
        if self.row != PickerRow::Hex {
            self.row = PickerRow::Hex;
            self.hex.clear();
        }
        if self.hex.len() == HEX_LEN {
            self.hex.clear();
        }
        self.hex.push(ch.to_ascii_uppercase());
        if self.hex.len() == HEX_LEN {
            match parse_hex(&self.hex) {
                Some(color) => {
                    self.color = color;
                    self.sync_preset_cursor();
                }
                None => unreachable!("six hex digits always parse"),
            }
        }
        true
    }

    /// Backspace in the hex field. Starting from a mirrored colour, the
    /// first Backspace opens the field with the colour's digits so the
    /// last one can be edited.
    pub fn hex_backspace(&mut self) {
        if self.row != PickerRow::Hex {
            return;
        }
        if self.hex.is_empty() {
            self.hex = hex_of(self.color);
        }
        self.hex.pop();
    }

    /// What a click on the popup does: a channel bar sets that channel and
    /// focuses its row, a preset is taken.
    pub fn click(&mut self, hit: Hit) {
        match hit {
            Hit::Channel(channel, value) => {
                self.row = PickerRow::Channel(channel);
                self.set_channel(channel, value);
            }
            Hit::Preset(index) => self.select_preset(index),
        }
    }

    fn sync_preset_cursor(&mut self) {
        if let Some(index) = preset_index(self.color) {
            self.preset = index;
        }
    }
}

pub fn preset_index(color: RgbColor) -> Option<usize> {
    PAINT_PALETTE
        .iter()
        .position(|candidate| *candidate == color)
}

pub fn hex_of(color: RgbColor) -> String {
    format!("{:02X}{:02X}{:02X}", color.r, color.g, color.b)
}

fn parse_hex(hex: &str) -> Option<RgbColor> {
    if hex.len() != HEX_LEN {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(RgbColor::new(r, g, b))
}

// ----- the popup -----

struct PopupLayout {
    preview: Rect,
    channels: [Rect; 3],
    hex: Rect,
    presets: Rect,
    keys: Rect,
}

/// What a click on the popup landed on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hit {
    /// A click on a channel's bar, at the value that column stands for.
    Channel(Channel, u8),
    Preset(usize),
}

/// The popup, centered over `area`.
fn popup(area: Rect) -> Rect {
    let width = POPUP_WIDTH.min(area.width);
    let height = POPUP_HEIGHT.min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn layout(popup: Rect) -> Option<PopupLayout> {
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(popup)
        .inner(Margin::new(1, 0));
    if inner.height < 7 || inner.width < LABEL_WIDTH + BAR_WIDTH + 4 {
        return None;
    }
    let rows = Layout::vertical([
        Constraint::Length(1), // preview + hex of the working colour
        Constraint::Length(1), // R
        Constraint::Length(1), // G
        Constraint::Length(1), // B
        Constraint::Length(1), // hex field
        Constraint::Length(1), // presets
        Constraint::Length(1), // keys
    ])
    .split(inner);
    Some(PopupLayout {
        preview: rows[0],
        channels: [rows[1], rows[2], rows[3]],
        hex: rows[4],
        presets: rows[5],
        keys: rows[6],
    })
}

fn channel_bar_rect(row: Rect) -> Rect {
    Rect::new(
        row.x + LABEL_WIDTH,
        row.y,
        BAR_WIDTH.min(row.width.saturating_sub(LABEL_WIDTH)),
        1,
    )
}

fn preset_rect(row: Rect, index: usize) -> Rect {
    Rect::new(
        row.x + LABEL_WIDTH + index as u16 * PRESET_WIDTH,
        row.y,
        PRESET_WIDTH,
        1,
    )
}

/// How many of the bar's columns a channel value fills.
fn channel_fill(value: u8, bar_width: u16) -> u16 {
    ((value as u32 * bar_width as u32 + 127) / 255) as u16
}

/// The channel value a click on the bar's `column` (0-based) stands for.
fn channel_value_at(column: u16, bar_width: u16) -> u8 {
    let last = bar_width.saturating_sub(1).max(1) as u32;
    ((column.min(bar_width - 1) as u32 * 255 + last / 2) / last) as u8
}

fn contains(rect: Rect, col: u16, row: u16) -> bool {
    col >= rect.x && row >= rect.y && col < rect.right() && row < rect.bottom()
}

/// What a click at `col`, `row` (0-based, frame coordinates) lands on in
/// the popup `draw` centers over `area`. The labels, the hex field, and
/// everything outside the bars and the presets are dead.
pub fn hit(area: Rect, col: u16, row: u16) -> Option<Hit> {
    let layout = layout(popup(area))?;
    for (channel, channel_row) in [Channel::Red, Channel::Green, Channel::Blue]
        .into_iter()
        .zip(layout.channels)
    {
        let bar = channel_bar_rect(channel_row);
        if contains(bar, col, row) {
            return Some(Hit::Channel(
                channel,
                channel_value_at(col - bar.x, bar.width),
            ));
        }
    }
    (0..PAINT_PALETTE.len())
        .find(|index| contains(preset_rect(layout.presets, *index), col, row))
        .map(Hit::Preset)
}

fn rgb(color: RgbColor) -> Color {
    Color::Rgb(color.r, color.g, color.b)
}

/// The popup, centered over `area`, titled `title`.
pub fn draw(frame: &mut Frame, area: Rect, picker: &ColorPicker, title: &str) {
    let popup = popup(area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(format!(" {title} "))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    frame.render_widget(block, popup);
    let Some(layout) = layout(popup) else {
        return;
    };

    let label = |text: &str, focused: bool| {
        let style = if focused {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        Span::styled(
            format!("{:<width$}", text, width = LABEL_WIDTH as usize),
            style,
        )
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("", false),
            Span::styled("   ", Style::default().bg(rgb(picker.color))),
            Span::styled(
                format!("  #{}", hex_of(picker.color)),
                Style::default()
                    .fg(rgb(picker.color))
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        layout.preview,
    );

    for (channel, channel_row) in [Channel::Red, Channel::Green, Channel::Blue]
        .into_iter()
        .zip(layout.channels)
    {
        let (name, tint) = match channel {
            Channel::Red => ("Red", Color::Rgb(255, 96, 96)),
            Channel::Green => ("Green", Color::Rgb(96, 224, 96)),
            Channel::Blue => ("Blue", Color::Rgb(96, 160, 255)),
        };
        let focused = picker.row == PickerRow::Channel(channel);
        let value = picker.channel_value(channel);
        let bar = channel_bar_rect(channel_row);
        let filled = channel_fill(value, bar.width) as usize;
        let empty = bar.width as usize - filled;
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label(name, focused),
                Span::styled("█".repeat(filled), Style::default().fg(tint)),
                Span::styled("░".repeat(empty), Style::default().fg(theme::TEXT_FAINT())),
                Span::styled(
                    format!(" {value:>3}"),
                    Style::default().fg(if focused {
                        theme::TEXT_BRIGHT()
                    } else {
                        theme::TEXT()
                    }),
                ),
            ])),
            channel_row,
        );
    }

    let hex_focused = picker.row == PickerRow::Hex;
    let hex_text = picker.hex_text();
    let mut hex_spans = vec![
        label("Hex", hex_focused),
        Span::styled("#", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            hex_text.clone(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if hex_focused {
        hex_spans.push(Span::styled(
            "_".repeat(HEX_LEN - hex_text.len()),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
        hex_spans.push(Span::styled("█", Style::default().fg(theme::AMBER())));
    }
    frame.render_widget(Paragraph::new(Line::from(hex_spans)), layout.hex);

    let presets_focused = picker.row == PickerRow::Presets;
    let mut preset_spans = vec![label("Presets", presets_focused)];
    for (index, color) in PAINT_PALETTE.iter().copied().enumerate() {
        let on_it = color == picker.color;
        let cursor = presets_focused && index == picker.preset;
        let text = match (on_it, cursor) {
            (true, _) => "•",
            (false, true) => "◦",
            (false, false) => " ",
        };
        preset_spans.push(Span::styled(
            format!("{text:<width$}", width = PRESET_WIDTH as usize),
            if on_it || cursor {
                theme::punch_through(rgb(color)).add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(rgb(color))
            },
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(preset_spans)), layout.presets);

    let keys = Line::from(vec![
        Span::styled("↑↓", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" row  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" ±1  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("⇧←→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" ±16  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("0-9 a-f", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" hex  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("⏎", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" apply  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
    ]);
    frame.render_widget(Paragraph::new(keys), layout.keys);
}

#[cfg(test)]
#[path = "color_picker_test.rs"]
mod color_picker_test;
