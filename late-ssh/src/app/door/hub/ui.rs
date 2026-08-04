use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::state::HubGame;
use crate::app::common::{primitives::hint_line, theme};

/// View data the renderer needs for one frame of the Games hub.
pub struct HubView {
    pub selected: usize,
    pub delete_confirm: bool,
    pub rebels_enabled: bool,
    pub nethack_enabled: bool,
    pub dcss_enabled: bool,
    pub brogue_enabled: bool,
    pub usurper_enabled: bool,
    pub dopewars_enabled: bool,
    pub codekeep_enabled: bool,
    /// Players currently in the Lateania world, shown on its landing card.
    pub lateania_online: usize,
}

/// The sidebar column width, including its right rule column. Sized to the
/// longest label ("Green Dragon") plus the two-cell indent.
pub const SIDEBAR_WIDTH: u16 = 19;

/// Minimum hub viewport. The width floor keeps the landing pane at least as
/// wide as the narrowest landing's own too-small guard (Lateania's 36).
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 6;

/// One row of the sidebar: a muted group header, a selectable game (index
/// into [`HubGame::ALL`]), or a blank separator between groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarRow {
    Header(&'static str),
    Game(usize),
    Blank,
}

/// The sidebar rows in display order: each group opens with its header,
/// groups separated by a blank row. Shared by the renderer and the click hit
/// test so they cannot drift.
fn sidebar_rows() -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    let mut current_group = None;
    for (i, game) in HubGame::ALL.iter().enumerate() {
        let group = game.group();
        if current_group != Some(group) {
            if current_group.is_some() {
                rows.push(SidebarRow::Blank);
            }
            rows.push(SidebarRow::Header(group.label()));
            current_group = Some(group);
        }
        rows.push(SidebarRow::Game(i));
    }
    rows
}

/// First visible row for a viewport `height` rows tall: 0 while everything
/// fits, otherwise the window follows the selected game's row, roughly
/// centered, clamped to the list ends.
fn sidebar_scroll(rows: &[SidebarRow], selected: usize, height: usize) -> usize {
    if rows.len() <= height || height == 0 {
        return 0;
    }
    let selected_row = rows
        .iter()
        .position(|row| *row == SidebarRow::Game(selected))
        .unwrap_or(0);
    let max_scroll = rows.len() - height;
    selected_row.saturating_sub(height / 2).min(max_scroll)
}

pub fn draw_games_hub(frame: &mut Frame, area: Rect, view: &HubView) {
    if area.height < MIN_HEIGHT || area.width < MIN_WIDTH {
        frame.render_widget(
            Paragraph::new("Terminal too small for Games")
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // breathing room under the top border
            Constraint::Min(0),    // sidebar + selected game's landing
            Constraint::Length(1), // footer hints
        ])
        .split(area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(SIDEBAR_WIDTH), Constraint::Min(0)])
        .split(layout[1]);

    let selected = view.selected.min(HubGame::ALL.len() - 1);
    draw_sidebar(frame, body[0], selected);

    // The selected game owns the pane beside the sidebar, rendered with its
    // real landing (logo, stats, actions).
    match HubGame::ALL[selected] {
        HubGame::Lateania => crate::app::door::lateania::screen::draw_landing(
            frame,
            body[1],
            view.delete_confirm,
            view.lateania_online,
        ),
        HubGame::Rebels => {
            crate::app::door::rebels::render::draw_landing(frame, body[1], view.rebels_enabled);
        }
        HubGame::Nethack => {
            crate::app::door::nethack::render::draw_landing(frame, body[1], view.nethack_enabled);
        }
        HubGame::Dcss => {
            crate::app::door::dcss::render::draw_landing(frame, body[1], view.dcss_enabled);
        }
        HubGame::Brogue => {
            crate::app::door::brogue::render::draw_landing(frame, body[1], view.brogue_enabled);
        }
        HubGame::Usurper => {
            crate::app::door::usurper::render::draw_landing(frame, body[1], view.usurper_enabled);
        }
        HubGame::GreenDragon => {
            crate::app::door::greendragon::screen::draw_landing(
                frame,
                body[1],
                view.delete_confirm,
            );
        }
        HubGame::Dopewars => {
            crate::app::door::dopewars::render::draw_landing(frame, body[1], view.dopewars_enabled);
        }
        HubGame::Darkroom => {
            crate::app::door::darkroom::screen::draw_landing(frame, body[1], view.delete_confirm);
        }
        HubGame::Codekeep => {
            crate::app::door::codekeep::render::draw_landing(frame, body[1], view.codekeep_enabled);
        }
    }

    draw_footer(frame, layout[2]);
}

fn draw_sidebar(frame: &mut Frame, area: Rect, selected: usize) {
    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(theme::BORDER_DIM()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = sidebar_rows();
    let scroll = sidebar_scroll(&rows, selected, inner.height as usize);
    let pad = usize::from(inner.width).saturating_sub(2);
    let lines: Vec<Line> = rows
        .iter()
        .skip(scroll)
        .take(inner.height as usize)
        .map(|row| match row {
            SidebarRow::Header(label) => Line::from(Span::styled(
                format!(" {label}"),
                Style::default().fg(theme::TEXT_MUTED()),
            )),
            SidebarRow::Game(i) => {
                let style = if *i == selected {
                    Style::default()
                        .fg(theme::BG_SELECTION())
                        .bg(theme::AMBER())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT_DIM())
                };
                let label = HubGame::ALL[*i].label();
                Line::from(Span::styled(format!("  {label:<pad$}"), style))
            }
            SidebarRow::Blank => Line::default(),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let hints: &[(&str, &str)] = &[
        ("\u{2191} \u{2193}  or  j k", "switch game"),
        ("Enter", "play"),
    ];
    frame.render_widget(Paragraph::new(hint_line(hints)), area);
}

/// Which sidebar game (if any) sits at terminal cell `(x, y)`, given the hub
/// body rect (the same area `draw_games_hub` renders into). Mirrors the
/// layout above (breathing row, footer row, right rule column); `selected`
/// reproduces the scroll position. Used for click-to-select.
pub fn sidebar_hit_test(area: Rect, selected: usize, x: u16, y: u16) -> Option<usize> {
    if area.height < MIN_HEIGHT || area.width < MIN_WIDTH {
        return None;
    }
    let inner = Rect {
        x: area.x,
        y: area.y + 1,
        width: SIDEBAR_WIDTH - 1,
        height: area.height - 2,
    };
    if x < inner.x || x >= inner.x + inner.width || y < inner.y || y >= inner.y + inner.height {
        return None;
    }
    let rows = sidebar_rows();
    let scroll = sidebar_scroll(&rows, selected, usize::from(inner.height));
    match rows.get(scroll + usize::from(y - inner.y)) {
        Some(SidebarRow::Game(i)) => Some(*i),
        Some(SidebarRow::Header(_) | SidebarRow::Blank) | None => None,
    }
}
