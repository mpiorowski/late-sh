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

use super::svc::{CHARACTER_SLOTS, SlotSummary};

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

    fn handle_mouse(&self, app: &mut App, mouse: crate::app::input::MouseEvent) -> bool {
        handle_mouse(app, mouse)
    }
}

pub struct LateaniaScreenView<'a> {
    pub delete_confirm: bool,
    pub state: Option<&'a super::state::State>,
    pub usernames: &'a UsernameLookup<'a>,
    /// Players currently in the Lateania world, shown on the landing.
    pub online: usize,
    /// This account's character slots, for the landing's select list.
    pub slots: &'a [SlotSummary],
    /// Highlighted slot on the landing.
    pub slot_cursor: usize,
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

    draw_landing(
        frame,
        area,
        view.delete_confirm,
        view.online,
        view.slots,
        view.slot_cursor,
    );
}

fn handle_key(app: &mut App, byte: u8) -> bool {
    if app.door_delete_confirm {
        return handle_delete_confirm_key(app, byte);
    }

    if app.lateania_state.is_some() {
        return handle_active_lateania_key(app, byte);
    }

    match byte {
        b'j' | b'J' => {
            move_slot_cursor(app, 1);
            true
        }
        b'k' | b'K' => {
            move_slot_cursor(app, -1);
            true
        }
        b'\r' | b'\n' => {
            app.door_delete_confirm = false;
            app.lateania_service
                .select_slot(app.user_id, app.lateania_slot_cursor as i16);
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

/// Move the landing's slot cursor, clamped to the character slots that exist.
fn move_slot_cursor(app: &mut App, delta: i32) {
    let max = CHARACTER_SLOTS as i32 - 1;
    let next = app.lateania_slot_cursor as i32 + delta;
    app.lateania_slot_cursor = next.clamp(0, max) as usize;
}

fn handle_mouse(app: &mut App, mouse: crate::app::input::MouseEvent) -> bool {
    // Only meaningful once you're in the world with a character; the landing has
    // no clickable chips.
    let Some(state) = app.lateania_state.as_mut() else {
        return false;
    };
    super::input::handle_mouse(state, mouse)
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

    match key {
        b'A' => {
            move_slot_cursor(app, -1);
            true
        }
        b'B' => {
            move_slot_cursor(app, 1);
            true
        }
        _ => false,
    }
}

fn handle_delete_confirm_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'y' | b'Y' | b'\r' | b'\n' => {
            app.door_delete_confirm = false;
            let slot = app.lateania_slot_cursor as i16;
            // A reset slot must not stay a backtick stop: hopping back in
            // would silently start a fresh character there.
            app.lateania_detached_at = None;
            app.leave_lateania();
            app.lateania_service
                .delete_character_task(app.user_id, slot);
            app.banner = Some(Banner::success(&format!(
                "Slot {} reset. Enter to start a new character there.",
                slot + 1
            )));
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
    // Esc must route through `input::handle_key` like every other byte, not
    // be special-cased here: that function is what cancels an in-progress
    // chat compose on Esc instead of leaving, and what gates a genuine leave
    // behind a confirming second press. Short-circuiting Esc here used to
    // skip both, so Esc while chatting closed the game and a single
    // accidental Esc always logged the player straight out.
    let Some(state) = app.lateania_state.as_mut() else {
        return true;
    };
    match super::input::handle_key(state, byte) {
        super::input::InputAction::Leave => {
            app.lateania_detached_at = None;
            app.leave_lateania();
        }
        // Arm the recency window that keeps Lateania on the backtick cycle,
        // then hop: the cycle's screen switch tears the session down
        // (autosave + world leave), so this must be armed before it runs.
        super::input::InputAction::Detach => {
            app.lateania_detached_at = Some(std::time::Instant::now());
            app.detach_door_game();
        }
        super::input::InputAction::Ignored | super::input::InputAction::Handled => {}
    }
    true
}

/// Lateania landing, used both by the standalone screen fallback and the Games
/// hub when Lateania is the selected card.
pub fn draw_landing(
    frame: &mut Frame,
    area: Rect,
    delete_confirm: bool,
    online: usize,
    slots: &[SlotSummary],
    slot_cursor: usize,
) {
    draw_launch_copy(frame, area, delete_confirm, online, slots, slot_cursor);
}

/// One row of the character-select list: the highlighted slot gets a `>`
/// marker and bright text; an empty slot reads as an invitation to start one.
fn slot_row(slot: &SlotSummary, highlighted: bool) -> Line<'static> {
    let marker_color = if highlighted {
        theme::SUCCESS()
    } else {
        theme::TEXT_FAINT()
    };
    let marker_style = if highlighted {
        Style::default()
            .fg(marker_color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(marker_color)
    };
    let desc = if slot.occupied {
        match slot.class {
            Some(class) => format!("{}, Lv {}", class.name(), slot.level),
            None => format!("Lv {} - no class chosen yet", slot.level),
        }
    } else {
        "empty - start a new character".to_string()
    };
    let desc_color = if slot.occupied {
        theme::TEXT_BRIGHT()
    } else {
        theme::TEXT_FAINT()
    };
    Line::from(vec![
        Span::styled(
            format!(
                "{} {}. ",
                if highlighted { ">" } else { " " },
                slot.slot + 1
            ),
            marker_style,
        ),
        Span::styled(desc, Style::default().fg(desc_color)),
    ])
}

fn draw_launch_copy(
    frame: &mut Frame,
    area: Rect,
    delete_confirm: bool,
    online: usize,
    slots: &[SlotSummary],
    slot_cursor: usize,
) {
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
        "10,000 chips, and the LMG badge the first time",
        24,
    ));
    lines.push(landing::stat(
        "Frontier King",
        "10,000 chips, and the LKN badge the first time",
        24,
    ));
    lines.push(landing::stat(
        "Yssgar, Sundering Deep",
        "20,000 chips, and the LYS badge the first time",
        24,
    ));
    lines.push(landing::stat(
        "Kaethyr Ascendant",
        "20,000 chips, and the LKA badge the first time",
        24,
    ));
    lines.push(Line::from(Span::styled(
        "  Each crown pays once per character, and at most once every 7 days per account.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::raw(""));
    lines.push(landing::heading("Choose Your Character"));
    for slot in slots {
        lines.push(slot_row(slot, slot.slot as usize == slot_cursor));
    }
    lines.push(Line::raw(""));
    lines.push(landing::hint("j/k or up/down", "highlight a slot", 19));
    lines.push(landing::action(
        ">",
        "Enter",
        "play the highlighted slot",
        theme::SUCCESS(),
    ));
    lines.push(landing::action(
        " ",
        "d",
        "reset the highlighted slot",
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
            format!("Delete the character in slot {}?", slot_cursor + 1),
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
