use ratatui::prelude::*;
use ratatui::widgets::*;
use ratatui_textarea::{CursorMove, TextArea};

#[derive(Debug, Clone)]
pub struct PinstarTheme {
    pub accent: Color,
    #[allow(dead_code)]
    pub heading: Color,
    pub success: Color,
    #[allow(dead_code)]
    pub warning: Color,
    #[allow(dead_code)]
    pub destructive: Color,
    pub muted: Color,
    pub text: Color,
    #[allow(dead_code)]
    pub fg: Color,
    pub bg: Color,
    #[allow(dead_code)]
    pub border: Color,
    #[allow(dead_code)]
    pub tag: Color,
    #[allow(dead_code)]
    pub folder: Color,
    pub highlight_fg: Color,
    pub highlight_bg: Color,
}

impl Default for PinstarTheme {
    fn default() -> Self {
        Self::current()
    }
}

impl PinstarTheme {
    pub fn current() -> Self {
        use crate::app::common::theme::*;
        Self {
            accent: BORDER_ACTIVE(),
            heading: TEXT_BRIGHT(),
            success: SUCCESS(),
            warning: AMBER(),
            destructive: ERROR(),
            muted: TEXT_DIM(),
            text: TEXT(),
            fg: TEXT_BRIGHT(),
            bg: BG_CANVAS(),
            border: BORDER(),
            tag: MENTION(),
            folder: CHAT_AUTHOR(),
            highlight_fg: BG_CANVAS(),
            highlight_bg: BORDER_ACTIVE(),
        }
    }

    pub fn bg_style(&self) -> Style {
        Style::default().bg(self.bg)
    }

    pub fn preview_bg(&self) -> Color {
        derive_color(self.bg, -15).unwrap_or(self.bg)
    }

    pub fn hint_line_bg(&self) -> Color {
        derive_color(self.bg, -8).unwrap_or(self.bg)
    }

    pub fn preview_bg_style(&self) -> Style {
        Style::default().bg(self.preview_bg())
    }

    pub fn hint_line_bg_style(&self) -> Style {
        Style::default().bg(self.hint_line_bg())
    }

    pub fn parse_color(color_code: Option<&str>, theme: &PinstarTheme) -> Color {
        match color_code {
            Some(s) if s.starts_with('#') => {
                if s.len() == 7 {
                    let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(0);
                    let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(0);
                    let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(0);
                    Color::Rgb(r, g, b)
                } else {
                    theme.accent
                }
            }
            Some("1") | Some("red") => Color::Rgb(255, 82, 82),
            Some("2") | Some("orange") => Color::Rgb(255, 152, 0),
            Some("3") | Some("yellow") => Color::Rgb(255, 235, 59),
            Some("4") | Some("green") => Color::Rgb(76, 175, 80),
            Some("5") | Some("cyan") => Color::Rgb(0, 188, 212),
            Some("6") | Some("purple") => Color::Rgb(156, 39, 176),
            _ => theme.accent,
        }
    }
}

fn derive_color(base: Color, delta: i16) -> Option<Color> {
    match base {
        Color::Rgb(r, g, b) => {
            let clamp = |v: i16| v.clamp(0, 255) as u8;
            Some(Color::Rgb(
                clamp(r as i16 + delta),
                clamp(g as i16 + delta),
                clamp(b as i16 + delta),
            ))
        }
        other => Some(other),
    }
}

