use late_core::models::door_rc::DoorRcGame;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::state::HubGame;
use crate::app::common::{primitives::hint_line, theme};

/// The rc config modal, when open: which game's file plus the stored content.
pub struct RcModalView<'a> {
    pub game: DoorRcGame,
    /// The account's stored config; `None` when it has never been set.
    pub content: Option<&'a str>,
}

/// View data the renderer needs for one frame of the Games hub.
pub struct HubView<'a> {
    pub selected: usize,
    pub delete_confirm: bool,
    pub rebels_enabled: bool,
    pub nethack_enabled: bool,
    pub dcss_enabled: bool,
    pub brogue_enabled: bool,
    pub usurper_enabled: bool,
    pub dopewars_enabled: bool,
    pub bashquest_enabled: bool,
    pub codekeep_enabled: bool,
    /// Players currently in the Lateania world, shown on its landing card.
    pub lateania_online: usize,
    /// This account's character slots, for the landing card's select list.
    pub lateania_slots: Vec<crate::app::door::lateania::svc::SlotSummary>,
    pub lateania_slot_cursor: usize,
    /// Lateania's backtick-detach recency window is live: the sidebar marks
    /// it as a game in progress (a hop or Enter re-joins the character).
    pub lateania_live: bool,
    /// Roguelike doors with a live detached game this session: the sidebar
    /// marks them and their landing offers resume instead of launch.
    pub nethack_live: bool,
    pub dcss_live: bool,
    pub brogue_live: bool,
    /// The rc config modal, drawn over the hub while open.
    pub rc_modal: Option<RcModalView<'a>>,
}

impl HubView<'_> {
    /// Whether this game counts as in progress: a detached roguelike session
    /// to resume, or Lateania inside its backtick-detach recency window.
    fn is_live(&self, game: HubGame) -> bool {
        match game {
            HubGame::Lateania => self.lateania_live,
            HubGame::Nethack => self.nethack_live,
            HubGame::Dcss => self.dcss_live,
            HubGame::Brogue => self.brogue_live,
            HubGame::Rebels
            | HubGame::Usurper
            | HubGame::GreenDragon
            | HubGame::Dopewars
            | HubGame::Bashquest
            | HubGame::Darkroom
            | HubGame::Codekeep => false,
        }
    }
}

/// The sidebar column width, including its right rule column. Sized to the
/// longest label ("Green Dragon") plus the two-cell indent.
pub const SIDEBAR_WIDTH: u16 = 19;

/// Minimum hub viewport. The width floor keeps the landing pane at least as
/// wide as the narrowest landing's own too-small guard (Lateania's 36).
const MIN_WIDTH: u16 = 60;
const MIN_HEIGHT: u16 = 6;

/// One row of the sidebar: a muted group header, a selectable game (index
/// into [`HubGame::ALL`]), a blank separator between groups, or the faint
/// always-on backtick hint at the top (the games that detach and hop:
/// Lateania and the roguelikes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarRow {
    Header(&'static str),
    Game(usize),
    Blank,
    HopHint,
}

