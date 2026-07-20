use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::{
    activity::event::ActivityGame,
    common::{primitives::Banner, theme},
    door::game::{DoorGame, DoorGameId},
    door::landing,
    files::terminal_image::TerminalImageFrame,
    state::App,
};
use crate::usernames::UsernameLookup;

pub const GAME: LateaniaDoorGame = LateaniaDoorGame;

pub struct LateaniaDoorGame;

impl DoorGame for LateaniaDoorGame {
    type View<'a> = LateaniaScreenView<'a>;

    fn id(&self) -> DoorGameId {
        DoorGameId::Lateania
    }

    fn title(&self) -> &'static str {
        "Lateania"
    }

    fn description(&self) -> &'static str {
        "A persistent terminal world of six great lands: shared rooms, seventeen classes, crafting and taming trades, quests, player housing, companions, titles, and loot."
    }

    fn activity_game(&self) -> Option<ActivityGame> {
        Some(ActivityGame::Mud)
    }

    fn draw(
        &self,
        frame: &mut Frame,
        area: Rect,
        view: &LateaniaScreenView<'_>,
        _terminal_images: &mut TerminalImageFrame,
    ) {
        draw_screen(frame, area, view);
    }

    fn handle_key(&self, app: &mut App, byte: u8) -> bool {
        handle_key(app, byte)
    }

    fn handle_arrow(&self, app: &mut App, key: u8) -> bool {
        handle_arrow(app, key)
    }

    fn leave_active(&self, app: &mut App) -> bool {
        leave_active_game(app)
    }
}

pub struct LateaniaScreenView<'a> {
    pub delete_confirm: bool,
    pub state: Option<&'a super::state::State>,
    pub usernames: &'a UsernameLookup<'a>,
    /// Players currently in the Lateania world, shown on the landing.
    pub online: usize,
}

fn draw_screen(frame: &mut Frame, area: Rect, view: &LateaniaScreenView<'_>) {
    if let Some(state) = view.state {
        super::ui::draw_page(frame, area, state, view.usernames);
        return;
    }

    if area.height < 8 || area.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Lateania"), area);
        return;
    }

    draw_landing(frame, area, view.delete_confirm, view.online);
}

fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.door_delete_confirm {
        return handle_delete_confirm_key(app, byte);
    }

    if app.lateania_state.is_some() {
        return handle_active_lateania_key(app, byte);
    }

    match byte {
        b'j' | b'J' | b'k' | b'K' => true,
        b'\r' | b'\n' => {
            app.door_delete_confirm = false;
            app.enter_lateania();
            true
        }
        b'd' | b'D' => {
            app.door_delete_confirm = true;
            true
        }
        _ => false,
    }
}

fn handle_arrow(app: &mut App, key: u8) -> bool {
    if app.door_delete_confirm {
        return true;
    }

    if app.lateania_state.is_some() {
        let Some(state) = app.lateania_state.as_mut() else {
            return true;
        };
        let _ = super::input::handle_arrow(state, key);
        return true;
    }

    matches!(key, b'A' | b'B')
}

fn leave_active_game(app: &mut App) -> bool {
    if app.door_delete_confirm {
        app.door_delete_confirm = false;
        return true;
    }

    if app.lateania_state.is_some() {
        app.leave_lateania();
        true
    } else {
        false
    }
}

fn handle_delete_confirm_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'y' | b'Y' | b'\r' | b'\n' => {
            app.door_delete_confirm = false;
            app.leave_lateania();
            app.lateania_service.delete_character_task(app.user_id);
            app.banner = Some(Banner::success(
                "Lateania character reset. Enter the world to start over.",
            ));
            true
        }
        b'n' | b'N' | b'd' | b'D' | b'q' | b'Q' | 0x1B => {
            app.door_delete_confirm = false;
            true
        }
        _ => true,
    }
}

fn handle_active_lateania_key(app: &mut App, byte: u8) -> bool {
    if byte == 0x1B {
        app.leave_lateania();
        return true;
    }

    let Some(state) = app.lateania_state.as_mut() else {
        return true;
    };
    if super::input::handle_key(state, byte) == super::input::InputAction::Leave {
        app.leave_lateania();
    }
    true
}

