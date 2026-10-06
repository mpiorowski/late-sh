//! The paint colour picker: a pure state machine over one working colour.
//!
//! The picker edits a copy; `State::apply_color_picker` is what makes the
//! working colour the paint colour. Rows are the three channels, a hex
//! field, and the preset strip; the keys move between rows and nudge the
//! focused one. Layout, drawing, and hit tests live in `ui.rs`.

use dartboard_core::RgbColor;

use super::state::PAINT_PALETTE;

/// Six hex digits make a colour; fewer is a field still being typed.
pub const HEX_LEN: usize = 6;
/// Shift+arrow step on a channel.
pub const COARSE_STEP: i16 = 16;

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

#[cfg(test)]
#[path = "color_picker_test.rs"]
mod color_picker_test;
