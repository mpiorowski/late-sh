use late_core::models::chat_message_gild::GildTier;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{
    chat::gild::{input::can_afford, state::GildModalState},
    common::{primitives::thousands, theme},
};

const POPUP_WIDTH: u16 = 64;
const POPUP_HEIGHT: u16 = 13;

pub(crate) fn draw_modal(frame: &mut Frame, area: Rect, state: &GildModalState, chip_balance: i64) {
    let Some(target) = state.target() else {
        return;
    };
    let popup = centered_rect(area, POPUP_WIDTH, POPUP_HEIGHT);
    frame.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Gild ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()))
        .style(Style::default().bg(theme::BG_CANVAS()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // author
        Constraint::Length(1), // preview
        Constraint::Length(1), // gap
        Constraint::Length(1), // bronze
        Constraint::Length(1), // silver
        Constraint::Length(1), // gold
        Constraint::Length(1), // gap
        Constraint::Length(1), // balance
        Constraint::Length(1), // keys
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Gilding ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("@{}", target.author_username),
                Style::default()
                    .fg(theme::CHAT_AUTHOR())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&target.preview, inner.width as usize),
            Style::default()
                .fg(theme::TEXT_DIM())
                .add_modifier(Modifier::ITALIC),
        ))),
        rows[1],
    );

    for (index, tier) in GildTier::ALL.iter().enumerate() {
        draw_tier_row(
            frame,
            rows[3 + index],
            index,
            *tier,
            state.selected_tier() == *tier,
            can_afford(chip_balance, *tier),
        );
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Balance ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(thousands(chip_balance), Style::default().fg(theme::AMBER())),
            Span::styled(
                "  ·  the rest is burned",
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ])),
        rows[7],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Enter", Style::default().fg(theme::SUCCESS())),
            Span::styled(" gild  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("j/k", Style::default().fg(theme::AMBER())),
            Span::styled(" tier  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("Esc", Style::default().fg(theme::ERROR())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ])),
        rows[8],
    );
}

/// One tier row: `> ◆◆  Silver     2,000  author gets 1,333`. A tier the
/// balance cannot cover still shows its price (that is the whole point of
/// having it on screen) but reads as out of reach.
fn draw_tier_row(
    frame: &mut Frame,
    area: Rect,
    index: usize,
    tier: GildTier,
    selected: bool,
    affordable: bool,
) {
    let marker_color = match tier {
        GildTier::Bronze => theme::BADGE_BRONZE(),
        GildTier::Silver => theme::BADGE_SILVER(),
        GildTier::Gold => theme::BADGE_GOLD(),
    };
    let text_color = if affordable {
        theme::TEXT()
    } else {
        theme::TEXT_FAINT()
    };
    let line = Line::from(vec![
        Span::styled(
            if selected { "> " } else { "  " },
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(
            format!("{} ", index + 1),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
        Span::styled(
            format!("{:<4}", tier.marker()),
            Style::default()
                .fg(marker_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{:<8}", tier.label()),
            Style::default().fg(text_color),
        ),
        Span::styled(
            format!("{:>7}", thousands(tier.price())),
            Style::default().fg(text_color),
        ),
        Span::styled(
            format!("   author gets {}", thousands(tier.author_share())),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]);
    let style = if selected {
        theme::selection_style()
    } else {
        Style::default().bg(theme::BG_CANVAS())
    };
    frame.render_widget(Paragraph::new(line).style(style), area);
}

fn truncate(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
