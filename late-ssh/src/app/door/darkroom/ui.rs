//! Rendering for the Dark Room door: the live page and the Games-hub landing
//! card. Upstream is deliberately bare (a column of buttons, a column of
//! stores, and text fading in), and the terminal is if anything a better fit
//! for that than the browser was, so nothing here decorates.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use late_core::models::profile_award::{award_badge, award_category_label};

use crate::app::common::theme;
use crate::app::door::landing;

use super::data::{self, Building, Resource, ResourceKind};
use super::model::{Game, View};
use super::pace;
use super::state::{Ending, EndingBeat, Row, State};

/// Label column width for the stores/pack sidebar rows: the longest label
/// ("trading post", "sulphur mine") plus a two-space gutter, so a count never
/// butts up against the name it belongs to.
pub const SIDEBAR_LABEL_PAD: usize = 14;

/// The stores whose own name is too long for that column, and what the sidebar
/// calls them instead. Ours, not upstream's: upstream has no fixed-width
/// column to fit and abbreviates only where it feels like it (its own armours
/// are already "l armour"/"i armour"/"s armour", which is the style followed
/// here).
///
/// This is an exception list, not a second naming scheme: everything absent
/// renders under its real name, and `ui_test` is what keeps the list complete
/// as new stores arrive.
pub static SIDEBAR_LABELS: [(Resource, &str); 8] = [
    (Resource::KineticArmour, "k armour"),
    (Resource::FluidRecycler, "recycler"),
    (Resource::HypoBlueprint, "bp: hypo"),
    (Resource::KineticArmourBlueprint, "bp: armour"),
    (Resource::DisruptorBlueprint, "bp: disruptor"),
    (Resource::PlasmaRifleBlueprint, "bp: plasma"),
    (Resource::StimBlueprint, "bp: stim"),
    (Resource::GlowstoneBlueprint, "bp: glowstone"),
];

/// What the sidebar calls a store.
pub fn sidebar_label(resource: Resource) -> &'static str {
    SIDEBAR_LABELS
        .iter()
        .find(|(listed, _)| *listed == resource)
        .map(|(_, label)| *label)
        .unwrap_or_else(|| resource.label())
}

/// Total width of the stores/pack sidebar column: the two-space indent, the
/// label column, and room for the widest value a long run reaches
/// ("10000 +100/5s"). Too narrow and the income suffix clips off the edge.
pub const SIDEBAR_WIDTH: u16 = 30;

/// The live game page.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    let Some(game) = state.game() else {
        let loading = Paragraph::new(Line::from(Span::styled(
            "the dark is quiet...",
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .centered();
        frame.render_widget(loading, area);
        return;
    };

    // The ending takes the whole panel, and there is nothing underneath it any
    // more: the save is gone by the time it is up.
    if let Some(ending) = state.ending.as_ref() {
        draw_ending(frame, area, ending);
        return;
    }

    // The ascent takes the whole panel: no stores, no log, just the sky.
    if let Some(flight) = state.flight.as_ref() {
        super::ui_world::draw_space(frame, area, flight);
        return;
    }

    let block = Block::default()
        .title(format!(" {} ", title_for(state, game)))
        .title_style(
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // room / village status
        Constraint::Length(1), // gap
        Constraint::Fill(1),   // actions | stores
        Constraint::Length(1), // gap
        Constraint::Length(6), // notifications
        Constraint::Length(1), // footer
    ])
    .split(inner);

    frame.render_widget(Paragraph::new(status_line(state, game)), rows[0]);

    // The wasteland is a map, not a column of buttons.
    if state.view == View::World {
        let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDEBAR_WIDTH)])
            .split(rows[2]);
        super::ui_world::draw_world(frame, columns[0], state);
        frame.render_widget(Paragraph::new(pack_lines(state, game)), columns[1]);
    } else {
        let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Length(SIDEBAR_WIDTH)])
            .split(rows[2]);
        let (actions, cursor_line) = action_lines(state);
        let scroll = action_scroll(cursor_line, actions.len(), columns[0].height as usize);
        frame.render_widget(Paragraph::new(actions).scroll((scroll, 0)), columns[0]);
        frame.render_widget(Paragraph::new(stores_lines(state, game)), columns[1]);
    }

    frame.render_widget(
        Paragraph::new(log_lines(state, rows[4].height as usize)),
        rows[4],
    );
    frame.render_widget(Paragraph::new(footer(state, game)), rows[5]);

    // The modal, over everything.
    super::ui_event::draw(frame, inner, state);
}

