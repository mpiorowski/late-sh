use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

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

    let lines = items
        .iter()
        .enumerate()
        .map(|(index, item)| item_row(index == state.selected_index(), item))
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_item_detail(frame: &mut Frame, area: Rect, item: Option<&ShopCatalogItem>) {
    let Some(item) = item else {
        return;
    };

    let action = if item.owned {
        "unlocked"
    } else if item.is_cat_companion() {
        "unlock cat"
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

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &ShopState) {
    let selected = state.selected_item();
    let enter_label = if selected.is_some_and(|item| item.owned) {
        "already unlocked"
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
        Span::styled(" category  ", text),
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
    let status = if item.owned { "owned" } else { "locked" };
    let status_style = if item.owned {
        Style::default().fg(theme::SUCCESS())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    Line::from(vec![
        Span::styled(
            format!("  {marker} "),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled(format!("{:<22}", item.name), name_style),
        Span::styled(status, status_style),
    ])
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
