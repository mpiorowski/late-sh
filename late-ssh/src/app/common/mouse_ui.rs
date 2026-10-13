//! Rendering helpers for mouse surfaces. Drawing and hit geometry share one
//! buffer, wrapped lines included.
use super::{mouse::MouseState, theme};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};

pub(crate) struct Surface<'a> {
    pub(crate) buffer: &'a mut Buffer,
}
impl Surface<'_> {
    pub(crate) fn render_widget(&mut self, widget: impl Widget, area: Rect) {
        widget.render(area, self.buffer);
    }
}

pub(crate) fn buttons_height<T>(width: u16, buttons: &[(&str, T)]) -> u16 {
    button_rects(Rect::new(0, 0, width, u16::MAX), buttons)
        .last()
        .map_or(0, |r| r.y + 1)
}
fn button_rects<T>(area: Rect, buttons: &[(&str, T)]) -> Vec<Rect> {
    if area.is_empty() {
        return Vec::new();
    }
    let mut x = 0;
    let mut y = 0;
    buttons
        .iter()
        .map(|(label, _)| {
            let width = (Span::raw(*label).width() as u16).min(area.width);
            if x > 0 && x + width > area.width {
                x = 0;
                y += 1;
            }
            let rect = Rect::new(area.x + x, area.y + y, width, u16::from(y < area.height));
            x = x.saturating_add(width).saturating_add(1);
            rect
        })
        .collect()
}
pub(crate) fn buttons<T: Clone, P: Copy + PartialEq>(
    surface: &mut Surface<'_>,
    area: Rect,
    mouse: &MouseState<T, P>,
    buttons: &[(&str, T)],
) {
    for ((label, target), rect) in buttons.iter().zip(button_rects(area, buttons)) {
        if rect.is_empty() {
            continue;
        }
        surface.render_widget(
            Paragraph::new(*label).style(Style::default().fg(theme::AMBER_GLOW())),
            rect,
        );
        mouse.hit(rect, target.clone());
    }
}

/// A pane of `rows` virtual lines shown through `area`. `draw` gets the whole
/// virtual content rect to lay out in, and the window of it that is on
/// screen: only what overlaps the window needs painting, and the buffer
/// behind it is only that tall. Widgets clip to it, so a row half out of
/// view is drawn as far as it shows.
pub(crate) fn scroll<T: Clone, P: Copy + PartialEq>(
    surface: &mut Surface<'_>,
    area: Rect,
    mouse: &MouseState<T, P>,
    pane: P,
    rows: usize,
    focus: std::ops::Range<usize>,
    draw: impl FnOnce(&mut Surface<'_>, Rect, Rect),
) {
    if area.is_empty() {
        return;
    }
    let rows = rows
        .max(usize::from(area.height))
        .min(usize::from(u16::MAX));
    let offset = mouse.pane_range(area, pane, rows, focus);
    let content = Rect::new(0, 0, area.width.saturating_sub(1), rows as u16);
    if content.width == 0 {
        return;
    }
    let window = Rect::new(0, offset as u16, content.width, area.height);
    let mut buffer = Buffer::empty(window);
    let mark = mouse.mark();
    draw(
        &mut Surface {
            buffer: &mut buffer,
        },
        content,
        window,
    );
    mouse.translate(
        mark,
        Rect {
            width: content.width,
            ..area
        },
        offset,
    );
    for y in 0..area.height {
        for x in 0..content.width {
            surface.buffer[(area.x + x, area.y + y)] = buffer[(x, window.y + y)].clone();
        }
    }
    if rows > usize::from(area.height) {
        let thumb = offset * usize::from(area.height.saturating_sub(1))
            / rows.saturating_sub(usize::from(area.height)).max(1);
        for y in 0..area.height {
            surface.buffer[(area.right() - 1, area.y + y)]
                .set_symbol(if usize::from(y) == thumb {
                    "█"
                } else {
                    "│"
                })
                .set_style(Style::default().fg(theme::AMBER_DIM()));
        }
    }
}

/// Each logical line carries its action, if any. Measure and paint the same
/// Paragraph, so wrapping cannot shift a target onto a different item.
pub(crate) fn line_height(line: &Line<'_>, width: u16) -> usize {
    if width == 0 {
        return 0;
    }
    Paragraph::new(line.clone())
        .wrap(Wrap { trim: false })
        .line_count(width)
        .max(1)
}
/// Lines laid out in `area`, painted only where they overlap `window` (the
/// whole area when nothing scrolls).
pub(crate) fn lines<T: Clone, P: Copy + PartialEq>(
    surface: &mut Surface<'_>,
    area: Rect,
    window: Rect,
    mouse: &MouseState<T, P>,
    lines: &[(Line<'static>, Option<T>)],
) {
    let mut y = area.y;
    for (line, target) in lines {
        if y >= area.bottom() || y >= window.bottom() {
            break;
        }
        let height = (line_height(line, area.width) as u16).min(area.bottom() - y);
        let rect = Rect::new(area.x, y, area.width, height);
        if rect.bottom() > window.y {
            surface.render_widget(
                Paragraph::new(line.clone()).wrap(Wrap { trim: false }),
                rect,
            );
            if let Some(target) = target {
                mouse.hit(rect, target.clone());
            }
        }
        y = y.saturating_add(height);
    }
}

#[cfg(test)]
#[path = "mouse_ui_test.rs"]
mod tests;
