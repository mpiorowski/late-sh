use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use super::proxy::ProxyStatus;
use super::state::{Mode, State};
use crate::app::common::theme;
use crate::app::door::landing;
use crate::app::door::rebels::render::blit_screen;

pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    match state.mode() {
        Mode::Launcher => draw_landing(frame, area, state.is_enabled()),
        Mode::Running => draw_running(frame, area, state),
    }
}

pub fn draw_landing(frame: &mut Frame, area: Rect, enabled: bool) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let action_line = if enabled {
        landing::action(">", "Enter", "defend the Gate", theme::SUCCESS())
    } else {
        Line::from(Span::styled(
            "Currently unavailable",
            Style::default().fg(theme::ERROR()),
        ))
    };

    let mut lines = vec![Line::raw("")];
    lines.extend(codekeep_logo());
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "Build a deck. Defend the Gate. ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Push back the Pale.", Style::default().fg(theme::AMBER_DIM())),
        ]),
        Line::from(Span::styled(
            "A tactical roguelike with 70+ cards, a five-column battlefield, and a Keep that remembers.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        landing::stat("runs", "procedural three-act campaigns", 10),
        landing::stat("build", "cards become attacks or lasting emplacements", 10),
        landing::stat("save", "automatic, private to your late.sh account", 10),
        Line::from(""),
        landing::heading("Launch"),
        action_line,
        Line::from(""),
        landing::heading("Once Inside"),
        landing::hint("arrows", "move through menus, maps, cards, and targets", 10),
        landing::hint("Enter", "confirm the highlighted choice", 10),
        landing::hint("q / Esc", "back; q on the main menu returns here", 10),
        landing::hint("Ctrl-C", "save and return to the Games hub", 10),
        Line::from(""),
        Line::from(Span::styled(
            "https://github.com/tooyipjee/codekeep",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ]);

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn codekeep_logo() -> Vec<Line<'static>> {
    [
        " ██████╗ ██████╗ ██████╗ ███████╗██╗  ██╗███████╗███████╗██████╗ ",
        "██╔════╝██╔═══██╗██╔══██╗██╔════╝██║ ██╔╝██╔════╝██╔════╝██╔══██╗",
        "██║     ██║   ██║██║  ██║█████╗  █████╔╝ █████╗  █████╗  ██████╔╝",
        "██║     ██║   ██║██║  ██║██╔══╝  ██╔═██╗ ██╔══╝  ██╔══╝  ██╔═══╝ ",
        "╚██████╗╚██████╔╝██████╔╝███████╗██║  ██╗████████╗███████╗██║     ",
        " ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝╚═╝  ╚═╝╚═══════╝╚══════╝╚═╝     ",
    ]
    .into_iter()
    .map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
    })
    .collect()
}

fn draw_running(frame: &mut Frame, area: Rect, state: &State) {
    let Some(proxy) = state.proxy() else {
        return;
    };
    if proxy.status() == ProxyStatus::Connecting {
        frame.render_widget(Paragraph::new("Starting CodeKeep..."), area);
        return;
    }
    // Closed wakes the renderer before the app's next tick returns to Games.
    // Preserve the final game frame during that sub-tick transition instead of
    // mislabeling it as a new connection attempt.
    let buf = frame.buffer_mut();
    proxy.with_screen(|screen| blit_screen(buf, area, screen));
}