fn title_for(state: &State, game: &Game) -> String {
    match state.view {
        View::Room => game.room_title().to_string(),
        View::Outside => game.outside_title().to_string(),
        View::Path => "A Dusty Path".to_string(),
        View::World => "A Barren World".to_string(),
        View::Fabricator => data::FABRICATOR_TITLE.to_string(),
        View::Ship => "An Old Starship".to_string(),
    }
}

/// What is in the pack right now, shown beside the map.
fn pack_lines(state: &State, game: &Game) -> Vec<Line<'static>> {
    let Some(trip) = game.expedition.as_ref() else {
        return Vec::new();
    };
    let _ = state;
    let mut lines = vec![Line::from(Span::styled(
        "pack",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))];
    for (item, count) in &trip.outfit {
        if *count <= 0 {
            continue;
        }
        lines.push(landing::stat(
            sidebar_label(*item),
            &count.to_string(),
            SIDEBAR_LABEL_PAD,
        ));
    }
    let free = game.capacity() - trip.load();
    lines.push(Line::from(""));
    lines.push(landing::stat(
        "free",
        &format!("{:.0}/{:.0}", free.max(0.0), game.capacity()),
        SIDEBAR_LABEL_PAD,
    ));
    lines
}

/// The one line that says what the world is doing right now.
fn status_line(state: &State, game: &Game) -> Line<'static> {
    let text = match state.view {
        View::Room => format!(
            "the fire is {}. the room is {}.",
            game.fire.text(),
            game.temperature.text()
        ),
        View::Outside => format!(
            "pop {}/{}, {} gathering",
            game.population,
            game.max_population(),
            game.gatherers()
        ),
        View::Path => format!(
            "armour: {}. water: {}. {} to carry.",
            game.armour_label(),
            game.max_water(),
            game.capacity() as i64
        ),
        View::World => String::new(),
        // The blueprints found so far, which is the whole of what the
        // fabricator has to say about itself.
        View::Fabricator => {
            let known: Vec<&str> = data::Blueprint::ALL
                .into_iter()
                .filter(|blueprint| game.blueprints.contains(blueprint))
                .map(data::Blueprint::label)
                .collect();
            match known.is_empty() {
                true => format!("{}: none yet", data::SECTION_BLUEPRINTS),
                false => format!("{}: {}", data::SECTION_BLUEPRINTS, known.join(", ")),
            }
        }
        View::Ship => match game.ship.as_ref() {
            Some(ship) => format!("hull: {}. engine: {}.", ship.hull, ship.thrusters),
            None => String::new(),
        },
    };
    Line::from(Span::styled(text, Style::default().fg(theme::TEXT())))
}

