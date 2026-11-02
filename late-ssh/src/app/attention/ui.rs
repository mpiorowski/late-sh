//! The attention guest's panel: one row per item, the selected one marked
//! while its tile is focused, the act Enter does at the row's end. Drawn
//! only while the list has rows; the guest is down otherwise.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use super::state::Item;
use crate::app::common::theme;

/// Paint the rows into `area`, scrolled so the selected one is in view.
/// `selected` is clamped to the rows, since they come and go.
pub fn draw(frame: &mut Frame, area: Rect, items: &[Item], selected: usize, focused: bool) {
    if items.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let width = area.width as usize;
    let visible = area.height as usize;
    let selected = selected.min(items.len() - 1);
    let first = (selected + 1).saturating_sub(visible);
    let amber = Style::default().fg(theme::AMBER());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let lines: Vec<Line<'static>> = items
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
        .map(|(index, item)| {
            let marked = focused && index == selected;
            let words_style = match marked {
                true => bright.add_modifier(Modifier::BOLD),
                false => bright,
            };
            let act = item.act();
            let head = format!("{} {} ", if marked { "▌" } else { " " }, item.glyph());
            let budget = width.saturating_sub(head.width() + act.width() + 1);
            let words = cut_to(&item.label(), budget);
            let pad = width.saturating_sub(head.width() + words.width() + act.width());
            Line::from(vec![
                Span::styled(head, amber),
                Span::styled(words, words_style),
                Span::raw(" ".repeat(pad)),
                Span::styled(act.to_string(), faint),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// `text` cut to `budget` columns, an ellipsis on the end when it was cut.
fn cut_to(text: &str, budget: usize) -> String {
    if text.width() <= budget {
        return text.to_string();
    }
    if budget == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > budget - 1 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}
