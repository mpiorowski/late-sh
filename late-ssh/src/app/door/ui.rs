use std::sync::OnceLock;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::{
    common::theme,
    files::inline_image::{InlineImageRenderSettings, render_rgba_preview},
    files::terminal_image::{
        TerminalImageData, TerminalImageFrame, TerminalImagePlacement, TerminalImageProtocol,
        terminal_image_from_bytes,
    },
    state::DOOR_SELECTION_LATEANIA,
};
use crate::usernames::UsernameLookup;
use uuid::Uuid;

const FRONTIER_BANNER_PNG: &[u8] = include_bytes!("../../../assets/lateania/frontier-banner.png");
const BANNER_IMAGE_COLS: u32 = 54;
const BANNER_IMAGE_ROWS: u32 = 15;
const FRONTIER_BANNER_IMAGE_ID: Uuid = Uuid::from_u128(0x4c41_5445_414e_4941_4652_4f4e_0001);

pub struct DoorHubView<'a> {
    pub game_selection: usize,
    pub delete_confirm: bool,
    pub lateania_state: Option<&'a super::lateania::state::State>,
    pub usernames: &'a UsernameLookup<'a>,
    pub terminal_image_protocol: Option<TerminalImageProtocol>,
}

pub fn draw_door_hub(
    frame: &mut Frame,
    area: Rect,
    view: &DoorHubView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if let Some(state) = view.lateania_state {
        super::lateania::ui::draw_page(frame, area, state, view.usernames);
        return;
    }

    if area.height < 8 || area.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Lateania"), area);
        return;
    }

    draw_lateania_landing(
        frame,
        area,
        view.game_selection == DOOR_SELECTION_LATEANIA,
        view.delete_confirm,
        view.terminal_image_protocol,
        terminal_images,
    );
}

fn draw_lateania_landing(
    frame: &mut Frame,
    area: Rect,
    selected: bool,
    delete_confirm: bool,
    terminal_image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if area.width >= 104 && area.height >= 22 {
            [Constraint::Min(48), Constraint::Length(58)]
        } else {
            [Constraint::Min(0), Constraint::Length(0)]
        })
        .split(area);

    draw_launch_copy(frame, layout[0], selected, delete_confirm);
    if layout.len() > 1 && layout[1].width > 0 {
        draw_frontier_art(frame, layout[1], terminal_image_protocol, terminal_images);
    }
}

fn draw_launch_copy(frame: &mut Frame, area: Rect, selected: bool, delete_confirm: bool) {
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
        "Shared rooms, old-school classes, frontier quests, shops, titles, loot, and real persistence.",
        Style::default().fg(theme::TEXT_DIM()),
    )));
    lines.push(Line::raw(""));
    lines.extend(world_stats());
    lines.push(Line::raw(""));
    lines.push(section("Enter The World"));
    lines.push(action_line(
        if selected { ">" } else { " " },
        "Enter",
        "step through the gate",
        theme::SUCCESS(),
    ));
    lines.push(action_line(
        " ",
        "d",
        "reset your saved character",
        theme::ERROR(),
    ));
    lines.push(action_line(" ", "?", "open the guide", theme::AMBER()));
    lines.push(Line::raw(""));
    lines.push(section("Once Inside"));
    lines.push(hint_line("w/a/s/d + arrows", "move"));
    lines.push(hint_line("space / 1-9 / z", "fight, cast, flee"));
    lines.push(hint_line(
        "o / j / k / r / f",
        "look, quests, titles, recall, follow",
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
            "Press 4 any time to return here. Esc leaves the live world back to this gate.",
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn draw_frontier_art(
    frame: &mut Frame,
    area: Rect,
    terminal_image_protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
) {
    let inner = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(BANNER_IMAGE_ROWS as u16),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(area);

    if !draw_native_frontier_banner(inner[1], terminal_image_protocol, terminal_images) {
        frame.render_widget(Paragraph::new(frontier_banner_preview().to_vec()), inner[1]);
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "The Frontier is open",
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        fact_line("20", "frontier zones"),
        fact_line("1,298", "rooms in the world"),
        fact_line("100", "generated frontier items"),
        fact_line("5", "classes with unlockable abilities"),
        Line::raw(""),
        Line::from(Span::styled(
            "Your character persists. The world persists. Other adventurers are really there.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    if area.height >= 30 {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Launch, pick a class, and make a name worth wearing.",
            Style::default().fg(theme::TEXT_BRIGHT()),
        )));
    }
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner[3]);
}