/// The sidebar rows in display order: the ` hop hint leads the whole nav,
/// then each group opens with its header, groups separated by a blank row.
/// Shared by the renderer and the click hit test so they cannot drift.
fn sidebar_rows() -> Vec<SidebarRow> {
    let mut rows = vec![SidebarRow::HopHint, SidebarRow::Blank];
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

pub fn draw_games_hub(frame: &mut Frame, area: Rect, view: &HubView<'_>) {
    if area.height < MIN_HEIGHT || area.width < MIN_WIDTH {
        crate::app::common::primitives::draw_too_small(frame, area, "Games", MIN_WIDTH, MIN_HEIGHT);
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
    draw_sidebar(frame, body[0], selected, view);

    // The selected game owns the pane beside the sidebar, rendered with its
    // real landing (logo, stats, actions).
    match HubGame::ALL[selected] {
        HubGame::Lateania => crate::app::door::lateania::screen::draw_landing(
            frame,
            body[1],
            view.delete_confirm,
            view.lateania_online,
            &view.lateania_slots,
            view.lateania_slot_cursor,
        ),
        HubGame::Rebels => {
            crate::app::door::rebels::render::draw_landing(frame, body[1], view.rebels_enabled);
        }
        HubGame::Nethack => {
            crate::app::door::nethack::render::draw_landing(
                frame,
                body[1],
                view.nethack_enabled,
                view.nethack_live,
            );
        }
        HubGame::Dcss => {
            crate::app::door::dcss::render::draw_landing(
                frame,
                body[1],
                view.dcss_enabled,
                view.dcss_live,
            );
        }
        HubGame::Brogue => {
            crate::app::door::brogue::render::draw_landing(
                frame,
                body[1],
                view.brogue_enabled,
                view.brogue_live,
            );
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
        HubGame::Bashquest => {
            crate::app::door::bashquest::render::draw_landing(
                frame,
                body[1],
                view.bashquest_enabled,
            );
        }
        HubGame::Darkroom => {
            crate::app::door::darkroom::screen::draw_landing(frame, body[1], view.delete_confirm);
        }
        HubGame::Codekeep => {
            crate::app::door::codekeep::render::draw_landing(frame, body[1], view.codekeep_enabled);
        }
    }

    draw_footer(frame, layout[2]);

    if let Some(modal) = &view.rc_modal {
        draw_rc_modal(frame, area, modal);
    }
}

/// How many config lines the modal previews before eliding the rest.
const RC_PREVIEW_LINES: usize = 10;

fn draw_rc_modal(frame: &mut Frame, area: Rect, modal: &RcModalView<'_>) {
    let game_name = match modal.game {
        DoorRcGame::Nethack => "NetHack",
        DoorRcGame::Dcss => "DCSS",
    };
    let popup = centered_rect(area, 64, 19);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(format!(
            " {game_name} config ({}) ",
            modal.game.file_label()
        ))
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // breathing room
            Constraint::Length(2), // explainer
            Constraint::Length(1), // gap
            Constraint::Min(1),    // stored config preview
            Constraint::Length(1), // footer hints
        ])
        .split(inner);

    let explainer = vec![
        Line::from(Span::styled(
            " Paste into this window to replace the whole file.",
            Style::default().fg(theme::TEXT_BRIGHT()),
        )),
        Line::from(Span::styled(
            " Saved to your account and applied at every launch.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    frame.render_widget(Paragraph::new(explainer), layout[1]);

    let preview: Vec<Line> = match modal.content {
        Some(content) => {
            let total_lines = content.lines().count();
            let visible = usize::from(layout[3].height)
                .saturating_sub(2)
                .min(RC_PREVIEW_LINES);
            let mut lines: Vec<Line> = content
                .lines()
                .take(visible)
                .map(|line| {
                    Line::from(Span::styled(
                        format!(" {line}"),
                        Style::default().fg(theme::TEXT_DIM()),
                    ))
                })
                .collect();
            if total_lines > visible {
                lines.push(Line::from(Span::styled(
                    format!(" ... {} more lines", total_lines - visible),
                    Style::default().fg(theme::TEXT_MUTED()),
                )));
            }
            lines.push(Line::from(Span::styled(
                format!(" {} bytes, {} lines", content.len(), total_lines),
                Style::default().fg(theme::TEXT_FAINT()),
            )));
            lines
        }
        None => vec![Line::from(Span::styled(
            " No custom config yet. House defaults apply.",
            Style::default().fg(theme::TEXT_MUTED()),
        ))],
    };
    frame.render_widget(Paragraph::new(preview), layout[3]);

    let hints: &[(&str, &str)] = &[("paste", "replace"), ("x", "clear"), ("Esc", "close")];
    frame.render_widget(Paragraph::new(hint_line(hints)), layout[4]);
}

/// A centred rectangle of the given size, clamped to `area`.
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

fn draw_sidebar(frame: &mut Frame, area: Rect, selected: usize, view: &HubView) {
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
                if view.is_live(HubGame::ALL[*i]) {
                    // A detached game in progress: a green pip after the name,
                    // on both the selected and unselected row styles.
                    let livepad = pad.saturating_sub(label.len() + 2);
                    Line::from(vec![
                        Span::styled(format!("  {label} "), style),
                        Span::styled("\u{25cf}", style.fg(theme::SUCCESS())),
                        Span::styled(format!("{:<livepad$}", ""), style),
                    ])
                } else {
                    Line::from(Span::styled(format!("  {label:<pad$}"), style))
                }
            }
            // The standing invitation atop the nav: Lateania and the
            // roguelikes detach on ` and hop between each other and chat.
            // Faint on purpose; the green pip carries the "live right now"
            // signal.
            SidebarRow::HopHint => Line::from(Span::styled(
                "  ` hop in & out",
                Style::default().fg(theme::TEXT_FAINT()),
            )),
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
        Some(SidebarRow::Header(_) | SidebarRow::Blank | SidebarRow::HopHint) | None => None,
    }
}
