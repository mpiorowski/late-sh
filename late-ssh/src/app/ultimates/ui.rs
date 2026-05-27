use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::{format_cooldown, manifest::UltimateKind, owned_ultimates, state::UltimateState};
use crate::app::{
    common::{primitives::Banner, theme},
    hub::shop::state::ShopState,
    input::ParsedInput,
    state::App,
};

pub fn open_ultimate_modal(app: &mut App) {
    app.ultimate_state.clamp_selection(&app.shop_state);
    if owned_ultimates(&app.shop_state).is_empty() {
        app.banner = Some(Banner::error("No Ultimates unlocked yet"));
        return;
    }
    app.show_help = false;
    app.show_settings = false;
    app.show_hub_modal = false;
    app.show_profile_modal = false;
    app.show_bonsai_modal = false;
    app.show_cat_modal = false;
    app.show_ultimate_modal = true;
    app.refresh_ultimate_cooldowns();
}

pub fn handle_input(app: &mut App, event: ParsedInput) {
    match event {
        ParsedInput::Byte(0x1B | b'q' | b'Q') | ParsedInput::Char('q' | 'Q') => {
            app.show_ultimate_modal = false;
        }
        ParsedInput::Arrow(b'A')
        | ParsedInput::Byte(b'k' | b'K')
        | ParsedInput::Char('k' | 'K') => {
            app.ultimate_state.move_selection(-1, &app.shop_state);
        }
        ParsedInput::Arrow(b'B')
        | ParsedInput::Byte(b'j' | b'J')
        | ParsedInput::Char('j' | 'J') => {
            app.ultimate_state.move_selection(1, &app.shop_state);
        }
        ParsedInput::Byte(b'\r' | b'\n') => {
            if let Some(kind) = app.ultimate_state.selected_kind(&app.shop_state) {
                app.cast_ultimate(kind);
            }
        }
        _ => {}
    }
}

pub fn draw(frame: &mut Frame, area: Rect, state: &UltimateState, shop: &ShopState) {
    let popup = centered_percent_rect(54, 46, area);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Ultimates ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "Choose an owned ultimate to cast server-wide.",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])),
        layout[0],
    );
    draw_list(frame, layout[1], state, shop);
    draw_footer(frame, layout[2]);
}

fn draw_list(frame: &mut Frame, area: Rect, state: &UltimateState, shop: &ShopState) {
    let items = owned_ultimates(shop);
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "no ultimates unlocked",
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
        .map(|(index, item)| {
            let selected = index == state.selected_index();
            let name_style = if selected {
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT_BRIGHT())
            };
            let marker = if selected { ">" } else { " " };
            let kind = UltimateKind::from_sku(&item.sku);
            let cooldown = kind.and_then(|kind| state.cooldown_remaining(kind));
            let state_text = cooldown
                .map(|remaining| format!("cooldown {}", format_cooldown(remaining)))
                .unwrap_or_else(|| "ready".to_string());
            let state_style = if cooldown.is_some() {
                Style::default().fg(theme::TEXT_FAINT())
            } else {
                Style::default().fg(theme::SUCCESS())
            };
            Line::from(vec![
                Span::styled(
                    format!("  {marker} "),
                    Style::default().fg(theme::AMBER_DIM()),
                ),
                Span::styled(item.name.clone(), name_style),
                Span::styled("  ", Style::default()),
                Span::styled(state_text, state_style),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let text = Style::default().fg(theme::TEXT_DIM());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled("j/k", key),
            Span::styled(" select  ", text),
            Span::styled("Enter", key),
            Span::styled(" cast  ", text),
            Span::styled("Esc/q", key),
            Span::styled(" close", text),
        ])),
        area,
    );
}

fn centered_percent_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