fn draw_native_frontier_banner(
    area: Rect,
    protocol: Option<TerminalImageProtocol>,
    terminal_images: &mut TerminalImageFrame,
) -> bool {
    let Some(protocol) = protocol else {
        return false;
    };
    let Some(data) = frontier_terminal_image(protocol) else {
        return false;
    };
    if !data.supports_protocol(protocol) {
        return false;
    }
    let width = data.display_cols.min(area.width);
    let height = data.display_rows.min(area.height);
    if width == 0 || height == 0 {
        return false;
    }
    let image_area = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    terminal_images.push(TerminalImagePlacement {
        message_id: FRONTIER_BANNER_IMAGE_ID,
        area: image_area,
        data: data.clone(),
    });
    true
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

fn world_stats() -> Vec<Line<'static>> {
    vec![
        stat_line(
            "20 frontier zones",
            "boss quests, titles, and bounty rewards",
        ),
        stat_line("1,298 rooms", "towns, shops, capitals, dungeons, and wilds"),
        stat_line("5 classes", "Warrior, Mage, Cleric, Rogue, Ranger"),
        stat_line("shared runtime", "mob state and combat persist server-side"),
    ]
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

fn stat_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("{label:<18}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn fact_line(value: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{value:>6} "),
            Style::default()
                .fg(theme::BADGE_GOLD())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn action_line(
    marker: &str,
    key: &str,
    label: &str,
    color: ratatui::style::Color,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{marker} "),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{key:<8}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn hint_line(key: &str, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {key:<19}  "),
            Style::default().fg(theme::AMBER_DIM()),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn frontier_banner_preview() -> &'static [Line<'static>] {
    static PREVIEW: OnceLock<Vec<Line<'static>>> = OnceLock::new();
    PREVIEW
        .get_or_init(render_frontier_banner_preview)
        .as_slice()
}

fn frontier_terminal_image(protocol: TerminalImageProtocol) -> Option<&'static TerminalImageData> {
    static KITTY: OnceLock<Option<TerminalImageData>> = OnceLock::new();
    static ITERM2: OnceLock<Option<TerminalImageData>> = OnceLock::new();
    static SIXEL: OnceLock<Option<TerminalImageData>> = OnceLock::new();
    let slot = match protocol {
        TerminalImageProtocol::Kitty => &KITTY,
        TerminalImageProtocol::Iterm2 => &ITERM2,
        TerminalImageProtocol::Sixel => &SIXEL,
    };
    slot.get_or_init(|| {
        terminal_image_from_bytes(
            FRONTIER_BANNER_PNG,
            BANNER_IMAGE_COLS,
            BANNER_IMAGE_ROWS,
            protocol,
        )
        .ok()
    })
    .as_ref()
}

fn render_frontier_banner_preview() -> Vec<Line<'static>> {
    let Ok(image) = image::load_from_memory(FRONTIER_BANNER_PNG) else {
        return fallback_banner_preview();
    };
    render_rgba_preview(
        &image.to_rgba8(),
        BANNER_IMAGE_COLS,
        BANNER_IMAGE_ROWS,
        InlineImageRenderSettings::default(),
    )
    .unwrap_or_else(|_| fallback_banner_preview())
}

fn fallback_banner_preview() -> Vec<Line<'static>> {
    [
        "  The Frontier banner could not be rendered.",
        "  Enter Lateania and find the wilds yourself.",
    ]
    .into_iter()
    .map(|line| Line::from(Span::styled(line, Style::default().fg(theme::AMBER_DIM()))))
    .collect()
}
