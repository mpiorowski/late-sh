use late_core::models::marketplace::{
    AQUARIUM_FOOD_SKU, AQUARIUM_MAX_FISH, CHAT_CONSUMABLE_ITEM_KIND, PET_FOOD_SKU,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{
    common::theme,
    hub::aquarium::creature::{CreatureDef, load_default_creatures},
};

use super::{
    catalog::ShopCategory,
    state::{PendingRoomEffect, ShopState},
    svc::ShopCatalogItem,
};

use std::sync::OnceLock;

pub fn draw(frame: &mut Frame, area: Rect, state: &ShopState, pet_species: &str) {
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
    draw_body(frame, sections[5], state, pet_species);
    draw_footer(frame, sections[6], state, pet_species);
    if let Some(pending) = state.pending_room_effect() {
        draw_room_effect_confirm(frame, area, pending);
    }
}

fn draw_categories(frame: &mut Frame, area: Rect, state: &ShopState) {
    let mut spans = vec![Span::raw("  ")];
    let mut rects = [Rect::new(0, 0, 0, 0); ShopCategory::ALL.len()];
    let mut cursor_x = area.x.saturating_add(2);
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
        let label = format!(" {} ", category.label());
        let width = label.chars().count() as u16;
        rects[index] = Rect::new(cursor_x, area.y, width, area.height.min(1));
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        cursor_x = cursor_x.saturating_add(width).saturating_add(1);
    }
    state.set_category_rects(rects);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_body(frame: &mut Frame, area: Rect, state: &ShopState, pet_species: &str) {
    let columns =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).split(area);
    draw_item_list(frame, columns[0], state);
    draw_item_detail(
        frame,
        columns[1],
        state,
        state.selected_item(),
        state.entitlements().has_aquarium(),
        pet_species,
    );
}