/// Lateania landing, used both by the standalone screen fallback and the Games
/// hub when Lateania is the selected card.
pub fn draw_landing(frame: &mut Frame, area: Rect, delete_confirm: bool, online: usize) {
    draw_launch_copy(frame, area, delete_confirm, online);
}

fn draw_launch_copy(frame: &mut Frame, area: Rect, delete_confirm: bool, online: usize) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let mut lines = vec![Line::raw("")];
    lines.extend(lateania_logo());
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::styled(
            "A persistent terminal world ",
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "by Tasmania of hardlygospel.github.io",
            Style::default().fg(theme::AMBER_DIM()),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        "Shared rooms, seventeen classes, crafting and taming trades, boss quests, player housing, companions, titles, loot, and real persistence.",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.extend(world_stats(online));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Boss Achievements"));
    lines.push(landing::stat(
        "Archdemon Mal'gareth",
        "10,000 chips + LMG badge, once per account",
        22,
    ));
    lines.push(landing::stat(
        "Frontier King",
        "20,000 chips + LKN badge, once per account",
        22,
    ));
    lines.push(landing::stat(
        "Yssgar, Sundering Deep",
        "LYS badge, once per account; no chips, only glory",
        22,
    ));
    lines.push(landing::stat(
        "Kaethyr Ascendant",
        "LKA badge, once per account; no chips, only glory",
        22,
    ));
    lines.push(Line::from(Span::styled(
        "  Repeat clears keep titles and loot, but these chip payouts are lifetime claims.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Enter The World"));
    lines.push(landing::action(
        ">",
        "Enter",
        "step through the gate",
        theme::SUCCESS(),
    ));
    lines.push(landing::action(
        " ",
        "d",
        "reset your saved character",
        theme::ERROR(),
    ));
    lines.push(landing::action(" ", "?", "open the guide", theme::AMBER()));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Once Inside"));
    lines.push(landing::hint("w/a/s/d + arrows", "move", 19));
    lines.push(landing::hint("space / 1-9 / z", "fight, cast, flee", 19));
    lines.push(landing::hint(
        "o / j / k / r / f",
        "look, quests, titles, recall, follow",
        19,
    ));

    if delete_confirm {
        lines.push(Line::raw(""));
        lines.push(Line::from(vec![Span::styled(
            "Delete your Lateania character?",
            Style::default()
                .fg(theme::ERROR())
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![
            Span::styled("Enter/Y", Style::default().fg(theme::ERROR())),
            Span::styled(" confirm  ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled("N/Esc", Style::default().fg(theme::AMBER())),
            Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
        ]));
    } else {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Esc leaves the live world back to this gate.",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn lateania_logo() -> Vec<Line<'static>> {
    [
        "██╗      █████╗ ████████╗███████╗ █████╗ ███╗   ██╗██╗ █████╗",
        "██║     ██╔══██╗╚══██╔══╝██╔════╝██╔══██╗████╗  ██║██║██╔══██╗",
        "██║     ███████║   ██║   █████╗  ███████║██╔██╗ ██║██║███████║",
        "██║     ██╔══██║   ██║   ██╔══╝  ██╔══██║██║╚██╗██║██║██╔══██║",
        "███████╗██║  ██║   ██║   ███████╗██║  ██║██║ ╚████║██║██║  ██║",
        "╚══════╝╚═╝  ╚═╝   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝  ╚═══╝╚═╝╚═╝  ╚═╝",
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

fn world_stats(online: usize) -> Vec<Line<'static>> {
    let online_label = if online == 1 {
        "1 adventurer".to_string()
    } else {
        format!("{online} adventurers")
    };
    vec![
        landing::stat(&online_label, "in the world right now", 22),
        landing::stat(
            "six great lands",
            "frontier, reaches, ash, lakes, greenwood & isles",
            22,
        ),
        landing::stat(
            "8,600+ rooms",
            "towns, capitals, wilds, mazes, caves, and homes",
            22,
        ),
        landing::stat(
            "17 classes",
            "the five originals plus twelve new callings",
            22,
        ),
        landing::stat(
            "trades & taming",
            "gather, craft, fish, and tame fifty wild beasts",
            22,
        ),
        landing::stat(
            "shared runtime",
            "mob state and combat persist server-side",
            22,
        ),
        landing::stat("5 home tiers", "buy and furnish your own place", 22),
    ]
}