/// The action column: what the player can do, with the cursor and any
/// cooldown or cost, split under upstream's build/craft/buy legends. Returns
/// the lines and which of them the cursor is on, for the scroll offset.
fn action_lines(state: &State) -> (Vec<Line<'static>>, usize) {
    let selected = state.selected();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut cursor_line = 0;
    let mut section = None;
    for row in state.rows() {
        let row_section = state.row_section(row);
        if row_section.is_some() && row_section != section {
            section = row_section;
            if let Some(open) = section {
                lines.push(Line::from(Span::styled(
                    open.legend().to_string(),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                )));
            }
        }

        let is_selected = row == selected;
        if is_selected {
            cursor_line = lines.len();
        }
        let marker = if is_selected { "> " } else { "  " };
        // A row at its ceiling stays in the list (upstream keeps the button,
        // greyed) but reads as spent.
        let dim = state.row_at_maximum(row);
        let label_style = match (is_selected, dim) {
            (true, _) => Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
            (false, true) => Style::default().fg(theme::TEXT_FAINT()),
            (false, false) => Style::default().fg(theme::TEXT()),
        };
        let mut spans = vec![
            Span::styled(marker, Style::default().fg(theme::AMBER())),
            Span::styled(state.row_label(row), label_style),
        ];
        let cooldown = state.row_cooldown(row);
        if cooldown > 0 {
            spans.push(Span::styled(
                format!("  {cooldown}s"),
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        } else if let Some(cost) = cost_hint(state, row) {
            spans.push(Span::styled(
                format!("  {cost}"),
                Style::default().fg(theme::TEXT_DIM()),
            ));
        }
        lines.push(Line::from(spans));
    }
    (lines, cursor_line)
}

/// Keep the cursor on screen once the action column outgrows its box. The
/// offset is derived from the cursor every frame rather than remembered, so
/// there is no scroll state to fall out of step with the row list.
fn action_scroll(cursor_line: usize, total: usize, height: usize) -> u16 {
    if total <= height || height == 0 {
        return 0;
    }
    let centered = cursor_line.saturating_sub(height / 2);
    centered.min(total - height) as u16
}

fn cost_hint(state: &State, row: Row) -> Option<String> {
    let cost = state.row_cost(row);
    if cost.is_empty() {
        return None;
    }
    Some(
        cost.iter()
            .map(|(resource, amount)| format!("{amount} {}", resource.label()))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// The stores column. Only resources the player has actually seen appear,
/// which is how the game reveals itself. Each row carries its net income per
/// tick, the terminal stand-in for upstream's hover tooltip, so wood quietly
/// climbing (the builder, the gatherers) is visible and attributable.
fn stores_lines(state: &State, game: &Game) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if !game.perks.is_empty() {
        lines.push(Line::from(Span::styled(
            "perks",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        for perk in &game.perks {
            lines.push(Line::from(Span::styled(
                format!("  {}", perk.label()),
                Style::default().fg(theme::TEXT_DIM()),
            )));
        }
        lines.push(Line::from(""));
    }
    let _ = state;
    let income = game.income_per_tick();
    let tick = pace::slowed(data::INCOME_DELAY);
    // Upstream lists weapons in their own box and hides the expedition gear on
    // the room screen entirely (it belongs to the path). The gear block is
    // ours: until the path exists there is nowhere else a crafted waterskin
    // could show up.
    for (heading, bucket) in [
        (
            "stores",
            [
                ResourceKind::Basic,
                ResourceKind::Good,
                ResourceKind::Tool,
                ResourceKind::Special,
            ]
            .as_slice(),
        ),
        ("weapons", [ResourceKind::Weapon].as_slice()),
        ("gear", [ResourceKind::Upgrade].as_slice()),
    ] {
        let held: Vec<Resource> = Resource::ALL
            .into_iter()
            .filter(|resource| game.has_seen(*resource) && bucket.contains(&resource.kind()))
            .collect();
        if held.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            heading,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        for resource in held {
            let value = match income.get(&resource) {
                Some(rate) => format!("{} {}/{}s", game.store(resource), fmt_income(*rate), tick),
                None => game.store(resource).to_string(),
            };
            lines.push(landing::stat(
                sidebar_label(resource),
                &value,
                SIDEBAR_LABEL_PAD,
            ));
        }
    }
    let standing: Vec<&Building> = Building::ALL
        .iter()
        .filter(|building| game.building_count(**building) > 0)
        .collect();
    if !standing.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "buildings",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        for building in standing {
            // Traps list bare and baited separately, like upstream's village.
            if *building == Building::Trap {
                let (bare, baited) = game.trap_rows();
                if bare > 0 {
                    lines.push(landing::stat("trap", &bare.to_string(), SIDEBAR_LABEL_PAD));
                }
                if baited > 0 {
                    lines.push(landing::stat(
                        "baited trap",
                        &baited.to_string(),
                        SIDEBAR_LABEL_PAD,
                    ));
                }
                continue;
            }
            lines.push(landing::stat(
                building.label(),
                &game.building_count(*building).to_string(),
                SIDEBAR_LABEL_PAD,
            ));
        }
    }
    lines
}

/// A signed income rate, whole when it is whole ("+2", "-3"), one decimal when
/// it is not ("+0.5").
fn fmt_income(rate: f64) -> String {
    if rate.fract() == 0.0 {
        format!("{:+}", rate as i64)
    } else {
        format!("{rate:+.1}")
    }
}

/// The notification log, newest last, filling the space it has.
fn log_lines(state: &State, height: usize) -> Vec<Line<'static>> {
    let all: Vec<&str> = state.log().collect();
    let start = all.len().saturating_sub(height);
    all[start..]
        .iter()
        .map(|message| {
            Line::from(Span::styled(
                (*message).to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ))
        })
        .collect()
}

/// The key hints, plus the honest word on today's allowance: a village that
/// has stopped growing must never look like a bug.
fn footer(state: &State, game: &Game) -> Line<'static> {
    let mut spans = vec![
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" do   ", Style::default().fg(theme::TEXT_DIM())),
    ];
    if matches!(state.view, View::Outside | View::Path) {
        spans.push(Span::styled("+/-", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            match state.view {
                View::Path => " pack   ",
                _ => " worker   ",
            },
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled("</>", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " x10   ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if state.view == View::World {
        spans.push(Span::styled(
            "arrows",
            Style::default().fg(theme::AMBER_DIM()),
        ));
        spans.push(Span::styled(
            " walk   ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if game.forest_unlocked && state.view != View::World {
        spans.push(Span::styled("Tab", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " switch   ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    spans.push(Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())));
    spans.push(Span::styled(
        match state.view {
            View::World => " park the trip",
            _ => " leave",
        },
        Style::default().fg(theme::TEXT_DIM()),
    ));

    let remaining = state.credit_remaining();
    let (text, color) = if state.credit_exhausted() {
        (
            "   the village rests until tomorrow".to_string(),
            theme::AMBER_DIM(),
        )
    } else {
        (
            format!(
                "   {}h{:02}m of village time left today",
                remaining / 3600,
                (remaining % 3600) / 60
            ),
            theme::TEXT_FAINT(),
        )
    };
    spans.push(Span::styled(text, Style::default().fg(color)));
    Line::from(spans)
}

/// The two lines that say the run is over for good.
const ENDING_WIPED: &str = "the save is gone. the room is dark and cold again.";
const ENDING_PROMPT: &str = "press any key to step outside";

/// The ending: upstream's closing prose, the run's last figures, the badge it
/// earned, and the one key left to press. Border-less and centered, like the
/// ascent it follows.
fn draw_ending(frame: &mut Frame, area: Rect, ending: &Ending) {
    let lines = ending_lines(ending);
    // Anchor on every beat, revealed or not, so the text does not crawl up the
    // screen as the epitaph arrives.
    let top = (area.height as usize).saturating_sub(lines.len()) / 2;
    let mut padded: Vec<Line<'static>> = vec![Line::from(""); top];
    padded.extend(lines);
    frame.render_widget(Paragraph::new(padded).centered(), area);
}

/// One line per beat, in order, with the unrevealed ones left blank.
fn ending_lines(ending: &Ending) -> Vec<Line<'static>> {
    let revealed = ending.revealed_count();
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut previous: Option<&EndingBeat> = None;
    for (index, beat) in ending.beats().iter().enumerate() {
        // A blank line wherever the epitaph changes register: prose, then the
        // figures, then the badge, then the way out.
        if previous.is_some_and(|last| std::mem::discriminant(last) != std::mem::discriminant(beat))
        {
            lines.push(Line::from(""));
        }
        previous = Some(beat);
        let shown = index < revealed;
        for line in beat_lines(beat) {
            lines.push(if shown { line } else { Line::from("") });
        }
    }
    lines
}

/// How one beat reads. The unrevealed ones still take their rows, so this is
/// also what reserves the space for them.
fn beat_lines(beat: &EndingBeat) -> Vec<Line<'static>> {
    match beat {
        EndingBeat::Prose(text) => vec![Line::from(Span::styled(
            (*text).to_string(),
            Style::default().fg(theme::TEXT()),
        ))],
        // Padded to a fixed width so the centered column lines up.
        EndingBeat::Stat { label, value } => vec![Line::from(Span::styled(
            format!("{label:>16}   {value:<22}"),
            Style::default().fg(theme::TEXT_DIM()),
        ))],
        EndingBeat::Award(escape) => vec![
            Line::from(vec![
                Span::styled(
                    format!("[{}]  ", award_badge(escape.award_category(), 1)),
                    Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    award_category_label(escape.award_category()).to_string(),
                    Style::default().fg(theme::TEXT_BRIGHT()),
                ),
            ]),
            Line::from(Span::styled(
                escape.reward_line().to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ],
        EndingBeat::Prompt => vec![
            Line::from(Span::styled(
                ENDING_WIPED.to_string(),
                Style::default().fg(theme::TEXT()),
            )),
            Line::from(Span::styled(
                ENDING_PROMPT.to_string(),
                Style::default().fg(theme::TEXT_FAINT()),
            )),
        ],
    }
}

/// The two-column landing card for the Games hub.
pub fn draw_landing(frame: &mut Frame, area: Rect, delete_confirm: bool) {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];

    let mut lines = vec![Line::raw("")];
    lines.extend(title_art());
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "the fire is dead. the room is freezing.",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Light it, and see what the light brings in. Then build a",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "village around it, walk out into the wasteland, and find",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "a way off this rock.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        landing::heading("How it runs here"),
        landing::hint("pace", "the village runs slower than the original", 10),
        landing::hint(
            "time",
            "it grows while you are connected, anywhere on late.sh",
            10,
        ),
        landing::hint(
            "daily",
            &format!(
                "{}h of village time a day, so it lasts weeks",
                pace::DAILY_CREDIT_SECS / 3600
            ),
            10,
        ),
        landing::hint(
            "floor",
            &format!(
                "even a short visit banks {}m once the village stands",
                pace::DAILY_CREDIT_FLOOR_SECS / 60
            ),
            10,
        ),
        Line::from(""),
        landing::heading("Rewards"),
        landing::stat(
            "Fly out",
            "15,000 chips, and the ADE badge the first time",
            15,
        ),
        landing::stat(
            "Fleet beacon",
            "20,000 chips, and the ADB badge the first time",
            15,
        ),
        Line::from(Span::styled(
            "  Every run that gets out pays: the ending wipes the save,",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(Span::styled(
            "  so a repeat is the whole arc again, not a shortcut.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::from(""),
        landing::heading("The second pass"),
        Line::from(Span::styled(
            "Flying out once makes you a veteran. Your next map carries",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "the ravaged battleship, a wreck no first run ever sees: clear",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "it, kill the immortal wanderer, take the fleet beacon, and",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "fly out holding it for the second ending.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
    ]);

    if delete_confirm {
        lines.push(landing::action(
            "!",
            "d",
            "press again to burn it all down and start over",
            theme::ERROR(),
        ));
    } else {
        lines.push(landing::action(
            ">",
            "Enter",
            "light the fire",
            theme::SUCCESS(),
        ));
        lines.push(landing::action("x", "d", "start over", theme::ERROR()));
    }

    lines.extend([
        Line::from(""),
        landing::heading("Once Inside"),
        landing::hint(
            "j/k, w/s, arrows",
            "move the cursor; Enter or space picks",
            18,
        ),
        landing::hint("Tab", "switch between the room and outside", 18),
        landing::hint("+/- and </>", "move one or ten villagers between jobs", 18),
        landing::hint("wasd / arrows", "walk the wasteland, steer the ship", 18),
        landing::hint("Esc", "park the trip and step out; time keeps banking", 18),
    ]);

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "A port of A Dark Room by Michael Townsend / Doublespeak Games,",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::from(Span::styled(
        "open sourced under the MPL. The original, and the paid mobile",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::from(Span::styled(
        "and Steam versions, are at doublespeakgames.com.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    frame.render_widget(Paragraph::new(lines), inner);
}

/// The block-letter title, stacked over two rows like Green Dragon's so an
/// eleven-character name still fits the hub card. Amber because the whole game
/// is lit by the one fire.
fn title_art() -> Vec<Line<'static>> {
    [
        "██████╗  █████╗ ██████╗ ██╗  ██╗",
        "██╔══██╗██╔══██╗██╔══██╗██║ ██╔╝",
        "██║  ██║███████║██████╔╝█████╔╝ ",
        "██║  ██║██╔══██║██╔══██╗██╔═██╗ ",
        "██████╔╝██║  ██║██║  ██║██║  ██╗",
        "╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝",
        "██████╗  ██████╗  ██████╗ ███╗   ███╗",
        "██╔══██╗██╔═══██╗██╔═══██╗████╗ ████║",
        "██████╔╝██║   ██║██║   ██║██╔████╔██║",
        "██╔══██╗██║   ██║██║   ██║██║╚██╔╝██║",
        "██║  ██║╚██████╔╝╚██████╔╝██║ ╚═╝ ██║",
        "╚═╝  ╚═╝ ╚═════╝  ╚═════╝ ╚═╝     ╚═╝",
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
