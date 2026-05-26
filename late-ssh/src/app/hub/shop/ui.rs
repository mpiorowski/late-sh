use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::UnicodeWidthStr;

use crate::app::common::theme;

use super::{catalog::ShopCategory, state::ShopState, svc::ShopCatalogItem};

pub fn draw(frame: &mut Frame, area: Rect, state: &ShopState) {
    let sections = Layout::vertical([
        Constraint::Length(1), // heading
        Constraint::Length(1), // balance
        Constraint::Length(1), // breathing
        Constraint::Length(1), // categories
        Constraint::Length(1), // breathing
        Constraint::Min(8),    // body
        Constraint::Length(1), // footer
    ])
    .split(area);

    frame.render_widget(Paragraph::new(section_heading("Shop")), sections[0]);
    frame.render_widget(Paragraph::new(balance_line(state.balance())), sections[1]);
    draw_categories(frame, sections[3], state);
    draw_body(frame, sections[5], state);
    draw_footer(frame, sections[6], state);
}

fn draw_categories(frame: &mut Frame, area: Rect, state: &ShopState) {
    let mut spans = vec![Span::raw("  ")];
    for (index, category) in ShopCategory::ALL.iter().copied().enumerate() {
        let selected = index == state.selected_category_index();
        let style = if selected {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", category.label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_body(frame: &mut Frame, area: Rect, state: &ShopState) {
    let columns =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    draw_item_list(frame, columns[0], state);
    draw_item_detail(frame, columns[1], state.selected_item());
}

fn draw_item_list(frame: &mut Frame, area: Rect, state: &ShopState) {
    let items = state.visible_items();
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "no items here yet",
                    Style::default().fg(theme::TEXT_FAINT()),
                ),
            ])),
            area,
        );
        return;
    }

    let rows = item_list_rows(state.selected_category(), &items);
    let selected_row = rows
        .iter()
        .position(
            |row| matches!(row, ItemListRow::Item { index, .. } if *index == state.selected_index()),
        )
        .unwrap_or(state.selected_index());
    let height = area.height.max(1) as usize;
    let start = visible_window_start(selected_row, rows.len(), height);
    let lines = rows
        .iter()
        .skip(start)
        .take(height)
        .map(|row| match row {
            ItemListRow::Section(label) => section_row(label),
            ItemListRow::Item { index, item } => item_row(*index == state.selected_index(), item),
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

enum ItemListRow<'a> {
    Section(&'static str),
    Item {
        index: usize,
        item: &'a ShopCatalogItem,
    },
}

fn item_list_rows<'a>(
    category: ShopCategory,
    items: &[&'a ShopCatalogItem],
) -> Vec<ItemListRow<'a>> {
    if category != ShopCategory::Badges {
        return items
            .iter()
            .enumerate()
            .map(|(index, item)| ItemListRow::Item { index, item })
            .collect();
    }

    let mut rows = Vec::with_capacity(items.len() + 2);
    let mut current_section = None;
    for (index, item) in items.iter().enumerate() {
        let section = badge_section_label(item);
        if current_section != Some(section) {
            rows.push(ItemListRow::Section(section));
            current_section = Some(section);
        }
        rows.push(ItemListRow::Item { index, item });
    }
    rows
}

fn badge_section_label(item: &ShopCatalogItem) -> &'static str {
    match item.badge_tier.as_deref() {
        Some("premium") => "Premium",
        Some("basic") => "Basic",
        _ => "Other",
    }
}

fn visible_window_start(selected_index: usize, item_count: usize, height: usize) -> usize {
    if item_count <= height {
        return 0;
    }

    let half_height = height / 2;
    selected_index
        .saturating_sub(half_height)
        .min(item_count.saturating_sub(height))
}

