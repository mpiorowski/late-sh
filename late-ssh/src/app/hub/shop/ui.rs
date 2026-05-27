use late_core::models::marketplace::AQUARIUM_MAX_FISH;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    common::theme,
    hub::aquarium::creature::{CreatureDef, load_default_creatures},
};

use super::{catalog::ShopCategory, state::ShopState, svc::ShopCatalogItem};

use std::sync::OnceLock;

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
    draw_item_detail(
        frame,
        columns[1],
        state.selected_item(),
        state.entitlements().has_aquarium(),
    );
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

fn draw_item_detail(
    frame: &mut Frame,
    area: Rect,
    item: Option<&ShopCatalogItem>,
    has_aquarium: bool,
) {
    let Some(item) = item else {
        return;
    };

    let action = if item.equipped {
        "displaying"
    } else if item.is_aquarium_fish() && !has_aquarium {
        "needs aquarium"
    } else if item.is_aquarium_fish() {
        "buy fish"
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
    } else if item.is_aquarium_fish() && !has_aquarium {
        Style::default()
            .fg(theme::TEXT_DIM())
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
    if item.is_aquarium_fish() {
        if !has_aquarium {
            lines.push(Line::from(vec![
                Span::raw("  unlock "),
                Span::styled(
                    "Aquarium first",
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        if let Some(size) = &item.aquarium_size {
            lines.push(Line::from(vec![
                Span::raw("  size   "),
                Span::styled(size.clone(), Style::default().fg(theme::TEXT_DIM())),
            ]));
        }
        lines.push(Line::from(vec![
            Span::raw("  active "),
            Span::styled(
                format!("{}", item.active_quantity),
                Style::default().fg(theme::SUCCESS()),
            ),
            Span::styled(
                format!(" / {} owned", item.quantity),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  tank   "),
            Span::styled(
                format!("max {AQUARIUM_MAX_FISH} active"),
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

    let preview = aquarium_preview_lines(item, area.width);
    if preview.is_empty() {
        frame.render_widget(Paragraph::new(lines), area);
        return;
    }

    let info_height = lines.len().min(area.height as usize) as u16;
    let sections =
        Layout::vertical([Constraint::Length(info_height), Constraint::Min(0)]).split(area);
    frame.render_widget(Paragraph::new(lines), sections[0]);

    if sections[1].height > 0 {
        frame.render_widget(Paragraph::new(preview), sections[1]);
    }
}

fn aquarium_preview_lines(item: &ShopCatalogItem, width: u16) -> Vec<Line<'static>> {
    let Some(creature_name) = item.aquarium_creature.as_deref() else {
        return Vec::new();
    };
    let Some(def) = aquarium_creature_def(creature_name) else {
        return Vec::new();
    };
    let variant = def.best_variant(1, 0, 0);
    let preview_width = width.saturating_sub(2) as usize;
    if preview_width == 0 {
        return Vec::new();
    }

    let mut lines = vec![Line::from("")];
    lines.extend(variant.art.iter().map(|line| {
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                truncate_display_width(line, preview_width),
                Style::default().fg(theme::BORDER_ACTIVE()),
            ),
        ])
    }));
    lines
}

fn aquarium_creature_def(name: &str) -> Option<&'static CreatureDef> {
    static CREATURES: OnceLock<Vec<CreatureDef>> = OnceLock::new();
    CREATURES
        .get_or_init(|| {
            load_default_creatures().unwrap_or_else(|error| {
                tracing::warn!(?error, "aquarium creature defs failed to load");
                Vec::new()
            })
        })
        .iter()
        .find(|def| def.name == name)
}

fn truncate_display_width(value: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut out = String::new();
    for ch in value.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if width + ch_width > max_width {
            break;
        }
        width += ch_width;
        out.push(ch);
    }
    out
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &ShopState) {
    let selected = state.selected_item();
    let has_aquarium = state.entitlements().has_aquarium();
    let enter_label = if selected.is_some_and(|item| item.equipped) {
        "clear"
    } else if selected.is_some_and(|item| item.is_aquarium_fish() && !has_aquarium) {
        "needs aquarium"
    } else if selected.is_some_and(|item| item.is_aquarium_fish()) {
        "buy one"
    } else if selected.is_some_and(|item| item.owned && item.slot.is_some()) {
        "display"
    } else if selected.is_some_and(|item| item.owned) {
        "unlocked"
    } else {
        "buy"
    };
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    let mut spans = vec![
        Span::raw("  "),
        Span::styled("j/k", key),
        Span::styled(" select  ", text),
        Span::styled("[/]", key),
        Span::styled(" subtab  ", text),
        Span::styled("Enter", key),
        Span::styled(format!(" {enter_label}"), text),
    ];
    if selected.is_some_and(|item| item.is_aquarium_fish() && has_aquarium) {
        spans.extend([Span::styled("  +/-", key), Span::styled(" active", text)]);
    }
    if state.selected_category() == ShopCategory::Aquarium {
        spans.extend([
            Span::styled("  by ", text),
            Span::styled("github.com/mevanlc/reefs", key),
        ]);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
    } else if item.is_aquarium_fish() && item.quantity > 0 {
        "owned"
    } else if item.is_aquarium_fish() {
        "buy"
    } else if item.owned {
        "owned"
    } else {
        "locked"
    };
    let status_style = if item.equipped {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if item.owned || (item.is_aquarium_fish() && item.quantity > 0) {
        Style::default().fg(theme::SUCCESS())
    } else if item.is_aquarium_fish() {
        Style::default().fg(theme::AMBER())
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
        Span::styled(
            if item.is_aquarium_fish() {
                format!(" {}/{}", item.active_quantity, item.quantity)
            } else {
                String::new()
            },
            Style::default().fg(theme::TEXT_DIM()),
        ),
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