pub fn contains_cell(rect: Rect, col: u16, row: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && col >= rect.x
        && col < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

pub fn clamped_context_menu_rect(x: u16, y: u16, width: u16, height: u16, bounds: Rect) -> Rect {
    let width = width.min(bounds.width);
    let height = height.min(bounds.height);
    let max_x = bounds.right().saturating_sub(width);
    let max_y = bounds.bottom().saturating_sub(height);
    Rect::new(
        x.max(bounds.x).min(max_x),
        y.max(bounds.y).min(max_y),
        width,
        height,
    )
}

pub fn move_textarea_cursor_to_mouse(
    textarea: &mut TextArea,
    body_inner: Rect,
    mouse_col: u16,
    mouse_row: u16,
) {
    if textarea.lines().is_empty() || body_inner.width == 0 || body_inner.height == 0 {
        return;
    }

    let (scroll_row, scroll_col) = get_textarea_scroll(textarea);

    let row = mouse_row.saturating_sub(body_inner.y) as usize + scroll_row;
    let col = mouse_col.saturating_sub(body_inner.x) as usize + scroll_col;

    let max_row = textarea.lines().len().saturating_sub(1);
    let target_row = row.min(max_row);
    let max_col = textarea.lines()[target_row].chars().count();
    let target_col = col.min(max_col);

    textarea.move_cursor(CursorMove::Jump(target_row as u16, target_col as u16));
}

pub fn get_textarea_scroll(textarea: &TextArea) -> (usize, usize) {
    let mut scroll_row = 0;
    let mut scroll_col = 0;

    let debug_str = format!("{textarea:?}");
    if let Some(start) = debug_str.find("viewport: Viewport(") {
        let after_start = &debug_str[start + "viewport: Viewport(".len()..];
        if let Some(end) = after_start.find(')') {
            let number_str = &after_start[..end];
            if let Ok(number) = number_str.parse::<u64>() {
                scroll_row = ((number >> 16) & 0xFFFF) as usize;
                scroll_col = (number & 0xFFFF) as usize;
            }
        }
    }
    (scroll_row, scroll_col)
}

pub fn line_number_gutter(
    line_count: usize,
    cursor_row: usize,
    scroll_row: usize,
    height: u16,
    theme: &PinstarTheme,
    top_padding: u16,
) -> Paragraph<'static> {
    let digits = line_count.max(1).to_string().len();
    let display_lines = height as usize;
    let mut gutter_lines: Vec<Line<'static>> = Vec::with_capacity(display_lines);
    for i in 0..display_lines.min(line_count.saturating_sub(scroll_row)) {
        let current_line_idx = i + scroll_row;
        let is_current = current_line_idx == cursor_row;
        let style = if is_current {
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        gutter_lines.push(Line::from(vec![Span::styled(
            format!("{:>width$} ", current_line_idx + 1, width = digits),
            style,
        )]));
    }
    for _ in gutter_lines.len()..display_lines {
        gutter_lines.push(Line::from(Span::raw(" ")));
    }
    Paragraph::new(gutter_lines)
        .style(theme.preview_bg_style())
        .block(
            Block::default()
                .padding(Padding::new(0, 0, top_padding, 0))
                .style(theme.preview_bg_style()),
        )
}

pub fn fill_cursor_line_bg(frame: &mut Frame, editor: &TextArea, area: Rect, bg: Color) {
    if editor.selection_range().is_some() {
        return;
    }
    let (scroll_row, _) = get_textarea_scroll(editor);
    let cursor_row = editor.cursor().0;
    let screen_row = cursor_row.saturating_sub(scroll_row) as u16;
    let inner_y = editor.block().map(|b| b.inner(area).y).unwrap_or(area.y);
    let y = inner_y + screen_row;
    if y < area.y || y >= area.bottom() {
        return;
    }
    let buf = frame.buffer_mut();
    for x in area.left()..area.right() {
        if let Some(cell) = buf.cell_mut((x, y)) {
            cell.set_bg(bg);
        }
    }
}

pub fn get_menu_shortcut_char(
    menu_type: crate::app::pinstar::state::PinstarMenuType,
    label: &str,
) -> Option<char> {
    use crate::app::pinstar::state::PinstarMenuType;
    match menu_type {
        PinstarMenuType::Canvas => match label {
            "Create Connection" => Some('c'),
            "Delete Connection" => Some('d'),
            "Rename Node" => Some('r'),
            "Resize Node" => Some('s'),
            "Set Shape..." => Some('p'),
            "Set Border..." => Some('b'),
            "Set Color..." => Some('o'),
            "Delete All Connections" => Some('u'),
            "Delete Node" => Some('x'),
            "Add Text Node" => Some('t'),
            "Add Group" => Some('g'),
            _ => None,
        },
        PinstarMenuType::EdgeMenu => match label {
            "Set Color..." => Some('c'),
            "Set Style..." => Some('s'),
            "Delete Edge" => Some('d'),
            _ => None,
        },
        PinstarMenuType::ShapePicker => match label {
            "Rectangle" => Some('r'),
            "Diamond" => Some('d'),
            "Circle" => Some('c'),
            "Cylinder" => Some('y'),
            "Stadium" => Some('s'),
            "Remove Shape" => Some('x'),
            _ => None,
        },
        PinstarMenuType::BorderPicker => match label {
            "Plain" => Some('p'),
            "Rounded" => Some('r'),
            "Double" => Some('d'),
            "Thick" => Some('t'),
            "Dashed" => Some('s'),
            "Remove Border" => Some('x'),
            _ => None,
        },
        PinstarMenuType::ColorPicker | PinstarMenuType::EdgeColorPicker => match label {
            "Default" => Some('d'),
            "Red" => Some('r'),
            "Green" => Some('g'),
            "Yellow" => Some('y'),
            "Blue" => Some('b'),
            "Cyan" => Some('c'),
            "Purple" => Some('p'),
            "Orange" => Some('o'),
            _ => None,
        },
        PinstarMenuType::EdgeStylePicker => match label {
            "Solid" => Some('s'),
            "Dashed" => Some('d'),
            _ => None,
        },
        PinstarMenuType::OrientationPicker => match label {
            "Top-Down" => Some('t'),
            "Left-Right" => Some('l'),
            "Right-Left" => Some('r'),
            "Bottom-Up" => Some('b'),
            _ => None,
        },
        _ => None,
    }
}