fn draw_item_detail(frame: &mut Frame, area: Rect, item: Option<&ShopCatalogItem>) {
    let Some(item) = item else {
        return;
    };

    let action = if item.equipped {
        "displaying"
    } else if item.owned && item.slot.is_some() {
        "owned"
    } else if item.owned {
        "unlocked"
    } else if item.is_cat_companion() {
        "unlock cat"
    } else if item.is_chat_badge() {
        "buy badge"
    } else {
        "buy"
    };
    let status = if item.owned {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER())
    };

    let mut lines = vec![
        section_heading(&item.name),
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                item.description.clone(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::raw("  price  "),
            Span::styled(
                format!("{} chips", item.price_chips),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![Span::raw("  state  "), Span::styled(action, status)]),
    ];
    if item.owned && item.quantity > 0 {
        lines.push(Line::from(vec![
            Span::raw("  owned  "),
            Span::styled(
                item.quantity.to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    if let Some(uses) = item.remaining_uses {
        lines.push(Line::from(vec![
            Span::raw("  uses   "),
            Span::styled(uses.to_string(), Style::default().fg(theme::TEXT_DIM())),
        ]));
    }
    if let Some(slot) = &item.slot {
        lines.push(Line::from(vec![
            Span::raw("  slot   "),
            Span::styled(slot.clone(), Style::default().fg(theme::TEXT_DIM())),
        ]));
    }
    if item.equipped {
        lines.push(Line::from(vec![
            Span::raw("  chat   "),
            Span::styled(
                "shown next to your name",
                Style::default().fg(theme::SUCCESS()),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &ShopState) {
    let selected = state.selected_item();
    let enter_label = if selected.is_some_and(|item| item.equipped) {
        "clear"
    } else if selected.is_some_and(|item| item.owned && item.slot.is_some()) {
        "display"
    } else if selected.is_some_and(|item| item.owned) {
        "unlocked"
    } else {
        "buy"
    };
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    let line = Line::from(vec![
        Span::raw("  "),
        Span::styled("j/k", key),
        Span::styled(" select  ", text),
        Span::styled("[/]", key),
        Span::styled(" subtab  ", text),
        Span::styled("Enter", key),
        Span::styled(format!(" {enter_label}"), text),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn item_row(selected: bool, item: &ShopCatalogItem) -> Line<'static> {
    let marker = if selected { ">" } else { " " };
    let name_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let status = if item.equipped {
        "displaying"
    } else if item.owned {
        "owned"
    } else {
        "locked"
    };
    let status_style = if item.equipped {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if item.owned {
        Style::default().fg(theme::SUCCESS())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let display_name = if item.is_chat_badge() {
        item.badge_emoji
            .as_deref()
            .unwrap_or(&item.name)
            .to_string()
    } else {
        item.name.clone()
    };
    Line::from(vec![
        Span::styled(
            format!("  {marker} "),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled(pad_display_width(&display_name, 22), name_style),
        Span::styled(status, status_style),
    ])
}

fn section_row(label: &'static str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            label,
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn pad_display_width(value: &str, width: usize) -> String {
    let display_width = UnicodeWidthStr::width(value);
    let padding = width.saturating_sub(display_width);
    format!("{value}{}", " ".repeat(padding))
}

fn balance_line(balance: i64) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled("balance ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled(
            format!("{balance} chips"),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  /  cosmetics and companions use Late Chips",
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  -- ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" --", dim),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_window_start_keeps_selected_item_visible() {
        assert_eq!(visible_window_start(0, 20, 5), 0);
        assert_eq!(visible_window_start(3, 20, 5), 1);
        assert_eq!(visible_window_start(19, 20, 5), 15);
    }

    #[test]
    fn pad_display_width_handles_variation_selector_emoji() {
        let padded = pad_display_width("☀️", 6);
        assert_eq!(UnicodeWidthStr::width(padded.as_str()), 6);
        let padded = pad_display_width("🐱", 6);
        assert_eq!(UnicodeWidthStr::width(padded.as_str()), 6);
    }
}