fn draw_item_list(frame: &mut Frame, area: Rect, state: &ShopState) {
    let items = state.visible_items();
    if items.is_empty() {
        state.set_item_rects(Vec::new());
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

    let category = state.selected_category();
    let rows = item_list_rows(category, &items);
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
            ItemListRow::Item { index, item } => {
                item_row(category, *index == state.selected_index(), item, state)
            }
        })
        .collect::<Vec<_>>();
    let mut item_rects = Vec::new();
    for (i, row) in rows.iter().skip(start).take(height).enumerate() {
        if let ItemListRow::Item { index, .. } = row {
            let rect = Rect::new(area.x, area.y + i as u16, area.width, 1);
            item_rects.push((rect, *index));
        }
    }
    state.set_item_rects(item_rects);
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
    state: &ShopState,
    item: Option<&ShopCatalogItem>,
    has_aquarium: bool,
    pet_species: &str,
) {
    let Some(item) = item else {
        return;
    };

    let chat_effect_active =
        item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && chat_consumable_active(item, state);
    let action = if item.is_dynamic_bonsai() && item.equipped {
        "dynamic"
    } else if item.is_dynamic_bonsai() && item.owned {
        "classic"
    } else if item.equipped {
        "displaying"
    } else if item.is_consumable() {
        consumable_action_label(item, Some(chat_consumable_active(item, state)))
    } else if item.is_aquarium_fish() && !has_aquarium {
        "needs aquarium"
    } else if item.is_aquarium_fish() {
        "buy fish"
    } else if item.owned && item.slot.is_some() {
        "owned"
    } else if item.owned {
        "unlocked"
    } else if item.is_pet_companion() {
        "unlock pet"
    } else if item.is_chat_badge() {
        "buy badge"
    } else if item.is_ultimate_spell() {
        "buy spell"
    } else {
        "buy"
    };
    let status = if chat_effect_active || (item.owned && !item.is_consumable()) {
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
        section_heading(&item_detail_title(item)),
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
    if item.owned && item.quantity > 0 && item.item_kind != CHAT_CONSUMABLE_ITEM_KIND {
        lines.push(Line::from(vec![
            Span::raw("  owned  "),
            Span::styled(
                item.quantity.to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    if item.is_pet_companion() && item.owned {
        lines.push(Line::from(vec![
            Span::raw("  ascii  "),
            Span::styled(
                pet_species.to_string(),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   t to toggle cat/dog",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
    }
    if item.is_consumable() {
        lines.push(Line::from(vec![
            Span::raw("  use    "),
            Span::styled(
                consumable_use_hint(item),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]));
        if let Some(effect) = &item.effect_kind {
            lines.push(Line::from(vec![
                Span::raw("  effect "),
                Span::styled(effect.clone(), Style::default().fg(theme::TEXT_DIM())),
            ]));
        }
        if item.daily_limited {
            lines.push(Line::from(vec![
                Span::raw("  limit  "),
                Span::styled("once per UTC day", Style::default().fg(theme::TEXT_DIM())),
            ]));
        }
        if item.requires_room {
            lines.push(Line::from(vec![
                Span::raw("  target "),
                Span::styled("current room", Style::default().fg(theme::TEXT_DIM())),
            ]));
        }
    }
    if item.is_dynamic_bonsai() && item.owned {
        lines.push(Line::from(vec![
            Span::raw("  mode   "),
            Span::styled(
                if item.equipped { "dynamic" } else { "classic" },
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "   Enter toggles care modal",
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
        lines.push(Line::from(vec![
            Span::raw("  keys   "),
            Span::styled(
                "Enter buys another; +/- adds/removes owned fish from the tray",
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
    if item.equipped && item.is_chat_badge() {
        lines.push(Line::from(vec![
            Span::raw("  chat   "),
            Span::styled(
                "shown next to your name",
                Style::default().fg(theme::SUCCESS()),
            ),
        ]));
    }
    if item.equipped && item.is_dynamic_bonsai() {
        lines.push(Line::from(vec![
            Span::raw("  bonsai "),
            Span::styled(
                "w opens dynamic care",
                Style::default().fg(theme::SUCCESS()),
            ),
        ]));
    }

    let preview = aquarium_preview_lines(item, area.width);
    if preview.is_empty() {
        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
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

fn draw_footer(frame: &mut Frame, area: Rect, state: &ShopState, _pet_species: &str) {
    let selected = state.selected_item();
    let has_aquarium = state.entitlements().has_aquarium();
    let enter_label = if selected.is_some_and(|item| item.is_dynamic_bonsai() && item.equipped) {
        "classic"
    } else if selected.is_some_and(|item| item.is_dynamic_bonsai() && item.owned) {
        "dynamic"
    } else if selected.is_some_and(|item| item.equipped) {
        "clear"
    } else if selected.is_some_and(|item| item.is_aquarium_fish() && !has_aquarium) {
        "needs aquarium"
    } else if selected.is_some_and(|item| item.is_aquarium_fish()) {
        "buy one"
    } else if let Some(item) = selected.filter(|item| item.is_consumable()) {
        consumable_footer_label(item)
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
        Span::styled("h/l", key),
        Span::styled(" subtab  ", text),
        Span::styled("Enter", key),
        Span::styled(format!(" {enter_label}"), text),
    ];
    if selected.is_some_and(|item| item.is_aquarium_fish() && has_aquarium) {
        spans.extend([Span::styled("  +/-", key), Span::styled(" active", text)]);
    }
    if selected.is_some_and(|item| item.is_pet_companion() && item.owned) {
        spans.extend([
            Span::styled("  t", key),
            Span::styled(" toggle cat/dog", text),
        ]);
    }
    if state.selected_category() == ShopCategory::Aquarium {
        spans.extend([
            Span::styled("  by ", text),
            Span::styled("github.com/mevanlc/reefs", key),
        ]);
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_room_effect_confirm(frame: &mut Frame, area: Rect, pending: &PendingRoomEffect) {
    let popup = centered_rect(58, 10, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Confirm Room Effect ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = vec![
        Line::from(vec![
            Span::raw("  activate "),
            Span::styled(
                pending.item_name.clone(),
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  room     "),
            Span::styled(
                pending.room_label.clone(),
                Style::default()
                    .fg(theme::SUCCESS())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  price    "),
            Span::styled(
                format!("{} chips", pending.price_chips),
                Style::default().fg(theme::AMBER()),
            ),
        ]),
    ];
    if let Some(effect_kind) = &pending.effect_kind {
        lines.push(Line::from(vec![
            Span::raw("  effect   "),
            Span::styled(effect_kind.clone(), Style::default().fg(theme::TEXT_DIM())),
        ]));
    }
    if pending.daily_limited {
        lines.push(Line::from(vec![
            Span::raw("  limit    "),
            Span::styled("once per UTC day", Style::default().fg(theme::TEXT_DIM())),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled("Enter/y", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" confirm    ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/n", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]));

    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme::BG_CANVAS())),
        inner,
    );
}

fn item_row(
    category: ShopCategory,
    selected: bool,
    item: &ShopCatalogItem,
    state: &ShopState,
) -> Line<'static> {
    let marker = if selected { ">" } else { " " };
    let name_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let status = if item.is_dynamic_bonsai() && item.equipped {
        "dynamic"
    } else if item.is_dynamic_bonsai() && item.owned {
        "classic"
    } else if item.equipped {
        "displaying"
    } else if item.is_consumable() {
        consumable_row_status(item, state)
    } else if item.is_aquarium_fish() && item.quantity > 0 {
        "owned"
    } else if item.is_aquarium_fish() {
        "buy"
    } else if item.owned {
        "owned"
    } else {
        "locked"
    };
    let active_chat_consumable = item.item_kind == CHAT_CONSUMABLE_ITEM_KIND
        && !chat_room_bump_item(item)
        && chat_consumable_active(item, state);
    let status_style = if active_chat_consumable || item.equipped {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else if item.is_consumable() {
        Style::default().fg(theme::AMBER())
    } else if item.owned || (item.is_aquarium_fish() && item.quantity > 0) {
        Style::default().fg(theme::SUCCESS())
    } else if item.is_aquarium_fish() {
        Style::default().fg(theme::AMBER())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let display_name = if category == ShopCategory::Flags && item.is_flag_badge() {
        flag_display_name(item)
    } else if item.is_chat_badge() {
        badge_display_name(item)
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
            } else if item.is_consumable()
                && item.item_kind != CHAT_CONSUMABLE_ITEM_KIND
                && item.quantity > 0
            {
                format!(" x{}", item.quantity)
            } else {
                String::new()
            },
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

fn badge_display_name(item: &ShopCatalogItem) -> String {
    item.badge_emoji
        .as_deref()
        .unwrap_or(&item.name)
        .to_string()
}

fn consumable_action_label(item: &ShopCatalogItem, active: Option<bool>) -> &'static str {
    if chat_room_bump_item(item) {
        "activate"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && active == Some(true) {
        "active"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && item.requires_room {
        "confirm room"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND {
        "activate now"
    } else if item.sku == PET_FOOD_SKU || item.sku == AQUARIUM_FOOD_SKU {
        "buy food"
    } else {
        "buy"
    }
}

fn consumable_footer_label(item: &ShopCatalogItem) -> &'static str {
    if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND {
        "activate"
    } else if item.sku == PET_FOOD_SKU || item.sku == AQUARIUM_FOOD_SKU {
        "buy food"
    } else {
        "buy"
    }
}

fn consumable_row_status(item: &ShopCatalogItem, state: &ShopState) -> &'static str {
    if chat_room_bump_item(item) {
        "activate"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && chat_consumable_active(item, state) {
        "active"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && item.requires_room {
        "confirm"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND {
        "activate"
    } else {
        "buy"
    }
}

fn chat_room_bump_item(item: &ShopCatalogItem) -> bool {
    item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && item.effect_kind.as_deref() == Some("room_bump")
}

fn chat_consumable_active(item: &ShopCatalogItem, state: &ShopState) -> bool {
    if chat_room_bump_item(item) {
        return false;
    }
    let Some(effect_kind) = item.effect_kind.as_deref() else {
        return false;
    };
    if item.requires_room {
        state.active_room_effects().values().any(|effects| {
            effects
                .iter()
                .any(|effect| effect.source_sku == item.sku || effect.effect_kind == effect_kind)
        })
    } else {
        effect_kind == "bot_username_color" && state.bot_username_color_active()
    }
}

fn consumable_use_hint(item: &ShopCatalogItem) -> &'static str {
    if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND && item.requires_room {
        "Enter activates it on the selected chat room"
    } else if item.item_kind == CHAT_CONSUMABLE_ITEM_KIND {
        "Enter activates it immediately"
    } else if item.sku == AQUARIUM_FOOD_SKU {
        "/aquarium opens the tray; /aquarium feed spends one"
    } else if item.sku == PET_FOOD_SKU {
        "/feed spends one and sends the pet strolling"
    } else {
        "Enter buys one"
    }
}

fn item_detail_title(item: &ShopCatalogItem) -> String {
    if item.is_flag_badge() {
        flag_display_name(item)
    } else {
        item.name.clone()
    }
}

fn flag_display_name(item: &ShopCatalogItem) -> String {
    let label = item
        .sku
        .strip_prefix("badge_flag_")
        .map(flag_label)
        .unwrap_or_else(|| item.name.clone());
    let emoji = item.badge_emoji.as_deref().unwrap_or(&item.name);
    format!("{label} {emoji}")
}

fn flag_label(sku_suffix: &str) -> String {
    if sku_suffix.len() == 2 && sku_suffix.chars().all(|ch| ch.is_ascii_alphabetic()) {
        sku_suffix.to_ascii_uppercase()
    } else {
        sku_suffix.replace('_', " ")
    }
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
            "  /  shop items use Late Chips",
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ])
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width.min(area.width),
        height.min(area.height),
    )
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
